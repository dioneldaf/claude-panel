//! Claude Code hook that forwards a compact event summary to Claude Panel.
//!
//! Contract with Claude Code: always exit 0, print nothing, return in milliseconds.
//! The summary is sent as one UDP datagram to the loopback interface, which never
//! blocks and is harmless when the panel is not running.

// No console window may ever flash when Claude Code spawns the hook.
#![cfg_attr(windows, windows_subsystem = "windows")]

use cpanel_core::hook_event::{encode, summarize_value};
use std::io::Read;
use std::net::UdpSocket;
use std::sync::mpsc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Upper bound for waiting on stdin; the payload normally arrives at once.
const STDIN_TIMEOUT: Duration = Duration::from_millis(1500);
/// Payloads can embed tool output; read at most this much.
const MAX_STDIN_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Debug, PartialEq, Default)]
struct Args {
    profile: String,
    port: Option<u16>,
}

fn parse_args(args: impl IntoIterator<Item = String>) -> Args {
    let mut parsed = Args::default();
    let mut it = args.into_iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--profile" => parsed.profile = it.next().unwrap_or_default(),
            "--port" => parsed.port = it.next().and_then(|p| p.parse().ok()).filter(|p| *p != 0),
            _ => {}
        }
    }
    parsed
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Reads the first JSON value from stdin without requiring end-of-file, so an
/// unclosed pipe cannot stall the hook beyond the timeout.
fn read_payload() -> Option<serde_json::Value> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let stdin = std::io::stdin().lock().take(MAX_STDIN_BYTES);
        let value = serde_json::Deserializer::from_reader(stdin).into_iter::<serde_json::Value>().next();
        let _ = tx.send(value.and_then(Result::ok));
    });
    rx.recv_timeout(STDIN_TIMEOUT).ok().flatten()
}

fn run() -> Option<()> {
    let args = parse_args(std::env::args().skip(1));
    let port = args.port.unwrap_or_else(|| cpanel_core::resolve_port(std::env::var(cpanel_core::PORT_ENV).ok().as_deref()));
    let payload = read_payload()?;
    let summary = summarize_value(&payload, &args.profile, now_ms())?;
    let socket = UdpSocket::bind(("127.0.0.1", 0)).ok()?;
    socket.send_to(&encode(&summary), ("127.0.0.1", port)).ok()?;
    Some(())
}

fn main() {
    // Exit code 2 would block the user's action; nothing here may ever fail visibly.
    std::panic::set_hook(Box::new(|_| std::process::exit(0)));
    let _ = run();
    std::process::exit(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Args {
        parse_args(list.iter().map(|s| s.to_string()))
    }

    #[test]
    fn parses_profile_and_port() {
        assert_eq!(args(&["--profile", "claude-work"]), Args { profile: "claude-work".into(), port: None });
        assert_eq!(args(&["--port", "5000", "--profile", "x"]), Args { profile: "x".into(), port: Some(5000) });
    }

    #[test]
    fn ignores_unknown_and_malformed_arguments() {
        assert_eq!(args(&[]), Args::default());
        assert_eq!(args(&["--what", "--profile"]), Args::default());
        assert_eq!(args(&["--port", "nope"]), Args::default());
        assert_eq!(args(&["--port", "0"]), Args::default());
        assert_eq!(args(&["--port", "99999"]), Args::default());
    }
}
