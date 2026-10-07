; Installer hooks for the NSIS bundle (see bundle.windows.nsis.installerHooks).
;
; Uninstalling must never leave hook entries behind in the user's Claude Code
; settings: an entry pointing at a deleted executable makes Claude Code report an
; error for every event in every session. Before any file is deleted, the
; application's own "remove hooks" action is run for every profile. It is a strict
; no-op (no write, no backup) for profiles that have no entries.
;
; The launch-at-sign-in value under the per-user Run key is removed as well.
;
; An upgrade also runs the previous uninstaller (in update mode). The entries must
; survive that, because the new version is installed to the same path.

!macro NSIS_HOOK_PREUNINSTALL
  ${If} $UpdateMode <> 1
    DetailPrint "Removing hook entries from Claude Code settings..."
    nsExec::ExecToLog '"$INSTDIR\${MAINBINARYNAME}.exe" --remove-hooks'
    Pop $0
    DetailPrint "Hook removal finished with code $0."
    ; Launch-at-sign-in entry written by the application (per user).
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "${PRODUCTNAME}"
  ${EndIf}
!macroend
