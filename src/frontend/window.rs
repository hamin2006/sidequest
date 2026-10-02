//! The floating game window. Each frame the arcade draws into a ratatui buffer (the same code that runs
//! in a terminal), and this module paints that character grid with egui.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};
use eframe::egui::{self, Align2, Color32, FontData, FontDefinitions, FontFamily, FontId, Pos2, Rect as ERect, Vec2};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::style::{Color, Modifier};

use crate::arcade::{Arcade, Mode};
use crate::claude::{self, Event, Kind, Watcher};
use crate::config::Config;
use crate::hooks;
use crate::input::{Input, Key};

const DEFAULT_FG: Color32 = Color32::from_rgb(220, 222, 230);
const DEFAULT_BG: Color32 = Color32::from_rgb(10, 11, 16);

/// Standard 16 ANSI colours plus the xterm 256-colour cube.
pub fn to_color32(c: Color, fg: bool) -> Color32 {
    let ansi = |i: u8| -> Color32 {
        const T: [(u8, u8, u8); 16] = [
            (0, 0, 0),
            (205, 49, 49),
            (13, 188, 121),
            (229, 229, 16),
            (36, 114, 200),
            (188, 63, 188),
            (17, 168, 205),
            (229, 229, 229),
            (102, 102, 102),
            (241, 76, 76),
            (35, 209, 139),
            (245, 245, 67),
            (59, 142, 234),
            (214, 112, 214),
            (41, 184, 219),
            (255, 255, 255),
        ];
        let (r, g, b) = T[i as usize];
        Color32::from_rgb(r, g, b)
    };
    match c {
        Color::Reset => {
            if fg {
                DEFAULT_FG
            } else {
                DEFAULT_BG
            }
        }
        Color::Rgb(r, g, b) => Color32::from_rgb(r, g, b),
        Color::Black => ansi(0),
        Color::Red => ansi(1),
        Color::Green => ansi(2),
        Color::Yellow => ansi(3),
        Color::Blue => ansi(4),
        Color::Magenta => ansi(5),
        Color::Cyan => ansi(6),
        Color::Gray => ansi(7),
        Color::DarkGray => ansi(8),
        Color::LightRed => ansi(9),
        Color::LightGreen => ansi(10),
        Color::LightYellow => ansi(11),
        Color::LightBlue => ansi(12),
        Color::LightMagenta => ansi(13),
        Color::LightCyan => ansi(14),
        Color::White => ansi(15),
        Color::Indexed(i) if i < 16 => ansi(i),
        Color::Indexed(i) if i < 232 => {
            let i = i - 16;
            let level = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
            Color32::from_rgb(level(i / 36), level((i / 6) % 6), level(i % 6))
        }
        Color::Indexed(i) => {
            let v = 8 + (i - 232) * 10;
            Color32::from_rgb(v, v, v)
        }
    }
}

pub fn map_egui_key(k: egui::Key) -> Option<Key> {
    use egui::Key as E;
    Some(match k {
        E::ArrowUp => Key::Up,
        E::ArrowDown => Key::Down,
        E::ArrowLeft => Key::Left,
        E::ArrowRight => Key::Right,
        E::Enter => Key::Enter,
        E::Escape => Key::Esc,
        E::Space => Key::Space,
        E::Tab => Key::Tab,
        E::Backspace => Key::Backspace,
        _ => {
            let name = k.name();
            let mut chars = name.chars();
            let c = chars.next()?;
            if chars.next().is_some() {
                return None;
            }
            Key::Char(c.to_ascii_lowercase())
        }
    })
}

