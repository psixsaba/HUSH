# Decisions

Each choice is the one that keeps a commercial product on permissive licenses and, where two permissive options existed, the one with the better audio result.

## Pipeline order

Echo cancellation runs before noise removal. AEC3 needs the unprocessed microphone to see the echo. DeepFilterNet3 then removes what the canceller left behind, including noise. Reversing them makes the canceller adapt to a signal it did not hear.

## WebRTC AEC3, not a smaller canceller

`webrtc-audio-processing` 2.1 with the `bundled` feature compiles the BSD WebRTC audio module. The echo canceller is `EchoCanceller::Full`, which is AEC3, with delay estimation left on and the built-in noise suppressor left off. The high-pass filter stays on because AEC3 is designed to see a high-passed capture. A Rust reimplementation would be a second filter to keep correct. The C++ module is the one the browsers ship.

AEC3's echo-path delay is not added to the transmitted latency. The capture frame it returns is aligned with the microphone frame. The latency figure reported by HUSH is the noise suppressor's delay plus, in the live engine, the samples sitting in the input buffer.

## DeepFilterNet3 stayed in the tree

It is not the default. The VoiceBank table below is why.

`deep-filter-rt` 0.2 runs DeepFilterNet3 on tract and embeds the weights. The crate and the weights are MIT or Apache-2.0. Upstream stated that explicitly in November 2026; the crate's NOTICE records the commit. Tract keeps the runtime free of ONNX Runtime and of libtorch.

Suppression strength maps onto DeepFilterNet's attenuation limit: 0 bypasses the model and its delay, 1 is a 100 dB limit, which the model treats as unlimited. A dry/wet mix would need a second copy of the delayed microphone and would comb-filter if the delay were even one hop off. The model's own limit does not have that problem.

RNNoise, through `nnnoiseless` (BSD-3-Clause), is the low-CPU fallback. Its reported delay is one 10 ms hop, which is the overlap-add of its 20 ms window. Strength below 1 mixes that wet signal with a dry signal delayed by the same hop.

## DPDFNet8 is the default

The synthetic vowel clip is not a ranking. DeepFilterNet3's own delay test says the model suppresses tones. VoiceBank-DEMAND, all 824 test files, is the measurement that counts. PESQ below is wideband, computed locally, and is not shipped.

| System | SI-SDR | STOI | PESQ | DNSMOS OVRL | Delay | p95 frame |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| Noisy | 8.39 dB | 0.921 | 1.97 | 2.68 | | |
| DPDFNet8 48 kHz | 18.86 dB | 0.942 | 2.94 | 3.18 | 50 ms | 3.2 ms on `p232_001`, 3.1 ms on a 300-hop probe |
| DPDFNet2 48 kHz | 18.88 dB | 0.939 | 2.98 | 3.12 | 50 ms | 1.5 ms on the same file |
| DPDFNet8 16 kHz | 18.59 dB | 0.922 | 2.59 | 3.20 | 40 ms group delay, not wired in | |
| DPDFNet2 16 kHz | 18.38 dB | 0.920 | 2.58 | 3.20 | 40 ms group delay, not wired in | |
| RNNoise | 12.90 dB | 0.917 | 2.28 | 2.98 | 10 ms | 0.06 ms |
| DeepFilterNet3, tract | 8.89 dB | 0.840 | 1.92 | 2.66 | 30 ms | 0.63 ms |

DPDFNet8 stays the default. DPDFNet2 matches it on SI-SDR and PESQ and is about half the CPU, and it gives up a little STOI, BAK, and DNSMOS OVRL (3.12 against 3.18). The rule for the default is the better speech result, so the heavier model stays. DPDFNet2 is the same-latency lighter setting. Two DPDFNet8 hops, microphone plus incoming audio, are about 6.4 ms of the 10 ms frame on this machine.

The 16 kHz exports are not a low-latency mode. Their group delay is the same 40 ms, and wideband PESQ drops by about 0.35 because the band above 8 kHz is gone. STOI falls to the noisy clip. The library lists `baseline`, `dpdfnet4`, and both 8 kHz exports at the same 20 ms frame. They were not scored: a shorter window is not on offer, and the 16 kHz models that were scored already missed DPDFNet8.

ONNX Runtime (`ort` 2.0.0-rc.15, MIT or Apache-2.0) runs the 48 kHz graph. The STFT is the causal Vorbis window from the Python `StreamEnhancer`: 960-sample window, 480-sample hop. The Rust stream matches that Python stream at correlation 1.000 on `p232_001`. With the delay set to one hop, SI-SDR on that file was −34 dB and STOI was 0.39. After the 50 ms shift, the same file is SI-SDR 22.45, STOI 0.917, PESQ 3.17. DPDFNet2 on that file, same shift, is SI-SDR 22.49, STOI 0.914, PESQ 3.43.

