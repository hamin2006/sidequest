//! Claude Code hook entry points and the settings.json installer.
//!
//! Hooks must be invisible: no output (UserPromptSubmit output is added to Claude's context), no
//! failures, and back in milliseconds. Anything slow happens in a detached `sidequest _pop` process.

use std::io::{IsTerminal, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use crate::claude::{self, Event, Kind};
use crate::config::Config;
use crate::paths;

/// Hook events and the argument sidequest receives for each.
pub const HOOKS: [(&str, &str); 3] = [
    ("UserPromptSubmit", "start"),
    ("Stop", "stop"),
    ("Notification", "notify"),
];

fn session_from_stdin() -> String {
    let stdin = std::io::stdin();
    if stdin.is_terminal() {
        return "manual".into();
    }
    let mut text = String::new();
    let _ = stdin.lock().take(1 << 20).read_to_string(&mut text);
    serde_json::from_str::<Value>(&text)
        .ok()
        .and_then(|v| {
            v.get("session_id")
                .and_then(Value::as_str)
                .map(String::from)
        })
        .unwrap_or_else(|| "default".into())
}

/// Starts a fully detached copy of this program (own process group, no stdio).
pub fn spawn_self(args: &[&str]) -> Result<()> {
    let exe = std::env::current_exe().context("can't find own executable")?;
    let mut cmd = Command::new(exe);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    cmd.spawn().context("spawn")?;
    Ok(())
}

/// `sidequest hook <start|stop|notify>`. Never fails and never prints.
pub fn run_hook(which: &str) {
    let kind = match which {
        "start" => Kind::Start,
        "stop" => Kind::Stop,
        "notify" => Kind::Notify,
        _ => return,
    };
    let session = session_from_stdin();
    let term = std::env::var("TERM_PROGRAM").unwrap_or_default();
    let ms = claude::now_ms();
    let _ = claude::append(
        &claude::events_path(),
        &Event {
            ms,
            kind,
            session: session.clone(),
            term,
        },
    );
    if kind == Kind::Start && Config::load(&paths::config_path()).0.enabled {
        let _ = spawn_self(&["_pop", &session, &ms.to_string()]);
    }
}

/// The window's pid file: present while a window is open.
pub fn window_pidfile() -> PathBuf {
    paths::state_dir().join("window.pid")
}

#[cfg(unix)]
fn pid_alive(pid: i32) -> bool {
    // SAFETY: plain syscall with integer arguments.
    pid > 1
        && (unsafe { libc::kill(pid, 0) } == 0
            || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM))
}

#[cfg(not(unix))]
fn pid_alive(_pid: i32) -> bool {
    false
}

pub fn window_running() -> bool {
    std::fs::read_to_string(window_pidfile())
        .ok()
        .and_then(|t| t.trim().parse::<i32>().ok())
        .is_some_and(|pid| pid != std::process::id() as i32 && pid_alive(pid))
}

/// `sidequest _pop <session> <ms>`: wait, then open the window if that prompt is still running.
pub fn run_pop(session: &str, start_ms: u64) {
    let cfg = Config::load(&paths::config_path()).0;
    if !cfg.enabled {
        return;
    }
    std::thread::sleep(Duration::from_secs_f64(cfg.delay_secs));
    let events = claude::read_all(&claude::events_path());
    if claude::should_pop(&events, session, start_ms) && !window_running() {
        let _ = spawn_self(&["window", "--auto"]);
    }
}

// ---------- settings.json ----------

fn command_for(exe: &Path, arg: &str) -> String {
    let exe = exe.display().to_string();
    let exe = if exe.contains(' ') {
        format!("\"{exe}\"")
    } else {
        exe
    };
    format!("{exe} hook {arg}")
}

/// `…/sidequest hook start`, with or without quotes around a path containing spaces.
fn is_our_command(c: &str) -> bool {
    c.contains("sidequest")
        && HOOKS
            .iter()
            .any(|(_, arg)| c.trim_end().ends_with(&format!(" hook {arg}")))
}

fn is_ours(entry: &Value) -> bool {
    entry
        .get("hooks")
        .and_then(Value::as_array)
        .is_some_and(|hs| {
            hs.iter().any(|h| {
                h.get("command")
                    .and_then(Value::as_str)
                    .is_some_and(is_our_command)
            })
        })
}

/// Adds sidequest's hooks to a settings object, replacing any older sidequest entries and keeping
/// everything else exactly as it was.
pub fn add_hooks(settings: &mut Value, exe: &Path) -> Result<()> {
    if !settings.is_object() {
        bail!("settings.json is not a JSON object");
    }
    let hooks = settings
        .as_object_mut()
        .unwrap()
        .entry("hooks")
        .or_insert_with(|| json!({}));
    let Some(hooks) = hooks.as_object_mut() else {
        bail!("\"hooks\" in settings.json is not an object")
    };
    for (event, arg) in HOOKS {
        let list = hooks.entry(event).or_insert_with(|| json!([]));
        let Some(list) = list.as_array_mut() else {
            bail!("hooks.{event} is not a list")
        };
        list.retain(|e| !is_ours(e));
        list.push(json!({ "hooks": [{ "type": "command", "command": command_for(exe, arg), "timeout": 5 }] }));
    }
    Ok(())
}

