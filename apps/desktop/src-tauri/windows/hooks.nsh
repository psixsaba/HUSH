; Tauri NSIS hooks. The virtual audio driver is a separate test-signed package.
; This installer does not load a kernel driver.

!macro NSIS_HOOK_PREINSTALL
!macroend

!macro NSIS_HOOK_POSTINSTALL
  DetailPrint "HUSH is installed for the current user."
  DetailPrint "HUSH Microphone needs the test-signed virtual audio driver."
  DetailPrint "Build and sign it with the WDK. See drivers/windows/README.md."
!macroend

!macro NSIS_HOOK_PREUNINSTALL
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
!macroend
