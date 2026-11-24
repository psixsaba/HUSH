# HUSH on a Mac

These steps are for a MacBook Air with Apple silicon, macOS 15, and the Xcode Command Line Tools. Homebrew is not required. The first script does not use sudo. Installing the audio plugin does.

## Build

From a clean checkout:

```bash
scripts/macos/dev-setup-and-test.sh
```

That installs rustup under `~/.cargo` if `rustc` is missing, downloads CMake, Ninja, and Meson into `~/.local/hush-dev` if they are missing, downloads `dpdfnet8_48khz_hr.onnx` into `~/.cache/dpdfnet/models`, builds the arm64 `HUSH.driver`, and runs `cargo test` for the core, the C API, and the CLI.

ONNX Runtime itself is downloaded by the Rust build. You do not install it separately.

## Install the plugin

```bash
sudo scripts/macos/install-driver.sh
```

This copies `drivers/macos/build/HUSH.driver` to `/Library/Audio/Plug-Ins/HAL/HUSH.driver` and restarts `coreaudiod`. Then:

```bash
./target/release/hush selftest
```

`selftest` must print all three of these before you open a call:

- `ring: wrote samples on /hush-mic and read them back`
- a `frames:` line with p50 and p95 milliseconds and the percent of a 10 ms frame
- `devices: coreaudiod lists HUSH Microphone and HUSH Speaker`

If the device line fails, macOS did not load the plugin. On Apple silicon, `coreaudiod` often refuses an ad-hoc signature. Open System Settings, Privacy & Security, and allow the blocked plugin if macOS offers that button, then run the install command again. A Developer ID signature is what a plugin needs before it will load for someone else. This checkout signs with `codesign -` unless `CODESIGN_ID` is set.

`system_profiler SPAudioDataType` should show the same two names.

## Hear yourself

```bash
./target/release/hush devices
./target/release/hush run --bridge --aec --output "MacBook Air Speakers"
```

Use the output name `hush devices` printed, if it is not `MacBook Air Speakers`. Leave that process running. It publishes HUSH Microphone and plays HUSH Speaker on the built-in speakers.

## Zoom

1. Zoom, Settings, Audio.
2. Speaker: **HUSH Speaker**.
3. Microphone: **HUSH Microphone**.
4. Turn off **Automatically adjust microphone volume**.
5. Click **Test speaker**. You should hear the test tone from the MacBook speakers, a little later than with the built-in output. That delay is the cleaner.
6. Click **Test mic**. Speak. The level meter should move. The other side of a call should hear you without the room noise you can hear in the room.
7. Mute in HUSH is not Zoom's mute. Stop the test, or press mute in the HUSH window if you are using the app. The CLI stops with Control-C.

## Google Meet

1. Start or join a meeting.
2. Click the three dots in the meeting controls, then **Settings**, then **Audio**.
3. Microphone: **HUSH Microphone**.
4. Speaker: **HUSH Speaker**.
5. Speak and watch the microphone meter. The other participant should hear the cleaned voice. You should hear them from the MacBook speakers.
6. If Meet offers a device test before you join, pick the same two devices there.

## What a pass looks like

- `hush selftest` exits 0 and the p95 frame time is under 10 ms.
- Zoom's speaker test is audible on the built-in speakers.
- A second person hears your voice and less of the room.
- You hear the second person.

## What this machine cannot prove

The plugin was not loaded into `coreaudiod` where this tree was written. The first real proof is `hush selftest` on the Mac after the sudo install.