## There is no low-latency DPDFNet

The target was 40 ms. Krisp is about 25 ms. The Python stream's own note says the first samples come back after one 20 ms window. That is when `process` stops returning an empty array. It is not when the speech lines up.

On `p232_001`, the Python stream's group delay is 1920 samples, 40.0 ms, for `dpdfnet2_48khz_hr`, `dpdfnet8_48khz_hr`, `dpdfnet2`, and `dpdfnet8`. The 16 kHz hop is 160 samples, which is still 10 ms. Nothing in the registry advertises a shorter frame than 20 ms.

This pipeline has to emit a frame every 10 ms. The first 10 ms is spent filling the 20 ms window, so the aligned delay is 2400 samples, 50 ms. Prefilling that window with zeros was measured. The residual lag stayed one hop, and SI-SDR on the file fell to −17 dB until the shift was put back to 50 ms. The reported delay stays 50 ms.

DeepFilterNet3 is 30 ms and RNNoise is 10 ms. Neither is near DPDFNet8 on this set, so neither is offered as a low-latency mode that keeps the default quality. They remain selectable. The app labels show the delay next to the name.

## DeepFilterNet3 on tract does not match the published VoiceBank score

The published DeepFilterNet3 VoiceBank numbers are about PESQ 3.17 and STOI 0.944. This export, on the same 824 files, is PESQ 1.92 and STOI 0.840, essentially the noisy clip on PESQ and worse on STOI. What was checked:

- Sample rate 48 kHz, hop 480, one channel. The loader rejects anything else.
- Attenuation limit 100 dB, which the runtime treats as unlimited.
- Post-filter beta 0.02 and post-filter off. One real clip scored PESQ 2.43 and 2.40. Neither approaches the paper.
- Delay. The crate's own test measures 1440 samples on a vowel and our shift uses that. A lag search of the enhanced file against clean speech peaks at 0 after that shift, so the bad score is not a leftover delay.

The tract model does remove noise: DNSMOS BAK rises from 3.11 to 3.82. SIG falls from 3.32 to 2.95, which is the speech damage. The PyTorch checkpoint that produced the paper number is not this ONNX. Until a tract export reproduces that number, DeepFilterNet3 is not the quality model.

## Why the cleaned file is shifted

Offline cleaning flushes the model delay with zeros and then removes that many samples, so the WAV lines up with the input. The JSON still reports the delay that was removed. The benchmark may nudge alignment by up to 15 ms, because a denoiser smears onsets, and it records the lag it used.

## Incoming audio

HUSH Speaker is cleaned with a second suppressor, the same backend as the microphone, then played to the hardware output. The echo reference is that cleaned buffer, because that is the signal that will come back through the room. The canceller is not given a second copy of the model delay. The playback itself is already delayed by the suppressor, and AEC3 estimates the acoustic path from there.

The second DPDFNet session shares the one ONNX Runtime environment the process is allowed to create. A unit test loads two sessions. Turning processing off, or strength to 0, skips the incoming model and its delay. This path was not played through Core Audio here.

## Other voices

No permissive realtime model removes other speakers. WeSpeaker's embedding is Apache-2.0 and is not a separator. The published target-speaker checkpoints we looked at are trained on WSJ or published under a non-commercial grant. The setting exists so the choice is visible, and it refuses to start. The data recipe, the licenses, and the GPU estimate are in `docs/TARGET_SPEAKER.md`. `scripts/target_speaker/recipe.py` will not write a manifest for WSJ, LibriMix, WHAMR, AudioSet, or VoxCeleb.

## Virtual devices

One AudioServerPlugIn, built on libASPL 3.1.2 (MIT), publishes two devices:

- HUSH Microphone, 48 kHz float32 stereo, read by the call app.
- HUSH Speaker, 48 kHz float32 stereo, written by the call app.

The plugin does not run the neural net. The app writes the cleaned microphone into `/hush-mic`. The plugin writes the call's playback, averaged to mono, into `/hush-spk`. The app plays that segment on the hardware output and feeds it to AEC3. A HAL plugin is a poor place to open a second device, and coreaudiod's sandbox makes a direct hardware tap unreliable. The bridge is the app.

The segments are mode 0666. The plugin and the app are not always the same uid, and the reader increments an underrun counter, so the mapping has to be writable. Treat it as local audio, not a secret. A production build should move this to a per-user mach port.

