# Decisions

Each choice is the one that keeps a commercial product on permissive licenses and, where two permissive options existed, the one with the better audio result.

## Pipeline order

Echo cancellation runs before noise removal. AEC3 needs the unprocessed microphone to see the echo. DeepFilterNet3 then removes what the canceller left behind, including noise. Reversing them makes the canceller adapt to a signal it did not hear.

## WebRTC AEC3, not a smaller canceller

`webrtc-audio-processing` 2.1 with the `bundled` feature compiles the BSD WebRTC audio module. The echo canceller is `EchoCanceller::Full`, which is AEC3, with delay estimation left on and the built-in noise suppressor left off. The high-pass filter stays on because AEC3 is designed to see a high-passed capture. A Rust reimplementation would be a second filter to keep correct. The C++ module is the one the browsers ship.

AEC3's echo-path delay is not added to the transmitted latency. The capture frame it returns is aligned with the microphone frame. The latency figure reported by Hush is the noise suppressor's delay plus, in the live engine, the samples sitting in the input buffer.

## DeepFilterNet3 is the shipping suppressor

`deep-filter-rt` 0.2 runs DeepFilterNet3 on tract and embeds the weights. The crate and the weights are MIT or Apache-2.0. Upstream stated that explicitly in November 2026; the crate's NOTICE records the commit. Tract keeps the runtime free of ONNX Runtime and of libtorch.

Suppression strength maps onto DeepFilterNet's attenuation limit: 0 bypasses the model and its delay, 1 is a 100 dB limit, which the model treats as unlimited. A dry/wet mix would need a second copy of the delayed microphone and would comb-filter if the delay were even one hop off. The model's own limit does not have that problem.

RNNoise, through `nnnoiseless` (BSD-3-Clause), is the fallback when a machine cannot spend the DeepFilterNet budget. Its reported delay is one 10 ms hop, which is the overlap-add of its 20 ms window. Strength below 1 mixes that wet signal with a dry signal delayed by the same hop.

## DPDFNet8 is the default

The synthetic vowel clip is not a ranking. DeepFilterNet3's own delay test says the model suppresses tones. VoiceBank-DEMAND, all 824 test files, is the measurement that counts. PESQ below is wideband, computed locally, and is not shipped.

| System | SI-SDR | STOI | PESQ | DNSMOS OVRL | p95 frame |
| --- | ---: | ---: | ---: | ---: | --- |
| Noisy | 8.39 dB | 0.921 | 1.97 | 2.68 | |
| DPDFNet8 | 18.86 dB | 0.942 | 2.94 | 3.18 | 3.34 ms on a real file, 4.29 ms on a 300-hop probe |
| RNNoise | 12.90 dB | 0.917 | 2.28 | 2.98 | 0.06 ms |
| DeepFilterNet3, tract | 8.89 dB | 0.840 | 1.92 | 2.66 | 0.63 ms |

DPDFNet8 is inside the callback. ONNX Runtime (`ort` 2.0.0-rc.15, MIT or Apache-2.0) runs `dpdfnet8_48khz_hr.onnx`. The STFT is the causal Vorbis window from the Python `StreamEnhancer`: 960-sample window, 480-sample hop. The Rust stream matches that Python stream at correlation 1.000 on `p232_001`. The reported delay is 2400 samples (50 ms): one hop of buffering plus the four-hop lookahead the exporter documents for `center=True`. With the delay set to one hop, SI-SDR on that file was −34 dB and STOI was 0.39; PESQ, which aligns internally, still read 3.12. After the 50 ms shift, the same file is SI-SDR 22.4, STOI 0.917, PESQ 3.17.

The p95 hop is under 8 ms on this machine, so DPDFNet8 is the default. DeepFilterNet3 remains the lighter full-band option. RNNoise remains the low-CPU option.

## DeepFilterNet3 on tract does not match the published VoiceBank score

The published DeepFilterNet3 VoiceBank numbers are about PESQ 3.17 and STOI 0.944. This export, on the same 824 files, is PESQ 1.92 and STOI 0.840, essentially the noisy clip on PESQ and worse on STOI. What was checked:

- Sample rate 48 kHz, hop 480, one channel. The loader rejects anything else.
- Attenuation limit 100 dB, which the runtime treats as unlimited.
- Post-filter beta 0.02 and post-filter off. One real clip scored PESQ 2.43 and 2.40. Neither approaches the paper.
- Delay. The crate's own test measures 1440 samples on a vowel and our shift uses that. A lag search of the enhanced file against clean speech peaks at 0 after that shift, so the bad score is not a leftover delay.

