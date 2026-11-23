# Hush

Real-time noise and echo cancellation for calls. The microphone path is WebRTC AEC3, then DeepFilterNet3, with RNNoise when you want the smaller CPU bill. A virtual microphone and a virtual speaker, both named for the call apps that will select them, sit on top of that core on macOS. Windows uses the same core. Its kernel driver is specified and not built here.

The call app should use **Krisp Alt Microphone** and **Krisp Alt Speaker**. Hush has to be running. The speaker is how remote audio gets to your headphones, and it is the echo reference.

## Layout

```
crates/hush-core     48 kHz pipeline, devices, shared-memory ring
crates/hush-ffi      C API (include/hush.h)
crates/hush-cli      hush clean, hush run, hush devices
apps/desktop         Tauri 2 shell
drivers/macos        libASPL AudioServerPlugIn
drivers/windows      driver plan, IOCTLs, test-signing notes
native/ring          the ring, one implementation, macOS and Windows
bench                SI-SDR, STOI, DNSMOS P.835
```

Why these pieces, and not the others, is in [docs/DECISIONS.md](docs/DECISIONS.md).

## Licenses

The shipped code and weights are MIT, Apache-2.0, BSD-3-Clause, or MS-PL. Nothing GPL and nothing non-commercial is linked. PESQ is not included.

| Piece | License | Role |
| --- | --- | --- |
| This repository | MIT OR Apache-2.0 | Pipeline, CLI, app, plugin, harness |
| webrtc-audio-processing 2.1, bundled | BSD-3-Clause | AEC3 |
| deep-filter-rt 0.2 and DeepFilterNet3 weights | MIT OR Apache-2.0 | Noise removal on tract |
| nnnoiseless 0.5.2 (RNNoise) | BSD-3-Clause | Low-CPU fallback |
| tract (via deep-filter-rt) | MIT OR Apache-2.0 | ONNX runtime for DeepFilterNet3 |
| cpal, hound, rtrb | Apache-2.0 / MIT | Devices, WAV, realtime queues |
| libASPL 3.1.2 | MIT | macOS AudioServerPlugIn |
| Tauri 2, React, Radix | MIT OR Apache-2.0 | Desktop shell |
| numpy, scipy, soundfile, pystoi, onnxruntime | BSD / MIT | Benchmark only |
| DPDFNet and its weights | Apache-2.0 | Benchmark candidate, not in the audio callback |
| DNSMOS P.835 model and scorer | MIT (Microsoft DNS Challenge) | Benchmark only, downloaded, not committed |
| Virtual Audio Driver (when fetched) | MIT / MS-PL | Windows driver base |
| SysVAD (not vendored) | MS-PL | Windows reference sample |
| PESQ | ITU, not permissive | Do not add |

Linux links libasound from the system. That library is LGPL and is not shipped. macOS links Core Audio, which is the OS.

## Build

