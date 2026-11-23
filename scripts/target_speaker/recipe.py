#!/usr/bin/env python3
"""Write a target-speaker training manifest from permissive corpora only.

This does not download data and does not train. It checks that every path
the operator points at is on the license list in docs/TARGET_SPEAKER.md.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path

ALLOWED = {
    "librispeech-train-clean-360": "CC BY 4.0",
    "musan-noise": "CC BY 4.0",
    "voicebank-demand-noise": "CC BY 4.0",
}

REFUSED = {
    "wsj",
    "wsj0",
    "librimix",
    "whamr",
    "audioset",
    "voxceleb",
}


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--speech", type=Path, required=True, help="LibriSpeech train-clean-360 root")
    parser.add_argument("--noise", type=Path, required=True, help="MUSAN noise root")
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--source-id", action="append", default=None)
    args = parser.parse_args()
    source_ids = args.source_id or ["librispeech-train-clean-360", "musan-noise"]

    for source_id in source_ids:
        key = source_id.lower()
        if any(blocked in key for blocked in REFUSED):
            raise SystemExit(f"refusing {source_id}: not on the permissive list")
        if key not in ALLOWED:
            raise SystemExit(f"refusing {source_id}: add it to ALLOWED only with a permissive license")

    if not args.speech.is_dir() or not args.noise.is_dir():
        raise SystemExit("speech and noise directories must exist. This script does not download them.")

    speakers = sorted(path for path in args.speech.rglob("*") if path.is_dir())
    noises = sorted(path for path in args.noise.rglob("*.wav"))
    manifest = {
        "sample_rate": 48000,
        "clip_seconds": 4,
        "enrollment_seconds": 3,
        "sir_db": [-5, 5],
        "snr_db": [0, 15],
        "licenses": {key: ALLOWED[key.lower()] for key in source_ids},
        "embedding": {
            "model": "WeSpeaker ResNet34",
            "license": "Apache-2.0",
            "frozen": True,
        },
        "speech_dir": str(args.speech),
        "noise_dir": str(args.noise),
        "speaker_dirs": len(speakers),
        "noise_files": len(noises),
        "note": "Enrollment must be a different utterance from the same speaker, never the mixture clip.",
    }
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(manifest, indent=2))
    print(f"wrote {args.out} speakers {len(speakers)} noises {len(noises)}")


if __name__ == "__main__":
    main()
