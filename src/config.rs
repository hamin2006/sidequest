//! Optional `~/.config/sidequest/config.toml`. A missing or broken file means defaults.

use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Pop the window up automatically while Claude works.
    pub enabled: bool,
    /// Only pop up if Claude is still working after this many seconds.
    pub delay_secs: f64,
    /// Close an auto-opened window when Claude finishes (otherwise it just pauses).
    pub auto_close: bool,
    /// Bring the terminal Claude runs in back to the front when the window closes.
    pub focus_terminal: bool,
    pub always_on_top: bool,
    pub font_size: f32,
    /// Window size in character cells.
    pub cols: u16,
    pub rows: u16,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: true,
            delay_secs: 8.0,
            auto_close: true,
            focus_terminal: true,
            always_on_top: true,
            font_size: 15.0,
            cols: 104,
            rows: 34,
        }
    }
}

impl Config {
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut c: Config = toml::from_str(text).map_err(|e| e.to_string())?;
        c.delay_secs = c.delay_secs.clamp(0.0, 600.0);
        c.font_size = c.font_size.clamp(8.0, 40.0);
        c.cols = c.cols.clamp(40, 300);
        c.rows = c.rows.clamp(16, 120);
        Ok(c)
    }

    pub fn load(path: &Path) -> (Self, Option<String>) {
        match std::fs::read_to_string(path) {
            Ok(t) => match Self::parse(&t) {
                Ok(c) => (c, None),
                Err(e) => {
                    (Self::default(), Some(format!("ignoring {}: {}", path.display(), e.lines().next().unwrap_or(&e))))
                }
            },
            Err(_) => (Self::default(), None),
        }
    }

    pub fn write(&self, path: &Path) -> std::io::Result<()> {
        let text = toml::to_string_pretty(self).map_err(std::io::Error::other)?;
        crate::store::write_atomic(path, format!("# sidequest settings\n{text}").as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_clamps_and_defaults() {
        let c = Config::parse("delay_secs = -5\nfont_size = 100\n").unwrap();
        assert_eq!(c.delay_secs, 0.0);
        assert_eq!(c.font_size, 40.0);
        assert!(c.enabled);
        assert!(Config::parse("nope = 1").is_err());
    }

    #[test]
    fn write_then_load() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("c.toml");
        let c = Config { delay_secs: 3.0, enabled: false, ..Config::default() };
        c.write(&p).unwrap();
        assert_eq!(Config::load(&p).0, c);
        std::fs::write(&p, "delay_secs = \"x\"").unwrap();
        let (c, warn) = Config::load(&p);
        assert_eq!(c, Config::default());
        assert!(warn.is_some());
    }
}