/// Removes sidequest's hooks, dropping lists/objects that become empty.
pub fn remove_hooks(settings: &mut Value) -> bool {
    let Some(hooks) = settings.get_mut("hooks").and_then(Value::as_object_mut) else {
        return false;
    };
    let mut changed = false;
    for (event, _) in HOOKS {
        if let Some(list) = hooks.get_mut(event).and_then(Value::as_array_mut) {
            let before = list.len();
            list.retain(|e| !is_ours(e));
            changed |= list.len() != before;
            if list.is_empty() {
                hooks.remove(event);
            }
        }
    }
    if hooks.is_empty() {
        settings.as_object_mut().map(|o| o.remove("hooks"));
    }
    changed
}

pub fn installed(settings: &Value) -> bool {
    HOOKS.iter().all(|(event, _)| {
        settings
            .get("hooks")
            .and_then(|h| h.get(*event))
            .and_then(Value::as_array)
            .is_some_and(|l| l.iter().any(is_ours))
    })
}

fn read_settings(path: &Path) -> Result<Value> {
    match std::fs::read_to_string(path) {
        Ok(t) if t.trim().is_empty() => Ok(json!({})),
        Ok(t) => serde_json::from_str(&t)
            .with_context(|| format!("{} isn't valid JSON; not touching it", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(json!({})),
        Err(e) => Err(e.into()),
    }
}

fn write_settings(path: &Path, v: &Value) -> Result<()> {
    if path.exists() {
        std::fs::copy(path, path.with_extension("json.sidequest-backup"))
            .context("backing up settings.json")?;
    }
    crate::store::write_atomic(path, (serde_json::to_string_pretty(v)? + "\n").as_bytes())?;
    Ok(())
}

pub fn install(path: &Path, exe: &Path) -> Result<()> {
    let mut v = read_settings(path)?;
    add_hooks(&mut v, exe)?;
    write_settings(path, &v)
}

pub fn uninstall(path: &Path) -> Result<bool> {
    let mut v = read_settings(path)?;
    let changed = remove_hooks(&mut v);
    if changed {
        write_settings(path, &v)?;
    }
    Ok(changed)
}

pub fn is_installed(path: &Path) -> bool {
    read_settings(path).map(|v| installed(&v)).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_preserves_other_settings_and_hooks() {
        let mut s = json!({
            "theme": "dark",
            "hooks": { "Stop": [{ "hooks": [{ "type": "command", "command": "say done" }] }], "PreToolUse": [{ "matcher": "Bash", "hooks": [] }] }
        });
        add_hooks(&mut s, Path::new("/usr/local/bin/sidequest")).unwrap();
        add_hooks(&mut s, Path::new("/usr/local/bin/sidequest")).unwrap(); // idempotent
        assert!(installed(&s));
        assert_eq!(s["theme"], "dark");
        assert_eq!(
            s["hooks"]["Stop"].as_array().unwrap().len(),
            2,
            "the user's own Stop hook is kept"
        );
        assert_eq!(
            s["hooks"]["UserPromptSubmit"][0]["hooks"][0]["command"],
            "/usr/local/bin/sidequest hook start"
        );
        assert!(s["hooks"]["PreToolUse"].is_array());

        assert!(remove_hooks(&mut s));
        assert!(!installed(&s));
        assert_eq!(s["hooks"]["Stop"][0]["hooks"][0]["command"], "say done");
        assert!(s["hooks"].get("UserPromptSubmit").is_none());
    }

    #[test]
    fn uninstall_cleans_up_empty_sections() {
        let mut s = json!({});
        add_hooks(&mut s, Path::new("/x/my apps/sidequest")).unwrap();
        assert_eq!(
            s["hooks"]["Stop"][0]["hooks"][0]["command"],
            "\"/x/my apps/sidequest\" hook stop"
        );
        remove_hooks(&mut s);
        assert_eq!(s, json!({}));
    }

    #[test]
    fn refuses_to_touch_broken_files() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("settings.json");
        std::fs::write(&p, "{ not json").unwrap();
        assert!(install(&p, Path::new("/bin/sidequest")).is_err());
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "{ not json");

        let mut weird = json!({ "hooks": [] });
        assert!(add_hooks(&mut weird, Path::new("/bin/sidequest")).is_err());
    }

    #[test]
    fn install_writes_a_backup() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("settings.json");
        std::fs::write(&p, "{\"theme\":\"light\"}").unwrap();
        install(&p, Path::new("/bin/sidequest")).unwrap();
        assert!(is_installed(&p));
        assert_eq!(
            std::fs::read_to_string(p.with_extension("json.sidequest-backup")).unwrap(),
            "{\"theme\":\"light\"}"
        );
        assert!(uninstall(&p).unwrap());
        assert!(!uninstall(&p).unwrap());
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
        assert_eq!(v, json!({"theme": "light"}));
    }
}