/// Menlo (macOS) for the grid, Apple Symbols as a fallback for rarer glyphs. Silently keeps egui's
/// built-in fonts if the system fonts aren't there (e.g. on Linux).
fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let mut add = |name: &str, path: &str, first: bool| {
        if let Ok(bytes) = std::fs::read(path) {
            fonts.font_data.insert(name.to_string(), Arc::new(FontData::from_owned(bytes)));
            for family in [FontFamily::Monospace, FontFamily::Proportional] {
                let list = fonts.families.entry(family).or_default();
                if first {
                    list.insert(0, name.to_string());
                } else {
                    list.push(name.to_string());
                }
            }
        }
    };
    add("menlo", "/System/Library/Fonts/Menlo.ttc", true);
    add("symbols", "/System/Library/Fonts/Apple Symbols.ttf", false);
    ctx.set_fonts(fonts);
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum BlockPart {
    Full,
    Upper,
    Lower,
}

/// Block and shade characters: how much of the cell to fill and with what intensity.
fn block_shape(sym: &str) -> Option<(f32, BlockPart)> {
    Some(match sym {
        "█" => (1.0, BlockPart::Full),
        "▓" => (0.75, BlockPart::Full),
        "▒" => (0.5, BlockPart::Full),
        "░" => (0.25, BlockPart::Full),
        "▀" => (1.0, BlockPart::Upper),
        "▄" => (1.0, BlockPart::Lower),
        _ => return None,
    })
}

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

struct WindowApp {
    arcade: Arcade,
    term: Terminal<TestBackend>,
    watcher: Watcher,
    font: FontId,
    last: Instant,
    last_poll: Instant,
    first_frame: bool,
    done_term: Arc<Mutex<Option<String>>>,
    /// Debug/testing: keys to inject at given times and a path to save a screenshot of the window.
    script: std::collections::VecDeque<(f64, Key)>,
    snapshot: Option<(std::path::PathBuf, f64, bool)>,
    started: Instant,
}

/// `SIDEQUEST_SCRIPT="0.5:enter,1.0:space"`: keys to press at those times (seconds after opening).
fn parse_script(s: &str) -> std::collections::VecDeque<(f64, Key)> {
    s.split(',')
        .filter_map(|item| {
            let (t, k) = item.trim().split_once(':')?;
            let key = match k {
                "enter" => Key::Enter,
                "esc" => Key::Esc,
                "space" => Key::Space,
                "tab" => Key::Tab,
                "up" => Key::Up,
                "down" => Key::Down,
                "left" => Key::Left,
                "right" => Key::Right,
                c if c.chars().count() == 1 => Key::Char(c.chars().next()?),
                _ => return None,
            };
            Some((t.parse().ok()?, key))
        })
        .collect()
}

/// Minimal 24-bit BMP writer (no image crate needed).
fn write_bmp(path: &std::path::Path, img: &egui::ColorImage) -> std::io::Result<()> {
    let [w, h] = img.size;
    let row = (w * 3).div_ceil(4) * 4;
    let size = 54 + row * h;
    let mut out = Vec::with_capacity(size);
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&(size as u32).to_le_bytes());
    out.extend_from_slice(&[0, 0, 0, 0]);
    out.extend_from_slice(&54u32.to_le_bytes());
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(w as i32).to_le_bytes());
    out.extend_from_slice(&(h as i32).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&24u16.to_le_bytes());
    out.extend_from_slice(&[0u8; 24]);
    for y in (0..h).rev() {
        let start = out.len();
        for x in 0..w {
            let c = img.pixels[y * w + x];
            out.extend_from_slice(&[c.b(), c.g(), c.r()]);
        }
        out.resize(start + row, 0);
    }
    std::fs::write(path, out)
}

impl WindowApp {
    fn gather_input(&self, ctx: &egui::Context) -> (Input, bool) {
        let mut input = Input { held_reliable: true, ..Input::default() };
        let mut close = false;
        ctx.input(|i| {
            for ev in &i.events {
                match ev {
                    egui::Event::Key { key, pressed: true, repeat, modifiers, .. } => {
                        if modifiers.command && *key == egui::Key::W {
                            close = true;
                        }
                        // Named keys come from Key events; letters and symbols from Text (respects layouts).
                        if !*repeat
                            && let Some(k) = map_egui_key(*key)
                            && !matches!(k, Key::Char(_))
                        {
                            input.pressed.push(k);
                        }
                    }
                    egui::Event::Text(t) => {
                        for ch in t.chars().filter(|c| *c != ' ') {
                            input.pressed.push(Key::Char(ch.to_ascii_lowercase()));
                        }
                    }
                    _ => {}
                }
            }
            let held: HashSet<Key> = i.keys_down.iter().filter_map(|k| map_egui_key(*k)).collect();
            input.held = held;
            input.shift = i.modifiers.shift;
            close |= i.viewport().close_requested();
        });
        (input, close)
    }

