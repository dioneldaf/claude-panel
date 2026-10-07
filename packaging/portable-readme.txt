Claude Panel (portable)
=======================

An always-on-top widget that shows the live state of every open Claude Code
session. Unofficial community project; not affiliated with or endorsed by
Anthropic.

Run
---
Extract the whole archive to a folder of your choice and start
claude-panel.exe. Keep cpanel-hook.exe in the same folder. Nothing is
installed; settings are stored under %APPDATA% and %LOCALAPPDATA% in a folder
named io.github.dioneldaf.claude-panel.

Windows may show a SmartScreen warning because the executables are not
code-signed. Verify the SHA256SUMS file published with the release before
choosing "More info" and "Run anyway".

Hooks (optional)
----------------
The plug button in the panel installs hooks into your Claude Code settings so
the panel can show permission prompts, subagents and the exact end of a turn.
A backup of each settings.json is written first.

IMPORTANT: the hook entries contain the full path of cpanel-hook.exe.
Before deleting or moving this folder, remove them with the plug button or:

    claude-panel.exe --remove-hooks

Also clear "Launch at sign-in" in the tray menu if you ticked it.

If the folder was already deleted, open each settings.json under
%USERPROFILE%\.claude* and delete the hook entries whose command ends in
cpanel-hook.exe, or restore one of the settings.json.cpanel-backup-* files.

Documentation, source code and license: see the project page on GitHub.
