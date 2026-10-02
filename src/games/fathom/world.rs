//! The trench: a deterministic side-on cave map generated from a seed.

use serde::{Deserialize, Serialize};

use crate::rng::Rng;

pub const W: i32 = 96;
pub const H: i32 = 1100;
/// Metres per row.
pub const ROW_M: i32 = 10;

pub const WATER: u8 = 0;
pub const ROCK: u8 = 1;
pub const VENT: u8 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Zone {
    pub name: &'static str,
    /// First row of the zone.
    pub top: i32,
    /// Ambient light radius (sunlit water is visible without sonar).
    pub ambient: i32,
}

pub const ZONES: [Zone; 6] = [
    Zone { name: "Sunlit", top: 0, ambient: 99 },
    Zone { name: "Twilight", top: 20, ambient: 5 },
    Zone { name: "Midnight", top: 100, ambient: 0 },
    Zone { name: "Abyssal", top: 400, ambient: 0 },
    Zone { name: "Hadal", top: 600, ambient: 0 },
    Zone { name: "The Floor", top: H - 14, ambient: 0 },
];

pub fn zone_index(row: i32) -> usize {
    ZONES.iter().rposition(|z| row >= z.top).unwrap_or(0)
}

pub fn depth_m(row: i32) -> i32 {
    row.max(0) * ROW_M
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Species {
    Glimmer,
    Jelly,
    Gulper,
    Angler,
    Eel,
    Leviathan,
}

impl Species {
    pub const ALL: [Species; 6] =
        [Species::Glimmer, Species::Jelly, Species::Gulper, Species::Angler, Species::Eel, Species::Leviathan];

    pub fn id(self) -> &'static str {
        match self {
            Species::Glimmer => "glimmer",
            Species::Jelly => "jelly",
            Species::Gulper => "gulper",
            Species::Angler => "angler",
            Species::Eel => "eel",
            Species::Leviathan => "leviathan",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Wreck {
    pub x: i32,
    pub y: i32,
    /// Hull outline half-width (decorative).
    pub half: i32,
    pub scrap: u64,
    pub battery: bool,
    pub patch: bool,
    pub relic: bool,
    pub log: Option<u8>,
    pub beacon: bool,
    /// The Meridian, at the floor.
    pub meridian: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Spawn {
    pub species: Species,
    pub x: i32,
    pub y: i32,
}

#[derive(Debug)]
pub struct World {
    pub seed: u64,
    pub cells: Vec<u8>,
    pub wrecks: Vec<Wreck>,
    pub spawns: Vec<Spawn>,
    /// Where the Singer waits.
    pub singer: (i32, i32),
}

/// Smooth value noise in roughly [0, 1).
fn hash(seed: u64, x: i32, y: i32) -> f64 {
    let mut h = seed ^ (x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (y as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    h ^= h >> 33;
    h = h.wrapping_mul(0xff51_afd7_ed55_8ccd);
    h ^= h >> 33;
    (h >> 11) as f64 / (1u64 << 53) as f64
}

fn noise(seed: u64, x: f64, y: f64) -> f64 {
    let (x0, y0) = (x.floor() as i32, y.floor() as i32);
    let (fx, fy) = (x - x0 as f64, y - y0 as f64);
    let s = |t: f64| t * t * (3.0 - 2.0 * t);
    let (sx, sy) = (s(fx), s(fy));
    let a = hash(seed, x0, y0) + (hash(seed, x0 + 1, y0) - hash(seed, x0, y0)) * sx;
    let b = hash(seed, x0, y0 + 1) + (hash(seed, x0 + 1, y0 + 1) - hash(seed, x0, y0 + 1)) * sx;
    a + (b - a) * sy
}

impl World {
    #[inline]
    pub fn in_bounds(x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && x < W && y < H
    }

    #[inline]
    pub fn idx(x: i32, y: i32) -> usize {
        (y * W + x) as usize
    }

    pub fn get(&self, x: i32, y: i32) -> u8 {
        if Self::in_bounds(x, y) { self.cells[Self::idx(x, y)] } else { ROCK }
    }

    pub fn solid(&self, x: i32, y: i32) -> bool {
        self.get(x, y) == ROCK
    }

    /// A rock cell that touches water: the only rock sonar can ever see.
    pub fn is_edge(&self, x: i32, y: i32) -> bool {
        self.solid(x, y)
            && [(-1, 0), (1, 0), (0, -1), (0, 1)]
                .iter()
                .any(|(dx, dy)| Self::in_bounds(x + dx, y + dy) && !self.solid(x + dx, y + dy))
    }

    /// Centre of the main channel at a row: the guaranteed route to the floor.
    fn channel_x(seed: u64, y: i32) -> f64 {
        let wander = (noise(seed ^ 0xA5, 0.5, y as f64 / 38.0) - 0.5) * 2.0;
        let slow = (noise(seed ^ 0x5A, 0.5, y as f64 / 140.0) - 0.5) * 2.0;
        (W as f64 / 2.0 + wander * 22.0 + slow * 14.0).clamp(10.0, W as f64 - 11.0)
    }

    pub fn generate(seed: u64, missing_logs: &[u8]) -> World {
        let mut cells = vec![ROCK; (W * H) as usize];
        // 1. Noise caverns, rarer with depth.
        for y in 0..H {
            let depth = y as f64 / H as f64;
            let threshold = 0.56 + depth * 0.08;
            for x in 1..W - 1 {
                let n = noise(seed, x as f64 / 9.0, y as f64 / 5.5) * 0.7
                    + noise(seed ^ 0x77, x as f64 / 3.5, y as f64 / 2.5) * 0.3;
                if n > threshold || y < 6 {
                    cells[Self::idx(x, y)] = WATER;
                }
            }
        }
        // 2. Smooth into blobby caves.
        for _ in 0..2 {
            let prev = cells.clone();
            for y in 6..H - 1 {
                for x in 1..W - 1 {
                    let rock = (-1..=1)
                        .flat_map(|dy| (-1..=1).map(move |dx| (dx, dy)))
                        .filter(|&(dx, dy)| prev[Self::idx(x + dx, y + dy)] == ROCK)
                        .count();
                    cells[Self::idx(x, y)] = if rock >= 5 { ROCK } else { WATER };
                }
            }
        }
        // 3. The main channel, narrowing with depth.
        for y in 0..H - 2 {
            let cx = Self::channel_x(seed, y);
            let width = 11.0 - 6.0 * (y as f64 / H as f64) + (noise(seed ^ 0x33, 3.3, y as f64 / 12.0) - 0.5) * 4.0;
            let half = (width / 2.0).max(2.0);
            for x in (cx - half).floor() as i32..=(cx + half).ceil() as i32 {
                if x > 0 && x < W - 1 {
                    cells[Self::idx(x, y)] = WATER;
                }
            }
        }
        // 4. Walls and the floor basin.
        for y in 0..H {
            cells[Self::idx(0, y)] = ROCK;
            cells[Self::idx(W - 1, y)] = ROCK;
        }
        for x in 0..W {
            cells[Self::idx(x, H - 1)] = ROCK;
            cells[Self::idx(x, H - 2)] = ROCK;
        }
        let floor_cx = Self::channel_x(seed, H - 14) as i32;
        for y in H - 14..H - 2 {
            let half = 18 - (y - (H - 14)) / 2;
            for x in (floor_cx - half).max(1)..=(floor_cx + half).min(W - 2) {
                cells[Self::idx(x, y)] = WATER;
            }
        }
        let mut rng = Rng::new(seed ^ 0xF00D);
        // 5. A leviathan cavern in the Hadal zone.
        let lev_y = rng.range(700, 900);
        let lev_x = Self::channel_x(seed, lev_y) as i32;
        for y in lev_y - 7..=lev_y + 7 {
            for x in lev_x - 26..=lev_x + 26 {
                let (dx, dy) = ((x - lev_x) as f64 / 26.0, (y - lev_y) as f64 / 7.0);
                if dx * dx + dy * dy <= 1.0 && x > 0 && x < W - 1 {
                    cells[Self::idx(x, y)] = WATER;
                }
            }
        }
        // 6. Fill pockets that can't be reached from the surface.
        let mut seen = vec![false; cells.len()];
        let mut stack = vec![(W / 2, 1)];
        while let Some((x, y)) = stack.pop() {
            if !Self::in_bounds(x, y) || seen[Self::idx(x, y)] || cells[Self::idx(x, y)] == ROCK {
                continue;
            }
            seen[Self::idx(x, y)] = true;
            stack.extend([(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)]);
        }
        for (i, c) in cells.iter_mut().enumerate() {
            if *c == WATER && !seen[i] {
                *c = ROCK;
            }
        }
        let mut world = World { seed, cells, wrecks: vec![], spawns: vec![], singer: (floor_cx, H - 6) };
        world.place_vents(&mut rng);
        world.place_wrecks(&mut rng, missing_logs, floor_cx);
        world.place_creatures(&mut rng, (lev_x, lev_y));
        world
    }

    /// Water cell sitting on rock, with open water above (a seabed spot).
    fn seabed_spot(&self, rng: &mut Rng, top: i32, bottom: i32) -> Option<(i32, i32)> {
        for _ in 0..400 {
            let y = rng.range(top, bottom);
            let x = rng.range(4, W - 5);
            if self.get(x, y) == WATER
                && self.solid(x, y + 1)
                && (-2..=2).all(|dx| self.get(x + dx, y) == WATER && self.get(x + dx, y - 1) == WATER)
            {
                return Some((x, y));
            }
        }
        None
    }

    fn open_spot(&self, rng: &mut Rng, top: i32, bottom: i32) -> Option<(i32, i32)> {
        for _ in 0..200 {
            let (x, y) = (rng.range(2, W - 3), rng.range(top, bottom));
            if self.get(x, y) == WATER {
                return Some((x, y));
            }
        }
        None
    }

    fn place_vents(&mut self, rng: &mut Rng) {
        for _ in 0..14 {
            if let Some((x, y)) = self.seabed_spot(rng, 400, 640) {
                self.cells[Self::idx(x, y)] = VENT;
                if self.get(x + 1, y) == WATER && rng.chance(0.6) {
                    self.cells[Self::idx(x + 1, y)] = VENT;
                }
            }
        }
    }

    fn place_wrecks(&mut self, rng: &mut Rng, missing_logs: &[u8], floor_cx: i32) {
        // Ordinary wrecks, more and richer with depth.
        for (top, bottom, count) in [(22, 100, 3), (100, 400, 8), (400, 600, 6), (600, H - 20, 8)] {
            for _ in 0..count {
                if let Some((x, y)) = self.seabed_spot(rng, top, bottom) {
                    let deep = y as f64 / H as f64;
                    self.wrecks.push(Wreck {
                        x,
                        y,
                        half: rng.range(1, 3),
                        scrap: (8.0 + deep * 140.0 * (0.6 + rng.f64())) as u64,
                        battery: rng.chance(0.35),
                        patch: rng.chance(0.25),
                        relic: y > 300 && rng.chance(0.1 + deep * 0.15),
                        log: None,
                        beacon: rng.chance(0.4),
                        meridian: false,
                    });
                }
            }
        }
        // Each missing Meridian log lies in its own depth band.
        for &log in missing_logs {
            let top = 25 + log as i32 * 82;
            let bottom = (top + 80).min(H - 20);
            if let Some((x, y)) = self.seabed_spot(rng, top, bottom) {
                self.wrecks.push(Wreck {
                    x,
                    y,
                    half: 2,
                    scrap: 15,
                    battery: false,
                    patch: false,
                    relic: false,
                    log: Some(log),
                    beacon: true,
                    meridian: false,
                });
            }
        }
        self.wrecks.push(Wreck {
            x: floor_cx - 6,
            y: H - 3,
            half: 3,
            scrap: 0,
            battery: false,
            patch: false,
            relic: false,
            log: None,
            beacon: false,
            meridian: true,
        });
        let cells = &self.cells;
        self.wrecks.retain(|w| cells[Self::idx(w.x, w.y)] != ROCK);
    }

    fn place_creatures(&mut self, rng: &mut Rng, leviathan: (i32, i32)) {
        let table: [(Species, i32, i32, usize); 8] = [
            (Species::Glimmer, 15, 400, 40),
            (Species::Jelly, 25, 600, 26),
            (Species::Jelly, 600, H - 20, 10),
            (Species::Gulper, 110, H - 20, 22),
            (Species::Angler, 120, H - 20, 20),
            (Species::Eel, 160, 400, 8),
            (Species::Eel, 400, H - 20, 18),
            (Species::Glimmer, 400, H - 20, 16),
        ];
        for (sp, top, bottom, n) in table {
            for _ in 0..n {
                let spot = match sp {
                    Species::Gulper => self.seabed_spot(rng, top, bottom),
                    _ => self.open_spot(rng, top, bottom),
                };
                if let Some((x, y)) = spot {
                    self.spawns.push(Spawn { species: sp, x, y });
                }
            }
        }
        self.spawns.push(Spawn { species: Species::Leviathan, x: leviathan.0, y: leviathan.1 });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_and_connected_to_the_floor() {
        for seed in [1u64, 42, 2026] {
            let a = World::generate(seed, &[0, 5, 11]);
            let b = World::generate(seed, &[0, 5, 11]);
            assert!(
                a.cells == b.cells && a.wrecks == b.wrecks && a.spawns == b.spawns,
                "seed {seed} not deterministic"
            );

            // Flood from the surface must reach the floor basin.
            let mut seen = vec![false; a.cells.len()];
            let mut stack = vec![(W / 2, 1)];
            let mut deepest = 0;
            while let Some((x, y)) = stack.pop() {
                if !World::in_bounds(x, y) || seen[World::idx(x, y)] || a.solid(x, y) {
                    continue;
                }
                seen[World::idx(x, y)] = true;
                deepest = deepest.max(y);
                stack.extend([(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)]);
            }
            assert!(deepest >= H - 4, "seed {seed}: floor unreachable (deepest {deepest})");
            assert!(!a.solid(a.singer.0, a.singer.1));
            for w in &a.wrecks {
                assert!(seen[World::idx(w.x, w.y)], "seed {seed}: unreachable wreck at {},{}", w.x, w.y);
            }
            for s in &a.spawns {
                assert!(!a.solid(s.x, s.y));
            }
        }
    }

    #[test]
    fn logs_meridian_and_zones() {
        let w = World::generate(7, &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]);
        let logs: Vec<u8> = w.wrecks.iter().filter_map(|x| x.log).collect();
        assert!(logs.len() >= 10, "most log wrecks placed: {logs:?}");
        assert_eq!(w.wrecks.iter().filter(|x| x.meridian).count(), 1);
        assert!(w.spawns.iter().any(|s| s.species == Species::Leviathan));
        assert_eq!(zone_index(0), 0);
        assert_eq!(zone_index(150), 2);
        assert_eq!(zone_index(H - 3), 5);
        assert_eq!(depth_m(234), 2340);

        let none = World::generate(7, &[]);
        assert!(none.wrecks.iter().all(|x| x.log.is_none()));
    }

    #[test]
    fn noise_is_bounded() {
        for i in 0..1000 {
            let n = noise(3, i as f64 * 0.37, i as f64 * 0.11);
            assert!((0.0..=1.0).contains(&n));
        }
    }
}
