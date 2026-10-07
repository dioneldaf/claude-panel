# Changelog

All notable changes to this project are documented in this file. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## [0.1.0] - 2026-10-07

First public release.

### Added

- Always-on-top, frameless, translucent panel with one row per open Claude Code
  session: name, project folder, profile, traffic light, state and time in state.
- Six states: working, needs you, done, waiting for subagent, waiting for process,
  paused.
- Session discovery from the local session registry of every `~/.claude*` profile;
  stale entries are dropped by checking the process and its start time.
- Optional Claude Code hooks for fine-grained state, delivered by a small forwarder
  over loopback UDP. Install, removal and status from the panel or the command line,
  with a dry-run preview, a timestamped backup and an atomic write.
- Native notifications when a session needs the user or finishes its turn, debounced,
  with a mute toggle.
- Mini mode (one light per session), tray icon, persisted position, mode and mute flag.
- Pixel-art mascot, icons and font drawn in code; spring-based drag feedback; reduced
  motion support.
- The panel is hidden while no session is open and only the tray icon remains. The
  mascot brings the panel in when the first session opens and carries it away a few
  seconds after the last one closes; both sequences can be interrupted, the mini mode
  has shorter variants, and reduced motion gets a short fade.
- Launch at sign-in, on by default for the installed build (per-user registry entry),
  with a tray checkbox; never re-enabled by an upgrade once turned off.
- Hook entries that point to a missing executable are reported as broken and can be
  repaired or removed from the panel.
- Demo mode with fake sessions, and a demo cycle that closes and reopens all sessions.
- Windows installer (per user, no administrator rights) that removes the hook entries
  and the sign-in entry on uninstall, and a portable archive with checksums.
