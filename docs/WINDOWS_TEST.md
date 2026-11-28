# HUSH on Windows 10 or 11 (x64)

Secure Boot stays on. You do not turn on Test Mode.

Run `HUSH_0.3.1_x64-setup.exe` from the `v0.3.1` release (the MSI installs the same app). The installer downloads VB-CABLE Pack45 from VB-Audio, checks SHA-256 `b950e39f01af1d04ea623c8f6d8eb9b6ea5c477c637295fabf20631c85116bfb`, and runs `VBCABLE_Setup_x64.exe -i -h`. If those flags do not finish, it opens VB-Audio's setup window. Reboot once if HUSH Microphone does not appear. The uninstaller removes HUSH and asks before it removes VB-CABLE.

VB-CABLE is donationware and is not included in the installer. Credit and a donate link are in the installer window and in the app: https://vb-audio.com/Cable/ and https://vb-audio.com/Services/donate.htm. The cable is shared with OBS and similar apps. Only one program should play into CABLE Input.

In Zoom, Meet, Discord, or Teams, select **HUSH Microphone**. Incoming-voice cleaning is off in this mode because Windows has no free virtual speaker. The test-signed HushVad driver, which does include HUSH Speaker, is in [DEVELOPER.md](DEVELOPER.md).

## მოკლედ

1. Secure Boot ჩართული დატოვე. Test Mode არ გჭირდება.
2. გაუშვი `HUSH_0.3.1_x64-setup.exe`. ინსტალერი VB-CABLE-ს ჩამოტვირთავს და SHA-256-ს შეამოწმებს.
3. თუ მიკროფონი არ გამოჩნდა, ერთხელ გადატვირთე.
4. გახსენი HUSH და ჩართე დიდი ON/OFF.
5. Zoom, Meet, Discord ან Teams-ში აირჩიე **HUSH Microphone**.

Test Mode და HushVad დეველოპერის გზაა. ის `docs/DEVELOPER.md`-შია.

## Developer note: test signing

The steps below are the old HushVad bring-up. Skip them for the VB-CABLE install. The same material is in [DEVELOPER.md](DEVELOPER.md).

## 1. Turn on test signing

Secure Boot must be off. In Windows, **Settings, Privacy & security, Device security, Secure Boot** should say Off. If it says On, turn it off in the UEFI firmware and boot Windows again. BitLocker may ask for the recovery key when Secure Boot changes. Save that key before you start.

Open **PowerShell as administrator** and run:

```powershell
bcdedit /set testsigning on
shutdown /r /t 0
```

After reboot the desktop watermark says **Test Mode**.

Turn it back off when you are done with this driver:

```powershell
bcdedit /set testsigning off
```

That also needs a reboot. The HUSH driver will not load after that.

## 2. Install

Run the NSIS setup from the release. It asks for administrator approval. It does four things:

- Installs HUSH under `C:\Program Files\HUSH`
- Copies `onnxruntime.dll` next to `HUSH.exe` and `hush.exe`
- Trusts `HushTest.cer` in **Trusted Root** and **Trusted Publishers**, then loads the driver with `pnputil` and SetupAPI
- Registers two login entries: the tray app, and `hush.exe service` (the user-session bridge)

