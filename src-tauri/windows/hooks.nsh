; Extra installer steps for DesktopAssistant (Tauri NSIS hooks).

; Before files are removed, take this app's hook entries out of Claude Code's settings, the same
; as unticking "Connect to Claude Code". Otherwise every Claude Code event would try to run a
; deleted da-hook.exe. The result is ignored: the uninstall always carries on.
;
; Tauri runs this hook before its own "is the app running?" check, so that check runs here first:
; if the user cancels it, the uninstall stops with the hooks still connected.
!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro CheckIfAppIsRunning "$INSTDIR\${MAINBINARYNAME}.exe" "${PRODUCTNAME}"
  ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --disconnect'
!macroend
