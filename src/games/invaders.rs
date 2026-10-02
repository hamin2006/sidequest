//! Space Invaders: a marching grid, crumbling shields, a bonus saucer and speed-ups as they thin out.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::Game;
use crate::input::{Input, Key};
use crate::rng::Rng;
use crate::store::Score;
use crate::ui;

pub const W: i32 = 60;
pub const H: i32 = 22;
const COLS: i32 = 9;
const ROWS: i32 = 5;
const SPRITE: [[&str; 2]; 3] = [["{@}", "}@{"], ["/O\\", "\\O/"], ["<#>", ">#<"]];
const POINTS: [u64; 3] = [30, 20, 10];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Alien {
    pub x: i32,
    pub y: i32,
    pub kind: usize,
    /// Column in the formation, used to pick which alien fires.
    pub col: i32,
    pub alive: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Shot {
    pub x: i32,
    pub y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Invaders {
    pub player_x: f64,
    pub shot: Option<Shot>,
    pub bombs: Vec<Shot>,
    pub aliens: Vec<Alien>,
    dir: i32,
    step_timer: f64,
    frame: usize,
    pub shields: Vec<(i32, i32, u8)>,
    ufo: Option<(f64, f64)>,
    ufo_timer: f64,
    bomb_timer: f64,
    pub score: u64,
    pub lives: u32,
    pub wave: u32,
    pub over: bool,
    invuln: f64,
    flashes: Vec<(i32, i32, f64)>,
    rng: Rng,
}

impl Default for Invaders {
    fn default() -> Self {
        Self::new()
    }
}

impl Invaders {
    pub fn new() -> Self {
        Self::with_rng(Rng::from_time())
    }

    pub fn with_rng(rng: Rng) -> Self {
        let mut g = Self {
            player_x: W as f64 / 2.0,
            shot: None,
            bombs: vec![],
            aliens: vec![],
            dir: 1,
            step_timer: 0.0,
            frame: 0,
            shields: vec![],
            ufo: None,
            ufo_timer: 18.0,
            bomb_timer: 1.5,
            score: 0,
            lives: 3,
            wave: 1,
            over: false,
            invuln: 0.0,
            flashes: vec![],
            rng,
        };
        g.start_wave();
        g
    }

    fn start_wave(&mut self) {
        let top = 2 + (self.wave as i32 - 1).min(4);
        self.aliens = (0..ROWS)
            .flat_map(|r| {
                (0..COLS).map(move |c| Alien {
                    x: 6 + c * 5,
                    y: top + r,
                    kind: [0, 1, 1, 2, 2][r as usize],
                    col: c,
                    alive: true,
                })
            })
            .collect();
        self.dir = 1;
        self.shot = None;
        self.bombs.clear();
        self.shields = (0..4)
            .flat_map(|b| (0..2).flat_map(move |dy| (0..6).map(move |dx| (8 + b * 13 + dx, H - 5 + dy, 3u8))))
            .collect();
    }

    pub fn alive(&self) -> usize {
        self.aliens.iter().filter(|a| a.alive).count()
    }

    /// Seconds between marching steps: faster with fewer aliens and in later waves.
    fn step_interval(&self) -> f64 {
        let frac = self.alive() as f64 / (ROWS * COLS) as f64;
        (0.04 + 0.55 * frac) * 0.9f64.powi(self.wave as i32 - 1)
    }

    fn march(&mut self) {
        self.frame ^= 1;
        let hits_edge = self.aliens.iter().filter(|a| a.alive).any(|a| {
            let nx = a.x + self.dir;
            nx < 1 || nx + 3 > W - 1
        });
        if hits_edge {
            self.dir = -self.dir;
            for a in &mut self.aliens {
                a.y += 1;
            }
        } else {
            for a in &mut self.aliens {
                a.x += self.dir;
            }
        }
        if self.aliens.iter().any(|a| a.alive && a.y >= H - 3) {
            self.over = true;
        }
    }

    fn drop_bomb(&mut self) {
        // The lowest alien of a random non-empty column fires.
        let shooters: Vec<(i32, i32)> = (0..COLS)
            .filter_map(|c| {
                self.aliens.iter().filter(|a| a.alive && a.col == c).max_by_key(|a| a.y).map(|a| (a.x, a.y))
            })
            .collect();
        if let Some(&(x, y)) = shooters.get(self.rng.below(shooters.len())) {
            self.bombs.push(Shot { x: x + 1, y: y as f64 + 1.0 });
        }
    }

    fn hit_shield(&mut self, x: i32, y: i32) -> bool {
        if let Some(s) = self.shields.iter_mut().find(|s| s.0 == x && s.1 == y && s.2 > 0) {
            s.2 -= 1;
            return true;
        }
        false
    }

    pub fn tick(&mut self, dt: f64, left: bool, right: bool, fire: bool) {
        let dt = dt.min(0.1);
        self.invuln = (self.invuln - dt).max(0.0);
        self.flashes.retain_mut(|f| {
            f.2 -= dt;
            f.2 > 0.0
        });
        let speed = 32.0;
        if left {
            self.player_x -= speed * dt;
        }
        if right {
            self.player_x += speed * dt;
        }
        self.player_x = self.player_x.clamp(2.0, W as f64 - 3.0);
        if fire && self.shot.is_none() {
            self.shot = Some(Shot { x: self.player_x.round() as i32, y: (H - 2) as f64 });
        }

        self.step_timer += dt;
        let interval = self.step_interval();
        while self.step_timer >= interval {
            self.step_timer -= interval;
            self.march();
        }

        // Player shot.
        if let Some(mut s) = self.shot.take() {
            let from = s.y;
            s.y -= 40.0 * dt;
            let mut alive = s.y >= 0.0;
            for y in (s.y.floor() as i32..=from.floor() as i32).rev() {
                if !alive {
                    break;
                }
                if self.hit_shield(s.x, y) {
                    alive = false;
                    break;
                }
                if let Some(a) = self.aliens.iter_mut().find(|a| a.alive && a.y == y && s.x >= a.x && s.x < a.x + 3) {
                    a.alive = false;
                    self.score += POINTS[a.kind];
                    self.flashes.push((a.x, a.y, 0.25));
                    alive = false;
                    break;
                }
                if let Some((ux, _)) = self.ufo
                    && y == 1
                    && (s.x as f64) >= ux
                    && (s.x as f64) < ux + 5.0
                {
                    self.score += [50, 100, 150, 300][self.rng.below(4)];
                    self.flashes.push((ux as i32, 1, 0.4));
                    self.ufo = None;
                    alive = false;
                    break;
                }
            }
            if alive {
                self.shot = Some(s);
            }
        }

        // Bombs.
        self.bomb_timer -= dt;
        if self.bomb_timer <= 0.0 {
            self.bomb_timer = (1.2 - 0.08 * self.wave as f64).max(0.35) * (0.6 + self.rng.f64());
            self.drop_bomb();
        }
        let px = self.player_x.round() as i32;
        let mut bombs = std::mem::take(&mut self.bombs);
        bombs.retain_mut(|b| {
            let from = b.y;
            b.y += 14.0 * dt;
            for y in from.floor() as i32..=b.y.floor() as i32 {
                if self.hit_shield(b.x, y) {
                    return false;
                }
                if y == H - 1 && (b.x - px).abs() <= 1 && self.invuln <= 0.0 {
                    self.lives = self.lives.saturating_sub(1);
                    self.invuln = 1.5;
                    self.flashes.push((px - 1, H - 1, 0.6));
                    if self.lives == 0 {
                        self.over = true;
                    }
                    return false;
                }
            }
            b.y < H as f64
        });
        self.bombs = bombs;

        // Saucer.
        self.ufo_timer -= dt;
        if self.ufo.is_none() && self.ufo_timer <= 0.0 {
            self.ufo_timer = 20.0 + self.rng.f64() * 15.0;
            self.ufo = Some(if self.rng.chance(0.5) { (0.0, 12.0) } else { (W as f64 - 5.0, -12.0) });
        }
        if let Some((x, v)) = self.ufo {
            let nx = x + v * dt;
            self.ufo = (nx > -5.0 && nx < W as f64).then_some((nx, v));
        }

        if self.alive() == 0 && !self.over {
            self.wave += 1;
            self.lives = (self.lives + 1).min(5);
            self.start_wave();
        }
    }
}

impl Game for Invaders {
    fn update(&mut self, dt: f64, input: &Input) {
        if self.over {
            if input.was(Key::Enter) {
                *self = Invaders::with_rng(self.rng.clone());
            }
            return;
        }
        let left = input.is_held(Key::Left)
            || input.is_held(Key::Char('a'))
            || input.was(Key::Left)
            || input.was(Key::Char('a'));
        let right = input.is_held(Key::Right)
            || input.is_held(Key::Char('d'))
            || input.was(Key::Right)
            || input.was(Key::Char('d'));
        let fire = input.was(Key::Space) || input.is_held(Key::Space) || input.was(Key::Up) || input.char_pressed('w');
        self.tick(dt, left, right, fire);
    }

    fn draw(&mut self, f: &mut Frame, area: Rect) {
        let (w, h) = (W as u16 + 2, H as u16 + 4);
        if ui::too_small(f, area, w, h) {
            return;
        }
        let r = ui::centered(area, w, h);
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(format!(" Score {}", self.score), Style::new().fg(ui::GOOD)),
                Span::styled(format!("   Wave {}", self.wave), Style::new().fg(ui::DIM)),
                Span::styled(format!("   {}", "▲ ".repeat(self.lives as usize)), Style::new().fg(ui::ACCENT)),
            ])),
            Rect { height: 1, ..r },
        );
        let field = Rect { y: r.y + 1, height: H as u16 + 2, ..r };
        f.render_widget(ui::panel("Space Invaders", ui::FAINT).style(Style::new().bg(Color::Rgb(8, 9, 16))), field);
        let (ox, oy) = (field.x + 1, field.y + 1);
        let buf = f.buffer_mut();
        let put = |buf: &mut ratatui::buffer::Buffer, x: i32, y: i32, s: &str, st: Style| {
            if x >= 0 && y >= 0 && x < W && y < H {
                let max = (W - x) as usize;
                let s: String = s.chars().take(max).collect();
                buf.set_string(ox + x as u16, oy + y as u16, s, st);
            }
        };
        for &(x, y, hp) in &self.shields {
            if hp > 0 {
                let g = ["", "░", "▒", "█"][hp as usize];
                put(buf, x, y, g, Style::new().fg(Color::Rgb(90, 200, 120)));
            }
        }
        let colors = [Color::Rgb(230, 120, 255), Color::Rgb(110, 210, 255), Color::Rgb(140, 240, 140)];
        for a in self.aliens.iter().filter(|a| a.alive) {
            put(buf, a.x, a.y, SPRITE[a.kind][self.frame], Style::new().fg(colors[a.kind]));
        }
        if let Some((x, _)) = self.ufo {
            put(buf, x as i32, 1, "<=O=>", Style::new().fg(Color::Rgb(255, 90, 90)));
        }
        if let Some(s) = &self.shot {
            put(buf, s.x, s.y as i32, "|", Style::new().fg(Color::Rgb(255, 240, 140)));
        }
        for b in &self.bombs {
            put(
                buf,
                b.x,
                b.y as i32,
                if (b.y * 4.0) as i32 % 2 == 0 { "!" } else { "¡" },
                Style::new().fg(Color::Rgb(255, 120, 90)),
            );
        }
        for &(x, y, _) in &self.flashes {
            put(buf, x, y, "***", Style::new().fg(Color::Rgb(255, 220, 120)));
        }
        let blink = self.invuln > 0.0 && (self.invuln * 8.0) as i32 % 2 == 0;
        if !blink {
            put(buf, self.player_x.round() as i32 - 1, H - 1, "/▲\\", Style::new().fg(Color::Rgb(140, 200, 255)));
        }
        f.render_widget(
            Paragraph::new("←/→ move · space fire · esc pause").style(Style::new().fg(ui::FAINT)),
            Rect { y: field.y + field.height, height: 1, ..r },
        );
        if self.over {
            ui::modal(
                f,
                field,
                "Game over",
                ui::BAD,
                vec![
                    Line::from(format!("Score {} · wave {}", self.score, self.wave)),
                    Line::from("enter · play again").style(Style::new().fg(ui::DIM)),
                ],
            );
        }
    }

    fn save(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }

    fn score(&self) -> Option<Score> {
        (self.score > 0).then(|| Score::higher(self.score))
    }

    fn finished(&self) -> bool {
        self.over
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game() -> Invaders {
        Invaders::with_rng(Rng::new(9))
    }

    #[test]
    fn shooting_an_alien_scores() {
        let mut g = game();
        g.bomb_timer = 99.0;
        let target = g.aliens.iter().filter(|a| a.kind == 2).max_by_key(|a| a.y).unwrap().clone();
        g.player_x = (target.x + 1) as f64;
        // Clear the shield in the way.
        g.shields.retain(|s| s.0 != target.x + 1);
        g.step_timer = -10.0; // freeze the march
        g.tick(0.016, false, false, true);
        for _ in 0..60 {
            g.tick(0.016, false, false, false);
        }
        assert!(g.score >= 10, "score {}", g.score);
        assert_eq!(g.alive(), 44);
    }

    #[test]
    fn march_reverses_and_descends_at_edges() {
        let mut g = game();
        let y0 = g.aliens[0].y;
        for _ in 0..40 {
            g.march();
        }
        assert!(g.aliens[0].y > y0, "stepped down at the edge");
        assert!(g.aliens.iter().all(|a| a.x >= 1 && a.x + 3 < W));
    }

    #[test]
    fn bombs_cost_lives_and_end_the_game() {
        let mut g = game();
        g.bomb_timer = 99.0;
        g.step_timer = -1000.0;
        g.shields.clear();
        for _ in 0..3 {
            g.invuln = 0.0;
            g.bombs.push(Shot { x: g.player_x.round() as i32, y: (H - 2) as f64 });
            g.tick(0.1, false, false, false);
        }
        assert_eq!(g.lives, 0);
        assert!(g.over);
    }

    #[test]
    fn clearing_a_wave_starts_the_next() {
        let mut g = game();
        for a in &mut g.aliens {
            a.alive = false;
        }
        g.tick(0.01, false, false, false);
        assert_eq!(g.wave, 2);
        assert_eq!(g.alive(), 45);
        assert!(g.step_interval() < game().step_interval());
    }
}
