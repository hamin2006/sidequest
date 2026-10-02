//! FATHOM: a sonar roguelike RPG. This module ties together the hub ship, dives, the story and saves.
//! Design notes: docs/FATHOM.md.

pub mod dive;
pub mod lore;
pub mod progress;
pub mod world;

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::Game;
use crate::input::{Input, Key};
use crate::rng::Rng;
use crate::store::Score;
use crate::ui;
use dive::{Dive, DiveEvent, Outcome, fmt_thousands};
use progress::{Ending, Progress, TECHS, UPGRADES, Upgrade};
use world::{Species, ZONES};

const RELIC_VALUE: u64 = 150;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum View {
    Menu,
    Launch,
    Workshop,
    Research,
    Bestiary,
    Logbook,
}

const MENU: [(View, &str); 5] = [
    (View::Launch, "Dive"),
    (View::Workshop, "Workshop"),
    (View::Research, "Research"),
    (View::Bestiary, "Bestiary"),
    (View::Logbook, "Logbook"),
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Summary {
    pub outcome: Outcome,
    pub depth_m: i32,
    pub scrap: u64,
    pub relics: u32,
    pub logs: usize,
    pub record: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Scene {
    Intro { page: usize },
    Hub { view: View, sel: usize, item: usize },
    Dive(Box<Dive>),
    Summary(Summary),
    Ending { kind: Ending, page: usize },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fathom {
    pub progress: Progress,
    pub scene: Scene,
    rng: Rng,
    #[serde(skip)]
    note: Option<(String, Color, f64)>,
    #[serde(skip)]
    time: f64,
}

impl Default for Fathom {
    fn default() -> Self {
        Self::new()
    }
}

fn hub() -> Scene {
    Scene::Hub { view: View::Menu, sel: 0, item: 0 }
}

impl Fathom {
    pub fn new() -> Self {
        Self {
            progress: Progress::default(),
            scene: Scene::Intro { page: 0 },
            rng: Rng::from_time(),
            note: None,
            time: 0.0,
        }
    }

    pub fn load(v: &Value) -> Option<Box<dyn Game>> {
        let mut f: Fathom = serde_json::from_value(v.clone()).ok()?;
        if let Scene::Dive(d) = &mut f.scene {
            d.rebuild();
        }
        Some(Box::new(f))
    }

    fn note(&mut self, text: impl Into<String>, color: Color) {
        self.note = Some((text.into(), color, self.time));
    }

    pub fn start_dive(&mut self, zone: usize) {
        let seed = self.rng.next_u64();
        self.scene = Scene::Dive(Box::new(Dive::new(seed, &self.progress, zone)));
    }

    /// Applies dive events and depth records to the persistent progress.
    fn absorb(&mut self, d: &mut Dive) {
        for e in std::mem::take(&mut d.events) {
            match e {
                DiveEvent::Echo(sp) => {
                    if self.progress.add_scan(sp) {
                        d.say(format!("Bestiary complete: {}. +research", lore::codex(sp).name), ui::GOOD);
                    }
                }
                DiveEvent::Log(l) => {
                    if self.progress.logs.insert(l) {
                        self.progress.xp += 40;
                    }
                }
                DiveEvent::Relay(z) => {
                    self.progress.relays.insert(z);
                }
            }
        }
        let depth = (d.max_row * world::ROW_M) as u32;
        if depth > self.progress.deepest_m {
            self.progress.xp += ((depth - self.progress.deepest_m) / 10) as u64;
            self.progress.deepest_m = depth;
        }
    }

    fn finish_dive(&mut self, d: &Dive, outcome: Outcome) {
        let p = &mut self.progress;
        p.dives += 1;
        let record = d.max_row * world::ROW_M >= p.deepest_m as i32 && d.max_row > 10;
        match outcome {
            Outcome::Surfaced => {
                p.scrap += d.cargo_scrap + d.cargo_relics as u64 * RELIC_VALUE;
                p.relics += d.cargo_relics;
                p.xp += d.cargo_scrap / 5 + d.cargo_relics as u64 * 30;
            }
            Outcome::Lost => p.losses += 1,
            Outcome::Ending(kind) => {
                p.ending = Some(kind);
                p.descents += 1;
                p.xp += 500;
                p.scrap += 1000;
                self.scene = Scene::Ending { kind, page: 0 };
                return;
            }
        }
        self.scene = Scene::Summary(Summary {
            outcome,
            depth_m: d.max_row * world::ROW_M,
            scrap: d.cargo_scrap,
            relics: d.cargo_relics,
            logs: d.logs_found.len(),
            record,
        });
    }

    fn view_len(&self, view: View) -> usize {
        match view {
            View::Menu => MENU.len(),
            View::Launch => self.progress.start_options().len(),
            View::Workshop => UPGRADES.len(),
            View::Research => TECHS.len(),
            View::Bestiary => Species::ALL.len(),
            View::Logbook => lore::LOGS.len(),
        }
    }

    fn update_hub(&mut self, input: &Input) {
        let Scene::Hub { mut view, mut sel, mut item } = self.scene.clone() else { return };
        for k in &input.pressed {
            if view == View::Menu {
                match k {
                    Key::Up | Key::Char('w') => sel = (sel + MENU.len() - 1) % MENU.len(),
                    Key::Down | Key::Char('s') => sel = (sel + 1) % MENU.len(),
                    Key::Enter | Key::Space | Key::Right | Key::Char('d') => {
                        view = MENU[sel].0;
                        item = 0;
                    }
                    _ => {}
                }
                continue;
            }
            let n = self.view_len(view).max(1);
            match k {
                Key::Esc | Key::Left | Key::Char('a') | Key::Backspace => view = View::Menu,
                Key::Up | Key::Char('w') => item = (item + n - 1) % n,
                Key::Down | Key::Char('s') => item = (item + 1) % n,
                Key::Enter | Key::Space => match view {
                    View::Launch => {
                        let zone = self.progress.start_options()[item.min(n - 1)];
                        self.start_dive(zone);
                        return;
                    }
                    View::Workshop => {
                        let u = UPGRADES[item].up;
                        match self.progress.buy_upgrade(u) {
                            Ok(()) => self.note(
                                format!("Installed {} level {}.", UPGRADES[item].name, self.progress.level(u)),
                                ui::GOOD,
                            ),
                            Err(e) => self.note(e, ui::WARN),
                        }
                    }
                    View::Research => {
                        let t = TECHS[item].tech;
                        match self.progress.buy_tech(t) {
                            Ok(()) => self.note(format!("Researched {}.", TECHS[item].name), ui::GOOD),
                            Err(e) => self.note(e, ui::WARN),
                        }
                    }
                    _ => {}
                },
                _ => {}
            }
        }
        self.scene = Scene::Hub { view, sel, item };
    }

    // ---------- Drawing ----------

    fn draw_story(&self, f: &mut Frame, area: Rect, title: &str, paras: &[&str], page: usize, footer: &str) {
        let w = 72.min(area.width.saturating_sub(4));
        let r = ui::centered(area, w, 16.min(area.height));
        let mut lines = vec![
            Line::styled(title.to_string(), Style::new().fg(Color::Rgb(160, 220, 255)).add_modifier(Modifier::BOLD)),
            Line::from(""),
        ];
        for (i, p) in paras.iter().enumerate().take(page + 1) {
            let fade = if i == page { Color::White } else { ui::DIM };
            lines.push(Line::styled(p.to_string(), Style::new().fg(fade)));
            lines.push(Line::from(""));
        }
        lines.push(Line::styled(footer.to_string(), Style::new().fg(ui::FAINT)));
        f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), r);
    }

    fn draw_hub(&self, f: &mut Frame, area: Rect, view: View, sel: usize, item: usize) {
        if area.width < 60 || area.height < 20 {
            ui::too_small(f, area, 60, 20);
            return;
        }
        let p = &self.progress;
        let left_w = 30.min(area.width / 3);
        let left = Rect { width: left_w, ..area };
        let right = Rect { x: area.x + left_w + 1, width: area.width - left_w - 1, ..area };

        let mut l = vec![
            Line::styled("      ▁▁▁▟▙▁▁", Style::new().fg(ui::DIM)),
            Line::styled("  ▁▁▟█▀▀▀▀▀▀█▙▁▁▁", Style::new().fg(Color::Rgb(200, 200, 210))),
            Line::styled(" ▜██ RV TERN ████▛", Style::new().fg(Color::Rgb(220, 90, 70))),
            Line::styled("≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈", Style::new().fg(Color::Rgb(60, 120, 190))),
            Line::from(""),
            Line::styled(format!(" {}", p.rank()), Style::new().fg(ui::ACCENT).add_modifier(Modifier::BOLD)),
            Line::from(vec![
                Span::styled(" ◈ scrap    ", Style::new().fg(ui::DIM)),
                Span::styled(p.scrap.to_string(), Style::new().fg(ui::WARN)),
            ]),
            Line::from(vec![
                Span::styled(" ✧ research ", Style::new().fg(ui::DIM)),
                Span::styled(p.research.to_string(), Style::new().fg(Color::Rgb(200, 160, 255))),
            ]),
            Line::from(vec![
                Span::styled(" ▼ deepest  ", Style::new().fg(ui::DIM)),
                Span::raw(format!("{} m", fmt_thousands(p.deepest_m as i32))),
            ]),
            Line::from(vec![
                Span::styled(" ⚓ rated    ", Style::new().fg(ui::DIM)),
                Span::raw(format!("{} m", fmt_thousands(p.rated_m()))),
            ]),
            Line::from(vec![
                Span::styled(" ✉ logs     ", Style::new().fg(ui::DIM)),
                Span::raw(format!("{}/12", p.logs.len())),
            ]),
            Line::from(vec![
                Span::styled(" ⌘ species  ", Style::new().fg(ui::DIM)),
                Span::raw(format!("{}/6", Species::ALL.iter().filter(|s| p.scanned(**s)).count())),
            ]),
            Line::from(""),
        ];
        for (i, (_, name)) in MENU.iter().enumerate() {
            let active = i == sel;
            let style = if active && view == View::Menu {
                Style::new().fg(Color::Black).bg(ui::ACCENT)
            } else if active {
                Style::new().fg(ui::ACCENT)
            } else {
                Style::new().fg(Color::Rgb(200, 204, 214))
            };
            l.push(Line::styled(format!("  {} {name:<12}", if active { "▸" } else { " " }), style));
        }
        f.render_widget(Paragraph::new(l), left);

        let (title, lines) = self.hub_panel(view, sel, item, right.width as usize);
        let mut lines = lines;
        if let Some((n, c, t)) = &self.note
            && self.time - t < 3.0
        {
            lines.push(Line::from(""));
            lines.push(Line::styled(n.clone(), Style::new().fg(*c)));
        }
        f.render_widget(
            Paragraph::new(lines)
                .wrap(Wrap { trim: false })
                .block(ui::panel(&title, if view == View::Menu { ui::FAINT } else { ui::ACCENT })),
            right,
        );
    }

    fn hub_panel(&self, view: View, sel: usize, item: usize, width: usize) -> (String, Vec<Line<'static>>) {
        let p = &self.progress;
        let row = |i: usize, text: String, ok: bool| {
            let st = if i == item {
                Style::new().fg(Color::Black).bg(if ok { ui::ACCENT } else { ui::DIM })
            } else if ok {
                Style::new().fg(Color::Rgb(220, 222, 230))
            } else {
                Style::new().fg(ui::DIM)
            };
            Line::styled(format!(" {text}"), st)
        };
        let hint = |s: &str| Line::styled(s.to_string(), Style::new().fg(ui::FAINT));
        match view {
            View::Menu => {
                let next = match MENU[sel].0 {
                    View::Launch => "Choose where to start your next dive.",
                    View::Workshop => "Spend scrap on the submersible.",
                    View::Research => "Spend research from completed bestiary entries.",
                    View::Bestiary => "Everything you've scanned in the dark.",
                    View::Logbook => "What happened to the Meridian.",
                    View::Menu => "",
                };
                let mut goal = vec![
                    Line::styled(next, Style::new().fg(Color::White)),
                    Line::from(""),
                    Line::styled("Next steps", Style::new().fg(ui::ACCENT)),
                ];
                if p.dives == 0 {
                    goal.push(Line::from(" · Dive, ping with space, and find a wreck beacon ◦."));
                }
                if p.deepest_m as i32 + 300 >= p.rated_m() && p.level(Upgrade::Hull) < 4 {
                    goal.push(Line::from(" · Your hull limits you. Upgrade the pressure hull."));
                }
                if p.logs.len() < 12 {
                    goal.push(Line::from(format!(" · {} Meridian logs are still down there.", 12 - p.logs.len())));
                }
                if let Some(e) = p.ending {
                    goal.push(Line::from(format!(
                        " · You {} at the floor. The trench keeps changing.",
                        if e == Ending::Answered { "answered the Song" } else { "went dark" }
                    )));
                }
                goal.push(Line::from(""));
                goal.push(Line::styled(
                    format!("Tip: {}", lore::TIPS[(p.dives as usize + sel) % lore::TIPS.len()]),
                    Style::new().fg(ui::DIM),
                ));
                goal.push(Line::from(""));
                goal.push(hint("↑↓ choose · enter open · esc pause"));
                ("RV Tern · topside".into(), goal)
            }
            View::Launch => {
                let mut l = vec![Line::styled("Lower the sub to:", Style::new().fg(ui::DIM)), Line::from("")];
                for (i, z) in p.start_options().into_iter().enumerate() {
                    let label = if z == 0 {
                        "The surface".to_string()
                    } else {
                        format!("{} relay · {} m", ZONES[z].name, fmt_thousands(world::depth_m(ZONES[z].top)))
                    };
                    l.push(row(i, label, true));
                }
                l.push(Line::from(""));
                l.push(Line::styled(
                    format!("Hull rated to {} m. Below that, pressure does damage.", fmt_thousands(p.rated_m())),
                    Style::new().fg(ui::DIM),
                ));
                l.push(hint("enter dive · esc back"));
                ("Dive".into(), l)
            }
            View::Workshop => {
                let mut l = vec![];
                for (i, d) in UPGRADES.iter().enumerate() {
                    let lvl = p.level(d.up);
                    let cost = p.upgrade_cost(d.up);
                    let effect = match d.up {
                        Upgrade::Hull => format!("rated {} m", fmt_thousands(p.rated_m())),
                        Upgrade::Battery => format!("{} capacity", p.max_battery()),
                        Upgrade::Sonar => format!("{} range", p.sonar_range()),
                        Upgrade::Quiet => format!("{}% noise", (p.noise_mult() * 100.0) as i32),
                        Upgrade::Plating => format!("{} hull", p.max_hull()),
                        Upgrade::Decoys => format!("{} decoys", p.decoys()),
                        Upgrade::Harpoon => {
                            if lvl == 0 {
                                "not fitted".into()
                            } else {
                                format!("damage {lvl}")
                            }
                        }
                    };
                    let pips = format!("{}{}", "■".repeat(lvl as usize), "□".repeat((d.max - lvl) as usize));
                    let price = cost.map(|c| format!("◈ {c}")).unwrap_or_else(|| "max".into());
                    let text = format!("{:<14} {pips:<5} {effect:<16} {price}", d.name);
                    l.push(row(i, text, cost.is_some_and(|c| c <= p.scrap)));
                }
                l.push(Line::from(""));
                l.push(hint(&format!("you have ◈ {} · enter buy · esc back", p.scrap)));
                ("Workshop".into(), l)
            }
            View::Research => {
                let mut l = vec![];
                for (i, t) in TECHS.iter().enumerate() {
                    let have = p.has(t.tech);
                    let text = format!(
                        "{} {:<18} {:<34} {}",
                        if have { "✓" } else { " " },
                        t.name,
                        t.desc,
                        if have { String::new() } else { format!("✧ {}", t.cost) }
                    );
                    l.push(row(i, text, !have && t.cost <= p.research));
                }
                l.push(Line::from(""));
                l.push(hint(&format!(
                    "you have ✧ {} · complete bestiary entries to earn more · enter research",
                    p.research
                )));
                ("Research".into(), l)
            }
            View::Bestiary => {
                let mut l = vec![];
                for (i, s) in Species::ALL.iter().enumerate() {
                    let c = lore::codex(*s);
                    let n = p.scan_count(*s);
                    let name = if n > 0 { c.name } else { "???" };
                    l.push(row(
                        i,
                        format!(
                            "{} {:<16} {}",
                            if n > 0 { c.glyph } else { "?" },
                            name,
                            "●".repeat(n as usize) + &"○".repeat(3 - n.min(3) as usize)
                        ),
                        true,
                    ));
                }
                let s = Species::ALL[item.min(5)];
                let c = lore::codex(s);
                l.push(Line::from(""));
                if p.scanned(s) {
                    l.push(Line::styled(c.text, Style::new().fg(Color::White)));
                    l.push(Line::from(""));
                    l.push(Line::styled(format!("Field note: {}", c.hint), Style::new().fg(ui::GOOD)));
                } else {
                    l.push(Line::styled(
                        "Catch it in three sonar echoes to complete this entry.",
                        Style::new().fg(ui::DIM),
                    ));
                }
                let _ = width;
                ("Bestiary".into(), l)
            }
            View::Logbook => {
                let mut l = vec![];
                for (i, log) in lore::LOGS.iter().enumerate() {
                    let found = p.logs.contains(&(i as u8));
                    let label = if found {
                        format!("{:>2}. {} · {}", i + 1, log.author, log.depth)
                    } else {
                        format!("{:>2}. ··· somewhere near {}", i + 1, log.depth)
                    };
                    l.push(row(i, label, found));
                }
                l.push(Line::from(""));
                let i = item.min(11);
                if p.logs.contains(&(i as u8)) {
                    l.push(Line::styled(lore::LOGS[i].text, Style::new().fg(Color::White)));
                } else {
                    l.push(Line::styled(
                        "Not recovered yet. Logs rest in wrecks with steady beacons.",
                        Style::new().fg(ui::DIM),
                    ));
                }
                ("Logbook · the Meridian, 1994".into(), l)
            }
        }
    }

    fn draw_summary(&self, f: &mut Frame, area: Rect, s: &Summary) {
        let (title, color, head) = match s.outcome {
            Outcome::Surfaced => ("Surfaced", ui::GOOD, "The RV Tern winches you aboard."),
            Outcome::Lost => (
                "Hull breach",
                ui::BAD,
                "The sea came in. The crew of the Tern pulls a recovery drone up an hour later.",
            ),
            Outcome::Ending(_) => ("The Floor", ui::ACCENT, ""),
        };
        let mut l = vec![Line::styled(head, Style::new().fg(Color::White)), Line::from("")];
        l.push(Line::from(format!(
            "Deepest point   {} m{}",
            fmt_thousands(s.depth_m),
            if s.record { "  ★ record" } else { "" }
        )));
        match s.outcome {
            Outcome::Surfaced => {
                l.push(Line::styled(
                    format!("Banked          ◈ {} scrap", s.scrap + s.relics as u64 * RELIC_VALUE),
                    Style::new().fg(ui::WARN),
                ));
                if s.relics > 0 {
                    l.push(Line::from(format!("Relics          {} (sold for ◈ {RELIC_VALUE} each)", s.relics)));
                }
            }
            _ => l.push(Line::styled(format!("Lost cargo      ◈ {}", s.scrap), Style::new().fg(ui::BAD))),
        }
        l.push(Line::from(format!("Logs recovered  {}", s.logs)));
        l.push(Line::styled("Knowledge (scans, logs, relays) is always kept.", Style::new().fg(ui::DIM)));
        l.push(Line::from(""));
        l.push(Line::styled("enter · back to the Tern", Style::new().fg(ui::FAINT)));
        ui::modal(f, area, title, color, l);
    }
}

impl Game for Fathom {
    fn update(&mut self, dt: f64, input: &Input) {
        self.time += dt;
        match &mut self.scene {
            Scene::Intro { page } => {
                if input.was(Key::Enter) || input.was(Key::Space) {
                    *page += 1;
                    if *page >= lore::INTRO.len() {
                        self.progress.seen_intro = true;
                        self.scene = hub();
                    }
                }
            }
            Scene::Hub { .. } => self.update_hub(input),
            Scene::Dive(_) => {
                let Scene::Dive(mut d) = std::mem::replace(&mut self.scene, hub()) else { unreachable!() };
                d.update(dt, input, &self.progress);
                self.absorb(&mut d);
                match d.outcome {
                    Some(o) => self.finish_dive(&d, o),
                    None => self.scene = Scene::Dive(d),
                }
            }
            Scene::Summary(_) => {
                if input.was(Key::Enter) || input.was(Key::Space) {
                    self.scene = hub();
                }
            }
            Scene::Ending { page, .. } => {
                if input.was(Key::Enter) || input.was(Key::Space) {
                    *page += 1;
                    if *page > 4 {
                        self.scene = hub();
                    }
                }
            }
        }
    }

    fn draw(&mut self, f: &mut Frame, area: Rect) {
        f.buffer_mut().set_style(area, Style::new().bg(Color::Rgb(4, 6, 10)));
        match &self.scene {
            Scene::Intro { page } => {
                self.draw_story(f, area, "F A T H O M", &lore::INTRO, *page, "enter · continue");
            }
            Scene::Hub { view, sel, item } => self.draw_hub(f, area, *view, *sel, *item),
            Scene::Dive(d) => d.draw(f, area, &self.progress),
            Scene::Summary(s) => self.draw_summary(f, area, s),
            Scene::Ending { kind, page } => {
                let (title, text) = match kind {
                    Ending::Answered => ("You answered the Song", lore::ENDING_ANSWER),
                    Ending::WentDark => ("You went dark", lore::ENDING_DARK),
                };
                if *page < 4 {
                    self.draw_story(f, area, title, &text, *page, "enter · continue");
                } else {
                    self.draw_story(
                        f,
                        area,
                        "Second Descent",
                        &["Thank you for diving.", "The trench reshapes itself every dive. Bonus: ◈ 1,000 scrap. The Song is still down there."],
                        1,
                        "enter · back to the Tern",
                    );
                }
            }
        }
    }

    fn save(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }

    fn score(&self) -> Option<Score> {
        (self.progress.deepest_m > 0).then(|| Score::higher(self.progress.deepest_m as u64))
    }

    fn realtime(&self) -> bool {
        matches!(&self.scene, Scene::Dive(d) if d.popup.is_none())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(k: Key) -> Input {
        Input { pressed: vec![k], ..Input::default() }
    }

    #[test]
    fn intro_hub_dive_and_back() {
        let mut g = Fathom::new();
        for _ in 0..lore::INTRO.len() {
            g.update(0.1, &press(Key::Enter));
        }
        assert!(matches!(g.scene, Scene::Hub { .. }));
        g.update(0.1, &press(Key::Enter)); // open "Dive"
        g.update(0.1, &press(Key::Enter)); // launch from the surface
        assert!(matches!(g.scene, Scene::Dive(_)));
        g.update(0.1, &press(Key::Space));
        // Save and reload mid-dive.
        let back = Fathom::load(&g.save()).unwrap();
        assert!(back.realtime());

        let Scene::Dive(d) = &mut g.scene else { panic!() };
        d.sub.hull = 0.0;
        g.update(0.1, &Input::default());
        assert!(matches!(g.scene, Scene::Summary(_)));
        assert_eq!(g.progress.losses, 1);
        g.update(0.1, &press(Key::Enter));
        assert!(matches!(g.scene, Scene::Hub { .. }));
    }

    #[test]
    fn surfacing_banks_cargo_and_workshop_buys() {
        let mut g = Fathom::new();
        g.scene = hub();
        g.start_dive(0);
        let Scene::Dive(d) = &mut g.scene else { panic!() };
        d.cargo_scrap = 200;
        d.cargo_relics = 1;
        d.outcome = Some(Outcome::Surfaced);
        let Scene::Dive(d) = std::mem::replace(&mut g.scene, hub()) else { panic!() };
        g.finish_dive(&d, Outcome::Surfaced);
        assert_eq!(g.progress.scrap, 200 + RELIC_VALUE);
        g.scene = Scene::Hub { view: View::Workshop, sel: 1, item: 0 };
        g.update(0.1, &press(Key::Enter));
        assert_eq!(g.progress.level(Upgrade::Hull), 1);
    }

    #[test]
    fn scans_complete_bestiary_entries() {
        let mut g = Fathom::new();
        g.start_dive(0);
        let Scene::Dive(mut d) = std::mem::replace(&mut g.scene, hub()) else { panic!() };
        d.events = vec![DiveEvent::Echo(Species::Jelly); 3];
        d.events.push(DiveEvent::Relay(2));
        d.events.push(DiveEvent::Log(3));
        g.absorb(&mut d);
        assert!(g.progress.scanned(Species::Jelly));
        assert!(g.progress.relays.contains(&2));
        assert!(g.progress.logs.contains(&3));
        assert_eq!(g.progress.start_options(), vec![0, 2]);
    }

    #[test]
    fn ending_flow() {
        let mut g = Fathom::new();
        g.start_dive(0);
        let Scene::Dive(d) = std::mem::replace(&mut g.scene, hub()) else { panic!() };
        g.finish_dive(&d, Outcome::Ending(Ending::Answered));
        assert!(matches!(g.scene, Scene::Ending { .. }));
        assert_eq!(g.progress.ending, Some(Ending::Answered));
        for _ in 0..5 {
            g.update(0.1, &press(Key::Enter));
        }
        assert!(matches!(g.scene, Scene::Hub { .. }));
    }
}
