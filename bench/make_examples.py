#!/usr/bin/env python3
"""Synthesize a small 48 kHz speech-like set. No dataset is downloaded."""

from __future__ import annotations

import math
from pathlib import Path

import numpy as np
import soundfile as sf

SR = 48_000
SECONDS = 10


def formant(n: int, f0: float, formants: list[tuple[float, float]], sr: int) -> np.ndarray:
    t = np.arange(n, dtype=np.float64) / sr
    glottal = np.zeros(n, dtype=np.float64)
    for k in range(1, 28):
        glottal += np.sin(2 * math.pi * f0 * k * t) / k
    # Two-pole resonators, one per formant. Coefficients are recomputed
    # only when the formant moves, which these vowels do not.
    out = np.zeros(n, dtype=np.float64)
    for freq, bandwidth in formants:
        r = math.exp(-math.pi * bandwidth / sr)
        theta = 2 * math.pi * freq / sr
        a1 = -2 * r * math.cos(theta)
        a2 = r * r
        y1 = 0.0
        y2 = 0.0
        acc = np.zeros(n, dtype=np.float64)
        for i, x in enumerate(glottal):
            y = x - a1 * y1 - a2 * y2
            y2, y1 = y1, y
            acc[i] = y
        peak = np.max(np.abs(acc)) + 1e-9
        out += acc / peak
    peak = np.max(np.abs(out)) + 1e-9
    return (out / peak).astype(np.float32)


def syllable(vowel: str, f0: float, dur: float, sr: int) -> np.ndarray:
    vowels = {
        "a": [(700, 80), (1200, 90), (2500, 120)],
        "i": [(300, 60), (2200, 100), (3000, 140)],
        "u": [(350, 70), (800, 80), (2300, 120)],
        "e": [(450, 70), (1800, 100), (2600, 130)],
        "o": [(500, 70), (900, 80), (2400, 120)],
    }
    n = int(dur * sr)
    tone = formant(n, f0, vowels[vowel], sr)
    env = np.hanning(n).astype(np.float32)
    burst = np.zeros(n, dtype=np.float32)
    burst_n = min(n, int(0.02 * sr))
    burst[:burst_n] = np.random.default_rng(n + int(f0)).standard_normal(burst_n).astype(np.float32) * 0.15
    return tone * env * 0.8 + burst


def speech(seed: int, f0: float) -> np.ndarray:
    rng = np.random.default_rng(seed)
    vowels = ["a", "i", "u", "e", "o", "a", "e", "i"]
    chunks = []
    total = 0
    target = SECONDS * SR
    while total < target:
        vowel = vowels[len(chunks) % len(vowels)]
        dur = float(rng.uniform(0.18, 0.32))
        piece = syllable(vowel, f0 * float(rng.uniform(0.94, 1.06)), dur, SR)
        gap = np.zeros(int(rng.uniform(0.04, 0.09) * SR), dtype=np.float32)
        chunks.append(piece)
        chunks.append(gap)
        total += len(piece) + len(gap)
    audio = np.concatenate(chunks)[:target]
    return audio / (np.max(np.abs(audio)) + 1e-9) * 0.6


def pink(n: int, rng: np.random.Generator) -> np.ndarray:
    white = rng.standard_normal(n).astype(np.float32)
    spec = np.fft.rfft(white)
    freqs = np.fft.rfftfreq(n, 1 / SR)
    freqs[0] = freqs[1]
    spec /= np.sqrt(freqs)
    noise = np.fft.irfft(spec, n=n).astype(np.float32)
    noise /= np.sqrt(np.mean(noise**2)) + 1e-9
    return noise


def mix(clean: np.ndarray, noise: np.ndarray, snr_db: float) -> np.ndarray:
    clean_rms = np.sqrt(np.mean(clean**2)) + 1e-9
    noise = noise / (np.sqrt(np.mean(noise**2)) + 1e-9) * clean_rms * 10 ** (-snr_db / 20)
    mixed = clean + noise
    peak = np.max(np.abs(mixed)) + 1e-9
    if peak > 0.98:
        mixed *= 0.98 / peak
    return mixed.astype(np.float32)


def main() -> None:
    out = Path(__file__).resolve().parent / "examples"
    out.mkdir(parents=True, exist_ok=True)
    rng = np.random.default_rng(7)
    clean = speech(1, 124.0)
    near_speech = speech(2, 186.0)
    far = speech(3, 110.0)
    noise = pink(len(clean), rng)
    noisy = mix(clean, noise, 5.0)
    delay = int(0.08 * SR)
    echo = np.zeros_like(near_speech)
    echo[delay:] = far[:-delay] * 0.55
    echo_near = mix(near_speech * 0.7 + echo, pink(len(near_speech), rng) * 0.15, 20.0)
    sf.write(out / "clean.wav", clean, SR, subtype="FLOAT")
    sf.write(out / "noisy.wav", noisy, SR, subtype="FLOAT")
    sf.write(out / "near.wav", near_speech, SR, subtype="FLOAT")
    sf.write(out / "far.wav", far, SR, subtype="FLOAT")
    sf.write(out / "echo_near.wav", echo_near, SR, subtype="FLOAT")
    print(f"wrote {len(list(out.glob('*.wav')))} clips to {out}")


if __name__ == "__main__":
    main()
