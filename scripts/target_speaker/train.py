#!/usr/bin/env python3
"""Training entry for the causal target-speaker masker.

No weights are produced by a default run. Without --steps the script prints
the GPU estimate and exits. With --steps and PyTorch installed, it overfits a
tiny random-initialized masker on synthetic sines so the loop can be checked
without LibriSpeech or a GPU. That toy is not the model to ship.
"""

from __future__ import annotations

import argparse

ESTIMATE = """
GPU estimate (not a measurement):
  model: causal 48 kHz masker, about 3e6 parameters, 20 ms window, 10 ms hop
  data: 100 hours of LibriSpeech + MUSAN mixtures from recipe.py
  machine: one A100 40 GB, batch 16 of 4 second clips
  time: on the order of one day for 100 hours, a few days for 500 hours and 30 epochs
PESQ is not part of the loss. Do not initialize from a non-commercial checkpoint.
WeSpeaker ResNet34 (Apache-2.0) is the frozen embedding.
"""


def toy_steps(steps: int) -> None:
    import torch

    torch.manual_seed(0)
    mask = torch.nn.Linear(8, 8)
    target = torch.sin(torch.linspace(0, 8, 8))
    opt = torch.optim.Adam(mask.parameters(), lr=1e-2)
    last = 0.0
    for _ in range(steps):
        pred = mask(target)
        loss = torch.mean((pred - target) ** 2)
        opt.zero_grad()
        loss.backward()
        opt.step()
        last = float(loss)
    if not torch.isfinite(torch.tensor(last)):
        raise SystemExit("toy step produced a non-finite loss")
    print(f"toy steps {steps} final loss {last:.4f} (not a shipping model)")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--manifest", help="JSON from recipe.py. Required for a real run, unused by the toy.")
    parser.add_argument("--steps", type=int, default=0, help="Toy overfit steps. 0 prints the estimate and exits.")
    args = parser.parse_args()
    print(ESTIMATE.strip())
    if args.steps <= 0:
        return
    try:
        import torch  # noqa: F401
    except ImportError as exc:
        raise SystemExit(f"PyTorch is not installed ({exc}). The estimate above is the plan.") from exc
    toy_steps(args.steps)


if __name__ == "__main__":
    main()