`hush service` is not a Windows service in session 0. The shared-memory rings are `Local\` names, and a session 0 process cannot open them. Both login entries run in the interactive user session. One of them owns the bridge. The other sees that and stays quiet.

If the setup says the driver was not loaded, test signing was still off. Finish step 1, reboot, then in an elevated PowerShell:

```powershell
& "C:\Program Files\HUSH\hush.exe" driver install --package "C:\Program Files\HUSH\resources\driver"
```

If the driver files were nested one folder deeper, pass the directory that contains `VirtualAudioDriver.inf`.

## 3. Confirm the endpoints

**Settings, System, Sound.** Input and output should both list HUSH:

- **HUSH Microphone**
- **HUSH Speaker**

The same names appear in Zoom, Meet, Discord, and Teams. Then:

```powershell
& "C:\Program Files\HUSH\hush.exe" selftest
```

Quit HUSH from the tray first so the self-test owns the bridge. A pass prints all of these and exits 0:

- `ring: wrote samples on a private segment and read them back`
- a `frames:` line with p50 and p95 milliseconds and the percent of a 10 ms frame
- `devices: Windows lists HUSH Microphone and HUSH Speaker`
- `driver: opened \\.\HushVad`
- `capture:` and `render:` lines with a non-silent rms

The p95 frame time should be under 10 ms. DPDFNet8 is the timer when `dpdfnet8_48khz_hr.onnx` is in the cache. The installer downloads it to `C:\ProgramData\HUSH\models` when it can, otherwise to `%USERPROFILE%\.cache\dpdfnet\models`. `hush models fetch` retries that download. Without the file, self-test times RNNoise and says so.

## 4. Hear a call

Leave HUSH running (the tray icon is enough). In the HUSH window, **Microphone** is your real mic and **Play the call on** is your real speakers or headphones. Do not pick HUSH Microphone or HUSH Speaker for those two. Those are what the call app uses.

### Zoom

1. Zoom, Settings, Audio.
2. Speaker: **HUSH Speaker**.
3. Microphone: **HUSH Microphone**.
4. Turn off **Automatically adjust microphone volume**.
5. **Test speaker**. The tone comes out of the device you picked in HUSH, a little later than the built-in output. That delay is the cleaner.
6. **Test mic**. Speak. The meter moves. The other side of a call should hear you without the room.

### Google Meet

1. Join a meeting.
2. Three dots, **Settings**, **Audio**.
3. Microphone: **HUSH Microphone**. Speaker: **HUSH Speaker**.
4. You hear the other person from your real speakers. They hear the cleaned mic.

### Discord

1. User Settings, Voice & Video.
2. Input Device: **HUSH Microphone**. Output Device: **HUSH Speaker**.
3. Turn off Discord's own noise suppression so it does not run a second denoiser on an already cleaned mic.
4. **Let's Check** plays the test tone through HUSH Speaker.

### Microsoft Teams

1. Settings, Devices.
2. Speaker: **HUSH Speaker**. Microphone: **HUSH Microphone**.
3. **Make a test call**. You hear yourself from the real speakers. The test call should not be full of room noise.

## Uninstall

**Settings, Apps, HUSH, Uninstall.** The uninstaller removes the device node, the `oem*.inf` package, and the two login entries. Test signing stays on until you turn it off with `bcdedit`.

From an elevated PowerShell, without removing the app:

```powershell
& "C:\Program Files\HUSH\hush.exe" driver uninstall
```

## What this CI run proved

GitHub Actions compiled the driver with the WDK, test-signed the `.sys` and the catalog, ran `cargo test` for the core, the C API, and the CLI on Windows, and built the NSIS and MSI installers. The runner cannot reboot into test signing, so it did not load `VirtualAudioDriver.sys` and did not open the endpoints. `hush selftest` on the runner is expected to pass the ring check and then report that the HUSH endpoints are missing.

A real Windows PC still has to prove:

- Sound settings lists both names after the reboot in Test Mode
- `hush selftest` prints the capture and render lines and exits 0
- Zoom, Meet, Discord, or Teams plays through HUSH Speaker and transmits HUSH Microphone
- The frame p95 stays under 10 ms on that CPU

## Production signing

Shipping this driver to other people's PCs needs a signature from Microsoft. A vendor signature alone is not enough on Windows 10 or 11.

1. An EV code-signing certificate from a CA that Microsoft accepts for kernel code.
2. A Partner Center hardware account.
3. Sign the cabinet with the EV certificate and submit it.

Two Microsoft signatures:

- **Attestation signing** in Partner Center. Microsoft signs the cabinet without an HLK run. This is the right first signature for a virtual audio device. Attestation-signed drivers load. They do not get the WHQL logo, and Microsoft can ask for an HLK pass later.
- **HLK / WHQL**. Run the Windows Hardware Lab Kit audio tests and submit the logs. A virtual device often fails tests that expect a jack, a volume knob, or a hardware mute. Those failures are the playlist, not the audio path. Budget a custom HLK playlist before a logo or a customer that requires WHQL.

`HUSH.exe` and `hush.exe` are user-mode binaries. They take a normal Authenticode signature. An EV certificate is what SmartScreen treats as a known publisher. An OV certificate signs the files and still shows more SmartScreen warnings.

The test certificate in the release (`HushTest.cer`, and `HushTest.pfx` inside the driver zip) is not that certificate. The pfx password is `hush-test`. It only exists so a development machine can load this build.
