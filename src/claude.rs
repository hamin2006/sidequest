//! The bridge to Claude Code.
//!
//! Hooks append one line per event to `events.log`. The game window tails that file to learn when
//! Claude starts and stops working. A plain append-only file means hooks never block on the window
//! and nothing breaks if the window isn't running.

use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::paths;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// You sent a prompt; Claude is working.
    Start,
    /// Claude finished its turn.
    Stop,
    /// Claude needs you (permission prompt or waiting for input).
    Notify,
    /// You closed an auto-opened window while Claude was still busy: don't reopen it this turn.
    Dismiss,
}

impl Kind {
    fn as_str(self) -> &'static str {
        match self {
            Kind::Start => "start",
            Kind::Stop => "stop",
            Kind::Notify => "notify",
            Kind::Dismiss => "dismiss",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "start" => Kind::Start,
            "stop" => Kind::Stop,
            "notify" => Kind::Notify,
            "dismiss" => Kind::Dismiss,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    pub ms: u64,
    pub kind: Kind,
    pub session: String,
    /// `TERM_PROGRAM` of the terminal Claude runs in (used to bring it back to the front).
    pub term: String,
}

pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

pub fn events_path() -> PathBuf {
    paths::state_dir().join("events.log")
}

fn clean(s: &str) -> String {
    let s: String = s.chars().filter(|c| !c.is_control() && *c != '\t').take(128).collect();
    if s.is_empty() { "-".into() } else { s }
}

pub fn format_event(e: &Event) -> String {
    format!("{}\t{}\t{}\t{}\n", e.ms, e.kind.as_str(), clean(&e.session), clean(&e.term))
}

pub fn parse_event(line: &str) -> Option<Event> {
    let mut p = line.trim_end_matches(['\n', '\r']).split('\t');
    let ms = p.next()?.parse().ok()?;
    let kind = Kind::parse(p.next()?)?;
    let session = p.next()?.to_string();
    let term = p.next().unwrap_or("-").to_string();
    Some(Event { ms, kind, session, term })
}

const MAX_LOG: u64 = 256 * 1024;

/// Appends an event (one small write, so concurrent hooks don't interleave) and keeps the log small.
pub fn append(path: &Path, e: &Event) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let mut f = OpenOptions::new().create(true).append(true).open(path)?;
    f.write_all(format_event(e).as_bytes())?;
    if f.metadata().map(|m| m.len()).unwrap_or(0) > MAX_LOG {
        let text = fs::read_to_string(path).unwrap_or_default();
        let lines: Vec<&str> = text.lines().collect();
        let keep = lines[lines.len().saturating_sub(400)..].join("\n") + "\n";
        crate::store::write_atomic(path, keep.as_bytes())?;
    }
    Ok(())
}

pub fn read_all(path: &Path) -> Vec<Event> {
    fs::read_to_string(path).unwrap_or_default().lines().filter_map(parse_event).collect()
}

/// A session counts as busy only for a while after its last start, in case Claude was killed before
/// its Stop hook could run.
const STALE_MS: u64 = 2 * 60 * 60 * 1000;

/// Should the delayed pop-up for (`session`, `start_ms`) still open the window?
pub fn should_pop(events: &[Event], session: &str, start_ms: u64) -> bool {
    let latest = events.iter().filter(|e| e.session == session && e.kind != Kind::Dismiss).max_by_key(|e| e.ms);
    let dismissed = events.iter().any(|e| e.kind == Kind::Dismiss && e.ms >= start_ms);
    matches!(latest, Some(e) if e.kind == Kind::Start && e.ms == start_ms) && !dismissed
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Signal {
    /// A session finished or needs attention.
    Done { term: String },
}

/// Tails the event log and tracks which sessions are busy.
pub struct Watcher {
    path: PathBuf,
    offset: u64,
    since_ms: u64,
    busy: HashMap<String, u64>,
    pub last_term: Option<String>,
}

impl Watcher {
    /// Only events after `since_ms` produce signals; earlier history just sets the busy state.
    pub fn new(path: PathBuf, since_ms: u64) -> Self {
        let mut w = Self { path, offset: 0, since_ms, busy: HashMap::new(), last_term: None };
        w.poll();
        w
    }

    pub fn poll(&mut self) -> Vec<Signal> {
        let mut out = vec![];
        let Ok(mut f) = fs::File::open(&self.path) else {
            return out;
        };
        let len = f.metadata().map(|m| m.len()).unwrap_or(0);
        if len < self.offset {
            // Rotated: rebuild from scratch.
            self.offset = 0;
            self.busy.clear();
        }
        if f.seek(SeekFrom::Start(self.offset)).is_err() {
            return out;
        }
        let mut buf = Vec::new();
        if f.read_to_end(&mut buf).is_err() {
            return out;
        }
        // Only consume complete lines.
        let Some(end) = buf.iter().rposition(|&b| b == b'\n') else {
            return out;
        };
        self.offset += end as u64 + 1;
        for line in String::from_utf8_lossy(&buf[..=end]).lines() {
            let Some(e) = parse_event(line) else { continue };
            if e.term != "-" {
                self.last_term = Some(e.term.clone());
            }
            match e.kind {
                Kind::Start => {
                    self.busy.insert(e.session, e.ms);
                }
                Kind::Stop | Kind::Notify => {
                    self.busy.remove(&e.session);
                    if e.ms >= self.since_ms {
                        out.push(Signal::Done { term: e.term });
                    }
                }
                Kind::Dismiss => {}
            }
        }
        out
    }

    pub fn busy(&self) -> usize {
        let now = now_ms();
        self.busy.values().filter(|&&ms| now.saturating_sub(ms) < STALE_MS).count()
    }

    pub fn busy_sessions(&self) -> Vec<String> {
        self.busy.keys().cloned().collect()
    }
}

/// Maps `TERM_PROGRAM` to the macOS app to re-activate.
pub fn terminal_app(term_program: &str) -> Option<&'static str> {
    Some(match term_program {
        "ghostty" => "Ghostty",
        "iTerm.app" => "iTerm",
        "Apple_Terminal" => "Terminal",
        "vscode" => "Visual Studio Code",
        "WezTerm" => "WezTerm",
        "kitty" => "kitty",
        "WarpTerminal" => "Warp",
        "Hyper" => "Hyper",
        "Tabby" => "Tabby",
        "zed" => "Zed",
        "cursor" => "Cursor",
        _ => return None,
    })
}

