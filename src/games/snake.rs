//! Classic Snake on a fixed board. Turns are buffered so quick double-taps aren't lost.

use std::collections::VecDeque;

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

pub const W: i32 = 24;
pub const H: i32 = 16;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snake {
    pub body: VecDeque<(i32, i32)>,
    pub dir: (i32, i32),
    queue: VecDeque<(i32, i32)>,
    pub food: (i32, i32),
    rng: Rng,
    acc: f64,
    pub speed: f64,
    pub score: u64,
    pub over: bool,
}

impl Default for Snake {
    fn default() -> Self {
        Self::new()
    }
}

impl Snake {
    pub fn new() -> Self {
        Self::with_rng(Rng::from_time())
    }

    pub fn with_rng(rng: Rng) -> Self {
        let mut s = Self {
            body: [(6, H / 2), (5, H / 2), (4, H / 2)].into_iter().collect(),
            dir: (1, 0),
            queue: VecDeque::new(),
            food: (0, 0),
            rng,
            acc: 0.0,
            speed: 7.0,
            score: 0,
            over: false,
        };
        s.place_food();
        s
    }

    fn place_food(&mut self) {
        let free: Vec<(i32, i32)> =
            (0..H).flat_map(|y| (0..W).map(move |x| (x, y))).filter(|c| !self.body.contains(c)).collect();
        if let Some(&c) = free.get(self.rng.below(free.len())) {
            self.food = c;
        }
    }

    /// Queue a turn; reversing straight into yourself is ignored.
    pub fn turn(&mut self, d: (i32, i32)) {
        let last = self.queue.back().copied().unwrap_or(self.dir);
        if d == last || (d.0 == -last.0 && d.1 == -last.1) || self.queue.len() >= 3 {
            return;
        }
        self.queue.push_back(d);
    }

    pub fn step(&mut self) {
        if let Some(d) = self.queue.pop_front() {
            self.dir = d;
        }
        let head = self.body[0];
        let next = (head.0 + self.dir.0, head.1 + self.dir.1);
        let eating = next == self.food;
        // The tail moves out of the way this step unless we're growing.
        let tail_free = if eating { 0 } else { 1 };
        let hits_self = self.body.iter().take(self.body.len() - tail_free).any(|c| *c == next);
        if next.0 < 0 || next.1 < 0 || next.0 >= W || next.1 >= H || hits_self {
            self.over = true;
            return;
        }
        self.body.push_front(next);
        if eating {
            self.score += 10;
            self.speed = (self.speed + 0.3).min(16.0);
            if self.body.len() as i32 >= W * H {
                self.over = true;
                return;
            }
            self.place_food();
        } else {
            self.body.pop_back();
        }
    }
}

impl Game for Snake {
    fn update(&mut self, dt: f64, input: &Input) {
        if self.over {
            if input.was(Key::Enter) || input.was(Key::Space) {
                *self = Snake::with_rng(self.rng.clone());
            }
            return;
        }
        for k in &input.pressed {
            if let Some(d) = crate::input::dir_of(*k) {
                self.turn(d);
            }
        }
        self.acc += dt.min(0.25);
        let period = 1.0 / self.speed;
        while self.acc >= period && !self.over {
            self.acc -= period;
            self.step();
        }
    }

    fn draw(&mut self, f: &mut Frame, area: Rect) {
        let bw = W as u16 * 2 + 2;
        let bh = H as u16 + 2;
        if ui::too_small(f, area, bw, bh + 2) {
            return;
        }
        let r = ui::centered(area, bw, bh + 2);
        let head = Line::from(vec![
            Span::styled(format!(" Score {} ", self.score), Style::new().fg(ui::GOOD)),
            Span::styled(format!(" length {}  speed {:.0}", self.body.len(), self.speed), Style::new().fg(ui::DIM)),
        ]);
        f.render_widget(Paragraph::new(head), Rect { height: 1, ..r });
        let board = Rect { y: r.y + 1, height: bh, ..r };
        f.render_widget(ui::panel("Snake", ui::FAINT).style(Style::new()), board);
        let buf = f.buffer_mut();
        let n = self.body.len().max(1) as f32;
        for (i, &(x, y)) in self.body.iter().enumerate() {
            let t = i as f32 / n;
            let g = (230.0 - 120.0 * t) as u8;
            let color = if i == 0 { Color::Rgb(170, 255, 170) } else { Color::Rgb(60, g, 90) };
            let cx = board.x + 1 + x as u16 * 2;
            let cy = board.y + 1 + y as u16;
            buf.set_string(cx, cy, if i == 0 { "██" } else { "▓▓" }, Style::new().fg(color));
        }
        let (fx, fy) = self.food;
        buf.set_string(
            board.x + 1 + fx as u16 * 2,
            board.y + 1 + fy as u16,
            "●",
            Style::new().fg(Color::Rgb(255, 90, 110)),
        );
        f.render_widget(
            Paragraph::new("arrows/WASD steer · esc pause").style(Style::new().fg(ui::FAINT)),
            Rect { y: board.y + bh, height: 1, ..r },
        );
        if self.over {
            ui::modal(
                f,
                board,
                "Game over",
                ui::BAD,
                vec![
                    Line::from(format!("Score {}", self.score)),
                    Line::from(""),
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

    fn snake() -> Snake {
        let mut s = Snake::with_rng(Rng::new(1));
        s.food = (20, 0);
        s
    }

    #[test]
    fn moves_eats_and_grows() {
        let mut s = snake();
        s.food = (7, H / 2);
        s.step();
        assert_eq!(s.body[0], (7, H / 2));
        assert_eq!(s.body.len(), 4);
        assert_eq!(s.score, 10);
        assert!(!s.body.contains(&s.food), "food never spawns on the snake");
    }

    #[test]
    fn walls_and_self_kill() {
        let mut s = snake();
        for _ in 0..30 {
            s.step();
        }
        assert!(s.over, "ran into the right wall");

        let mut s = snake();
        s.body = [(5, 5), (6, 5), (6, 6), (5, 6), (4, 6)].into_iter().collect();
        s.dir = (0, 1);
        s.step();
        assert!(s.over, "bit itself");
    }

    #[test]
    fn chasing_the_tail_is_allowed() {
        let mut s = snake();
        // Square loop: head moves into the cell the tail is leaving.
        s.body = [(5, 5), (5, 6), (6, 6), (6, 5)].into_iter().collect();
        s.dir = (1, 0);
        s.step();
        assert!(!s.over);
    }

    #[test]
    fn turn_buffer_rejects_reversals() {
        let mut s = snake();
        s.turn((-1, 0));
        assert!(s.queue.is_empty(), "can't reverse");
        s.turn((0, -1));
        s.turn((-1, 0)); // valid after turning up
        assert_eq!(s.queue.len(), 2);
        s.turn((-1, 0));
        assert_eq!(s.queue.len(), 2, "duplicates ignored");
    }

    #[test]
    fn save_round_trip() {
        let s = snake();
        let back: Snake = serde_json::from_value(s.save()).unwrap();
        assert_eq!(back.body, s.body);
    }
}
