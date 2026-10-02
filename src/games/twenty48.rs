//! 2048: slide tiles, merge equal neighbours, reach 2048 (and keep going if you like).

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

type Board = [[u32; 4]; 4];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Twenty48 {
    pub board: Board,
    pub score: u64,
    rng: Rng,
    undo: Option<(Board, u64)>,
    pub won: bool,
    keep_going: bool,
}

impl Default for Twenty48 {
    fn default() -> Self {
        Self::new()
    }
}

/// Slides one row to the left, merging pairs once. Returns the new row and points gained.
pub fn slide_row(row: [u32; 4]) -> ([u32; 4], u64) {
    let tiles: Vec<u32> = row.into_iter().filter(|&v| v != 0).collect();
    let mut out = [0u32; 4];
    let mut points = 0;
    let (mut i, mut o) = (0, 0);
    while i < tiles.len() {
        if i + 1 < tiles.len() && tiles[i] == tiles[i + 1] {
            out[o] = tiles[i] * 2;
            points += out[o] as u64;
            i += 2;
        } else {
            out[o] = tiles[i];
            i += 1;
        }
        o += 1;
    }
    (out, points)
}

fn rotate(b: &Board) -> Board {
    let mut r = [[0; 4]; 4];
    for (y, row) in r.iter_mut().enumerate() {
        for (x, cell) in row.iter_mut().enumerate() {
            *cell = b[3 - x][y];
        }
    }
    r
}

/// Applies a move in direction `d` (dx, dy). Returns the new board and points.
pub fn shift(b: &Board, d: (i32, i32)) -> (Board, u64) {
    // Rotate so the move becomes "left", slide, rotate back.
    let turns = match d {
        (-1, 0) => 0,
        (0, 1) => 1,
        (1, 0) => 2,
        _ => 3,
    };
    let mut cur = *b;
    for _ in 0..turns {
        cur = rotate(&cur);
    }
    let mut points = 0;
    for row in cur.iter_mut() {
        let (r, p) = slide_row(*row);
        *row = r;
        points += p;
    }
    for _ in 0..(4 - turns) % 4 {
        cur = rotate(&cur);
    }
    (cur, points)
}

pub fn can_move(b: &Board) -> bool {
    [(-1, 0), (1, 0), (0, -1), (0, 1)].iter().any(|&d| shift(b, d).0 != *b)
}

impl Twenty48 {
    pub fn new() -> Self {
        Self::with_rng(Rng::from_time())
    }

    pub fn with_rng(rng: Rng) -> Self {
        let mut g = Self { board: [[0; 4]; 4], score: 0, rng, undo: None, won: false, keep_going: false };
        g.spawn();
        g.spawn();
        g
    }

    fn spawn(&mut self) {
        let empty: Vec<(usize, usize)> =
            (0..4).flat_map(|y| (0..4).map(move |x| (x, y))).filter(|&(x, y)| self.board[y][x] == 0).collect();
        if let Some(&(x, y)) = empty.get(self.rng.below(empty.len())) {
            self.board[y][x] = if self.rng.chance(0.9) { 2 } else { 4 };
        }
    }

    pub fn play(&mut self, d: (i32, i32)) -> bool {
        let (next, points) = shift(&self.board, d);
        if next == self.board {
            return false;
        }
        self.undo = Some((self.board, self.score));
        self.board = next;
        self.score += points;
        self.spawn();
        if !self.won && self.board.iter().flatten().any(|&v| v >= 2048) {
            self.won = true;
        }
        true
    }

    pub fn over(&self) -> bool {
        !can_move(&self.board)
    }
}

fn tile_style(v: u32) -> Style {
    let (bg, fg) = match v {
        0 => (Color::Rgb(40, 42, 52), Color::Rgb(40, 42, 52)),
        2 => (Color::Rgb(238, 228, 218), Color::Rgb(90, 80, 70)),
        4 => (Color::Rgb(237, 224, 200), Color::Rgb(90, 80, 70)),
        8 => (Color::Rgb(242, 177, 121), Color::White),
        16 => (Color::Rgb(245, 149, 99), Color::White),
        32 => (Color::Rgb(246, 124, 95), Color::White),
        64 => (Color::Rgb(246, 94, 59), Color::White),
        128 => (Color::Rgb(237, 207, 114), Color::White),
        256 => (Color::Rgb(237, 204, 97), Color::White),
        512 => (Color::Rgb(237, 200, 80), Color::White),
        1024 => (Color::Rgb(237, 197, 63), Color::White),
        2048 => (Color::Rgb(237, 194, 46), Color::White),
        _ => (Color::Rgb(60, 58, 50), Color::Rgb(255, 230, 120)),
    };
    Style::new().bg(bg).fg(fg).add_modifier(Modifier::BOLD)
}

