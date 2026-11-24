; The default install downloads VB-CABLE. It does not turn on Test Mode.
; HushVad is installed only when HUSH_DEV_DRIVER=1.

!macro NSIS_HOOK_PREINSTALL
!macroend

!macro NSIS_HOOK_POSTINSTALL
  DetailPrint "Installing VB-CABLE for HUSH Microphone"
  StrCpy $1 "$INSTDIR\resources\install-driver.ps1"
  IfFileExists "$1" hush_install_run 0
  StrCpy $1 "$INSTDIR\install-driver.ps1"
  hush_install_run:
  nsExec::ExecToLog 'powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$1"'
  Pop $0
  ${If} $0 == 10
    MessageBox MB_OK|MB_ICONINFORMATION "HUSH is installed. Reboot once so VB-CABLE can finish. Secure Boot can stay on. Test Mode is not required.$\r$\n$\r$\nVB-CABLE is donationware by VB-Audio.$\r$\nhttps://vb-audio.com/Cable/$\r$\nDonate: https://vb-audio.com/Services/donate.htm"
  ${Else}
    MessageBox MB_OK|MB_ICONINFORMATION "HUSH uses VB-CABLE by VB-Audio (donationware). In Zoom, Meet, Discord, or Teams select HUSH Microphone.$\r$\nhttps://vb-audio.com/Cable/$\r$\nDonate: https://vb-audio.com/Services/donate.htm"
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  StrCpy $1 "$INSTDIR\resources\install-driver.ps1"
  IfFileExists "$1" hush_uninstall_ask 0
  StrCpy $1 "$INSTDIR\install-driver.ps1"
  hush_uninstall_ask:
  MessageBox MB_YESNO "Also remove VB-CABLE? OBS and other apps may be using that cable." IDYES hush_uninstall_cable
  nsExec::ExecToLog 'powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$1" -Uninstall'
  Pop $0
  Goto hush_uninstall_done
  hush_uninstall_cable:
  nsExec::ExecToLog 'powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$1" -Uninstall -RemoveCable'
  Pop $0
  hush_uninstall_done:
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
!macroend
