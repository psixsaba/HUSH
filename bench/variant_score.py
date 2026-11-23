#!/usr/bin/env python3
"""Score one DPDFNet export on the VoiceBank-DEMAND test set.

The WAVs stay in bench/.cache. This script does not commit them. PESQ is
optional and is not a product dependency. Alignment uses the lag measured on
the first file, because every export here is a fixed-delay stream.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

import numpy as np
import soundfile as sf
from scipy.signal import resample_poly

from metrics import dnsmos_p835, ensure_dnsmos, si_sdr, stoi_score

ROOT = Path(__file__).resolve().parent
CACHE = ROOT / ".cache" / "voicebank"
CLEAN = CACHE / "clean_testset_wav"
NOISY = CACHE / "noisy_testset_wav"


def load(path: Path) -> np.ndarray:
    audio, _sr = sf.read(path, always_2d=False)
    audio = np.asarray(audio, dtype=np.float32)
    if audio.ndim > 1:
        audio = audio.mean(axis=1)
    return audio


def to_rate(audio: np.ndarray, src: int, dst: int) -> np.ndarray:
    if src == dst:
        return np.asarray(audio, dtype=np.float32)
    g = np.gcd(src, dst)
    return resample_poly(audio, dst // g, src // g).astype(np.float32)


def pesq_wb(reference: np.ndarray, estimate: np.ndarray) -> float | None:
    try:
        from pesq import pesq
    except ImportError:
        return None
    reference = to_rate(reference, 48000, 16000)
    estimate = to_rate(estimate, 48000, 16000)
    n = min(len(reference), len(estimate))
    if n < 1600:
        return None
    return float(pesq(16000, reference[:n], estimate[:n], "wb"))


def best_lag(reference: np.ndarray, estimate: np.ndarray) -> int:
    ref = np.asarray(reference, dtype=np.float64)
    est = np.asarray(estimate, dtype=np.float64)
    n = min(len(ref), len(est))
    ref = ref[:n]
    est = est[:n]
    best = (-1e18, 0)
    for lag in range(-2400, 2401, 10):
        if lag >= 0:
            a, b = est[lag:], ref[: n - lag]
        else:
            a, b = est[: n + lag], ref[-lag:]
        if len(a) < 4000:
            continue
        score = float(np.dot(a - a.mean(), b - b.mean()))
        if score > best[0]:
            best = (score, lag)
    return best[1]


def apply_lag(estimate: np.ndarray, lag: int, length: int) -> np.ndarray:
    if lag >= 0:
        trimmed = estimate[lag:]
    else:
        trimmed = np.pad(estimate, (-lag, 0))
    if len(trimmed) < length:
        trimmed = np.pad(trimmed, (0, length - len(trimmed)))
    return trimmed[:length]


def summarize(values: list[float]) -> dict:
    if not values:
        return {}
    arr = np.asarray(values, dtype=np.float64)
    return {
        "mean": float(arr.mean()),
        "p50": float(np.median(arr)),
        "min": float(arr.min()),
        "max": float(arr.max()),
        "n": int(arr.size),
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--model", required=True)
    parser.add_argument("--sample-rate", type=int, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    if not NOISY.is_dir():
        raise SystemExit("VoiceBank-DEMAND test WAVs are not in bench/.cache/voicebank")

    from dpdfnet.stream import StreamEnhancer

    enhancer = StreamEnhancer(args.model)
    files = sorted(NOISY.glob("*.wav"))
    model = ensure_dnsmos(ROOT / ".cache")
    lag: int | None = None
    rows: list[dict] = []
    for index, path in enumerate(files, start=1):
        reference = load(CLEAN / path.name).astype(np.float64)
        noisy = load(path)
        chunk = to_rate(noisy, 48000, args.sample_rate)
        enhanced = np.asarray(enhancer.process(chunk, args.sample_rate), dtype=np.float32)
        enhancer.reset()
        enhanced = to_rate(enhanced, args.sample_rate, 48000)
        if lag is None:
            lag = best_lag(reference, enhanced)
            print(f"lag_samples {lag} ({lag / 48:.2f} ms)", flush=True)
        aligned = apply_lag(enhanced, lag, len(reference)).astype(np.float64)
        row = {
            "file": path.name,
            "si_sdr": si_sdr(reference, aligned),
            "stoi": stoi_score(reference, aligned, 48000),
            "pesq_wb": pesq_wb(reference, aligned),
        }
        if model is not None:
            row["dnsmos"] = dnsmos_p835(aligned, 48000, model)
        rows.append(row)
        if index % 25 == 0:
            print(
                f"{args.model}@{args.sample_rate} {index}/{len(files)} "
                f"si-sdr {np.mean([r['si_sdr'] for r in rows]):.2f}",
                flush=True,
            )
    dnsmos = {}
    if rows and "dnsmos" in rows[0]:
        for key in ("SIG", "BAK", "OVRL"):
            dnsmos[key] = summarize([row["dnsmos"][key] for row in rows])
    report = {
        "model": args.model,
        "sample_rate": args.sample_rate,
        "lag_samples_at_48k": lag,
        "lag_ms": None if lag is None else lag / 48.0,
        "n": len(rows),
        "si_sdr": summarize([row["si_sdr"] for row in rows]),
        "stoi": summarize([row["stoi"] for row in rows]),
        "pesq_wb": summarize([row["pesq_wb"] for row in rows if row["pesq_wb"] is not None]),
        "dnsmos_p835": dnsmos,
    }
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(report, indent=2))
    print(json.dumps({k: report[k] for k in ("model", "sample_rate", "lag_ms", "si_sdr", "stoi", "pesq_wb", "dnsmos_p835")}, indent=2))


if __name__ == "__main__":
    main()
