; Installer hooks for the NSIS bundle (see bundle.windows.nsis.installerHooks).
;
; Goal: an uninstall never leaves hook entries pointing at a deleted executable
; (Claude Code would report an error for every event in every session), while an
; upgrade or a cancelled uninstall never costs the user their hooks.
;
; How the uninstaller is reached, and what happens in each case:
;
;   Real uninstall (Settings > Apps, or uninstall.exe): Windows starts the
;     uninstaller, which copies itself to a temporary folder and runs from there,
;     so $EXEDIR differs from $INSTDIR. PREUNINSTALL removes this installation's
;     entries and records them in hooks-state.json; POSTUNINSTALL, reached only
;     when the files were really deleted, removes that record. Nothing is left.
;
;   Cancelled uninstall (the "application is running" prompt comes after
;     PREUNINSTALL): the entries are already removed, but the record survives
;     because POSTUNINSTALL never runs. The application restores them on its
;     next start.
;
;   Manual upgrade or reinstall with "uninstall before installing": the new
;     installer runs the old uninstaller in place ("uninstall.exe _?=<dir>",
;     without /UPDATE), so $EXEDIR equals $INSTDIR. The entries are removed like
;     in a real uninstall, but POSTUNINSTALL keeps the record, and the new
;     version restores them on its first start.
;
;   Silent (/S) or passive upgrade, and "do not uninstall": the old uninstaller
;     is not run at all; files are replaced in place and the entries stay valid.
;
; Only entries that point at this installation's cpanel-hook.exe, or at an
; executable that no longer exists, are removed. Another copy of the application
; (portable folder, development build) keeps its own entries.
;
; The launch-at-sign-in value under the per-user Run key is deleted by the
; bundler's own uninstall section; the application writes it again on start
; while the stored preference is on.

!macro NSIS_HOOK_PREUNINSTALL
  Push $0
  Push $1
  Push $2
  Push $3
  InitPluginsDir
  Delete "$PLUGINSDIR\hook-leftovers.txt"
  DetailPrint "Removing hook entries from Claude Code settings..."
  ; The timeout guarantees the uninstaller cannot hang on the child process.
  nsExec::ExecToLog /TIMEOUT=20000 '"$INSTDIR\${MAINBINARYNAME}.exe" --uninstall-cleanup --report "$PLUGINSDIR\hook-leftovers.txt"'
  Pop $0
  DetailPrint "Hook clean-up finished with code $0."
  ${If} $0 != "0"
    ; Collect the files that still need manual cleaning (written by the application).
    StrCpy $1 ""
    ${If} ${FileExists} "$PLUGINSDIR\hook-leftovers.txt"
      ClearErrors
      FileOpen $2 "$PLUGINSDIR\hook-leftovers.txt" r
      ${IfNot} ${Errors}
        ${Do}
          ClearErrors
          FileRead $2 $3
          ${If} ${Errors}
            ${ExitDo}
          ${EndIf}
          StrCpy $1 "$1$3"
        ${Loop}
        FileClose $2
      ${EndIf}
    ${EndIf}
    ${If} $1 == ""
      StrCpy $1 "(the clean-up did not finish; check every settings.json under your .claude folders)$\r$\n"
    ${EndIf}
    ; Not shown in silent mode (/SD); the application has written the same
    ; information to logs\uninstall-cleanup.log in its local data folder.
    MessageBox MB_OK|MB_ICONEXCLAMATION "${PRODUCTNAME} could not remove its hook entries from:$\r$\n$\r$\n$1$\r$\nUntil they are removed, Claude Code reports a hook error on every event. Delete the entries whose command ends in cpanel-hook.exe from these files, or restore a settings.json.cpanel-backup file. See the Uninstall section of the README:$\r$\n${CPANEL_README_URL}" /SD IDOK
  ${EndIf}
  Pop $3
  Pop $2
  Pop $1
  Pop $0
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ; Reached only after the files were deleted. A standalone uninstall runs from a
  ; temporary copy; an uninstall started by a newer installer runs in place.
  ${If} $EXEDIR != $INSTDIR
    SetShellVarContext current
    Delete "$APPDATA\${BUNDLEID}\hooks-state.json"
  ${EndIf}
!macroend

!define CPANEL_README_URL "https://github.com/dioneldaf/claude-panel#uninstall"
