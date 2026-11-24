; The installer is per-machine so the driver install is already elevated.
; install-driver.ps1 copies the CLI and ONNX Runtime next to HUSH.exe,
; trusts the test certificate, and creates the HUSH endpoints when test
; signing is on. A missing test-signing mode does not roll back the app.

!macro NSIS_HOOK_PREINSTALL
!macroend

!macro NSIS_HOOK_POSTINSTALL
  DetailPrint "Installing the HUSH virtual audio driver"
  StrCpy $1 "$INSTDIR\resources\install-driver.ps1"
  IfFileExists "$1" hush_install_run 0
  StrCpy $1 "$INSTDIR\install-driver.ps1"
  hush_install_run:
  nsExec::ExecToLog 'powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$1"'
  Pop $0
  ${If} $0 != 0
    MessageBox MB_OK|MB_ICONEXCLAMATION "HUSH is installed. The virtual audio driver still needs test signing. Open docs\WINDOWS_TEST.md, run bcdedit /set testsigning on from an elevated PowerShell, reboot, then run hush driver install."
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  StrCpy $1 "$INSTDIR\resources\install-driver.ps1"
  IfFileExists "$1" hush_uninstall_run 0
  StrCpy $1 "$INSTDIR\install-driver.ps1"
  hush_uninstall_run:
  nsExec::ExecToLog 'powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$1" -Uninstall'
  Pop $0
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
!macroend
