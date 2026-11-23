"""Intrusive and non-intrusive scores for the Hush benchmark.

DNSMOS P.835 follows the MIT-licensed Microsoft DNS Challenge scorer
(dnsmos_local.py): 16 kHz audio, 9.01 s hops, sig_bak_ovr.onnx, then the
published non-personalized polynomial map. PESQ is intentionally absent.
"""

from __future__ import annotations

import math
from pathlib import Path

import numpy as np
import soundfile as sf
from scipy.signal import resample_poly

SAMPLING_RATE = 16_000
INPUT_LENGTH = 9.01
MODEL_URL = "https://github.com/microsoft/DNS-Challenge/raw/master/DNSMOS/DNSMOS/sig_bak_ovr.onnx"


def load_mono(path: Path) -> tuple[np.ndarray, int]:
    audio, sr = sf.read(path, always_2d=False)
    audio = np.asarray(audio, dtype=np.float64)
    if audio.ndim > 1:
        audio = audio.mean(axis=1)
    return audio, int(sr)


def align(reference: np.ndarray, estimate: np.ndarray, max_lag: int) -> tuple[np.ndarray, np.ndarray, int]:
    """Shift estimate onto reference. Lag is clamped so a noise correlation cannot wander."""
    n = min(len(reference), len(estimate))
    reference = reference[:n]
    estimate = estimate[:n]
    if n < 32 or max_lag <= 0:
        return reference, estimate, 0
    search = min(max_lag, n // 4)
    ref = reference - reference.mean()
    est = estimate - estimate.mean()
    corr = np.correlate(est, ref, mode="full")
    center = len(est) - 1
    window = corr[center - search : center + search + 1]
    lag = int(np.argmax(window) - search)
    if lag > 0:
        estimate = np.concatenate([estimate[lag:], np.zeros(lag)])
    elif lag < 0:
        estimate = np.concatenate([np.zeros(-lag), estimate[:lag]])
    return reference, estimate, lag


def si_sdr(reference: np.ndarray, estimate: np.ndarray) -> float:
    reference = reference - np.mean(reference)
    estimate = estimate - np.mean(estimate)
    denom = np.dot(reference, reference) + 1e-12
    target = np.dot(reference, estimate) * reference / denom
    noise = estimate - target
    return float(10.0 * math.log10((np.dot(target, target) + 1e-12) / (np.dot(noise, noise) + 1e-12)))


def stoi_score(reference: np.ndarray, estimate: np.ndarray, sr: int) -> float:
    from pystoi import stoi

    n = min(len(reference), len(estimate))
    return float(stoi(reference[:n], estimate[:n], sr, extended=False))


def _polyfit(sig: float, bak: float, ovr: float) -> tuple[float, float, float]:
    # Non-personalized P.835 map from Microsoft DNS-Challenge dnsmos_local.py (MIT).
    p_ovr = np.poly1d([-0.06766283, 1.11546468, 0.04602535])
    p_sig = np.poly1d([-0.08397278, 1.22083953, 0.0052439])
    p_bak = np.poly1d([-0.13166888, 1.60915514, -0.39604546])
    return float(p_sig(sig)), float(p_bak(bak)), float(p_ovr(ovr))


def ensure_dnsmos(cache: Path) -> Path | None:
    cache.mkdir(parents=True, exist_ok=True)
    path = cache / "sig_bak_ovr.onnx"
    if path.exists() and path.stat().st_size > 100_000:
        return path
    try:
        import urllib.request

        urllib.request.urlretrieve(MODEL_URL, path)
    except Exception as exc:  # noqa: BLE001 - the harness reports a skip
        print(f"DNSMOS model download failed: {exc}")
        return None
    if path.stat().st_size < 100_000:
        text = path.read_text(errors="ignore")[:120]
        print(f"DNSMOS file is not a model ({text!r})")
        path.unlink(missing_ok=True)
        return None
    return path


def dnsmos_p835(audio: np.ndarray, sr: int, model: Path) -> dict[str, float]:
    import onnxruntime as ort

    if sr != SAMPLING_RATE:
        audio = resample_poly(audio, SAMPLING_RATE, sr)
        sr = SAMPLING_RATE
    need = int(INPUT_LENGTH * sr)
    while len(audio) < need:
        audio = np.concatenate([audio, audio])
    hops = int(math.floor(len(audio) / sr) - INPUT_LENGTH) + 1
    session = ort.InferenceSession(str(model), providers=["CPUExecutionProvider"])
    sigs, baks, ovrs = [], [], []
    for idx in range(max(hops, 1)):
        seg = audio[int(idx * sr) : int((idx + INPUT_LENGTH) * sr)]
        if len(seg) < need:
            continue
        features = seg.astype(np.float32)[np.newaxis, :]
        raw = session.run(None, {"input_1": features})[0][0]
        sig, bak, ovr = _polyfit(float(raw[0]), float(raw[1]), float(raw[2]))
        sigs.append(sig)
        baks.append(bak)
        ovrs.append(ovr)
    return {
        "SIG": float(np.mean(sigs)),
        "BAK": float(np.mean(baks)),
        "OVRL": float(np.mean(ovrs)),
    }
