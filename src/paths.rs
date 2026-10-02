//! Where sidequest keeps its config, saves and hook state.

use std::path::PathBuf;

fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

fn xdg(var: &str, fallback: &str) -> PathBuf {
    std::env::var_os(var)
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home().join(fallback))
}

/// `$SIDEQUEST_STATE_DIR`, else `$XDG_STATE_HOME/sidequest`, else `~/.local/state/sidequest`.
pub fn state_dir() -> PathBuf {
    if let Some(p) = std::env::var_os("SIDEQUEST_STATE_DIR") {
        return PathBuf::from(p);
    }
    xdg("XDG_STATE_HOME", ".local/state").join("sidequest")
}

/// `$SIDEQUEST_CONFIG`, else `~/.config/sidequest/config.toml`.
pub fn config_path() -> PathBuf {
    if let Some(p) = std::env::var_os("SIDEQUEST_CONFIG") {
        return PathBuf::from(p);
    }
    xdg("XDG_CONFIG_HOME", ".config")
        .join("sidequest")
        .join("config.toml")
}

/// `$SIDEQUEST_CLAUDE_SETTINGS`, else `~/.claude/settings.json`.
pub fn claude_settings() -> PathBuf {
    if let Some(p) = std::env::var_os("SIDEQUEST_CLAUDE_SETTINGS") {
        return PathBuf::from(p);
    }
    home().join(".claude").join("settings.json")
}
