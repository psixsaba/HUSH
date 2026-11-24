# Developer options

The install most people use does not need these steps. macOS uses the HAL plugin. Windows uses VB-CABLE and leaves Secure Boot on. This file is the test-signed HushVad driver and the Developer ID switch.

## Windows test-signed HushVad

HushVad is the virtual microphone and virtual speaker in this repository. Windows will load it only while test signing is on, which requires Secure Boot off. It is not the default installer path.

From an elevated PowerShell, after the app is installed:

```powershell
$env:HUSH_DEV_DRIVER = "1"
# or, from the extracted driver package:
hush driver install --package dist\driver
```

The package contains `VirtualAudioDriver.inf`, `virtualaudiodriver.sys`, `virtualaudiodriver.cat`, and `HushTest.cer`. The test certificate password is `hush-test`. CI generates a new certificate on every package build. It is not an EV certificate.

Turn test signing on, then reboot:

```powershell
bcdedit /set testsigning on
shutdown /r /t 0
```

The desktop watermark says Test Mode. `hush service` sees `\\.\HushVad` and uses that driver instead of VB-CABLE, including HUSH Speaker. Turn it off when you are done:

```powershell
bcdedit /set testsigning off
```

`hush driver uninstall` removes the device node and the login entries for the bridge.

## macOS Developer ID

`.github/workflows/windows-package.yml` runs `scripts/macos/sign-if-secrets.sh`. The step does nothing unless all of these secrets are set:

- `APPLE_CERTIFICATE_P12` (base64)
- `APPLE_CERTIFICATE_PASSWORD`
- `APPLE_SIGNING_IDENTITY`
- `APPLE_ID`
- `APPLE_TEAM_ID`
- `APPLE_APP_PASSWORD`

With them set, the step signs the app and the HAL plugin with Developer ID and submits the app to notarization. Without them, the build stays ad-hoc signed.