Rust stable (1.96 or newer; DeepFilterNet's crate asks for it). CMake, a C++ compiler, and Meson, because AEC3 builds the bundled C++ module with Meson.

### Linux

```bash
sudo apt-get install -y build-essential pkg-config libasound2-dev cmake clang meson ninja-build g++
export CC=gcc CXX=g++
# Some Clang installs cannot see libstdc++ headers. Bindgen needs them.
export BINDGEN_EXTRA_CLANG_ARGS="-cxx-isystem /usr/include/c++/13 -cxx-isystem /usr/include/x86_64-linux-gnu/c++/13"
cargo test -p hush-core -p hush-ffi -p hush-cli --release
cargo build -p hush-cli --release
```

### macOS

```bash
brew install cmake meson ninja
cargo build -p hush-cli -p hush-desktop --release
drivers/macos/build.sh          # add HUSH_UNIVERSAL=1 for Intel and Apple Silicon
sudo scripts/macos/install-driver.sh
```

Unsigned package and disk image, after the driver and the release CLI exist:

```bash
mkdir -p dist
scripts/macos/build-unsigned-pkg.sh
scripts/macos/build-unsigned-dmg.sh
```

`pkgbuild` and `hdiutil` are macOS tools. They were not run in the environment that produced this tree.

The desktop UI, from `apps/desktop`:

```bash
npm install
npm run dev          # http://127.0.0.1:43123  preview, no audio engine
npm run tauri dev    # the shell, wired to the core
```

### Windows

```powershell
# Rust stable, Visual Studio Build Tools, Meson, Ninja.
cargo build -p hush-cli --release
```

The kernel driver is the next step on a machine with the WDK. [drivers/windows/README.md](drivers/windows/README.md) covers test signing and the production choice between Partner Center attestation and HLK.

## Use

```bash
hush devices
hush clean --input bench/examples/noisy.wav --output /tmp/clean.wav --json /tmp/clean.json
hush clean --input bench/examples/echo_near.wav --far bench/examples/far.wav --aec --output /tmp/echo.wav
hush run --ns rnnoise                 # live mic, prints one line per 10 ms frame
hush run --bridge --output "Headphones"
```

`--ns` is `dpdfnet` (default), `deepfilter`, `rnnoise`, or `off`. `--strength 0` skips the suppressor and its delay. Strength 1 is the model's full attenuation. The DPDFNet ONNX is not in git. `dpdfnet download dpdfnet8_48khz_hr` puts it in `~/.cache/dpdfnet/models/`, or set `HUSH_DPDF_ONNX`.

`hush run` prints processing time and CPU for each frame. CPU is that time divided by the 10 ms frame.

## Benchmark

```bash
python3 -m venv bench/.venv
bench/.venv/bin/pip install -r bench/requirements.txt
python3 bench/make_examples.py
cargo build -p hush-cli --release
bench/.venv/bin/python bench/harness.py
```

The harness writes `bench/results/latest.json`. It reports SI-SDR, STOI, and DNSMOS P.835 (SIG, BAK, OVRL), plus the CLI's latency and CPU. How to fetch DNS Challenge and VoiceBank-DEMAND, and why PESQ is absent, is [docs/DATASETS.md](docs/DATASETS.md).

## What a real Mac or PC still has to prove

This tree was built and measured on Linux. It was not loaded into coreaudiod, not signed, and not run through Zoom.

On a Mac:

- `HUSH_UNIVERSAL=1 drivers/macos/build.sh` produces `KrispAltMicrophone.driver`.
- After install, `system_profiler SPAudioDataType` should list Krisp Alt Microphone and Krisp Alt Speaker.
- Start Hush, select the pair in a call, and confirm the other side hears the cleaned mic and you hear them.
- Re-measure CPU on Apple Silicon. The shipping model stays DeepFilterNet3 until that number, and a DPDFNet comparison in-process, say otherwise. The budget used here is a p95 frame under 8 ms.

On a PC:

- Build the core with the MSVC toolchain.
- Build the virtual driver from the fetched sample with the WDK, test-sign it, and confirm the two endpoints enumerate.
- Attestation signing needs an EV certificate and a Partner Center account. HLK is separate and was not run.

VoiceBank-DEMAND test set, all 824 files, 48 kHz, release build on this machine. The WAVs are not in git. PESQ is wideband at 16 kHz and is an internal number only. DNSMOS is P.835.

| System | SI-SDR | STOI | PESQ | SIG | BAK | OVRL | Frame cost |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| Noisy | 8.39 dB | 0.921 | 1.97 | 3.32 | 3.11 | 2.68 | |
| DPDFNet8 | 18.86 dB | 0.942 | 2.94 | 3.40 | 4.12 | 3.18 | 3.3 ms p95, 50 ms delay |
| RNNoise | 12.90 dB | 0.917 | 2.28 | 3.33 | 3.83 | 2.98 | 0.06 ms p95, 10 ms delay |
| DeepFilterNet3 (tract) | 8.89 dB | 0.840 | 1.92 | 2.95 | 3.82 | 2.66 | 0.63 ms p95, 30 ms delay |

DPDFNet8 is the default. Its p95 hop on a VoiceBank file is 3.34 ms, and a 300-hop release probe peaked at 4.29 ms, both under the 8 ms budget. The Rust stream matches the Python causal enhancer at correlation 1.000 on `p232_001`. DeepFilterNet3 stays available as the lighter neural option. On this tract export it does not reproduce the published VoiceBank PESQ of about 3.17 and STOI of about 0.944: delay and sample rate check out, and turning the post-filter off does not close the gap. RNNoise is the low-CPU path, and on this set it beats that DeepFilterNet3 export.

The DNS Challenge blind set was not scored. The official archives are multi-gigabyte and are not in the git repository. A synthetic-vowel clip is still generated by `bench/make_examples.py` for a smoke test. Those STOI and SIG numbers are not a model ranking.

AEC3 plus DeepFilterNet3 on the synthetic echo clip: 5.5% CPU mean, 7.2% p95. A pure 80 ms echo in the unit test lost more than 8 dB in the last half second.