/// Brings the terminal back to the front (macOS). Silent no-op elsewhere or when unknown.
pub fn focus_terminal(term_program: &str) {
    if !cfg!(target_os = "macos") {
        return;
    }
    if let Some(app) = terminal_app(term_program) {
        let _ = std::process::Command::new("open")
            .args(["-a", app])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(ms: u64, kind: Kind, s: &str) -> Event {
        Event { ms, kind, session: s.into(), term: "ghostty".into() }
    }

    #[test]
    fn lines_round_trip_and_bad_lines_are_skipped() {
        let e = Event { ms: 5, kind: Kind::Notify, session: "a\tb\nc".into(), term: "".into() };
        let back = parse_event(&format_event(&e)).unwrap();
        assert_eq!(back.session, "abc", "tabs and newlines can't break the format");
        assert_eq!(back.term, "-");
        assert!(parse_event("garbage").is_none());
        assert!(parse_event("12\tbogus\ts\tt").is_none());
    }

    #[test]
    fn pop_decision() {
        let mut log = vec![ev(100, Kind::Start, "a")];
        assert!(should_pop(&log, "a", 100));
        assert!(!should_pop(&log, "a", 99), "a newer start owns the pop");
        log.push(ev(200, Kind::Stop, "a"));
        assert!(!should_pop(&log, "a", 100), "already finished");
        let log = vec![ev(100, Kind::Start, "a"), ev(150, Kind::Dismiss, "*")];
        assert!(!should_pop(&log, "a", 100), "you closed it this turn");
        let log = vec![ev(100, Kind::Start, "a"), ev(120, Kind::Start, "b")];
        assert!(should_pop(&log, "a", 100), "other sessions don't interfere");
    }

    #[test]
    fn watcher_tracks_busy_and_signals_new_events_only() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("events.log");
        let now = now_ms();
        append(&p, &ev(now - 5000, Kind::Start, "old")).unwrap();
        append(&p, &ev(now - 4000, Kind::Stop, "old")).unwrap();
        append(&p, &ev(now - 3000, Kind::Start, "a")).unwrap();
        let mut w = Watcher::new(p.clone(), now - 1000);
        assert_eq!(w.busy(), 1);
        assert!(w.poll().is_empty(), "history doesn't trigger 'done'");

        append(&p, &ev(now, Kind::Start, "b")).unwrap();
        assert!(w.poll().is_empty());
        assert_eq!(w.busy(), 2);
        // A half-written line is left for the next poll.
        fs::OpenOptions::new().append(true).open(&p).unwrap().write_all(format!("{now}\tstop\ta").as_bytes()).unwrap();
        assert!(w.poll().is_empty());
        fs::OpenOptions::new().append(true).open(&p).unwrap().write_all(b"\tghostty\n").unwrap();
        assert_eq!(w.poll(), vec![Signal::Done { term: "ghostty".into() }]);
        assert_eq!(w.busy(), 1);
        assert_eq!(w.last_term.as_deref(), Some("ghostty"));
    }

    #[test]
    fn log_rotation_keeps_watchers_working() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("events.log");
        let mut w = Watcher::new(p.clone(), 0);
        for i in 0..6000 {
            append(&p, &ev(i, if i % 2 == 0 { Kind::Start } else { Kind::Stop }, "s")).unwrap();
        }
        assert!(fs::metadata(&p).unwrap().len() <= MAX_LOG + 200);
        w.poll();
        append(&p, &ev(99_999, Kind::Stop, "s")).unwrap();
        assert!(w.poll().contains(&Signal::Done { term: "ghostty".into() }));
    }

    #[test]
    fn terminal_names() {
        assert_eq!(terminal_app("ghostty"), Some("Ghostty"));
        assert_eq!(terminal_app("iTerm.app"), Some("iTerm"));
        assert_eq!(terminal_app("weird"), None);
    }
}
