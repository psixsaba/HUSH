# Windows virtual microphone and speaker

The Rust core is the same crate as on macOS. On Windows the ring is a named file mapping, `Local\hush-krisp-alt-mic` and `Local\hush-krisp-alt-spk`, created by `native/ring/ring.c`. This machine has no Windows Driver Kit, so the kernel driver was not compiled and not test-signed here.

## Which sample to start from

Use [VirtualDrivers/Virtual-Audio-Driver](https://github.com/VirtualDrivers/Virtual-Audio-Driver). It is MIT and MS-PL, and it is already a virtual cable with a user-mode service. Microsoft's SysVAD sample (MS-PL, in the Windows driver samples) is the reference for a minimal WaveRT endpoint if you would rather begin from the official sample. Do not copy a GPL virtual cable into this tree.

The integration point is the driver's cyclic buffer:

1. The capture endpoint (what Zoom opens as a microphone) should read mono 48 kHz float from `Local\hush-krisp-alt-mic`, convert to the endpoint's format, and duplicate to stereo. That is the same read the macOS plugin does with `hush_ring_read`.
2. The render endpoint (what Zoom opens as a speaker) should average each stereo frame to mono and `hush_ring_write` it into `Local\hush-krisp-alt-spk`.
3. A kernel driver in session 0 cannot always open a `Local\` mapping created by the logged-in user. If `OpenFileMapping` fails from the driver, switch the contract to an IOCTL pair and keep the ring in user mode:
   - `HUSH_IOCTL_READ_MIC` returns the next captured frames to the app? No: the app is the producer of the mic. The service writes mic frames into the driver with `HUSH_IOCTL_WRITE_CAPTURE`, and reads speaker frames with `HUSH_IOCTL_READ_RENDER`.
   - Definitions live in `ioctl.h` in this directory.

`fetch.ps1` clones the Virtual Audio Driver at a pinned commit so the build does not float. Apply the buffer change in that tree. Do not vendor its full source into this repository until the patch builds, so a WDK update does not fork an unread driver.

## Test signing

On a Windows machine with the WDK and Visual Studio installed:

```powershell
# From an elevated prompt. This reboots.
bcdedit /set testsigning on

# Build the driver, then sign with a test certificate.
makecert -r -pe -ss PrivateCertStore -n "CN=Hush Test" HushTest.cer
# Or, with the WDK tools:
New-SelfSignedCertificate -Type CodeSigningCert -Subject "CN=Hush Test" -CertStoreLocation Cert:\CurrentUser\My
signtool sign /fd SHA256 /a /f HushTest.pfx /p <password> HushVirtualAudio.sys
```

Install the test certificate into `Trusted Root` and `Trusted Publishers` on that machine. `devcon install` or `pnputil /add-driver HushVirtualAudio.inf /install` loads the package only while test signing is on. Turn it off when you are done: `bcdedit /set testsigning off`.

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
