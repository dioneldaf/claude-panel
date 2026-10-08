//! Command-line interface of the application binary.

use crate::hooks::{self, Action};
use std::path::PathBuf;

#[derive(Debug, PartialEq, Default)]
pub struct Options {
    pub demo: bool,
    /// Demo that periodically drops to zero sessions (implies `demo`).
    pub demo_cycle: bool,
    /// Started by Windows at sign-in rather than by the user.
    pub autostart: bool,
    pub hooks: Option<HooksCommand>,
    pub dry_run: bool,
    /// With the uninstall clean-up: file that receives the paths needing manual attention.
    pub report: Option<PathBuf>,
    /// Explicit config directories; empty means auto-discovery.
    pub config_dirs: Vec<PathBuf>,
    pub help: bool,
    /// Hidden: play both notification cues once and exit (sound preview).
    pub play_sounds: bool,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum HooksCommand {
    Install,
    Remove,
    Status,
    /// Run by the uninstaller: remove this installation's entries everywhere.
    UninstallCleanup,
}

/// Exit code of the uninstall clean-up when some file could not be cleaned.
pub const EXIT_LEFTOVERS: i32 = 3;

const USAGE: &str = "
Usage: claude-panel [options]

  (no options)          Start the panel.
  --demo                Start with fake sessions cycling through every state.
  --demo-cycle          Like --demo, but all sessions close every few seconds, to
                        show the exit and entrance animations.
  --autostart           Used by the sign-in entry: start silently.
  --hooks-status        Print the hook installation state of each profile.
  --install-hooks       Add the panel hooks to each profile's settings.json.
  --remove-hooks        Remove the panel hooks from each profile's settings.json.
  --dry-run             With --install-hooks / --remove-hooks: print the changes only.
  --config-dir <dir>    Limit the action to this config directory (repeatable).
  --help                Print this text.

A timestamped backup is written next to every settings.json before it is modified.
The environment variable CPANEL_DEMO=1 is equivalent to --demo.
";

pub fn usage() -> String {
    format!("{}\n{USAGE}", cpanel_core::PRODUCT_NAME)
}

pub fn parse(args: impl IntoIterator<Item = String>, demo_env: Option<&str>) -> Options {
    let mut options = Options { demo: matches!(demo_env, Some("1" | "true")), ..Default::default() };
    let mut it = args.into_iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--demo" => options.demo = true,
            "--demo-cycle" => {
                options.demo = true;
                options.demo_cycle = true;
            }
            "--autostart" => options.autostart = true,
            "--install-hooks" => options.hooks = Some(HooksCommand::Install),
            "--remove-hooks" => options.hooks = Some(HooksCommand::Remove),
            "--hooks-status" => options.hooks = Some(HooksCommand::Status),
            "--uninstall-cleanup" => options.hooks = Some(HooksCommand::UninstallCleanup),
            "--report" => options.report = it.next().map(PathBuf::from),
            "--dry-run" => options.dry_run = true,
            "--config-dir" => options.config_dirs.extend(it.next().map(PathBuf::from)),
            "--help" | "-h" => options.help = true,
            "--play-sounds" => options.play_sounds = true,
            _ => {}
        }
    }
    options
}

/// Runs a hooks command and returns the process exit code.
pub fn run_hooks(command: HooksCommand, options: &Options) -> i32 {
    let dirs = hooks::config_dirs(&options.config_dirs);
    let exe = hooks::hook_exe();
    let state = hooks::state_path();
    if command == HooksCommand::UninstallCleanup {
        return run_uninstall_cleanup(&dirs, options);
    }
    if dirs.is_empty() {
        println!("No Claude Code config directory found.");
        return 1;
    }
    let action = match command {
        HooksCommand::Status => {
            for profile in hooks::profile_views(&dirs, exe.as_deref()) {
                println!("{:<16} {:<14} {}", profile.tag, profile.hooks, profile.dir);
            }
            println!("hook executable: {}", exe.as_deref().unwrap_or("not found next to the application"));
            return 0;
        }
        HooksCommand::Install => Action::Install,
        HooksCommand::Remove => Action::Remove,
        HooksCommand::UninstallCleanup => unreachable!("handled above"),
    };
    if options.dry_run {
        print!("{}", hooks::preview(action, &dirs, exe.as_deref()));
        return 0;
    }
    let mut code = 0;
    for result in hooks::apply_recorded(action, &dirs, exe.as_deref(), state.as_deref()) {
        if !result.shared_with.is_empty() {
            println!(
                "{}: shared by profiles {}, {}; handled once with hook tag \"{}\"",
                result.file,
                result.tag,
                result.shared_with.join(", "),
                result.tag
            );
        }
        match (&result.error, result.changed) {
            (Some(error), _) => {
                code = 1;
                println!("{}: FAILED: {error}", result.file);
            }
            (None, true) => println!(
                "{}: updated (backup: {})",
                result.file,
                result.backup.as_deref().unwrap_or("none, file was created")
            ),
            (None, false) => println!("{}: already up to date", result.file),
        }
    }
    code
}

