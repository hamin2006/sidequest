//! End-to-end tests of the `sidequest` binary that don't need a display.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

fn run(args: &[&str], state: &Path, stdin: Option<&str>) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_sidequest"))
        .args(args)
        .env("SIDEQUEST_STATE_DIR", state)
        .env("SIDEQUEST_CONFIG", state.join("config.toml"))
        .env("SIDEQUEST_CLAUDE_SETTINGS", state.join("settings.json"))
        .env("TERM_PROGRAM", "ghostty")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    if let Some(s) = stdin {
        child.stdin.take().unwrap().write_all(s.as_bytes()).unwrap();
    }
    drop(child.stdin.take());
    child.wait_with_output().unwrap()
}

#[test]
fn hooks_are_silent_and_logged() {
    let d = tempfile::tempdir().unwrap();
    // Pop-ups off so no window is ever spawned in CI.
    std::fs::write(d.path().join("config.toml"), "enabled = false\n").unwrap();
    for (ev, input) in [
        ("start", r#"{"session_id":"abc"}"#),
        ("notify", "not json at all"),
        ("stop", r#"{"session_id":"abc"}"#),
        ("bogus", ""),
    ] {
        let out = run(&["hook", ev], d.path(), Some(input));
        assert!(out.status.success(), "hook {ev} failed");
        assert!(out.stdout.is_empty() && out.stderr.is_empty(), "hook {ev} printed something");
    }
    let log = std::fs::read_to_string(d.path().join("events.log")).unwrap();
    let lines: Vec<&str> = log.lines().collect();
    assert_eq!(lines.len(), 3, "{log}");
    assert!(lines[0].contains("\tstart\tabc\tghostty"));
    assert!(lines[1].contains("\tnotify\tdefault\t"));
    assert!(lines[2].contains("\tstop\tabc\t"));
}

#[test]
fn pop_does_nothing_when_claude_already_finished() {
    let d = tempfile::tempdir().unwrap();
    // A short delay: the stop below lands before the background pop-up checks.
    std::fs::write(d.path().join("config.toml"), "delay_secs = 0.6\n").unwrap();
    run(&["hook", "start"], d.path(), Some(r#"{"session_id":"q"}"#));
    run(&["hook", "stop"], d.path(), Some(r#"{"session_id":"q"}"#));
    let log = std::fs::read_to_string(d.path().join("events.log")).unwrap();
    let start_ms = log.lines().next().unwrap().split('\t').next().unwrap().to_string();
    let out = run(&["_pop", "q", &start_ms], d.path(), None);
    assert!(out.status.success());
    std::thread::sleep(std::time::Duration::from_millis(1500));
    assert!(!d.path().join("window.pid").exists(), "no window for a prompt that already finished");
}

#[test]
fn hooks_install_status_uninstall_round_trip() {
    let d = tempfile::tempdir().unwrap();
    let settings = d.path().join("settings.json");
    std::fs::write(
        &settings,
        r#"{"theme":"dark","hooks":{"Stop":[{"hooks":[{"type":"command","command":"say hi"}]}]}}"#,
    )
    .unwrap();
    assert_eq!(String::from_utf8_lossy(&run(&["hooks", "status"], d.path(), None).stdout).trim(), "not installed");
    let out = run(&["hooks", "install"], d.path(), None);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&run(&["hooks", "status"], d.path(), None).stdout).trim(), "installed");
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&settings).unwrap()).unwrap();
    assert_eq!(v["theme"], "dark");
    assert_eq!(v["hooks"]["Stop"].as_array().unwrap().len(), 2);
    assert!(settings.with_extension("json.sidequest-backup").exists());
    assert!(run(&["hooks", "uninstall"], d.path(), None).status.success());
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&settings).unwrap()).unwrap();
    assert_eq!(v["hooks"]["Stop"][0]["hooks"][0]["command"], "say hi");
    assert!(v["hooks"].get("UserPromptSubmit").is_none());
}

#[test]
fn on_off_status_and_bad_input() {
    let d = tempfile::tempdir().unwrap();
    assert!(run(&["off"], d.path(), None).status.success());
    let status = String::from_utf8_lossy(&run(&["status"], d.path(), None).stdout).to_string();
    assert!(status.contains("pop-up:  off"), "{status}");
    assert!(run(&["on"], d.path(), None).status.success());
    let status = String::from_utf8_lossy(&run(&["status"], d.path(), None).stdout).to_string();
    assert!(status.contains("pop-up:  on"));

    let out = run(&["play", "tetris"], d.path(), None);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("unknown game"));
    let out = run(&["play", "snake"], d.path(), None);
    assert!(!out.status.success(), "play needs a terminal");
}
