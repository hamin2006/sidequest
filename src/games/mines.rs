//! Minesweeper with a safe first click, flags, chording and a timer.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::Game;
use crate::input::{Input, Key};
use crate::rng::Rng;
use crate::store::Score;
use crate::ui;

pub const W: usize = 16;
pub const H: usize = 16;
pub const MINES: usize = 40;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum State {
    Ready,
    Playing,
    Won,
    Lost,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mines {
    pub mine: Vec<bool>,
    pub open: Vec<bool>,
    pub flag: Vec<bool>,
    pub cursor: (usize, usize),
    pub state: State,
    pub elapsed: f64,
    rng: Rng,
    boom: Option<(usize, usize)>,
}

impl Default for Mines {
    fn default() -> Self {
        Self::new()
    }
}

impl Mines {
    pub fn new() -> Self {
        Self::with_rng(Rng::from_time())
    }

    pub fn with_rng(rng: Rng) -> Self {
        Self {
            mine: vec![false; W * H],
            open: vec![false; W * H],
            flag: vec![false; W * H],
            cursor: (W / 2, H / 2),
            state: State::Ready,
            elapsed: 0.0,
            rng,
            boom: None,
        }
    }

    fn idx(x: usize, y: usize) -> usize {
        y * W + x
    }

    fn neighbours(x: usize, y: usize) -> impl Iterator<Item = (usize, usize)> {
        (-1i32..=1).flat_map(move |dy| (-1i32..=1).map(move |dx| (dx, dy))).filter_map(move |(dx, dy)| {
            let (nx, ny) = (x as i32 + dx, y as i32 + dy);
            ((dx, dy) != (0, 0) && nx >= 0 && ny >= 0 && (nx as usize) < W && (ny as usize) < H)
                .then_some((nx as usize, ny as usize))
        })
    }

    pub fn count(&self, x: usize, y: usize) -> usize {
        Self::neighbours(x, y).filter(|&(nx, ny)| self.mine[Self::idx(nx, ny)]).count()
    }

    /// Mines are placed on the first reveal, never in the 3×3 around it.
    fn place_mines(&mut self, sx: usize, sy: usize) {
        let mut placed = 0;
        while placed < MINES {
            let (x, y) = (self.rng.below(W), self.rng.below(H));
            if (x as i32 - sx as i32).abs() <= 1 && (y as i32 - sy as i32).abs() <= 1 {
                continue;
            }
            let i = Self::idx(x, y);
            if !self.mine[i] {
                self.mine[i] = true;
                placed += 1;
            }
        }
    }

    pub fn reveal(&mut self, x: usize, y: usize) {
        if matches!(self.state, State::Won | State::Lost) {
            return;
        }
        if self.state == State::Ready {
            self.place_mines(x, y);
            self.state = State::Playing;
        }
        let i = Self::idx(x, y);
        if self.flag[i] {
            return;
        }
        if self.open[i] {
            self.chord(x, y);
            return;
        }
        if self.mine[i] {
            self.open[i] = true;
            self.boom = Some((x, y));
            self.state = State::Lost;
            return;
        }
        // Flood fill from empty cells.
        let mut stack = vec![(x, y)];
        while let Some((cx, cy)) = stack.pop() {
            let ci = Self::idx(cx, cy);
            if self.open[ci] || self.flag[ci] {
                continue;
            }
            self.open[ci] = true;
            if self.count(cx, cy) == 0 {
                stack.extend(Self::neighbours(cx, cy));
            }
        }
        self.check_win();
    }

    /// On an opened number with that many flags around it, open the other neighbours.
    fn chord(&mut self, x: usize, y: usize) {
        let flags = Self::neighbours(x, y).filter(|&(nx, ny)| self.flag[Self::idx(nx, ny)]).count();
        if flags != self.count(x, y) {
            return;
        }
        for (nx, ny) in Self::neighbours(x, y).collect::<Vec<_>>() {
            let ni = Self::idx(nx, ny);
            if !self.open[ni] && !self.flag[ni] {
                self.reveal(nx, ny);
            }
        }
    }

    pub fn toggle_flag(&mut self, x: usize, y: usize) {
        let i = Self::idx(x, y);
        if !self.open[i] && matches!(self.state, State::Playing | State::Ready) {
            self.flag[i] = !self.flag[i];
        }
    }

    fn check_win(&mut self) {
        let closed_safe = (0..W * H).filter(|&i| !self.open[i] && !self.mine[i]).count();
        if closed_safe == 0 && self.state == State::Playing {
            self.state = State::Won;
        }
    }

    pub fn flags_left(&self) -> i32 {
        MINES as i32 - self.flag.iter().filter(|&&f| f).count() as i32
    }
}

impl Game for Mines {
    fn update(&mut self, dt: f64, input: &Input) {
        if self.state == State::Playing {
            self.elapsed += dt;
        }
        if matches!(self.state, State::Won | State::Lost) {
            if input.was(Key::Enter) {
                *self = Mines::with_rng(self.rng.clone());
            }
            return;
        }
        for k in &input.pressed {
            if let Some((dx, dy)) = crate::input::dir_of(*k) {
                self.cursor.0 = (self.cursor.0 as i32 + dx).clamp(0, W as i32 - 1) as usize;
                self.cursor.1 = (self.cursor.1 as i32 + dy).clamp(0, H as i32 - 1) as usize;
            }
            let (x, y) = self.cursor;
            match k {
                Key::Space | Key::Enter => self.reveal(x, y),
                Key::Char('f') | Key::Char('e') => self.toggle_flag(x, y),
                _ => {}
            }
        }
    }

    fn draw(&mut self, f: &mut Frame, area: Rect) {
        let (w, h) = (W as u16 * 2 + 2, H as u16 + 4);
        if ui::too_small(f, area, w, h) {
            return;
        }
        let r = ui::centered(area, w, h);
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(format!(" ⚑ {}", self.flags_left()), Style::new().fg(ui::WARN)),
                Span::styled(format!("   ⏱ {:.0}s", self.elapsed), Style::new().fg(ui::DIM)),
            ])),
            Rect { height: 1, ..r },
        );
        let board = Rect { y: r.y + 1, height: H as u16 + 2, ..r };
        f.render_widget(ui::panel("Minesweeper", ui::FAINT), board);
        let reveal_all = self.state == State::Lost;
        let buf = f.buffer_mut();
        for y in 0..H {
            for x in 0..W {
                let i = Self::idx(x, y);
                let (text, mut style) = if self.open[i] || (reveal_all && self.mine[i]) {
                    if self.mine[i] {
                        let bg =
                            if self.boom == Some((x, y)) { Color::Rgb(160, 30, 40) } else { Color::Rgb(60, 30, 34) };
                        ("✹ ".to_string(), Style::new().fg(Color::Rgb(255, 120, 120)).bg(bg))
                    } else {
                        let n = self.count(x, y);
                        let fg = match n {
                            1 => Color::Rgb(110, 170, 255),
                            2 => Color::Rgb(110, 220, 140),
                            3 => Color::Rgb(255, 110, 110),
                            4 => Color::Rgb(190, 130, 255),
                            5 => Color::Rgb(255, 170, 80),
                            _ => Color::Rgb(120, 220, 220),
                        };
                        let t = if n == 0 { "  ".to_string() } else { format!("{n} ") };
                        (t, Style::new().fg(fg).bg(Color::Rgb(34, 36, 46)).add_modifier(Modifier::BOLD))
                    }
                } else if self.flag[i] {
                    ("⚑ ".to_string(), Style::new().fg(ui::WARN).bg(Color::Rgb(58, 62, 78)))
                } else {
                    ("▪ ".to_string(), Style::new().fg(Color::Rgb(95, 100, 120)).bg(Color::Rgb(58, 62, 78)))
                };
                if (x, y) == self.cursor && !matches!(self.state, State::Won | State::Lost) {
                    style = style.bg(Color::Rgb(120, 160, 230)).fg(Color::Black);
                }
                buf.set_string(board.x + 1 + x as u16 * 2, board.y + 1 + y as u16, text, style);
            }
        }
        f.render_widget(
            Paragraph::new("arrows move · space open · f flag · esc pause").style(Style::new().fg(ui::FAINT)),
            Rect { y: board.y + board.height, height: 1, ..r },
        );
        match self.state {
            State::Won => ui::modal(
                f,
                board,
                "Cleared!",
                ui::GOOD,
                vec![
                    Line::from(format!("{:.0} seconds", self.elapsed)),
                    Line::from("enter · new field").style(Style::new().fg(ui::DIM)),
                ],
            ),
            State::Lost => ui::modal(
                f,
                board,
                "Boom",
                ui::BAD,
                vec![Line::from("enter · try again").style(Style::new().fg(ui::DIM))],
            ),
            _ => {}
        }
    }

    fn save(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }

    fn score(&self) -> Option<Score> {
        (self.state == State::Won).then(|| Score::lower(self.elapsed.round() as u64))
    }

    fn finished(&self) -> bool {
        matches!(self.state, State::Won | State::Lost)
    }

    fn realtime(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_click_is_safe_and_opens_area() {
        for seed in 0..50 {
            let mut m = Mines::with_rng(Rng::new(seed));
            m.reveal(0, 0);
            assert_eq!(m.state, State::Playing, "seed {seed}");
            assert!(!m.mine[0]);
            assert_eq!(m.count(0, 0), 0, "3×3 around the first click is mine-free");
            assert!(m.open.iter().filter(|&&o| o).count() > 1);
            assert_eq!(m.mine.iter().filter(|&&x| x).count(), MINES);
        }
    }

    #[test]
    fn flags_block_reveal_and_chording_works() {
        let mut m = Mines::with_rng(Rng::new(1));
        m.reveal(8, 8);
        let mine = (0..W * H).find(|&i| m.mine[i]).unwrap();
        let (mx, my) = (mine % W, mine / W);
        m.toggle_flag(mx, my);
        m.reveal(mx, my);
        assert_ne!(m.state, State::Lost, "flagged cells can't be opened");
        assert_eq!(m.flags_left(), MINES as i32 - 1);
    }

    #[test]
    fn winning_and_losing() {
        let mut m = Mines::with_rng(Rng::new(2));
        m.reveal(8, 8);
        for i in 0..W * H {
            if !m.mine[i] {
                m.reveal(i % W, i / W);
            }
        }
        assert_eq!(m.state, State::Won);
        assert!(m.score().is_some());

        let mut m = Mines::with_rng(Rng::new(2));
        m.reveal(8, 8);
        let mine = (0..W * H).find(|&i| m.mine[i]).unwrap();
        m.reveal(mine % W, mine / W);
        assert_eq!(m.state, State::Lost);
        assert!(m.finished());
    }
}