The microphone reader is timestamp-addressed, so two apps hear the same signal. It sits 20 ms behind the writer and jumps forward if it falls more than 250 ms behind. The speaker reader is sequential, because there is one consumer: the app.

Device latency published to Core Audio is the model's delay plus that 20 ms. The initial value assumes DeepFilterNet3 (30 ms) plus one frame plus the jitter, and `OnStartIO` replaces it with the value the publisher stored.

Intel and Apple Silicon are one plugin. `HUSH_UNIVERSAL=1` passes both architectures to CMake. The deployment target is macOS 12 so Monterey-era Intel machines are included. This VM cannot compile or load the plugin. The source matches libASPL 3.1.2's documented callbacks. That is not a substitute for `system_profiler` on a Mac.

## Installers are unsigned on purpose

`scripts/macos/build-unsigned-pkg.sh` and `build-unsigned-dmg.sh` are the packaging path. An unsigned package installs for development. Shipping to other people needs a Developer ID Application certificate, notarization, and for the HAL plugin a signature that coreaudiod will load. Ad-hoc signing is what `build.sh` does by default. On Apple Silicon that is often not enough for a plugin running inside coreaudiod.

## Desktop shell

Tauri 2, React, TypeScript, Tailwind, and Radix primitives (the shadcn set: button, switch, slider, select, label, card). The window calls the same `RealtimeSession` as the CLI. The browser preview of the Vite app does not process audio; the controls are the real ones once the webview is up.

## cpal 0.16 lists HUSH Speaker

`hush devices` uses cpal. cpal 0.15 asked Core Audio for output formats with the input flag set, so any output that was not the current system default failed `supported_output_configs` and disappeared. system_profiler still showed HUSH Speaker, and Zoom could open it, because they do not use that probe. cpal 0.16 fixes the probe (`audio_unit_from_device` is no longer called with `input = true` for an output). `list_devices` also merges `host.devices()` filtered by `supports_output` / `supports_input`, so a virtual endpoint that one iterator drops is still named. The settings window does not pick a HUSH endpoint as the real microphone or the real speakers.

`scripts/macos/dev-setup-and-test.sh` runs `cargo build -p hush-cli --release` after the tests. `cargo test` builds the test harness, not `target/release/hush`.

## Windows

The core stays the same crate. The rings are `Local\hush-mic` and `Local\hush-spk`. A kernel miniport in session 0 cannot open those, so `hush service` runs in the user session and feeds `\Device\HushVad` with the IOCTLs in `drivers/windows/ioctl.h`. The driver copies that queue into the capture DMA buffer and queues render audio back out. 48 kHz is one sample in, one sample out. Other rates are a zero-order hold.

The file-mapping DACL is set in `CreateFileMapping`, not afterwards. That is the same constraint as the macOS ring: Darwin returns EINVAL from `fchmod` on a POSIX shm fd, and a Windows section keeps the DACL it was created with. The DACL grants the logged-in user, Administrators, and SYSTEM. The view is mapped with `FILE_MAP_READ | FILE_MAP_WRITE`. `FILE_MAP_ALL_ACCESS` also demands `WRITE_DAC`, which a world-readable DACL does not include, so the map would fail for the process that did not create the section. The control device uses the WDK SDDL `SDDL_DEVOBJ_SYS_ALL_ADM_RWX_WORLD_RWX_RES_RWX` for the same reason.

One named mutex, `Local\HushBridge`, decides which process owns the IOCTL loop. Login starts the tray app and `hush.exe service`. The service takes the mutex. The app waits two seconds and skips the loop if the service already has it. Closing the window hides it to the tray. Quit exits the app and leaves the service running.

