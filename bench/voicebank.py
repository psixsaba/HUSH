#!/usr/bin/env python3
"""Score the VoiceBank-DEMAND test set. The WAVs stay in bench/.cache and are not committed.

PESQ is computed only when the internal `pesq` package is installed. That package
embeds the ITU reference and is not a product dependency.
"""

from __future__ import annotations

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
    audio = np.asarray(audio, dtype=np.float64)
    if audio.ndim > 1:
        audio = audio.mean(axis=1)
    return audio


def pesq_wb(reference: np.ndarray, estimate: np.ndarray, sr: int) -> float | None:
    try:
        from pesq import pesq
    except ImportError:
        return None
    if sr != 16000:
        reference = resample_poly(reference, 16000, sr)
        estimate = resample_poly(estimate, 16000, sr)
        sr = 16000
    n = min(len(reference), len(estimate))
    if n < 1600:
        return None
    return float(pesq(sr, reference[:n], estimate[:n], "wb"))


def summarize(rows: list[dict], key: str) -> dict:
    values = [row[key] for row in rows if row.get(key) is not None]
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


def score_dir(name: str, directory: Path, model: Path | None) -> dict:
    rows = []
    files = sorted(directory.glob("*.wav"))
    for index, estimate_path in enumerate(files, start=1):
        reference_path = CLEAN / estimate_path.name
        if not reference_path.exists():
            continue
        reference = load(reference_path)
        estimate = load(estimate_path)
        n = min(len(reference), len(estimate))
        reference = reference[:n]
        estimate = estimate[:n]
        row = {
            "file": estimate_path.name,
            "si_sdr": si_sdr(reference, estimate),
            "stoi": stoi_score(reference, estimate, 48000),
            "pesq_wb": pesq_wb(reference, estimate, 48000),
        }
        if model is not None:
            row["dnsmos"] = dnsmos_p835(estimate, 48000, model)
        rows.append(row)
        if index % 50 == 0:
            print(f"{name} {index}/{len(files)}", flush=True)
    dnsmos = {}
    if rows and "dnsmos" in rows[0]:
        for key in ("SIG", "BAK", "OVRL"):
            dnsmos[key] = summarize(
                [{"dnsmos": row["dnsmos"][key]} for row in rows],
                "dnsmos",
            )
    return {
        "n": len(rows),
        "si_sdr": summarize(rows, "si_sdr"),
        "stoi": summarize(rows, "stoi"),
        "pesq_wb": summarize(rows, "pesq_wb"),
        "dnsmos_p835": dnsmos,
    }


def main() -> None:
    if not CLEAN.is_dir() or not NOISY.is_dir():
        raise SystemExit("VoiceBank-DEMAND test WAVs are not in bench/.cache/voicebank")
    model = ensure_dnsmos(ROOT / ".cache")
    report = {
        "set": "VoiceBank-DEMAND test, 48 kHz, CC BY 4.0, not committed",
        "systems": {
            "noisy": score_dir("noisy", NOISY, model),
        },
    }
    for name, folder in (
        ("dpdfnet8", CACHE / "out-dpdf"),
        ("deepfilternet3", CACHE / "out-df"),
        ("rnnoise", CACHE / "out-rnn"),
    ):
        if folder.is_dir() and any(folder.glob("*.wav")):
            report["systems"][name] = score_dir(name, folder, model)
            stoi = report["systems"][name]["stoi"].get("mean")
            pesq = report["systems"][name]["pesq_wb"].get("mean")
            print(f"{name} STOI {stoi} PESQ {pesq}", flush=True)
    dest = ROOT / "results" / "voicebank.json"
    dest.parent.mkdir(parents=True, exist_ok=True)
    dest.write_text(json.dumps(report, indent=2))
    print(dest)


if __name__ == "__main__":
    sys.exit(main())
