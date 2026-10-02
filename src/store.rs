//! Saved games and best scores, written atomically so a crash can't corrupt them.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Better {
    Higher,
    Lower,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Score {
    pub value: u64,
    pub better: Better,
}

impl Score {
    pub fn higher(value: u64) -> Self {
        Self { value, better: Better::Higher }
    }

    pub fn lower(value: u64) -> Self {
        Self { value, better: Better::Lower }
    }

    pub fn beats(&self, other: &Score) -> bool {
        match self.better {
            Better::Higher => self.value > other.value,
            Better::Lower => self.value < other.value,
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Meta {
    best: BTreeMap<String, Score>,
    last_game: Option<String>,
}

pub struct Store {
    dir: PathBuf,
    meta: Meta,
}

pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension(format!("tmp{}", std::process::id()));
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path)
}

impl Store {
    pub fn open(dir: PathBuf) -> Self {
        let meta = fs::read_to_string(dir.join("meta.json"))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        Self { dir, meta }
    }

    fn save_path(&self, id: &str) -> PathBuf {
        self.dir.join("saves").join(format!("{id}.json"))
    }

    fn write_meta(&self) {
        if let Ok(bytes) = serde_json::to_vec_pretty(&self.meta) {
            let _ = write_atomic(&self.dir.join("meta.json"), &bytes);
        }
    }

    pub fn load(&self, id: &str) -> Option<serde_json::Value> {
        let text = fs::read_to_string(self.save_path(id)).ok()?;
        serde_json::from_str(&text).ok()
    }

    pub fn has_save(&self, id: &str) -> bool {
        self.save_path(id).exists()
    }

    pub fn save(&self, id: &str, value: &serde_json::Value) {
        if let Ok(bytes) = serde_json::to_vec(value) {
            let _ = write_atomic(&self.save_path(id), &bytes);
        }
    }

    pub fn delete(&self, id: &str) {
        let _ = fs::remove_file(self.save_path(id));
    }

    pub fn best(&self, id: &str) -> Option<Score> {
        self.meta.best.get(id).copied()
    }

    /// Records a score; returns true if it's a new best.
    pub fn record(&mut self, id: &str, score: Score) -> bool {
        let better = self.meta.best.get(id).is_none_or(|b| score.beats(b));
        if better {
            self.meta.best.insert(id.to_string(), score);
            self.write_meta();
        }
        better
    }

    pub fn last_game(&self) -> Option<&str> {
        self.meta.last_game.as_deref()
    }

    pub fn set_last_game(&mut self, id: &str) {
        if self.meta.last_game.as_deref() != Some(id) {
            self.meta.last_game = Some(id.to_string());
            self.write_meta();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saves_and_scores_round_trip() {
        let d = tempfile::tempdir().unwrap();
        let mut s = Store::open(d.path().to_path_buf());
        assert!(s.load("snake").is_none());
        s.save("snake", &serde_json::json!({"len": 5}));
        assert!(s.has_save("snake"));
        assert!(s.record("snake", Score::higher(10)));
        assert!(!s.record("snake", Score::higher(5)));
        assert!(s.record("mines", Score::lower(90)));
        assert!(s.record("mines", Score::lower(60)));
        s.set_last_game("snake");

        let s2 = Store::open(d.path().to_path_buf());
        assert_eq!(s2.load("snake").unwrap()["len"], 5);
        assert_eq!(s2.best("snake").unwrap().value, 10);
        assert_eq!(s2.best("mines").unwrap().value, 60);
        assert_eq!(s2.last_game(), Some("snake"));
        s2.delete("snake");
        assert!(!s2.has_save("snake"));
    }

    #[test]
    fn corrupt_files_are_ignored() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("meta.json"), "{nope").unwrap();
        std::fs::create_dir_all(d.path().join("saves")).unwrap();
        std::fs::write(d.path().join("saves/x.json"), "garbage").unwrap();
        let s = Store::open(d.path().to_path_buf());
        assert!(s.best("x").is_none());
        assert!(s.load("x").is_none());
    }
}