/// The uninstaller's clean-up. Exit code 0 means nothing of this installation is
/// left in any settings file; [`EXIT_LEFTOVERS`] means the report lists files that
/// need manual attention. Always appends to the log in the application data folder.
fn run_uninstall_cleanup(dirs: &[cpanel_core::registry::ConfigDir], options: &Options) -> i32 {
    let Some(my_exe) = std::env::current_exe().ok().and_then(|exe| hooks::expected_hook_exe(&exe)) else {
        return EXIT_LEFTOVERS;
    };
    let report = hooks::uninstall_cleanup(dirs, hooks::state_path().as_deref(), &my_exe);

    let mut log = format!("uninstall clean-up for {my_exe}\n");
    for file in &report.cleaned {
        log.push_str(&format!("  removed hook entries: {file}\n"));
    }
    for leftover in &report.leftovers {
        log.push_str(&format!("  NOT CLEANED: {} ({})\n", leftover.file, leftover.reason));
    }
    if report.leftovers.is_empty() {
        log.push_str("  no hook entries of this installation remain\n");
    } else {
        log.push_str("  remove the entries whose command ends in cpanel-hook.exe from the files above (README, section Uninstall)\n");
    }
    print!("{log}");
    if let Some(path) = hooks::uninstall_log_path() {
        cpanel_core::eventlog::EventLog::new(path, 256 * 1024).append_block(&log);
    }
    if let Some(path) = &options.report {
        let _ = std::fs::write(path, hooks::leftover_report(&report.leftovers, 500));
    }
    if report.leftovers.is_empty() {
        0
    } else {
        EXIT_LEFTOVERS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_list(list: &[&str], env: Option<&str>) -> Options {
        parse(list.iter().map(|s| s.to_string()), env)
    }

    #[test]
    fn usage_starts_with_the_product_name_and_lists_the_hook_actions() {
        let text = usage();
        assert!(text.starts_with(cpanel_core::PRODUCT_NAME));
        for flag in ["--demo", "--install-hooks", "--remove-hooks", "--hooks-status", "--dry-run", "--config-dir"] {
            assert!(text.contains(flag), "{flag}");
        }
    }

    #[test]
    fn defaults_to_starting_the_panel() {
        assert_eq!(parse_list(&[], None), Options::default());
    }

    #[test]
    fn demo_comes_from_flag_or_environment() {
        assert!(parse_list(&["--demo"], None).demo);
        assert!(parse_list(&[], Some("1")).demo);
        assert!(!parse_list(&[], Some("0")).demo);
    }

    #[test]
    fn demo_cycle_implies_demo_and_autostart_is_recognised() {
        let o = parse_list(&["--demo-cycle"], None);
        assert!(o.demo && o.demo_cycle && !o.autostart);
        let o = parse_list(&["--autostart"], None);
        assert!(o.autostart && !o.demo && !o.demo_cycle);
        assert!(!parse_list(&["--demo"], None).demo_cycle);
    }

    #[test]
    fn parses_the_uninstall_cleanup_with_its_report_file() {
        let o = parse_list(&["--uninstall-cleanup", "--report", "C:/tmp/leftovers.txt"], None);
        assert_eq!(o.hooks, Some(HooksCommand::UninstallCleanup));
        assert_eq!(o.report, Some(PathBuf::from("C:/tmp/leftovers.txt")));
        assert_eq!(parse_list(&["--report"], None).report, None);
    }

    #[test]
    fn parses_hook_commands_with_targets() {
        let o = parse_list(&["--install-hooks", "--dry-run", "--config-dir", "C:/a", "--config-dir", "C:/b"], None);
        assert_eq!(o.hooks, Some(HooksCommand::Install));
        assert!(o.dry_run);
        assert_eq!(o.config_dirs, vec![PathBuf::from("C:/a"), PathBuf::from("C:/b")]);
        assert_eq!(parse_list(&["--remove-hooks"], None).hooks, Some(HooksCommand::Remove));
        assert_eq!(parse_list(&["--hooks-status"], None).hooks, Some(HooksCommand::Status));
    }

    #[test]
    fn play_sounds_is_a_hidden_preview_flag() {
        let o = parse_list(&["--play-sounds"], None);
        assert!(o.play_sounds && !o.demo && o.hooks.is_none());
        assert!(!parse_list(&[], None).play_sounds);
        assert!(!usage().contains("--play-sounds"));
    }

    #[test]
    fn ignores_unknown_arguments_and_a_dangling_config_dir() {
        let o = parse_list(&["--unknown", "--config-dir"], None);
        assert_eq!(o, Options::default());
    }
}
