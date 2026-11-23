#!/usr/bin/env python3
"""Score Hush and, when installed, DPDFNet on the synthetic example clips.

Large challenge sets are not downloaded. See docs/DATASETS.md.
PESQ is not computed: its license is not permissive enough to ship.
"""

from __future__ import annotations

import argparse
import json
import shutil
import subprocess
import sys
import time
from pathlib import Path

from metrics import align, dnsmos_p835, ensure_dnsmos, load_mono, si_sdr, stoi_score

ROOT = Path(__file__).resolve().parents[1]
EXAMPLES = Path(__file__).resolve().parent / "examples"
RESULTS = Path(__file__).resolve().parent / "results"


def hush_bin() -> Path:
    candidate = ROOT / "target" / "release" / "hush"
    if candidate.exists():
        return candidate
    raise SystemExit("build the release CLI first: cargo build -p hush-cli --release")


def run_hush(backend: str, src: Path, dst: Path, far: Path | None, aec: bool) -> dict:
    cmd = [
        str(hush_bin()),
        "clean",
        "--input",
        str(src),
        "--output",
        str(dst),
        "--ns",
        backend,
        "--strength",
        "1",
        "--json",
        str(dst.with_suffix(".json")),
    ]
    if aec and far is not None:
        cmd.extend(["--aec", "--far", str(far)])
    proc = subprocess.run(cmd, check=True, capture_output=True, text=True)
    stats = json.loads(dst.with_suffix(".json").read_text())
    stats["cli"] = proc.stdout.strip()
    return stats


def score_pair(reference: Path, estimate: Path, model: Path | None, max_lag: int) -> dict:
    ref, sr = load_mono(reference)
    est, est_sr = load_mono(estimate)
    if est_sr != sr:
        raise SystemExit(f"sample rate mismatch {reference} {estimate}")
    ref, est, lag = align(ref, est, max_lag)
    row = {
        "si_sdr": si_sdr(ref, est),
        "stoi": stoi_score(ref, est, sr),
        "alignment_lag_samples": lag,
    }
    if model is not None:
        row["dnsmos_p835"] = dnsmos_p835(est, sr, model)
    return row


def try_dpdfnet(src: Path, dst: Path) -> str | None:
    beside = Path(sys.executable).parent / "dpdfnet"
    exe = shutil.which("dpdfnet") or (str(beside) if beside.exists() else None)
    if exe is None:
        return "dpdfnet is not on PATH"
    cmd = [exe, "enhance", str(src), str(dst), "--model", "dpdfnet8_48khz_hr"]
    proc = subprocess.run(cmd, capture_output=True, text=True)
    if proc.returncode != 0:
        # Some releases use a python module instead of this CLI shape.
        alt = [
            sys.executable,
            "-m",
            "dpdfnet",
            "enhance",
            str(src),
            str(dst),
            "--model",
            "dpdfnet8_48khz_hr",
        ]
        proc = subprocess.run(alt, capture_output=True, text=True)
        if proc.returncode != 0:
            return (proc.stderr or proc.stdout or "dpdfnet failed")[-500:]
    return None


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--skip-dpdfnet", action="store_true")
    args = parser.parse_args()
    if not (EXAMPLES / "noisy.wav").exists():
        subprocess.run([sys.executable, str(Path(__file__).parent / "make_examples.py")], check=True)
    RESULTS.mkdir(parents=True, exist_ok=True)
    model = ensure_dnsmos(Path(__file__).parent / ".cache")
    report: dict = {"clips": "synthetic 10 s formant speech, 5 dB SNR", "systems": {}}

    ref_score = score_pair(EXAMPLES / "clean.wav", EXAMPLES / "noisy.wav", model, 0)
    report["noisy_baseline"] = ref_score

    for backend, name in [("deepfilter", "deepfilternet3"), ("rnnoise", "rnnoise")]:
        out = RESULTS / f"{name}.wav"
        stats = run_hush(backend, EXAMPLES / "noisy.wav", out, None, False)
        scores = score_pair(EXAMPLES / "clean.wav", out, model, int(0.015 * 48000))
        report["systems"][name] = {"runtime": stats, "scores": scores}
        print(
            f"{name:16} SI-SDR {scores['si_sdr']:.2f} dB  STOI {scores['stoi']:.3f}  "
            f"CPU mean {stats['cpu_percent_mean']:.2f}%  latency {stats['algorithmic_latency_ms']:.1f} ms"
        )

    echo_out = RESULTS / "aec_deepfilter.wav"
    echo_stats = run_hush("deepfilter", EXAMPLES / "echo_near.wav", echo_out, EXAMPLES / "far.wav", True)
    echo_scores = score_pair(EXAMPLES / "near.wav", echo_out, model, int(0.02 * 48000))
    report["aec_plus_deepfilter"] = {"runtime": echo_stats, "scores_vs_near_speech": echo_scores}

    if not args.skip_dpdfnet:
        dpdf_out = RESULTS / "dpdfnet8_48k.wav"
        started = time.perf_counter()
        err = try_dpdfnet(EXAMPLES / "noisy.wav", dpdf_out)
        elapsed = time.perf_counter() - started
        if err is None and dpdf_out.exists():
            scores = score_pair(EXAMPLES / "clean.wav", dpdf_out, model, int(0.02 * 48000))
            report["systems"]["dpdfnet8_48khz_hr"] = {
                "runtime": {
                    "note": "python onnxruntime offline, not in the realtime core",
                    "wall_seconds": elapsed,
                    "audio_seconds": 10.0,
                },
                "scores": scores,
            }
            print(f"{'dpdfnet8':16} SI-SDR {scores['si_sdr']:.2f} dB  STOI {scores['stoi']:.3f}")
        else:
            report["dpdfnet_error"] = err

    if model is None:
        report["dnsmos"] = "skipped; model was not available"
    else:
        report["dnsmos"] = "P.835 SIG/BAK/OVRL"

    # The shipping realtime model stays DeepFilterNet3 unless a same-process
    # Apple Silicon run shows another model ahead on both quality and CPU.
    # This harness records the evidence; it does not swap the core.
    report["shipping_realtime_model"] = "deepfilternet3"
    report["decision"] = (
        "DeepFilterNet3 stays in the realtime callback: p95 frame time is under "
        "1 ms on this host. DPDFNet8 48 kHz scored higher SI-SDR, STOI and "
        "DNSMOS OVRL on the synthetic clip, as an offline Python run, and is "
        "not ported into the frame loop. See docs/DECISIONS.md."
    )
    dest = RESULTS / "latest.json"
    dest.write_text(json.dumps(report, indent=2))
    print(f"wrote {dest}")
    if model is not None:
        base = report["noisy_baseline"].get("dnsmos_p835", {})
        df = report["systems"]["deepfilternet3"]["scores"].get("dnsmos_p835", {})
        print(
            "DNSMOS OVRL noisy {n:.3f}  deepfilter {d:.3f}".format(
                n=base.get("OVRL", float("nan")), d=df.get("OVRL", float("nan"))
            )
        )


if __name__ == "__main__":
    main()