    fn paint(&self, ui: &egui::Ui, origin: Pos2, cell: Vec2) {
        let painter = ui.painter();
        let buf = self.term.backend().buffer();
        let area = buf.area;
        for y in 0..area.height {
            let top = origin.y + y as f32 * cell.y;
            // Backgrounds, merged into runs.
            let mut x = 0;
            while x < area.width {
                let c = &buf[(x, y)];
                let rev = c.modifier.contains(Modifier::REVERSED);
                let bg = to_color32(if rev { c.fg } else { c.bg }, rev);
                let start = x;
                while x < area.width {
                    let n = &buf[(x, y)];
                    let nrev = n.modifier.contains(Modifier::REVERSED);
                    if to_color32(if nrev { n.fg } else { n.bg }, nrev) != bg {
                        break;
                    }
                    x += 1;
                }
                if bg != DEFAULT_BG {
                    let r = ERect::from_min_size(
                        Pos2::new(origin.x + start as f32 * cell.x, top),
                        Vec2::new((x - start) as f32 * cell.x + 0.5, cell.y + 0.5),
                    );
                    painter.rect_filled(r, 0.0, bg);
                }
            }
            // Glyphs: ASCII in runs, everything else one cell at a time so the grid stays aligned.
            let mut x = 0;
            while x < area.width {
                let c = &buf[(x, y)];
                let sym = c.symbol();
                if sym.trim().is_empty() {
                    x += 1;
                    continue;
                }
                let rev = c.modifier.contains(Modifier::REVERSED);
                let mut fg = to_color32(if rev { c.bg } else { c.fg }, !rev);
                if c.modifier.contains(Modifier::DIM) {
                    fg = fg.gamma_multiply(0.55);
                }
                if sym.is_ascii() {
                    let start = x;
                    let mut run = String::new();
                    while x < area.width {
                        let n = &buf[(x, y)];
                        let s = n.symbol();
                        let nrev = n.modifier.contains(Modifier::REVERSED);
                        let nfg = to_color32(if nrev { n.bg } else { n.fg }, !nrev);
                        if !s.is_ascii()
                            || nfg != to_color32(if rev { c.bg } else { c.fg }, !rev)
                            || n.modifier != c.modifier
                        {
                            break;
                        }
                        run.push_str(if s.is_empty() { " " } else { s });
                        x += 1;
                    }
                    painter.text(
                        Pos2::new(origin.x + start as f32 * cell.x, top),
                        Align2::LEFT_TOP,
                        run.trim_end(),
                        self.font.clone(),
                        fg,
                    );
                } else if let Some((frac, part)) = block_shape(sym) {
                    // Block elements are drawn as rectangles so walls tile without gaps.
                    let left = origin.x + x as f32 * cell.x;
                    let (y0, h) = match part {
                        BlockPart::Full => (top, cell.y),
                        BlockPart::Upper => (top, cell.y / 2.0),
                        BlockPart::Lower => (top + cell.y / 2.0, cell.y / 2.0),
                    };
                    let bg = to_color32(if rev { c.fg } else { c.bg }, rev);
                    let col = if frac >= 1.0 { fg } else { lerp_color(bg, fg, frac) };
                    painter.rect_filled(
                        ERect::from_min_size(Pos2::new(left, y0), Vec2::new(cell.x + 0.6, h + 0.6)),
                        0.0,
                        col,
                    );
                    x += 1;
                } else {
                    let cx = origin.x + x as f32 * cell.x + cell.x / 2.0;
                    painter.text(Pos2::new(cx, top), Align2::CENTER_TOP, sym, self.font.clone(), fg);
                    x += 1;
                }
            }
        }
    }
}

