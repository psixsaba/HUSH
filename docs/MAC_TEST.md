# HUSH on a Mac

## One command

```bash
curl -fsSL https://raw.githubusercontent.com/psixsaba/HUSH/main/install.sh | bash
```

`uninstall.sh` in this repository removes `/Applications/HUSH.app` and the HAL plugin.

## Install the disk image

1. Open `HUSH_0.3.0_aarch64.dmg` from the `v0.3.0` pre-release. The app is built for Apple silicon. The driver inside the package is universal.
2. Drag **HUSH.app** to **Applications**.
3. Double-click **HUSH_0.3.0_macos.pkg**. Enter an administrator password. The package installs `HUSH.driver` into `/Library/Audio/Plug-Ins/HAL` and runs `killall coreaudiod`.
4. Open HUSH. Turn the large switch ON. It stays on across restarts when that was the last state. Start on login is a separate switch.
5. In Zoom, Meet, Discord, or Teams select **HUSH Microphone** and **HUSH Speaker**.
6. If the window says the virtual driver is missing, click **Install driver**. macOS asks for an administrator password and copies the plugin that shipped inside the app.

This build is signed ad hoc unless Developer ID secrets are present in CI. It is not notarized without those secrets. On macOS 15 Sequoia, if macOS blocks the app or the plugin:

1. Try to open HUSH.
2. Open **System Settings > Privacy & Security**.
3. Click **Open Anyway**.

`xattr -cr /Applications/HUSH.app` clears the quarantine flag. The one-line installer does that itself.

If the plugin never appears in Sound settings, open System Settings, Privacy & Security, and allow the blocked plugin if macOS offers that button. `coreaudiod` on Apple silicon often refuses an ad-hoc plugin until that click.

To uninstall, double-click **Uninstall HUSH.command** on the disk image, or run `sudo /usr/local/share/hush/uninstall-hush.sh`. Drag HUSH.app to the Trash after that.

## Build from source

These steps are for a MacBook Air with Apple silicon, macOS 15, and the Xcode Command Line Tools. Homebrew is not required. The first script does not use sudo. Installing the audio plugin does.

## Build

From a clean checkout:

```bash
scripts/macos/dev-setup-and-test.sh
```

That installs rustup under `~/.cargo` if `rustc` is missing, downloads CMake, Ninja, and Meson into `~/.local/hush-dev` if they are missing, downloads `dpdfnet8_48khz_hr.onnx` into `~/.cache/dpdfnet/models`, builds the arm64 `HUSH.driver`, runs `cargo test` for the core, the C API, and the CLI, and then `cargo build -p hush-cli --release`. The test command does not leave `target/release/hush` on disk. The build command does.

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

Use the output name `hush devices` printed, if it is not `MacBook Air Speakers`. Leave that process running. It publishes HUSH Microphone and plays HUSH Speaker on the built-in speakers. The first time Terminal (or HUSH.app) opens the microphone, macOS asks for permission under System Settings, Privacy & Security, Microphone. Until that is allowed, the capture side stays silent.

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