The tract model does remove noise: DNSMOS BAK rises from 3.11 to 3.82. SIG falls from 3.32 to 2.95, which is the speech damage. The PyTorch checkpoint that produced the paper number is not this ONNX. Until a tract export reproduces that number, DeepFilterNet3 is not the quality model.

## Why the cleaned file is shifted

Offline cleaning flushes the model delay with zeros and then removes that many samples, so the WAV lines up with the input. The JSON still reports the delay that was removed. The benchmark may nudge alignment by up to 15 ms, because a denoiser smears onsets, and it records the lag it used.

## Virtual devices

One AudioServerPlugIn, built on libASPL 3.1.2 (MIT), publishes two devices:

- Krisp Alt Microphone, 48 kHz float32 stereo, read by the call app.
- Krisp Alt Speaker, 48 kHz float32 stereo, written by the call app.

The plugin does not run the neural net. The app writes the cleaned microphone into `/hush-krisp-alt-mic`. The plugin writes the call's playback, averaged to mono, into `/hush-krisp-alt-spk`. The app plays that segment on the hardware output and feeds it to AEC3. A HAL plugin is a poor place to open a second device, and coreaudiod's sandbox makes a direct hardware tap unreliable. The bridge is the app.

The segments are mode 0666. The plugin and the app are not always the same uid, and the reader increments an underrun counter, so the mapping has to be writable. Treat it as local audio, not a secret. A production build should move this to a per-user mach port.

The microphone reader is timestamp-addressed, so two apps hear the same signal. It sits 20 ms behind the writer and jumps forward if it falls more than 250 ms behind. The speaker reader is sequential, because there is one consumer: the app.

Device latency published to Core Audio is the model's delay plus that 20 ms. The initial value assumes DeepFilterNet3 (30 ms) plus one frame plus the jitter, and `OnStartIO` replaces it with the value the publisher stored.

Intel and Apple Silicon are one plugin. `HUSH_UNIVERSAL=1` passes both architectures to CMake. The deployment target is macOS 12 so Monterey-era Intel machines are included. This VM cannot compile or load the plugin. The source matches libASPL 3.1.2's documented callbacks. That is not a substitute for `system_profiler` on a Mac.

## Installers are unsigned on purpose

`scripts/macos/build-unsigned-pkg.sh` and `build-unsigned-dmg.sh` are the packaging path. An unsigned package installs for development. Shipping to other people needs a Developer ID Application certificate, notarization, and for the HAL plugin a signature that coreaudiod will load. Ad-hoc signing is what `build.sh` does by default. On Apple Silicon that is often not enough for a plugin running inside coreaudiod.

## Desktop shell

Tauri 2, React, TypeScript, Tailwind, and Radix primitives (the shadcn set: button, switch, slider, select, label, card). The window calls the same `RealtimeSession` as the CLI. The browser preview of the Vite app does not process audio; the controls are the real ones once the webview is up.

## Windows, after macOS

The core stays the same crate. The ring uses `CreateFileMapping` of `Local\hush-krisp-alt-mic` and `Local\hush-krisp-alt-spk` on Windows. The driver itself is not compiled here: there is no WDK on this machine. The intended base is the MIT/MS-PL Virtual Audio Driver project, with SysVAD (MS-PL) as the reference for a simple virtual endpoint. See `drivers/windows/README.md`. Test signing is documented there, and so is the production path: EV certificate, Partner Center attestation versus an HLK submission.

## What was left out on license grounds

- PESQ. The ITU reference is not a permissive license. It is not a dependency and the harness does not compute it. Internal research can add it locally.
- BlackHole and other GPL virtual devices. libASPL is the plugin library instead.
- espeak-ng, which is GPL, for the example speech. The clips are an original formant synthesizer.
- WebRTC's older noise suppressor. It is BSD, and it is turned off because DeepFilterNet3 is the better suppressor, not because of the license.

## Benchmark clips

Ten seconds of synthetic vowels at 48 kHz, mixed with pink noise at 5 dB SNR, plus a separate echo clip. They are generated, not copied out of DNS Challenge or VoiceBank-DEMAND. Those sets are the right next measurement and are too large, and too variously licensed, to commit. `docs/DATASETS.md` has the URLs.