impl eframe::App for WindowApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        ctx.request_repaint_after(Duration::from_millis(16));
        if self.first_frame {
            self.first_frame = false;
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }

        let (mut input, close_requested) = self.gather_input(&ctx);
        let elapsed = self.started.elapsed().as_secs_f64();
        while self.script.front().is_some_and(|(t, _)| *t <= elapsed) {
            if let Some((_, k)) = self.script.pop_front() {
                input.pressed.push(k);
            }
        }
        if let Some((path, at, requested)) = &mut self.snapshot {
            let shot = ctx.input(|i| {
                i.events.iter().find_map(|e| match e {
                    egui::Event::Screenshot { image, .. } => Some(image.clone()),
                    _ => None,
                })
            });
            if let Some(img) = shot {
                let _ = write_bmp(path, &img);
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            } else if !*requested && elapsed >= *at {
                *requested = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
            }
        }
        if close_requested {
            self.arcade.save_current();
            // Closed by hand while Claude is still busy: don't pop up again this turn.
            if self.arcade.mode == Mode::Auto && self.arcade.claude_busy > 0 {
                let _ = claude::append(
                    &claude::events_path(),
                    &Event { ms: claude::now_ms(), kind: Kind::Dismiss, session: "*".into(), term: "-".into() },
                );
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        if self.last_poll.elapsed() > Duration::from_millis(200) {
            self.last_poll = Instant::now();
            if let Some(term) = super::pump_claude(&mut self.watcher, &mut self.arcade)
                && let Ok(mut d) = self.done_term.lock()
            {
                *d = Some(term);
            }
        }
        let dt = self.last.elapsed().as_secs_f64();
        self.last = Instant::now();
        self.arcade.update(dt, &input);
        if self.arcade.quit {
            self.arcade.save_current();
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

        // Fit the character grid to the window.
        let rect = ui.max_rect();
        ui.painter().rect_filled(rect, 0.0, DEFAULT_BG);
        let cell = ctx.fonts_mut(|f| Vec2::new(f.glyph_width(&self.font, 'M'), f.row_height(&self.font)));
        let cols = ((rect.width() / cell.x).floor() as u16).max(10);
        let rows = ((rect.height() / cell.y).floor() as u16).max(4);
        if self.term.backend().buffer().area.width != cols || self.term.backend().buffer().area.height != rows {
            self.term.backend_mut().resize(cols, rows);
        }
        let arcade = &mut self.arcade;
        let _ = self.term.draw(|f| arcade.draw(f));
        let used = Vec2::new(cols as f32 * cell.x, rows as f32 * cell.y);
        let origin = rect.min + (rect.size() - used) / 2.0;
        self.paint(ui, origin.floor(), cell);
    }
}

fn write_pidfile() {
    let _ = crate::store::write_atomic(&hooks::window_pidfile(), std::process::id().to_string().as_bytes());
}

fn clear_pidfile() {
    let p = hooks::window_pidfile();
    if std::fs::read_to_string(&p).is_ok_and(|t| t.trim() == std::process::id().to_string()) {
        let _ = std::fs::remove_file(p);
    }
}

pub fn run(arcade: Arcade, cfg: &Config) -> Result<()> {
    write_pidfile();
    let font = FontId::monospace(cfg.font_size);
    let size = Vec2::new(cfg.cols as f32 * cfg.font_size * 0.61 + 8.0, cfg.rows as f32 * cfg.font_size * 1.2 + 8.0);
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("sidequest")
        .with_app_id("sidequest")
        .with_inner_size(size)
        .with_min_inner_size([420.0, 260.0])
        .with_active(true);
    if cfg.always_on_top {
        viewport = viewport.with_always_on_top();
    }
    let options = eframe::NativeOptions { viewport, persist_window: false, ..Default::default() };
    let done_term = Arc::new(Mutex::new(None));
    let done_for_app = done_term.clone();
    let mode = arcade.mode;
    let result = eframe::run_native(
        "sidequest",
        options,
        Box::new(move |cc| {
            install_fonts(&cc.egui_ctx);
            Ok(Box::new(WindowApp {
                arcade,
                term: Terminal::new(TestBackend::new(cfg_cols(size), 30)).expect("in-memory terminal"),
                watcher: Watcher::new(claude::events_path(), claude::now_ms()),
                font,
                last: Instant::now(),
                last_poll: Instant::now(),
                first_frame: true,
                done_term: done_for_app,
                script: std::env::var("SIDEQUEST_SCRIPT").map(|s| parse_script(&s)).unwrap_or_default(),
                snapshot: std::env::var_os("SIDEQUEST_SNAPSHOT").map(|p| {
                    let at = std::env::var("SIDEQUEST_SNAPSHOT_AT").ok().and_then(|t| t.parse().ok()).unwrap_or(2.0);
                    (std::path::PathBuf::from(p), at, false)
                }),
                started: Instant::now(),
            }))
        }),
    );
    clear_pidfile();
    // Hand focus back to the terminal Claude is running in.
    if mode == Mode::Auto && cfg.focus_terminal {
        let term = done_term.lock().ok().and_then(|d| d.clone());
        let term = term.filter(|t| t != "-").or_else(|| Watcher::new(claude::events_path(), 0).last_term);
        if let Some(t) = term {
            claude::focus_terminal(&t);
        }
    }
    result.map_err(|e| anyhow!("couldn't open the game window: {e}"))
}

fn cfg_cols(size: Vec2) -> u16 {
    (size.x / 9.0) as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours() {
        assert_eq!(to_color32(Color::Rgb(1, 2, 3), true), Color32::from_rgb(1, 2, 3));
        assert_eq!(to_color32(Color::Reset, true), DEFAULT_FG);
        assert_eq!(to_color32(Color::Reset, false), DEFAULT_BG);
        assert_eq!(to_color32(Color::Indexed(16), true), Color32::from_rgb(0, 0, 0));
        assert_eq!(to_color32(Color::Indexed(231), true), Color32::from_rgb(255, 255, 255));
        assert_eq!(to_color32(Color::Indexed(232), true), Color32::from_rgb(8, 8, 8));
    }

    #[test]
    fn script_and_bmp() {
        let s = parse_script("0.5:enter, 1:space,bad,2:w,x:y");
        assert_eq!(
            s.into_iter().collect::<Vec<_>>(),
            vec![(0.5, Key::Enter), (1.0, Key::Space), (2.0, Key::Char('w'))]
        );
        let d = tempfile::tempdir().unwrap();
        let img = egui::ColorImage::new([3, 2], vec![Color32::RED; 6]);
        write_bmp(&d.path().join("x.bmp"), &img).unwrap();
        let bytes = std::fs::read(d.path().join("x.bmp")).unwrap();
        assert_eq!(&bytes[..2], b"BM");
        assert_eq!(bytes.len(), 54 + 12 * 2);
    }

    #[test]
    fn block_shapes() {
        assert_eq!(block_shape("█"), Some((1.0, BlockPart::Full)));
        assert_eq!(block_shape("▀").map(|b| b.1), Some(BlockPart::Upper));
        assert!(block_shape("a").is_none());
        assert_eq!(lerp_color(Color32::BLACK, Color32::WHITE, 0.5), Color32::from_rgb(128, 128, 128));
    }

    #[test]
    fn keys() {
        assert_eq!(map_egui_key(egui::Key::ArrowUp), Some(Key::Up));
        assert_eq!(map_egui_key(egui::Key::W), Some(Key::Char('w')));
        assert_eq!(map_egui_key(egui::Key::Num3), Some(Key::Char('3')));
        assert_eq!(map_egui_key(egui::Key::F1), None);
    }
}