The NSIS and MSI packages are per-machine Tauri bundles. Both run `install-driver.ps1`, which copies `onnxruntime.dll` next to the executables (the `ort` crate loads it from the exe directory) and calls `hush driver install`. That command trusts the test certificate, stages the inf with `pnputil`, creates the `ROOT\VirtualAudioDriver` node with SetupAPI, and binds the inf with `UpdateDriverForPlugAndPlayDevices`. No GPL devcon is shipped. Uninstall removes the node, every `C:\Windows\INF\oem*.inf` that mentions the driver (pnputil's labels are localized; the file contents are not), and the login registry values.

DPDFNet is resolved from `HUSH_DPDF_ONNX`, `%ProgramData%\HUSH\models`, `XDG_CACHE_HOME`, `%USERPROFILE%\.cache\dpdfnet\models`, and `HOME`. Windows does not set `HOME`. `hush models fetch` writes the Apache-2.0 ONNX into the first of those directories it can create.

CI (`.github/workflows/windows-package.yml`) installs the WDK, compiles `VirtualAudioDriver.sln` Release x64, test-signs the sys and the catalog, runs the Rust tests, and builds the installers. A missing kit or a compile error fails the job. The runner does not reboot into test signing, so it does not load the driver. The steps for a PC, and the EV-plus-attestation path for a real signature, are in `docs/WINDOWS_TEST.md`.

The Windows Rust job has to compile WebRTC's bundled C++ with MSVC. `windows-latest` puts MinGW ahead of `cl`, and Meson then builds Abseil against a WinRT header that does not compile. `scripts/ci/use-msvc.ps1` loads `vcvars64` and drops MinGW from `PATH`. The driver package job runs on `windows-2022` because that image's Visual Studio 2022 is what the WDK registers `WindowsKernelModeDriver10.0` with. The universal Mac plugin forwards both architectures to libASPL with CMake's `LIST_SEPARATOR`. An unquoted `arm64;x86_64` was split, and the x86_64 slice linked an arm64 static library.

## App window and the 0.2 installers

The ON/OFF control starts and stops `RealtimeSession` inside the Tauri process. The last on/off state, the device names, the model, and the three clean switches are stored in the webview's local storage, so a restart restores them. Start on login is separate: macOS writes a per-user LaunchAgent, Windows writes `HKCU\...\Run\HUSH`. The elevated Windows driver install only registers the user-session bridge, which has no window.

The macOS disk image carries `HUSH.app` and `HUSH_0.2.0_macos.pkg`. The package copies the universal HAL plugin and its postinstall script runs `killall coreaudiod`. The app can do the same copy through an administrator prompt. There is no Apple Developer ID in this project, so the signature is ad hoc and the image is not notarized. The Windows NSIS and MSI packages are unchanged in shape: per-machine, test-signed driver, Start Menu shortcut, uninstaller removes the driver.

Level meters are the RMS of the microphone frame and of the frame the pipeline just wrote. CPU and latency are the same counters the CLI prints.

## The big switch is passthrough

OFF does not stop the audio session. Stopping it drops the writer on `/hush-mic` (and stops playback into CABLE Input on Windows), so the call's microphone goes silent. OFF sets the pipeline's `enabled` flag to false. The session keeps running and publishes the raw microphone, with echo cancellation and the noise model bypassed. ON sets that flag back to true without tearing the session down. The same path is used by the tray item and the "Clean my microphone" switch. Device changes restart the session, because a live cpal stream cannot be pointed at a different device. Model changes go through the existing pipeline update.

The POSIX shared-memory objects are created with mode 0666 while `umask` is briefly 0. `fchmod` on a Darwin shm fd returns EINVAL, and a 0644 ring cannot be opened by `coreaudiod`. `fstat` `st_size` is treated as "at least" the ring size because Darwin rounds it up to a page.

## VB-CABLE on Windows

A free virtual microphone that other apps can select has to be a kernel driver with a Microsoft signature, or somebody else's signed driver. HushVad cannot be signed for free, so the default Windows path does not load it. `hush service` and the app use VB-CABLE when `\\.\HushVad` does not open. The cleaned microphone is written to the render endpoint CABLE Input. Call apps select the capture endpoint, renamed to HUSH Microphone. Echo cancellation uses WASAPI loopback of the real speakers. There is no free virtual speaker, so incoming-voice cleaning is off in this mode. The installer downloads Pack45, checks the pinned SHA-256, and runs VB-Audio's setup with `-i -h`, falling back to their window. The zip is not in the repository. Test Mode and the HushVad package stay available for driver work; the steps are in `docs/DEVELOPER.md`.

## What was left out on license grounds

- PESQ. The ITU reference is not a permissive license. It is not a dependency and the harness does not compute it. Internal research can add it locally.
- BlackHole and other GPL virtual devices. libASPL is the plugin library instead.
- espeak-ng, which is GPL, for the example speech. The clips are an original formant synthesizer.
- WebRTC's older noise suppressor. It is BSD, and it is turned off because DeepFilterNet3 is the better suppressor, not because of the license.

## Benchmark clips

Ten seconds of synthetic vowels at 48 kHz, mixed with pink noise at 5 dB SNR, plus a separate echo clip. They are generated, not copied out of DNS Challenge or VoiceBank-DEMAND. Those sets are the right next measurement and are too large, and too variously licensed, to commit. `docs/DATASETS.md` has the URLs.
