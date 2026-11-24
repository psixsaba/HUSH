# Windows virtual microphone and speaker

The Rust core is the same crate as on macOS. On Windows the ring is a named file mapping, `Local\hush-mic` and `Local\hush-spk`, created by `native/ring/ring.c`. This machine has no Windows Driver Kit, so the kernel driver was not compiled and not test-signed here.

## Which sample to start from

`Virtual-Audio-Driver/` in this directory is a vendored copy of [VirtualDrivers/Virtual-Audio-Driver](https://github.com/VirtualDrivers/Virtual-Audio-Driver) (MIT, copyright MikeTheTech). Microsoft's SysVAD sample (MS-PL) is the reference for a minimal WaveRT endpoint. Do not copy a GPL virtual cable into this tree.

`CMiniportWaveRTStream::WriteBytes` reads the capture queue filled by `HUSH_IOCTL_WRITE_CAPTURE` and converts it to the stream format (48 kHz is one-to-one; other rates are a zero-order hold). `ReadBytes` converts render audio back to 48 kHz mono float and queues it for `HUSH_IOCTL_READ_RENDER`. The control device is `\Device\HushVad`, opened from user mode as `\\.\HushVad`. Its DACL allows the logged-in user, the same trust as the shared-memory ring.

`hush service` is that user-mode helper. It has to run in the interactive session, not as LocalSystem, because the rings are `Local\` mappings. The Tauri app starts the same loop on Windows. It reads HUSH Microphone and writes the virtual mic, and it writes HUSH Speaker from the virtual speaker. The desktop app still owns the neural net: it cleans the mic before the ring, and it cleans the speaker after the ring.

CI runs `drivers/windows/ci-build.ps1` on `windows-latest`. The script tries Chocolatey `windowsdriverkit11`, then winget. If the kit is still absent it exits 0. If the kit is present, `msbuild` of `VirtualAudioDriver.sln` (Release, x64, `SignMode=Off`) must succeed. This Linux tree cannot compile the driver.

The helper is the only process that calls `DeviceIoControl`. The app publishes the rings. `ioctl.h` is the contract shared with `crates/hush-core/src/winbridge.rs`: device type `0x8000`, `WRITE_CAPTURE` function `0x800`, `READ_RENDER` function `0x801`, `METHOD_BUFFERED`, 480 mono floats at 48 kHz.

The driver sources are vendored. `fetch.ps1` is the record of the upstream commit, not a step you need to run again.

## Test signing

On a Windows machine with the WDK and Visual Studio installed:

```powershell
# From an elevated prompt. This reboots.
bcdedit /set testsigning on

# Build the driver, then sign with a test certificate.
makecert -r -pe -ss PrivateCertStore -n "CN=HUSH Test" HushTest.cer
# Or, with the WDK tools:
New-SelfSignedCertificate -Type CodeSigningCert -Subject "CN=HUSH Test" -CertStoreLocation Cert:\CurrentUser\My
signtool sign /fd SHA256 /a /f HushTest.pfx /p <password> HushVirtualAudio.sys
```

Install the test certificate into `Trusted Root` and `Trusted Publishers` on that machine. `pnputil /add-driver VirtualAudioDriver.inf /install` loads the package only while test signing is on. Then start HUSH, or run `hush service` in the same session, and point the call app at HUSH Microphone and HUSH Speaker. Turn test signing off when you are done: `bcdedit /set testsigning off`.

Secure Boot must be off for test signing, or the firmware will reject the test certificate. That is a development machine setting, not a shipping setting.

## Production signing

Shipping a kernel audio driver to other people's PCs requires:

1. An EV code-signing certificate from a CA that Microsoft accepts for kernel code.
2. A Partner Center hardware account.
3. A signature from Microsoft, not only from the EV certificate. Windows 10 and 11 refuse a kernel driver that is signed only by the vendor.

Two ways to get that Microsoft signature:

- **Attestation signing** (Partner Center). You upload the EV-signed cabinet. Microsoft signs it without an HLK run. This is the usual path for a virtual audio driver that does not match an HLK device category cleanly. Attestation-signed drivers load, but they do not get the same WHQL logo, and Microsoft can require an HLK pass later.
- **HLK / WHQL**. You run the Windows Hardware Lab Kit audio tests on the virtual endpoints and submit the logs. This is the logo path. A virtual device often needs a custom HLK playlist because the tests expect a jack, a volume knob, or a hardware mute. Budget time for those failures. They are test failures, not audio failures.

Attestation is the right first shipping signature. HLK is the one to add before a logo or before a customer requires WHQL. Neither was done here: there is no EV certificate and no Partner Center account in this environment.

The user-mode app (the Tauri shell and `hush.exe`) is a normal Authenticode signature, not a kernel signature. An EV certificate is still what SmartScreen treats as a known publisher. An ordinary OV certificate works for Authenticode but produces more SmartScreen warnings.