impl Game for Twenty48 {
    fn update(&mut self, _dt: f64, input: &Input) {
        if self.over() {
            if input.was(Key::Enter) {
                *self = Twenty48::with_rng(self.rng.clone());
            }
            return;
        }
        if self.won && !self.keep_going {
            if input.any() {
                self.keep_going = true;
            }
            return;
        }
        for k in &input.pressed {
            if let Some(d) = crate::input::dir_of(*k) {
                self.play(d);
            } else if *k == Key::Char('u')
                && let Some((b, s)) = self.undo.take()
            {
                self.board = b;
                self.score = s;
            }
        }
    }

    fn draw(&mut self, f: &mut Frame, area: Rect) {
        let (tw, th) = (8u16, 3u16);
        let (w, h) = (tw * 4 + 5, th * 4 + 5 + 2);
        if ui::too_small(f, area, w, h) {
            return;
        }
        let r = ui::centered(area, w, h);
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(format!(" Score {}", self.score), Style::new().fg(ui::GOOD)),
                Span::styled(if self.undo.is_some() { "   u undo" } else { "" }, Style::new().fg(ui::FAINT)),
            ])),
            Rect { height: 1, ..r },
        );
        let board = Rect { y: r.y + 1, height: th * 4 + 5, ..r };
        let buf = f.buffer_mut();
        buf.set_style(board, Style::new().bg(Color::Rgb(28, 30, 38)));
        for (y, row) in self.board.iter().enumerate() {
            for (x, &v) in row.iter().enumerate() {
                let tx = board.x + 1 + x as u16 * (tw + 1);
                let ty = board.y + 1 + y as u16 * (th + 1);
                let style = tile_style(v);
                for dy in 0..th {
                    buf.set_string(tx, ty + dy, " ".repeat(tw as usize), style);
                }
                if v > 0 {
                    let label = v.to_string();
                    let lx = tx + (tw - label.len() as u16) / 2;
                    buf.set_string(lx, ty + 1, &label, style);
                }
            }
        }
        f.render_widget(
            Paragraph::new("arrows/WASD slide · esc pause").style(Style::new().fg(ui::FAINT)),
            Rect { y: board.y + board.height, height: 1, ..r },
        );
        if self.over() {
            ui::modal(
                f,
                board,
                "No moves left",
                ui::BAD,
                vec![
                    Line::from(format!("Score {}", self.score)),
                    Line::from("enter · new game").style(Style::new().fg(ui::DIM)),
                ],
            );
        } else if self.won && !self.keep_going {
            ui::modal(
                f,
                board,
                "2048!",
                ui::WARN,
                vec![Line::from("You made 2048."), Line::from("any key · keep going").style(Style::new().fg(ui::DIM))],
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
        self.over()
    }

    fn realtime(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_merge_once_per_pair() {
        assert_eq!(slide_row([2, 2, 2, 2]), ([4, 4, 0, 0], 8));
        assert_eq!(slide_row([2, 0, 2, 4]), ([4, 4, 0, 0], 4));
        assert_eq!(slide_row([4, 4, 8, 0]), ([8, 8, 0, 0], 8));
        assert_eq!(slide_row([0, 0, 0, 2]), ([2, 0, 0, 0], 0));
        assert_eq!(slide_row([2, 4, 8, 16]), ([2, 4, 8, 16], 0));
    }

    #[test]
    fn shifts_in_every_direction() {
        let b: Board = [[2, 0, 0, 2], [0, 0, 0, 0], [0, 0, 0, 0], [2, 0, 0, 0]];
        assert_eq!(shift(&b, (-1, 0)).0[0], [4, 0, 0, 0]);
        assert_eq!(shift(&b, (1, 0)).0[0], [0, 0, 0, 4]);
        let up = shift(&b, (0, -1)).0;
        assert_eq!(up[0][0], 4);
        assert_eq!(up[3][0], 0);
        let down = shift(&b, (0, 1)).0;
        assert_eq!(down[3][0], 4);
        assert_eq!(down[3][3], 2);
    }

    #[test]
    fn detects_game_over_and_undo() {
        let stuck: Board = [[2, 4, 2, 4], [4, 2, 4, 2], [2, 4, 2, 4], [4, 2, 4, 2]];
        assert!(!can_move(&stuck));
        let mut g = Twenty48::with_rng(Rng::new(3));
        g.board = [[2, 2, 0, 0], [0; 4], [0; 4], [0; 4]];
        assert!(g.play((-1, 0)));
        assert_eq!(g.score, 4);
        let i = Input { pressed: vec![Key::Char('u')], ..Input::default() };
        g.update(0.0, &i);
        assert_eq!(g.board[0], [2, 2, 0, 0], "undo restored the previous position");
        assert_eq!(g.score, 0);
    }
}
