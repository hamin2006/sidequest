//! Everything that survives between dives: money, upgrades, research, knowledge and records.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::world::{Species, ZONES};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Ending {
    Answered,
    WentDark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Upgrade {
    Hull,
    Battery,
    Sonar,
    Quiet,
    Plating,
    Decoys,
    Harpoon,
}

pub struct UpgradeDef {
    pub up: Upgrade,
    pub name: &'static str,
    pub max: u8,
    pub costs: &'static [u64],
}

pub const UPGRADES: [UpgradeDef; 7] = [
    UpgradeDef { up: Upgrade::Hull, name: "Pressure hull", max: 4, costs: &[120, 320, 700, 1300] },
    UpgradeDef { up: Upgrade::Battery, name: "Battery bank", max: 4, costs: &[80, 200, 420, 800] },
    UpgradeDef { up: Upgrade::Sonar, name: "Sonar array", max: 4, costs: &[100, 260, 520, 900] },
    UpgradeDef { up: Upgrade::Quiet, name: "Quiet drive", max: 3, costs: &[150, 400, 850] },
    UpgradeDef { up: Upgrade::Plating, name: "Hull plating", max: 4, costs: &[90, 220, 450, 850] },
    UpgradeDef { up: Upgrade::Decoys, name: "Decoy rack", max: 3, costs: &[70, 180, 380] },
    UpgradeDef { up: Upgrade::Harpoon, name: "Harpoon", max: 2, costs: &[250, 600] },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Tech {
    EchoMemory,
    LureFilter,
    PressureModel,
    PulseCompression,
    SongAnalysis,
}

pub struct TechDef {
    pub tech: Tech,
    pub name: &'static str,
    pub desc: &'static str,
    pub cost: u32,
}

pub const TECHS: [TechDef; 5] = [
    TechDef { tech: Tech::EchoMemory, name: "Echo memory", desc: "Echoes linger 50% longer", cost: 3 },
    TechDef { tech: Tech::LureFilter, name: "Lure filter", desc: "Angler lures show red, not amber", cost: 2 },
    TechDef { tech: Tech::PressureModel, name: "Pressure model", desc: "Half damage below rated depth", cost: 4 },
    TechDef { tech: Tech::PulseCompression, name: "Pulse compression", desc: "Pings cost 30% less battery", cost: 4 },
    TechDef { tech: Tech::SongAnalysis, name: "Song analysis", desc: "Hear the leviathan's breathing", cost: 5 },
];

pub const RATED_M: [i32; 5] = [1500, 3000, 5000, 7500, 11000];
pub const RANKS: [(u64, &str); 5] =
    [(0, "Cadet"), (300, "Pilot"), (1000, "Deep Pilot"), (2500, "Abyssal Cartographer"), (6000, "Keeper of the Floor")];
/// Echoes of a species needed to complete its bestiary entry.
pub const SCANS_NEEDED: u8 = 3;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Progress {
    pub scrap: u64,
    pub research: u32,
    pub xp: u64,
    pub upgrades: BTreeMap<Upgrade, u8>,
    pub techs: BTreeSet<Tech>,
    pub scans: BTreeMap<String, u8>,
    pub logs: BTreeSet<u8>,
    /// Zone indices whose relay you've reached (dives can start there).
    pub relays: BTreeSet<usize>,
    pub deepest_m: u32,
    pub dives: u32,
    pub losses: u32,
    pub relics: u32,
    pub ending: Option<Ending>,
    pub seen_intro: bool,
    pub descents: u32,
}

impl Progress {
    pub fn level(&self, u: Upgrade) -> u8 {
        self.upgrades.get(&u).copied().unwrap_or(0)
    }

    pub fn has(&self, t: Tech) -> bool {
        self.techs.contains(&t)
    }

    pub fn rated_m(&self) -> i32 {
        RATED_M[self.level(Upgrade::Hull) as usize]
    }

    pub fn max_hull(&self) -> f64 {
        100.0 + 25.0 * self.level(Upgrade::Plating) as f64
    }

    pub fn max_battery(&self) -> f64 {
        100.0 + 40.0 * self.level(Upgrade::Battery) as f64
    }

    pub fn sonar_range(&self) -> f64 {
        16.0 + 5.0 * self.level(Upgrade::Sonar) as f64
    }

    pub fn noise_mult(&self) -> f64 {
        1.0 - 0.2 * self.level(Upgrade::Quiet) as f64
    }

    pub fn decoys(&self) -> u8 {
        1 + self.level(Upgrade::Decoys)
    }

    pub fn ping_cost(&self) -> f64 {
        if self.has(Tech::PulseCompression) { 7.0 } else { 10.0 }
    }

    pub fn echo_fade(&self) -> f64 {
        if self.has(Tech::EchoMemory) { 10.5 } else { 7.0 }
    }

    pub fn rank(&self) -> &'static str {
        RANKS.iter().rev().find(|(xp, _)| self.xp >= *xp).map(|(_, n)| *n).unwrap_or("Cadet")
    }

    pub fn upgrade_cost(&self, u: Upgrade) -> Option<u64> {
        let def = UPGRADES.iter().find(|d| d.up == u)?;
        def.costs.get(self.level(u) as usize).copied()
    }

    pub fn buy_upgrade(&mut self, u: Upgrade) -> Result<(), &'static str> {
        let cost = self.upgrade_cost(u).ok_or("Already at the maximum level")?;
        if self.scrap < cost {
            return Err("Not enough scrap");
        }
        self.scrap -= cost;
        *self.upgrades.entry(u).or_insert(0) += 1;
        Ok(())
    }

    pub fn buy_tech(&mut self, t: Tech) -> Result<(), &'static str> {
        if self.has(t) {
            return Err("Already researched");
        }
        let cost = TECHS.iter().find(|d| d.tech == t).map(|d| d.cost).unwrap_or(99);
        if self.research < cost {
            return Err("Not enough research");
        }
        self.research -= cost;
        self.techs.insert(t);
        Ok(())
    }

    pub fn scan_count(&self, s: Species) -> u8 {
        self.scans.get(s.id()).copied().unwrap_or(0)
    }

    pub fn scanned(&self, s: Species) -> bool {
        self.scan_count(s) >= SCANS_NEEDED
    }

    /// Records one echo of a species. Returns true when this completes its entry.
    pub fn add_scan(&mut self, s: Species) -> bool {
        let n = self.scans.entry(s.id().to_string()).or_insert(0);
        if *n >= SCANS_NEEDED {
            return false;
        }
        *n += 1;
        if *n == SCANS_NEEDED {
            self.research += if s == Species::Leviathan { 4 } else { 2 };
            self.xp += 60;
            return true;
        }
        false
    }

    /// Depths a dive can start from: the surface plus every relay reached.
    pub fn start_options(&self) -> Vec<usize> {
        let mut v = vec![0];
        v.extend(self.relays.iter().copied().filter(|&z| z > 0 && z < ZONES.len() - 1));
        v
    }

    pub fn missing_logs(&self) -> Vec<u8> {
        (0..12u8).filter(|l| !self.logs.contains(l)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buying_and_stats() {
        let mut p = Progress { scrap: 500, ..Progress::default() };
        assert_eq!(p.rated_m(), 1500);
        p.buy_upgrade(Upgrade::Hull).unwrap();
        assert_eq!(p.rated_m(), 3000);
        assert_eq!(p.scrap, 380);
        assert!(p.buy_upgrade(Upgrade::Hull).is_ok());
        assert_eq!(p.buy_upgrade(Upgrade::Hull), Err("Not enough scrap"));
        p.scrap = 1_000_000;
        for _ in 0..10 {
            let _ = p.buy_upgrade(Upgrade::Harpoon);
        }
        assert_eq!(p.level(Upgrade::Harpoon), 2, "capped at max");
        assert_eq!(p.buy_upgrade(Upgrade::Harpoon), Err("Already at the maximum level"));
    }

    #[test]
    fn scans_research_and_ranks() {
        let mut p = Progress::default();
        assert!(!p.add_scan(Species::Eel));
        assert!(!p.add_scan(Species::Eel));
        assert!(p.add_scan(Species::Eel));
        assert!(!p.add_scan(Species::Eel), "completes once");
        assert!(p.scanned(Species::Eel));
        assert_eq!(p.research, 2);
        assert_eq!(p.buy_tech(Tech::EchoMemory), Err("Not enough research"));
        assert!(p.buy_tech(Tech::LureFilter).is_ok());
        assert_eq!(p.research, 0);
        assert_eq!(p.rank(), "Cadet");
        p.xp = 2600;
        assert_eq!(p.rank(), "Abyssal Cartographer");
        p.relays.extend([2, 3, 5]);
        assert_eq!(p.start_options(), vec![0, 2, 3]);
        p.logs.insert(4);
        assert_eq!(p.missing_logs().len(), 11);
    }
}
