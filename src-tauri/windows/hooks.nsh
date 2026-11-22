; Extra installer steps for DesktopAssistant (Tauri NSIS hooks).

; Before files are removed, take this app's hook entries out of Claude Code's settings, the same
; as unticking "Connect to Claude Code". Otherwise every Claude Code event would try to run a
; deleted da-hook.exe. The result is ignored: the uninstall always carries on.
;
; Upgrades also run this uninstaller (Tauri's installer removes the old version first), but
; then the new da-hook.exe lands in the same folder, so the hooks must stay. An upgrade runs
; uninstall.exe in place, from the install folder ("_?="). A real uninstall, from Settings or by
; running uninstall.exe, runs a copy NSIS makes in a temp folder. So only disconnect when this
; uninstaller is not running from the install folder.
;
; Tauri runs this hook before its own "is the app running?" check, so that check runs here first:
; if the user cancels it, the uninstall stops with the hooks still connected.
!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro CheckIfAppIsRunning "$INSTDIR\${MAINBINARYNAME}.exe" "${PRODUCTNAME}"
  ${If} $EXEDIR != $INSTDIR
    ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --disconnect'
  ${EndIf}
!macroend
