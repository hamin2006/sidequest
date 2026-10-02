//! One dive: the submersible in the dark, sonar and echoes, creatures that hunt by sound, salvage,
//! pressure and battery. Pure simulation plus rendering; progression lives in `progress.rs`.

use std::collections::VecDeque;
use std::sync::Arc;

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph, Wrap};
use serde::{Deserialize, Serialize};

use super::lore;
use super::progress::{Progress, Tech, Upgrade};
use super::world::{self, H, ROCK, Species, VENT, W, World, ZONES};
use crate::input::{Input, Key};
use crate::rng::Rng;
use crate::ui;

/// Aspect-corrected distance: terminal cells are about twice as tall as they are wide.
pub fn dist(ax: i32, ay: i32, bx: i32, by: i32) -> f64 {
    let dx = (ax - bx) as f64 * 0.5;
    let dy = (ay - by) as f64;
    (dx * dx + dy * dy).sqrt()
}

/// True if nothing solid lies strictly between the two cells.
pub fn line_clear(world: &World, x0: i32, y0: i32, x1: i32, y1: i32) -> bool {
    let (dx, dy) = ((x1 - x0).abs(), -(y1 - y0).abs());
    let (sx, sy) = (if x0 < x1 { 1 } else { -1 }, if y0 < y1 { 1 } else { -1 });
    let (mut x, mut y, mut err) = (x0, y0, dx + dy);
    loop {
        if (x, y) == (x1, y1) {
            return true;
        }
        if (x, y) != (x0, y0) && world.solid(x, y) {
            return false;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
}

/// Cells in line of sight within `range`, sorted by distance. Only water and visible rock faces.
pub fn field_of_view(world: &World, ox: i32, oy: i32, range: f64) -> Vec<(usize, f32)> {
    let r = range.ceil() as i32;
    let mut out = vec![];
    for y in (oy - r).max(0)..=(oy + r).min(H - 1) {
        for x in (ox - 2 * r).max(0)..=(ox + 2 * r).min(W - 1) {
            let d = dist(ox, oy, x, y);
            if d > range {
                continue;
            }
            let rock = world.solid(x, y);
            if rock && !world.is_edge(x, y) {
                continue;
            }
            if line_clear(world, ox, oy, x, y) {
                out.push((World::idx(x, y), d as f32));
            }
        }
    }
    out.sort_by(|a, b| a.1.total_cmp(&b.1));
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Outcome {
    Surfaced,
    Lost,
    Ending(super::progress::Ending),
}

/// What happened this frame that the progression layer needs to know about.
#[derive(Debug, Clone, PartialEq)]
pub enum DiveEvent {
    Echo(Species),
    Log(u8),
    Relay(usize),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sub {
    pub x: i32,
    pub y: i32,
    pub facing: i32,
    pub hull: f64,
    pub battery: f64,
    pub lamp: bool,
    pub decoys: u8,
    move_cd: f64,
    salvage: Option<(usize, f64)>,
    #[serde(skip)]
    autopilot: Option<VecDeque<(i32, i32)>>,
    creak: f64,
    pub hurt: f64,
    harpoon_cd: f64,
    moved: f64,
    left_surface: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum CState {
    Idle,
    Hunt,
    Search,
    Retreat,
    Lunge,
    Flee,
    Asleep,
    Awake,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Creature {
    pub sp: Species,
    pub x: i32,
    pub y: i32,
    home: (i32, i32),
    state: CState,
    target: (i32, i32),
    timer: f64,
    move_acc: f64,
    cd: f64,
    pub alive: bool,
    hp: i32,
    disturbance: f64,
    pub flare: f64,
    facing: i32,
}

impl Creature {
    fn new(sp: Species, x: i32, y: i32, rng: &mut Rng) -> Self {
        let hp = match sp {
            Species::Glimmer | Species::Jelly => 1,
            Species::Eel => 2,
            Species::Gulper | Species::Angler => 3,
            Species::Leviathan => 999,
        };
        Self {
            sp,
            x,
            y,
            home: (x, y),
            state: if sp == Species::Leviathan { CState::Asleep } else { CState::Idle },
            target: (x, y),
            timer: rng.f64() * 3.0,
            move_acc: 0.0,
            cd: 0.0,
            alive: true,
            hp,
            disturbance: 0.0,
            flare: 0.0,
            facing: if rng.chance(0.5) { 1 } else { -1 },
        }
    }

    pub fn glyph(&self) -> &'static str {
        match self.sp {
            Species::Glimmer => "·",
            Species::Jelly => "Ω",
            Species::Gulper => "Θ",
            Species::Angler => "Ψ",
            Species::Eel => "ξ",
            Species::Leviathan => "█",
        }
    }

    pub fn color(&self) -> Color {
        match self.sp {
            Species::Glimmer => Color::Rgb(120, 255, 210),
            Species::Jelly => Color::Rgb(200, 130, 255),
            Species::Gulper => Color::Rgb(220, 90, 70),
            Species::Angler => Color::Rgb(170, 120, 90),
            Species::Eel => Color::Rgb(210, 230, 120),
            Species::Leviathan => Color::Rgb(150, 30, 40),
        }
    }

    pub fn lure(&self) -> Option<(i32, i32)> {
        (self.sp == Species::Angler && self.alive).then_some((self.x + self.facing * 2, self.y))
    }

    /// Leviathan body: 9 wide, 3 tall, centred on (x, y).
    fn covers(&self, x: i32, y: i32) -> bool {
        if self.sp == Species::Leviathan {
            (x - self.x).abs() <= 4 && (y - self.y).abs() <= 1
        } else {
            (x, y) == (self.x, self.y)
        }
    }

    /// Making noise the hydrophone can pick up?
    fn audible(&self, song_analysis: bool) -> bool {
        match self.sp {
            Species::Eel => matches!(self.state, CState::Hunt | CState::Search | CState::Retreat),
            Species::Leviathan => self.state == CState::Awake || song_analysis,
            Species::Gulper | Species::Angler => self.state == CState::Lunge,
            _ => false,
        }
    }
}

#[derive(Debug, Clone)]
struct Ping {
    t0: f64,
    x: i32,
    y: i32,
    cells: Vec<(usize, f32)>,
    next: usize,
    creatures: Vec<(usize, f32, bool)>,
}

#[derive(Debug, Clone)]
struct Ghost {
    x: i32,
    y: i32,
    glyph: &'static str,
    color: Color,
    t: f64,
    wide: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decoy {
    pub x: i32,
    pub y: i32,
    pub left: f64,
    pulse: f64,
}

#[derive(Debug, Clone)]
struct Bolt {
    x: f64,
    y: i32,
    dir: i32,
    left: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Popup {
    Log(u8),
    Floor,
    Choice,
    Help,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dive {
    pub seed: u64,
    pub missing_logs: Vec<u8>,
    pub looted: Vec<bool>,
    charted: Vec<u64>,
    pub sub: Sub,
    pub creatures: Vec<Creature>,
    pub decoys: Vec<Decoy>,
    pub time: f64,
    pub cargo_scrap: u64,
    pub cargo_relics: u32,
    pub logs_found: Vec<u8>,
    pub max_row: i32,
    pub outcome: Option<Outcome>,
    pub popup: Option<Popup>,
    rng: Rng,
    zone: usize,
    floor_seen: bool,
    pub show_map: bool,
    #[serde(skip)]
    world: Option<Arc<World>>,
    #[serde(skip)]
    echo: Vec<f32>,
    #[serde(skip)]
    pings: Vec<Ping>,
    #[serde(skip)]
    ghosts: Vec<Ghost>,
    #[serde(skip)]
    bolts: Vec<Bolt>,
    #[serde(skip)]
    msgs: VecDeque<(String, Color, f64)>,
    #[serde(skip)]
    pub events: Vec<DiveEvent>,
}

const NEVER: f32 = f32::NEG_INFINITY;
const LAMP_R: f64 = 2.6;
const BEACON_RANGE: f64 = 46.0;

impl Dive {
    pub fn new(seed: u64, progress: &Progress, start_zone: usize) -> Self {
        let missing = progress.missing_logs();
        let world = World::generate(seed, &missing);
        let mut rng = Rng::new(seed ^ 0xD1CE);
        let start_row = if start_zone == 0 { 3 } else { ZONES[start_zone].top + 2 };
        let (sx, sy) = (start_row..H - 2)
            .find_map(|y| {
                (0..W / 2)
                    .flat_map(|d| [W / 2 + d, W / 2 - d])
                    .find(|&x| !world.solid(x, y) && world.get(x, y) != VENT)
                    .map(|x| (x, y))
            })
            .unwrap_or((W / 2, 3));
        let creatures = world
            .spawns
            .iter()
            .filter(|s| dist(s.x, s.y, sx, sy) > 14.0)
            .map(|s| Creature::new(s.species, s.x, s.y, &mut rng))
            .collect();
        let n = world.wrecks.len();
        let mut d = Self {
            seed,
            missing_logs: missing,
            looted: vec![false; n],
            charted: vec![0; (W * H) as usize / 64 + 1],
            sub: Sub {
                x: sx,
                y: sy,
                facing: 1,
                hull: progress.max_hull(),
                battery: progress.max_battery(),
                lamp: true,
                decoys: progress.decoys(),
                move_cd: 0.0,
                salvage: None,
                autopilot: None,
                creak: 0.0,
                hurt: 0.0,
                harpoon_cd: 0.0,
                moved: 0.0,
                left_surface: start_zone > 0,
            },
            creatures,
            decoys: vec![],
            time: 0.0,
            cargo_scrap: 0,
            cargo_relics: 0,
            logs_found: vec![],
            max_row: sy,
            outcome: None,
            popup: None,
            rng,
            zone: world::zone_index(sy),
            floor_seen: false,
            show_map: false,
            world: None,
            echo: vec![],
            pings: vec![],
            ghosts: vec![],
            bolts: vec![],
            msgs: VecDeque::new(),
            events: vec![],
        };
        d.world = Some(Arc::new(world));
        d.echo = vec![NEVER; (W * H) as usize];
        let tip = lore::TIPS[d.rng.below(lore::TIPS.len())];
        d.say(format!("Tip: {tip}"), ui::DIM);
        d
    }

    /// Rebuilds the world and transient state after loading a save.
    pub fn rebuild(&mut self) {
        let world = World::generate(self.seed, &self.missing_logs);
        if self.looted.len() != world.wrecks.len() {
            self.looted = vec![false; world.wrecks.len()];
        }
        self.world = Some(Arc::new(world));
        self.echo = vec![NEVER; (W * H) as usize];
        self.say("Systems restored. Sonar memory cleared.", ui::DIM);
    }

    fn world(&self) -> Arc<World> {
        self.world.clone().expect("dive world not built")
    }

    pub fn say(&mut self, text: impl Into<String>, color: Color) {
        self.msgs.push_back((text.into(), color, self.time));
        while self.msgs.len() > 4 {
            self.msgs.pop_front();
        }
    }

    fn chart(&mut self, i: usize) {
        self.charted[i / 64] |= 1 << (i % 64);
    }

    pub fn is_charted(&self, i: usize) -> bool {
        self.charted.get(i / 64).is_some_and(|w| w & (1 << (i % 64)) != 0)
    }

    pub fn depth_m(&self) -> i32 {
        world::depth_m(self.sub.y)
    }

    fn hurt(&mut self, amount: f64, why: &str) {
        self.sub.hull -= amount;
        self.sub.hurt = 0.35;
        self.sub.salvage = None;
        self.sub.autopilot = None;
        self.say(why.to_string(), ui::BAD);
    }

    // ---------- Noise ----------

    /// Something made a sound: everything that hunts by ear reacts.
    fn noise(&mut self, x: i32, y: i32, radius: f64, loudness: f64) {
        for c in self.creatures.iter_mut().filter(|c| c.alive) {
            let d = dist(c.x, c.y, x, y);
            match c.sp {
                Species::Eel if d <= radius => {
                    if c.state != CState::Retreat {
                        c.state = CState::Hunt;
                        c.target = (x, y);
                        c.timer = 12.0;
                    }
                }
                Species::Glimmer if d <= radius.min(12.0) => {
                    c.state = CState::Flee;
                    let away = if c.x >= x { 1 } else { -1 };
                    c.target = (c.x + away * 10, c.y + if c.y >= y { 4 } else { -4 });
                    c.timer = 3.0;
                }
                Species::Leviathan if d <= 40.0 => {
                    c.disturbance += loudness;
                    if c.state == CState::Awake {
                        c.timer = 25.0;
                    }
                }
                _ => {}
            }
        }
    }

    // ---------- Player actions ----------

    fn ping(&mut self, progress: &Progress) {
        let cost = progress.ping_cost();
        if self.sub.battery < cost {
            self.say("Not enough battery to ping.", ui::WARN);
            return;
        }
        self.sub.battery -= cost;
        let world = self.world();
        let range = progress.sonar_range();
        let cells = field_of_view(&world, self.sub.x, self.sub.y, range);
        let mut visible = vec![false; 0];
        visible.resize((W * H) as usize, false);
        for &(i, _) in &cells {
            visible[i] = true;
        }
        let creatures = self
            .creatures
            .iter()
            .enumerate()
            .filter(|(_, c)| c.alive)
            .filter_map(|(i, c)| {
                let d = dist(self.sub.x, self.sub.y, c.x, c.y);
                let seen = if c.sp == Species::Leviathan {
                    (-4..=4).any(|dx| World::in_bounds(c.x + dx, c.y) && visible[World::idx(c.x + dx, c.y)])
                } else {
                    World::in_bounds(c.x, c.y) && visible[World::idx(c.x, c.y)]
                };
                (seen && d <= range + 4.0).then_some((i, d as f32, false))
            })
            .collect();
        self.pings.push(Ping { t0: self.time, x: self.sub.x, y: self.sub.y, cells, next: 0, creatures });
        let n = progress.noise_mult();
        self.noise(self.sub.x, self.sub.y, range * 1.3 * n, 35.0 * n);
    }

    fn drop_decoy(&mut self) {
        if self.sub.decoys == 0 {
            self.say("No decoys left.", ui::WARN);
            return;
        }
        self.sub.decoys -= 1;
        self.decoys.push(Decoy { x: self.sub.x, y: self.sub.y, left: 8.0, pulse: 0.0 });
        self.say("Decoy away. It's singing for eight seconds.", ui::ACCENT);
    }

    fn fire_harpoon(&mut self, progress: &Progress) {
        if progress.level(Upgrade::Harpoon) == 0 {
            self.say("No harpoon fitted (workshop upgrade).", ui::DIM);
            return;
        }
        if self.sub.harpoon_cd > 0.0 || self.sub.battery < 4.0 {
            return;
        }
        self.sub.harpoon_cd = 1.8;
        self.sub.battery -= 4.0;
        self.bolts.push(Bolt { x: self.sub.x as f64, y: self.sub.y, dir: self.sub.facing, left: 16.0 });
        self.noise(self.sub.x, self.sub.y, 10.0 * progress.noise_mult(), 10.0);
    }

    fn wreck_here(&self) -> Option<usize> {
        let world = self.world();
        // Nearest wreck in reach, preferring ones not yet stripped.
        world
            .wrecks
            .iter()
            .enumerate()
            .filter(|(_, w)| dist(w.x, w.y, self.sub.x, self.sub.y) <= 1.6)
            .min_by(|(i, a), (j, b)| {
                (self.looted[*i], dist(a.x, a.y, self.sub.x, self.sub.y))
                    .partial_cmp(&(self.looted[*j], dist(b.x, b.y, self.sub.x, self.sub.y)))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(i, _)| i)
    }

    fn start_salvage(&mut self) {
        match self.wreck_here() {
            Some(i) if self.looted[i] => self.say("Already stripped.", ui::DIM),
            Some(i) => {
                self.sub.salvage = Some((i, 0.0));
                self.say("Salvaging… hold still.", ui::ACCENT);
            }
            None => self.say("Nothing to salvage here. Find a wreck hatch ▣.", ui::DIM),
        }
    }

    fn finish_salvage(&mut self, i: usize, progress: &Progress) {
        let world = self.world();
        let w = &world.wrecks[i];
        self.looted[i] = true;
        if w.meridian {
            self.popup = Some(Popup::Choice);
            return;
        }
        let mut found = vec![];
        if w.scrap > 0 {
            self.cargo_scrap += w.scrap;
            found.push(format!("{} scrap", w.scrap));
        }
        if w.battery {
            self.sub.battery = (self.sub.battery + 40.0).min(progress.max_battery());
            found.push("a battery cell".into());
        }
        if w.patch {
            self.sub.hull = (self.sub.hull + 25.0).min(progress.max_hull());
            found.push("a hull patch".into());
        }
        if w.relic {
            self.cargo_relics += 1;
            found.push("a RELIC".into());
        }
        if let Some(l) = w.log {
            self.logs_found.push(l);
            self.events.push(DiveEvent::Log(l));
            self.popup = Some(Popup::Log(l));
            found.push(format!("Meridian log {}", l + 1));
        }
        self.say(format!("Recovered {}.", found.join(", ")), ui::GOOD);
    }

    fn plan_ascent(&mut self) {
        let world = self.world();
        let start = (self.sub.x, self.sub.y);
        let mut prev = vec![u32::MAX; (W * H) as usize];
        let mut q = VecDeque::from([start]);
        prev[World::idx(start.0, start.1)] = World::idx(start.0, start.1) as u32;
        let mut goal = None;
        while let Some((x, y)) = q.pop_front() {
            if y <= 1 {
                goal = Some((x, y));
                break;
            }
            for (dx, dy) in [(0, -1), (-1, 0), (1, 0), (0, 1), (-1, -1), (1, -1)] {
                let (nx, ny) = (x + dx, y + dy);
                if World::in_bounds(nx, ny)
                    && !world.solid(nx, ny)
                    && world.get(nx, ny) != VENT
                    && prev[World::idx(nx, ny)] == u32::MAX
                {
                    prev[World::idx(nx, ny)] = World::idx(x, y) as u32;
                    q.push_back((nx, ny));
                }
            }
        }
        let Some(mut cur) = goal else {
            self.say("No route to the surface found.", ui::BAD);
            return;
        };
        let mut path = VecDeque::new();
        while cur != start {
            path.push_front(cur);
            let p = prev[World::idx(cur.0, cur.1)] as i32;
            cur = (p % W, p / W);
        }
        self.sub.autopilot = Some(path);
        self.say("Autopilot: ascending along the known route. Any key takes back control.", ui::ACCENT);
    }

    fn try_move(&mut self, dx: i32, dy: i32, boost: bool, progress: &Progress) -> bool {
        let world = self.world();
        if dx != 0 {
            self.sub.facing = dx;
        }
        let (nx, ny) = (self.sub.x + dx, (self.sub.y + dy).max(0));
        if world.solid(nx, ny) {
            if boost {
                self.hurt(2.0, "You scrape the rock.");
                self.noise(self.sub.x, self.sub.y, 8.0 * progress.noise_mult(), 6.0);
            }
            return false;
        }
        self.sub.x = nx;
        self.sub.y = ny;
        self.sub.moved = 0.3;
        self.sub.salvage = None;
        let n = progress.noise_mult();
        if boost {
            self.noise(nx, ny, 10.0 * n, 4.0);
        } else {
            self.noise(nx, ny, 2.5 * n, 1.0);
        }
        true
    }

    // ---------- Update ----------

    pub fn update(&mut self, dt: f64, input: &Input, progress: &Progress) {
        if self.outcome.is_some() {
            return;
        }
        if let Some(p) = self.popup {
            self.update_popup(p, input);
            return;
        }
        if input.was(Key::Tab) || input.char_pressed('m') {
            self.show_map = !self.show_map;
        }
        if self.show_map {
            if input.was(Key::Esc) {
                self.show_map = false;
            }
            return;
        }
        let dt = dt.min(0.1);
        self.time += dt;
        let world = self.world();

        // Input.
        for k in &input.pressed {
            match k {
                Key::Space => self.ping(progress),
                Key::Char('f') => {
                    self.sub.lamp = !self.sub.lamp;
                    self.say(if self.sub.lamp { "Lamp on." } else { "Lamp off. Running dark." }, ui::DIM);
                }
                Key::Char('q') => self.drop_decoy(),
                Key::Char('e') => self.start_salvage(),
                Key::Char('h') => self.fire_harpoon(progress),
                Key::Char('?') => self.popup = Some(Popup::Help),
                Key::Char('r') => {
                    if self.sub.autopilot.is_some() {
                        self.sub.autopilot = None;
                    } else {
                        self.plan_ascent();
                    }
                }
                _ => {}
            }
            if *k != Key::Char('r') && self.sub.autopilot.is_some() && crate::input::dir_of(*k).is_some() {
                self.sub.autopilot = None;
                self.say("Manual control.", ui::DIM);
            }
        }

        // Movement.
        let empty = self.sub.battery <= 0.0;
        self.sub.move_cd -= dt;
        self.sub.moved -= dt;
        let boost = input.shift && !empty;
        let mut dir = if input.held_reliable { input.dir_held() } else { (0, 0) };
        if let Some(d) = input.dir_pressed() {
            dir = d;
            if !input.held_reliable || self.sub.move_cd > 0.05 {
                self.sub.move_cd = 0.0;
            }
        }
        if empty && dir.1 >= 0 {
            dir = (0, 0);
        }
        if let Some(path) = &mut self.sub.autopilot {
            if self.sub.move_cd <= 0.0 {
                if let Some((nx, ny)) = path.pop_front() {
                    let (dx, dy) = (nx - self.sub.x, ny - self.sub.y);
                    if !self.try_move(dx, dy, false, progress) {
                        self.sub.autopilot = None;
                    }
                    self.sub.move_cd = 1.0 / 8.0;
                } else {
                    self.sub.autopilot = None;
                }
            }
        } else if dir != (0, 0) && self.sub.move_cd <= 0.0 {
            self.try_move(dir.0, dir.1, boost, progress);
            let base = if dir.1 == 0 { 1.0 / 13.0 } else { 1.0 / 7.0 };
            self.sub.move_cd = if boost { base * 0.6 } else { base };
        }
        if empty {
            // Emergency ballast: drift upward.
            if self.rng.chance(dt * 1.5) && !world.solid(self.sub.x, self.sub.y - 1) {
                self.sub.y -= 1;
            }
            self.sub.lamp = false;
        }
        if self.sub.y > 8 {
            self.sub.left_surface = true;
        }

        // Battery, pressure, vents.
        let mut drain = 0.18;
        if self.sub.moved > 0.0 {
            drain += 0.25;
        }
        if self.sub.lamp {
            drain += 0.25;
        }
        if boost && self.sub.moved > 0.0 {
            drain += 0.6;
        }
        let near_vent = (-1..=1).any(|dx| (-1..=1).any(|dy| world.get(self.sub.x + dx, self.sub.y + dy) == VENT));
        if world.get(self.sub.x, self.sub.y) == VENT {
            self.sub.hull -= 6.0 * dt;
            self.sub.hurt = self.sub.hurt.max(0.1);
            if self.rng.chance(dt * 0.5) {
                self.say("Vent heat is cooking the hull!", ui::BAD);
            }
        } else if near_vent {
            drain -= 2.5;
        }
        self.sub.battery = (self.sub.battery - drain * dt).clamp(0.0, progress.max_battery());
        let rated_row = progress.rated_m() / world::ROW_M;
        if self.sub.y > rated_row {
            let excess = (self.sub.y - rated_row) as f64;
            let mut rate = 0.6 + excess / 25.0;
            if progress.has(Tech::PressureModel) {
                rate *= 0.5;
            }
            self.sub.hull -= rate * dt;
            self.sub.creak -= dt;
            if self.sub.creak <= 0.0 {
                self.sub.creak = 6.0;
                self.say(format!("The hull groans: {} m below its rating.", excess as i32 * world::ROW_M), ui::BAD);
            }
        }
        self.sub.hurt = (self.sub.hurt - dt).max(0.0);
        self.sub.harpoon_cd = (self.sub.harpoon_cd - dt).max(0.0);

        // Salvage progress.
        if let Some((i, p)) = self.sub.salvage {
            let p = p + dt / 2.0;
            if p >= 1.0 {
                self.sub.salvage = None;
                self.finish_salvage(i, progress);
            } else {
                self.sub.salvage = Some((i, p));
            }
        }

        // Pings sweep outward.
        let fade = progress.echo_fade() as f32;
        let mut pings = std::mem::take(&mut self.pings);
        for p in &mut pings {
            let r = ((self.time - p.t0) * 45.0) as f32;
            while p.next < p.cells.len() && p.cells[p.next].1 <= r {
                let (i, _) = p.cells[p.next];
                self.echo[i] = self.time as f32;
                let (x, y) = (i as i32 % W, i as i32 / W);
                if world.solid(x, y) || world.wrecks.iter().any(|w| w.x == x && w.y == y) {
                    self.chart(i);
                }
                p.next += 1;
            }
            for entry in p.creatures.iter_mut() {
                if entry.2 || entry.1 > r {
                    continue;
                }
                entry.2 = true;
                let c = &mut self.creatures[entry.0];
                if !c.alive {
                    continue;
                }
                if c.sp == Species::Jelly {
                    c.flare = 2.5;
                }
                self.ghosts.push(Ghost {
                    x: c.x,
                    y: c.y,
                    glyph: c.glyph(),
                    color: c.color(),
                    t: self.time,
                    wide: c.sp == Species::Leviathan,
                });
                self.events.push(DiveEvent::Echo(c.sp));
            }
        }
        pings.retain(|p| p.next < p.cells.len() || p.creatures.iter().any(|c| !c.2));
        self.pings = pings;
        let _ = fade;
        self.ghosts.retain(|g| self.time - g.t < 4.5);

        // Decoys and harpoon bolts.
        let mut decoy_noises = vec![];
        for d in &mut self.decoys {
            d.left -= dt;
            d.pulse -= dt;
            if d.pulse <= 0.0 {
                d.pulse = 0.5;
                decoy_noises.push((d.x, d.y));
            }
        }
        self.decoys.retain(|d| d.left > 0.0);
        for (x, y) in decoy_noises {
            self.noise(x, y, 32.0, 4.0);
        }
        let mut bolts = std::mem::take(&mut self.bolts);
        for b in &mut bolts {
            let step = 30.0 * dt;
            b.x += b.dir as f64 * step;
            b.left -= step;
            let bx = b.x.round() as i32;
            if world.solid(bx, b.y) {
                b.left = 0.0;
                continue;
            }
            if let Some(c) =
                self.creatures.iter_mut().find(|c| c.alive && c.sp != Species::Leviathan && c.covers(bx, b.y))
            {
                c.hp -= progress.level(Upgrade::Harpoon) as i32;
                b.left = 0.0;
                if c.hp <= 0 {
                    c.alive = false;
                    self.ghosts
                        .push(Ghost { x: c.x, y: c.y, glyph: "✕", color: ui::WARN, t: self.time, wide: false });
                }
            }
        }
        bolts.retain(|b| b.left > 0.0);
        self.bolts = bolts;

        self.update_creatures(dt, &world);

        // Zones, relays, the floor.
        let z = world::zone_index(self.sub.y);
        if z > self.zone {
            for (zi, zone) in ZONES.iter().enumerate().take(z + 1).skip(self.zone + 1) {
                self.events.push(DiveEvent::Relay(zi));
                if zi < ZONES.len() - 1 {
                    self.say(
                        format!(
                            "Entering the {} zone. Relay buoy deployed at {} m.",
                            zone.name,
                            world::depth_m(zone.top)
                        ),
                        ui::ACCENT,
                    );
                }
            }
        }
        self.zone = self.zone.max(z);
        self.max_row = self.max_row.max(self.sub.y);
        if self.sub.y >= ZONES[5].top && !self.floor_seen {
            self.floor_seen = true;
            self.popup = Some(Popup::Floor);
        }

        // Outcomes.
        if self.sub.hull <= 0.0 {
            self.sub.hull = 0.0;
            self.outcome = Some(Outcome::Lost);
        } else if self.sub.y <= 1 && self.sub.left_surface {
            self.outcome = Some(Outcome::Surfaced);
        }
        while self.msgs.front().is_some_and(|m| self.time - m.2 > 9.0) {
            self.msgs.pop_front();
        }
    }

    fn update_popup(&mut self, p: Popup, input: &Input) {
        match p {
            Popup::Choice => {
                if input.char_pressed('1') {
                    self.popup = None;
                    self.outcome = Some(Outcome::Ending(super::progress::Ending::Answered));
                } else if input.char_pressed('2') {
                    self.popup = None;
                    self.outcome = Some(Outcome::Ending(super::progress::Ending::WentDark));
                }
            }
            _ => {
                if input.was(Key::Enter) || input.was(Key::Esc) || input.was(Key::Space) {
                    self.popup = None;
                }
            }
        }
    }

    fn update_creatures(&mut self, dt: f64, world: &World) {
        let (sx, sy) = (self.sub.x, self.sub.y);
        let lamp = self.sub.lamp;
        let mut damage: Vec<(f64, f64, &'static str)> = vec![];
        let mut woke = false;
        for c in self.creatures.iter_mut().filter(|c| c.alive) {
            if (c.y - sy).abs() > 90 {
                continue; // far away: frozen until you get close
            }
            c.cd = (c.cd - dt).max(0.0);
            c.timer -= dt;
            c.flare = (c.flare - dt).max(0.0);
            let d = dist(c.x, c.y, sx, sy);
            let speed: f64;
            match c.sp {
                Species::Glimmer => {
                    if c.state == CState::Flee && c.timer > 0.0 {
                        speed = 6.0;
                    } else {
                        c.state = CState::Idle;
                        if c.timer <= 0.0 {
                            c.timer = 1.0 + self.rng.f64() * 2.5;
                            c.target = (c.home.0 + self.rng.range(-6, 6), c.home.1 + self.rng.range(-3, 3));
                        }
                        speed = 1.5;
                    }
                }
                Species::Jelly => {
                    if c.timer <= 0.0 {
                        c.timer = 3.0 + self.rng.f64() * 3.0;
                        c.target = (c.x + self.rng.range(-4, 4), c.y + self.rng.range(-2, 2));
                    }
                    speed = 0.8;
                    if d <= 1.2 && c.cd <= 0.0 {
                        c.cd = 2.5;
                        c.flare = 2.0;
                        damage.push((8.0, 15.0, "A jelly stings the hull. Systems drained!"));
                    }
                }
                Species::Gulper => {
                    speed = 0.0;
                    if d <= 1.2 && c.cd <= 0.0 {
                        c.cd = 3.0;
                        damage.push((18.0, 0.0, "Something enormous snaps at you from the wall!"));
                    }
                }
                Species::Angler => {
                    let lure_lit = lamp && d <= 6.0;
                    if c.state == CState::Lunge {
                        speed = 14.0;
                        c.target = (sx, sy);
                        if d <= 1.3 && c.cd <= 0.0 {
                            c.cd = 4.0;
                            damage.push((15.0, 0.0, "The 'beacon' had teeth. Angler bite!"));
                            c.state = CState::Retreat;
                            c.timer = 2.0;
                        } else if c.timer <= 0.0 {
                            c.state = CState::Retreat;
                            c.timer = 2.0;
                        }
                    } else if c.state == CState::Retreat {
                        speed = 4.0;
                        c.target = c.home;
                        if c.timer <= 0.0 {
                            c.state = CState::Idle;
                        }
                    } else {
                        speed = 0.0;
                        if (d <= 4.0 || lure_lit && d <= 5.0) && c.cd <= 0.0 {
                            c.state = CState::Lunge;
                            c.timer = 0.8;
                        }
                    }
                }
                Species::Eel => {
                    match c.state {
                        CState::Hunt => {
                            speed = 9.0;
                            if dist(c.x, c.y, c.target.0, c.target.1) <= 1.0 || c.timer <= 0.0 {
                                c.state = CState::Search;
                                c.timer = 4.0;
                            }
                        }
                        CState::Search => {
                            speed = 4.0;
                            if dist(c.x, c.y, c.target.0, c.target.1) <= 1.0 {
                                c.target = (c.target.0 + self.rng.range(-6, 6), c.target.1 + self.rng.range(-3, 3));
                            }
                            if c.timer <= 0.0 {
                                c.state = CState::Idle;
                                c.target = c.home;
                            }
                        }
                        CState::Retreat => {
                            speed = 8.0;
                            if c.timer <= 0.0 {
                                c.state = CState::Search;
                                c.target = (sx, sy);
                                c.timer = 4.0;
                            }
                        }
                        _ => {
                            speed = 2.5;
                            if c.timer <= 0.0 {
                                c.timer = 2.0 + self.rng.f64() * 3.0;
                                c.target = (c.home.0 + self.rng.range(-14, 14), c.home.1 + self.rng.range(-6, 6));
                            }
                        }
                    }
                    if matches!(c.state, CState::Hunt | CState::Search) && d <= 1.5 && c.cd <= 0.0 {
                        c.cd = 2.5;
                        damage.push((10.0, 0.0, "Hunter eel! It bites and darts away."));
                        c.state = CState::Retreat;
                        c.timer = 1.5;
                        let away = if c.x >= sx { 1 } else { -1 };
                        c.target = (c.x + away * 12, c.y + self.rng.range(-4, 4));
                    }
                }
                Species::Leviathan => {
                    c.disturbance = (c.disturbance - 3.0 * dt).max(0.0);
                    match c.state {
                        CState::Asleep => {
                            speed = 0.0;
                            if c.disturbance >= 100.0 {
                                c.state = CState::Awake;
                                c.timer = 25.0;
                                woke = true;
                            }
                        }
                        _ => {
                            speed = 3.5;
                            c.target = (sx, sy);
                            if c.timer <= 0.0 {
                                c.state = CState::Asleep;
                                c.disturbance = 40.0;
                            }
                            if (sx - c.x).abs() <= 5 && (sy - c.y).abs() <= 2 && c.cd <= 0.0 {
                                c.cd = 3.0;
                                damage.push((40.0, 0.0, "The leviathan rolls over you. The hull buckles!"));
                            }
                        }
                    }
                }
            }
            // Step toward the target.
            if speed > 0.0 {
                c.move_acc += dt * speed;
                while c.move_acc >= 1.0 {
                    c.move_acc -= 1.0;
                    let (tx, ty) = c.target;
                    let (dx, dy) = ((tx - c.x).signum(), (ty - c.y).signum());
                    if dx == 0 && dy == 0 {
                        break;
                    }
                    if dx != 0 {
                        c.facing = dx;
                    }
                    let passes = |x: i32, y: i32| {
                        c.sp == Species::Leviathan && World::in_bounds(x, y)
                            || !world.solid(x, y) && world.get(x, y) != VENT
                    };
                    // Horizontal moves are half as far visually, so take two.
                    let tries = [(dx, dy), (dx, 0), (0, dy)];
                    if let Some(&(mx, my)) =
                        tries.iter().find(|&&(mx, my)| (mx, my) != (0, 0) && passes(c.x + mx, c.y + my))
                    {
                        c.x += mx;
                        c.y += my;
                        if mx != 0 && my == 0 && passes(c.x + mx, c.y) && (tx - c.x).abs() > 0 {
                            c.x += mx;
                        }
                    } else {
                        // Stuck: pick a new wander target.
                        c.target = (c.x + self.rng.range(-5, 5), c.y + self.rng.range(-3, 3));
                        break;
                    }
                }
            }
        }
        if woke {
            self.say("The hull shudders. Something enormous is moving. GO DARK.", ui::BAD);
        }
        for (hull, batt, why) in damage {
            if batt > 0.0 {
                self.sub.battery = (self.sub.battery - batt).max(0.0);
            }
            self.hurt(hull, why);
        }
    }

    // ---------- Rendering ----------

    pub fn draw(&self, f: &mut Frame, area: Rect, progress: &Progress) {
        if area.width < 40 || area.height < 12 {
            ui::too_small(f, area, 40, 12);
            return;
        }
        let hud = Rect { height: 1, ..area };
        let foot = Rect { y: area.y + area.height - 2, height: 2, ..area };
        let view = Rect { y: area.y + 1, height: area.height - 3, ..area };
        self.draw_view(f.buffer_mut(), view, progress);
        self.draw_hud(f, hud, progress);
        self.draw_footer(f, foot);
        if self.show_map {
            self.draw_map(f, view);
        }
        if let Some(p) = self.popup {
            self.draw_popup(f, view, p);
        }
    }

    fn camera(&self, view: Rect) -> (i32, i32) {
        let vw = view.width as i32;
        let vh = view.height as i32;
        let cx = if vw >= W { -(vw - W) / 2 } else { (self.sub.x - vw / 2).clamp(0, W - vw) };
        let cy = (self.sub.y - vh / 2).clamp(0, (H - vh).max(0));
        (cx, cy)
    }

    fn draw_view(&self, buf: &mut Buffer, view: Rect, progress: &Progress) {
        let world = self.world();
        let (cx, cy) = self.camera(view);
        let t = self.time;
        let fade = progress.echo_fade();
        let lamp_r = if self.sub.lamp { LAMP_R + 0.5 * progress.level(Upgrade::Sonar).min(1) as f64 } else { 0.0 };
        let lure_color =
            if progress.has(Tech::LureFilter) { Color::Rgb(255, 80, 80) } else { Color::Rgb(255, 190, 80) };
        let sx = self.sub.x;
        let sy = self.sub.y;
        let lit = |x: i32, y: i32| -> bool {
            let d = dist(x, y, sx, sy);
            // Light depends on the depth of the cell being lit, not of the sub.
            let light = lamp_r.max(ZONES[world::zone_index(y)].ambient as f64);
            (y < ZONES[1].top) || (d <= light && line_clear(&world, sx, sy, x, y))
        };
        // Ping rings: water cells in the moving band.
        let mut ring = std::collections::HashSet::new();
        for p in &self.pings {
            let r = ((t - p.t0) * 45.0) as f32;
            let lo = p.cells.partition_point(|c| c.1 < r - 1.3);
            for &(i, d) in &p.cells[lo..] {
                if d > r {
                    break;
                }
                ring.insert(i);
            }
            let _ = (p.x, p.y);
        }

        for vy in 0..view.height as i32 {
            for vx in 0..view.width as i32 {
                let (x, y) = (cx + vx, cy + vy);
                let (px, py) = (view.x + vx as u16, view.y + vy as u16);
                let cell = &mut buf[(px, py)];
                if !World::in_bounds(x, y) {
                    cell.set_symbol(" ").set_style(Style::new().bg(Color::Rgb(2, 3, 6)));
                    continue;
                }
                let i = World::idx(x, y);
                let sunlit = y < ZONES[1].top;
                let bg = if sunlit {
                    Color::Rgb(8, 30 + (20 - y.min(20)) as u8 * 2, 60 + (20 - y.min(20)) as u8 * 3)
                } else {
                    Color::Rgb(2, 4, 8)
                };
                let kind = world.get(x, y);
                let is_lit = lit(x, y);
                let age = t as f32 - self.echo[i];
                let b = if age.is_finite() { (1.0 - age as f64 / fade).max(0.0) } else { 0.0 };
                let mut sym = " ";
                let mut style = Style::new().bg(bg);
                if kind == ROCK {
                    if !world.is_edge(x, y) {
                        style = style.bg(if sunlit { Color::Rgb(40, 34, 30) } else { Color::Rgb(2, 3, 6) });
                    } else if is_lit {
                        sym = "█";
                        style = style.fg(if sunlit { Color::Rgb(120, 100, 80) } else { Color::Rgb(150, 128, 100) });
                    } else if b > 0.0 {
                        sym = "█";
                        style = style.fg(Color::Rgb(
                            (20.0 + 30.0 * b) as u8,
                            (40.0 + 180.0 * b) as u8,
                            (55.0 + 190.0 * b) as u8,
                        ));
                    } else if self.is_charted(i) {
                        sym = "▒";
                        style = style.fg(Color::Rgb(30, 44, 56));
                    }
                } else {
                    if is_lit && !sunlit {
                        style = style.bg(Color::Rgb(14, 20, 28));
                    }
                    if kind == VENT {
                        let glow = 0.6 + 0.4 * ((t * 3.0 + x as f64).sin() * 0.5 + 0.5);
                        if dist(x, y, sx, sy) < 30.0 || self.is_charted(i) {
                            sym = "^";
                            style = style.fg(Color::Rgb((255.0 * glow) as u8, (110.0 * glow) as u8, 40));
                        }
                    } else if ring.contains(&i) {
                        sym = "·";
                        style = style.fg(Color::Rgb(70, 170, 200));
                    }
                }
                cell.set_symbol(sym).set_style(style);
            }
        }

        let mut put = |x: i32, y: i32, s: &str, st: Style| {
            let (vx, vy) = (x - cx, y - cy);
            if vx >= 0 && vy >= 0 && vx < view.width as i32 && vy < view.height as i32 {
                let c = &mut buf[(view.x + vx as u16, view.y + vy as u16)];
                c.set_symbol(s);
                if let Some(fg) = st.fg {
                    c.set_fg(fg);
                }
                if let Some(bg) = st.bg {
                    c.set_bg(bg);
                }
            }
        };

        // Wrecks: outline when charted or lit, beacons always (they're lights).
        for (wi, w) in world.wrecks.iter().enumerate() {
            let i = World::idx(w.x, w.y);
            let age = t as f32 - self.echo[i];
            let fresh = age.is_finite() && (age as f64) < fade;
            let known = fresh || self.is_charted(i) || lit(w.x, w.y);
            let looted = self.looted[wi];
            if known {
                let col = if looted {
                    Color::Rgb(80, 80, 90)
                } else if w.meridian {
                    Color::Rgb(255, 230, 150)
                } else if fresh || lit(w.x, w.y) {
                    Color::Rgb(255, 190, 90)
                } else {
                    Color::Rgb(120, 95, 60)
                };
                let top = format!("╔{}╗", "═".repeat((w.half * 2 + 1) as usize));
                for (k, ch) in top.chars().enumerate() {
                    put(w.x - w.half - 1 + k as i32, w.y - 1, &ch.to_string(), Style::new().fg(col));
                }
                put(w.x - w.half - 1, w.y, "║", Style::new().fg(col));
                put(w.x + w.half + 1, w.y, "║", Style::new().fg(col));
                put(w.x, w.y, "▣", Style::new().fg(col).add_modifier(Modifier::BOLD));
            }
            if w.beacon && !looted && dist(w.x, w.y, sx, sy) <= BEACON_RANGE {
                // Even blink: on for 0.6 s every 1.2 s.
                if (t / 0.6) as i64 % 2 == 0 {
                    put(w.x, w.y - 2, "◦", Style::new().fg(Color::Rgb(255, 190, 80)));
                }
            }
        }
        // The Singer glows faintly at the floor.
        let (gx, gy) = world.singer;
        if dist(gx, gy, sx, sy) < 34.0 {
            let pulse = ((t * 1.3).sin() * 0.5 + 0.5) as f32;
            put(
                gx,
                gy,
                "*",
                Style::new().fg(Color::Rgb((120.0 + 135.0 * pulse) as u8, (200.0 + 55.0 * pulse) as u8, 255)),
            );
        }

        // Ghosts: echoes of where things were.
        for g in &self.ghosts {
            let a = (1.0 - (t - g.t) / 4.5).max(0.0);
            let Color::Rgb(r, gg, bb) = g.color else { continue };
            let col = Color::Rgb((r as f64 * a) as u8, (gg as f64 * a) as u8, (bb as f64 * a) as u8);
            if g.wide {
                for dy in -1..=1 {
                    for dx in -4..=4 {
                        put(g.x + dx, g.y + dy, "▓", Style::new().fg(col));
                    }
                }
            } else {
                put(g.x, g.y, g.glyph, Style::new().fg(col));
            }
        }

        // Creatures you can actually see right now.
        for c in self.creatures.iter().filter(|c| c.alive) {
            if let Some((lx, ly)) = c.lure() {
                // Lures flicker twice per cycle: the tell.
                let phase = (t * 2.2) % 1.0;
                if dist(lx, ly, sx, sy) <= BEACON_RANGE && (phase < 0.12 || (0.24..0.36).contains(&phase)) {
                    put(lx, ly, "◦", Style::new().fg(lure_color));
                }
            }
            let flaring = c.flare > 0.0;
            let visible =
                lit(c.x, c.y) || flaring || c.sp == Species::Glimmer && (t * 1.7 + c.home.0 as f64).sin() > 0.93;
            if !visible {
                continue;
            }
            if c.sp == Species::Leviathan {
                for dy in -1..=1 {
                    for dx in -4..=4 {
                        if lit(c.x + dx, c.y + dy) {
                            put(c.x + dx, c.y + dy, "█", Style::new().fg(c.color()));
                        }
                    }
                }
                put(c.x + 3 * c.facing, c.y, "o", Style::new().fg(Color::Rgb(255, 70, 70)));
            } else {
                let col = if flaring { Color::Rgb(240, 170, 255) } else { c.color() };
                put(c.x, c.y, c.glyph(), Style::new().fg(col).add_modifier(Modifier::BOLD));
            }
        }
        for d in &self.decoys {
            if (t * 6.0) as i64 % 2 == 0 {
                put(d.x, d.y, "¤", Style::new().fg(Color::Rgb(255, 240, 120)));
            }
        }
        for b in &self.bolts {
            put(b.x.round() as i32, b.y, "-", Style::new().fg(Color::Rgb(230, 230, 230)));
        }
        // The submarine.
        let sub_col = if self.sub.hurt > 0.0 { Color::Rgb(255, 80, 80) } else { Color::Rgb(255, 225, 120) };
        put(
            sx,
            sy,
            if self.sub.facing >= 0 { "▶" } else { "◀" },
            Style::new().fg(sub_col).add_modifier(Modifier::BOLD),
        );
        if let Some((_, p)) = self.sub.salvage {
            let bar = format!("[{}{}]", "■".repeat((p * 6.0) as usize), "·".repeat(6 - (p * 6.0) as usize));
            for (k, ch) in bar.chars().enumerate() {
                put(sx - 4 + k as i32, sy - 1, &ch.to_string(), Style::new().fg(ui::GOOD));
            }
        }

        // Hydrophone: arrows on the edge of the view toward things you can hear but not see.
        let song = progress.has(Tech::SongAnalysis);
        for c in self.creatures.iter().filter(|c| c.alive && c.audible(song)) {
            let d = dist(c.x, c.y, sx, sy);
            if d > 48.0 {
                continue;
            }
            let (vx, vy) = (c.x - cx, c.y - cy);
            let on_screen = vx >= 0 && vy >= 0 && vx < view.width as i32 && vy < view.height as i32 && lit(c.x, c.y);
            if on_screen {
                continue;
            }
            let (ccx, ccy) = (view.width as f64 / 2.0, view.height as f64 / 2.0);
            let (ddx, ddy) = ((c.x - sx) as f64, (c.y - sy) as f64 * 2.0);
            let len = (ddx * ddx + ddy * ddy).sqrt().max(0.001);
            let s = ((ccx - 1.0) / (ddx / len).abs().max(0.001)).min((ccy - 1.0) / (ddy / len / 2.0).abs().max(0.001));
            let ex = (ccx + ddx / len * s).clamp(0.0, view.width as f64 - 1.0) as u16;
            let ey = (ccy + ddy / len / 2.0 * s).clamp(0.0, view.height as f64 - 1.0) as u16;
            let loud = (1.0 - d / 48.0).clamp(0.3, 1.0);
            let col = if c.sp == Species::Leviathan {
                Color::Rgb((200.0 * loud) as u8, 40, 50)
            } else {
                Color::Rgb((255.0 * loud) as u8, (190.0 * loud) as u8, 80)
            };
            let cell = &mut buf[(view.x + ex, view.y + ey)];
            cell.set_symbol("≈").set_fg(col);
        }
    }

    fn bar(value: f64, max: f64, width: usize, color: Color) -> Vec<Span<'static>> {
        let filled = ((value / max).clamp(0.0, 1.0) * width as f64).round() as usize;
        vec![
            Span::styled("█".repeat(filled), Style::new().fg(color)),
            Span::styled("░".repeat(width - filled), Style::new().fg(ui::FAINT)),
        ]
    }

    fn draw_hud(&self, f: &mut Frame, hud: Rect, progress: &Progress) {
        let z = world::zone_index(self.sub.y);
        let hull_col = if self.sub.hull < progress.max_hull() * 0.3 { ui::BAD } else { ui::GOOD };
        let batt_col = if self.sub.battery < 20.0 { ui::WARN } else { ui::ACCENT };
        let mut spans = vec![
            Span::styled(
                format!(" ▼ {:>6} m ", fmt_thousands(self.depth_m())),
                Style::new().fg(Color::White).add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("{:<9}", ZONES[z].name.to_uppercase()), Style::new().fg(ui::DIM)),
            Span::styled(" hull ", Style::new().fg(ui::DIM)),
        ];
        spans.extend(Self::bar(self.sub.hull, progress.max_hull(), 8, hull_col));
        spans.push(Span::styled(" batt ", Style::new().fg(ui::DIM)));
        spans.extend(Self::bar(self.sub.battery, progress.max_battery(), 8, batt_col));
        spans.push(Span::styled(format!("  ◈ {}", self.cargo_scrap), Style::new().fg(ui::WARN)));
        if self.cargo_relics > 0 {
            spans.push(Span::styled(format!(" ✦{}", self.cargo_relics), Style::new().fg(Color::Rgb(255, 150, 255))));
        }
        spans.push(Span::styled(format!("  ¤{}", self.sub.decoys), Style::new().fg(ui::DIM)));
        spans.push(Span::styled(
            if self.sub.lamp { "  ☼ lamp" } else { "  ● dark" },
            Style::new().fg(if self.sub.lamp { ui::WARN } else { ui::DIM }),
        ));
        let rated_row = progress.rated_m() / world::ROW_M;
        if self.sub.y > rated_row && (self.time * 2.0) as i64 % 2 == 0 {
            spans.push(Span::styled("  ⚠ PRESSURE", Style::new().fg(ui::BAD).add_modifier(Modifier::BOLD)));
        }
        if self.sub.battery <= 0.0 {
            spans.push(Span::styled("  ⚠ BATTERY DEAD: ASCENDING", Style::new().fg(ui::BAD)));
        }
        f.render_widget(Paragraph::new(Line::from(spans)).style(Style::new().bg(Color::Rgb(12, 14, 20))), hud);
    }

    fn draw_footer(&self, f: &mut Frame, foot: Rect) {
        let msg = self
            .msgs
            .back()
            .map(|(m, c, t0)| {
                let a = (1.0 - (self.time - t0 - 5.0).max(0.0) / 4.0).clamp(0.2, 1.0);
                let Color::Rgb(r, g, b) = *c else { return Line::from(m.clone()) };
                Line::styled(
                    format!(" {m}"),
                    Style::new().fg(Color::Rgb((r as f64 * a) as u8, (g as f64 * a) as u8, (b as f64 * a) as u8)),
                )
            })
            .unwrap_or_default();
        let keys = Line::styled(
            " move arrows/wasd · space ping · f lamp · q decoy · e salvage · h harpoon · r ascend · tab chart · ? help",
            Style::new().fg(ui::FAINT),
        );
        f.render_widget(Paragraph::new(vec![msg, keys]).style(Style::new().bg(Color::Rgb(8, 9, 14))), foot);
    }

    fn draw_map(&self, f: &mut Frame, view: Rect) {
        let world = self.world();
        let r = ui::centered(view, (W as u16 + 2).min(view.width), view.height);
        f.render_widget(Clear, r);
        let inner =
            Rect { x: r.x + 1, y: r.y + 1, width: r.width.saturating_sub(2), height: r.height.saturating_sub(2) };
        f.render_widget(ui::panel("Chart: this dive (tab to close)", ui::ACCENT), r);
        let rows = (self.max_row + 20).min(H);
        let per = (rows as f64 / inner.height as f64).max(1.0);
        let buf = f.buffer_mut();
        for vy in 0..inner.height {
            let y0 = (vy as f64 * per) as i32;
            let y1 = (((vy + 1) as f64 * per) as i32).max(y0 + 1);
            for vx in 0..inner.width.min(W as u16) {
                let x = vx as i32;
                let charted = (y0..y1.min(H)).any(|y| self.is_charted(World::idx(x, y)));
                if charted {
                    buf[(inner.x + vx, inner.y + vy)].set_symbol("▓").set_fg(Color::Rgb(40, 110, 130));
                }
            }
        }
        for (wi, w) in world.wrecks.iter().enumerate() {
            if self.is_charted(World::idx(w.x, w.y)) {
                let vy = (w.y as f64 / per) as u16;
                if vy < inner.height && (w.x as u16) < inner.width {
                    buf[(inner.x + w.x as u16, inner.y + vy)].set_symbol("▣").set_fg(if self.looted[wi] {
                        ui::FAINT
                    } else {
                        ui::WARN
                    });
                }
            }
        }
        let vy = (self.sub.y as f64 / per) as u16;
        if vy < inner.height && (self.sub.x as u16) < inner.width {
            buf[(inner.x + self.sub.x as u16, inner.y + vy)].set_symbol("▶").set_fg(Color::Rgb(255, 225, 120));
        }
        for (zi, z) in ZONES.iter().enumerate().skip(1) {
            let vy = (z.top as f64 / per) as u16;
            if vy < inner.height && z.top <= rows {
                let label = format!("{} {}m", z.name, world::depth_m(z.top));
                buf.set_string(
                    inner.x + inner.width.saturating_sub(label.len() as u16 + 1),
                    inner.y + vy,
                    label,
                    Style::new().fg(ui::DIM),
                );
                let _ = zi;
            }
        }
    }

    fn draw_popup(&self, f: &mut Frame, view: Rect, p: Popup) {
        let (title, color, body): (String, Color, Vec<Line>) = match p {
            Popup::Log(l) => {
                let log = &lore::LOGS[l as usize];
                (
                    format!("Meridian log {} of 12", l + 1),
                    ui::WARN,
                    vec![
                        Line::styled(format!("{} · {}", log.author, log.depth), Style::new().fg(ui::DIM)),
                        Line::from(""),
                        Line::from(log.text),
                        Line::from(""),
                        Line::styled("enter · continue", Style::new().fg(ui::FAINT)),
                    ],
                )
            }
            Popup::Floor => (
                "The Floor".into(),
                Color::Rgb(160, 220, 255),
                vec![
                    Line::from(lore::FLOOR_ARRIVAL),
                    Line::from(""),
                    Line::styled(
                        "Find the Meridian's hatch ▣ and salvage it.  enter · continue",
                        Style::new().fg(ui::FAINT),
                    ),
                ],
            ),
            Popup::Choice => (
                "The Song".into(),
                Color::Rgb(160, 220, 255),
                vec![
                    Line::from("Your hydrophone is full of your own voice. The Singer is waiting."),
                    Line::from(""),
                    Line::styled("1 · Answer the Song", Style::new().fg(ui::ACCENT)),
                    Line::styled("2 · Go dark and take the black box", Style::new().fg(ui::DIM)),
                ],
            ),
            Popup::Help => (
                "How to dive".into(),
                ui::ACCENT,
                vec![
                    Line::from("It's dark. space pings: walls and creatures flare up, then fade."),
                    Line::from("Every ping is loud. Eels swim to where the noise was. Move after you ping."),
                    Line::from("The lamp (f) shows what's right next to you. Turn it off to hide."),
                    Line::from("≈ on the edge of the screen: something you can hear moving."),
                    Line::from("◦ beacons mark wrecks (even blink). Lures flicker twice."),
                    Line::from("e on a wreck hatch ▣ to salvage. q drops a decoy. h fires the harpoon."),
                    Line::from("Below your hull's rating the pressure crushes you. Surface to bank cargo."),
                    Line::from("r autopilots to the surface. tab shows your chart. shift to boost (loud)."),
                    Line::from(""),
                    Line::styled("enter · back to the dark", Style::new().fg(ui::FAINT)),
                ],
            ),
        };
        let w = (view.width.saturating_sub(4)).min(78);
        // Height from the wrapped line count, so the box fits its text.
        let inner = w.saturating_sub(2).max(1) as usize;
        let h: usize = body.iter().map(|l| l.width().max(1).div_ceil(inner)).sum();
        let r = ui::centered(view, w, (h as u16 + 2).min(view.height));
        f.render_widget(Clear, r);
        f.render_widget(Paragraph::new(body).wrap(Wrap { trim: false }).block(ui::panel(&title, color)), r);
    }
}

pub fn fmt_thousands(n: i32) -> String {
    let s = n.abs().to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    if n < 0 { format!("-{out}") } else { out }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dive() -> Dive {
        Dive::new(11, &Progress::default(), 0)
    }

    fn press(k: Key) -> Input {
        Input { pressed: vec![k], ..Input::default() }
    }

    #[test]
    fn starts_in_water_near_the_surface() {
        let d = dive();
        assert!(!d.world().solid(d.sub.x, d.sub.y));
        assert!(d.sub.y < 10);
        assert_eq!(d.sub.hull, 100.0);
    }

    #[test]
    fn ping_reveals_walls_costs_battery_and_charts() {
        let p = Progress::default();
        let mut d = dive();
        let before = d.sub.battery;
        d.update(0.016, &press(Key::Space), &p);
        assert!(d.sub.battery < before - 9.0);
        for _ in 0..60 {
            d.update(0.02, &Input::default(), &p);
        }
        assert!(d.charted.iter().any(|w| *w != 0), "ping charted some rock");
        assert!(d.echo.iter().any(|e| e.is_finite()));
    }

    #[test]
    fn walls_block_line_of_sight() {
        let d = dive();
        let w = d.world();
        let fov = field_of_view(&w, d.sub.x, d.sub.y, 16.0);
        for &(i, _) in &fov {
            let (x, y) = (i as i32 % W, i as i32 / W);
            assert!(line_clear(&w, d.sub.x, d.sub.y, x, y));
            assert!(!w.solid(x, y) || w.is_edge(x, y), "interior rock never shows");
        }
        assert!(fov.windows(2).all(|p| p[0].1 <= p[1].1), "sorted by distance");
    }

    #[test]
    fn eels_hunt_noise_and_decoys_pull_them() {
        let p = Progress::default();
        let mut d = dive();
        d.creatures = vec![Creature::new(Species::Eel, d.sub.x + 20, d.sub.y + 6, &mut Rng::new(1))];
        d.noise(d.sub.x, d.sub.y, 30.0, 10.0);
        assert_eq!(d.creatures[0].state, CState::Hunt);
        assert_eq!(d.creatures[0].target, (d.sub.x, d.sub.y));
        d.drop_decoy();
        assert_eq!(d.sub.decoys, 0);
        d.sub.x += 0;
        d.update(0.6, &Input::default(), &p);
        assert_eq!(d.creatures[0].target, (d.decoys[0].x, d.decoys[0].y));
    }

    #[test]
    fn pressure_damages_below_rating_and_death_ends_the_dive() {
        let p = Progress::default();
        let mut d = dive();
        let rated = p.rated_m() / world::ROW_M;
        let w = d.world();
        let (x, y) = (rated + 60..H - 20)
            .find_map(|y| (1..W - 1).find(|&x| !w.solid(x, y) && w.get(x, y) != VENT).map(|x| (x, y)))
            .unwrap();
        d.sub.x = x;
        d.sub.y = y;
        d.creatures.clear();
        d.update(1.0, &Input::default(), &p);
        assert!(d.sub.hull < 100.0, "pressure hurt");
        d.sub.hull = 0.5;
        for _ in 0..50 {
            d.update(0.1, &Input::default(), &p);
        }
        assert_eq!(d.outcome, Some(Outcome::Lost));
    }

    #[test]
    fn salvaging_a_log_wreck() {
        // A fully rated hull so deep log wrecks don't crush the sub mid-salvage.
        let mut p = Progress::default();
        p.upgrades.insert(Upgrade::Hull, 4);
        let mut d = dive();
        let w = d.world();
        let (wi, wreck) = w.wrecks.iter().enumerate().find(|(_, w)| w.log.is_some()).unwrap();
        d.sub.x = wreck.x;
        d.sub.y = wreck.y;
        d.creatures.clear();
        d.update(0.01, &press(Key::Char('e')), &p);
        for _ in 0..30 {
            d.update(0.1, &Input::default(), &p);
        }
        assert!(d.looted[wi]);
        assert!(matches!(d.popup, Some(Popup::Log(_))));
        assert!(d.events.iter().any(|e| matches!(e, DiveEvent::Log(_))));
        d.update(0.01, &press(Key::Enter), &p);
        assert!(d.popup.is_none());
    }

    #[test]
    fn autopilot_surfaces() {
        let p = Progress::default();
        let mut d = Dive::new(5, &p, 2);
        d.creatures.clear();
        d.sub.battery = 1000.0;
        d.update(0.01, &press(Key::Char('r')), &p);
        for _ in 0..4000 {
            d.update(0.05, &Input::default(), &p);
            if d.outcome.is_some() {
                break;
            }
        }
        assert_eq!(d.outcome, Some(Outcome::Surfaced));
    }

    #[test]
    fn save_and_rebuild() {
        let p = Progress::default();
        let mut d = dive();
        d.update(0.016, &press(Key::Space), &p);
        let v = serde_json::to_value(&d).unwrap();
        let mut back: Dive = serde_json::from_value(v).unwrap();
        back.rebuild();
        assert_eq!(back.sub.x, d.sub.x);
        assert_eq!(back.charted, d.charted);
        back.update(0.016, &Input::default(), &p);
    }

    /// A crude bot that dives: down when it can, sideways when blocked, pings every few seconds.
    /// Run with `cargo test playthrough -- --ignored --nocapture` to see balance numbers.
    #[test]
    #[ignore]
    fn playthrough() {
        let p = Progress::default();
        for seed in 0..8 {
            let mut d = Dive::new(seed, &p, 0);
            let mut rng = Rng::new(seed);
            let mut side = 1;
            let mut ping_t = 0.0;
            let mut hits = 0.0;
            let mut last_hull = d.sub.hull;
            for _ in 0..(180.0 / 0.05) as usize {
                let w = d.world();
                let mut keys = vec![];
                if !w.solid(d.sub.x, d.sub.y + 1) {
                    keys.push(Key::Down);
                } else {
                    if w.solid(d.sub.x + side, d.sub.y) || rng.chance(0.02) {
                        side = -side;
                    }
                    keys.push(if side > 0 { Key::Right } else { Key::Left });
                }
                ping_t += 0.05;
                if ping_t > 3.0 {
                    ping_t = 0.0;
                    keys.push(Key::Space);
                }
                d.update(0.05, &Input { pressed: keys, ..Input::default() }, &p);
                if d.sub.hull < last_hull - 1.0 {
                    hits += last_hull - d.sub.hull;
                }
                last_hull = d.sub.hull;
                if d.outcome.is_some() || d.popup.is_some() {
                    d.popup = None;
                    if d.outcome.is_some() {
                        break;
                    }
                }
            }
            println!(
                "seed {seed}: depth {:>5} m  hull {:>5.1}  batt {:>5.1}  attack dmg {:>5.1}  outcome {:?}",
                d.max_row * 10,
                d.sub.hull,
                d.sub.battery,
                hits,
                d.outcome
            );
        }
    }

    #[test]
    fn thousands() {
        assert_eq!(fmt_thousands(10935), "10,935");
        assert_eq!(fmt_thousands(300), "300");
        assert_eq!(fmt_thousands(1_000_000), "1,000,000");
    }
}
