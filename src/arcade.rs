//! The shell around the games: menu, pause, resume countdown, autosave, best scores and the Claude
//! status bar. Shared by the window and the terminal frontends.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

use crate::games::{self, CATALOG, Game, GameInfo};
use crate::input::{Input, Key};
use crate::store::{Better, Store};
use crate::ui;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Opened automatically because Claude is busy: closes itself when Claude is done.
    Auto,
    /// Opened by you: just pauses when Claude is done.
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reason {
    User,
    Claude,
}

struct Pause {
    reason: Reason,
    sel: usize,
}

struct Session {
    info: &'static GameInfo,
    game: Box<dyn Game>,
    pause: Option<Pause>,
    countdown: f64,
}

enum Screen {
    Menu,
    Play(Box<Session>),
}

const PAUSE_ITEMS: [&str; 3] = ["Resume", "Restart", "Back to games"];
/// How long the "Claude is done" banner shows before an auto-opened window closes.
pub const DONE_LINGER: f64 = 1.2;

pub struct Arcade {
    pub mode: Mode,
    pub store: Store,
    screen: Screen,
    menu_sel: usize,
    confirm_new: bool,
    /// Number of Claude sessions currently working.
    pub claude_busy: usize,
    banner: Option<f64>,
    new_best: Option<f64>,
    pub quit: bool,
    /// Show window-only hints (minimize shortcut) in the pause menu.
    pub window_hints: bool,
    time: f64,
    autosave: f64,
    score_check: f64,
}

pub fn format_score(id: &str, value: u64, better: Better) -> String {
    match (id, better) {
        ("fathom", _) => format!("{value} m"),
        (_, Better::Lower) => format!("{value} s"),
        _ => value.to_string(),
    }
}

impl Arcade {
    /// `start`: open this game straight away. In auto mode with no `start`, the last game played is resumed.
    pub fn new(store: Store, mode: Mode, start: Option<&str>) -> Self {
        let mut a = Self {
            mode,
            store,
            screen: Screen::Menu,
            menu_sel: 0,
            confirm_new: false,
            claude_busy: 0,
            banner: None,
            new_best: None,
            quit: false,
            window_hints: false,
            time: 0.0,
            autosave: 0.0,
            score_check: 0.0,
        };
        let target = start
            .map(String::from)
            .or_else(|| (mode == Mode::Auto).then(|| a.store.last_game().map(String::from)).flatten());
        if let Some(info) = target.as_deref().and_then(games::find) {
            a.menu_sel = CATALOG.iter().position(|g| g.id == info.id).unwrap_or(0);
            a.open(info, false);
        }
        a
    }

    pub fn current_game(&self) -> Option<&'static str> {
        match &self.screen {
            Screen::Play(s) => Some(s.info.id),
            Screen::Menu => None,
        }
    }

    pub fn paused(&self) -> bool {
        match &self.screen {
            Screen::Play(s) => s.pause.is_some() || s.countdown > 0.0,
            Screen::Menu => true,
        }
    }

    fn open(&mut self, info: &'static GameInfo, fresh: bool) {
        let resumed = if fresh { None } else { self.store.load(info.id).and_then(|v| (info.load)(&v)) };
        let was_saved = resumed.is_some();
        let game = resumed.unwrap_or_else(|| (info.new)());
        let countdown = if was_saved && game.realtime() { 3.0 } else { 0.0 };
        self.store.set_last_game(info.id);
        self.screen = Screen::Play(Box::new(Session { info, game, pause: None, countdown }));
        self.autosave = 0.0;
    }

    /// Persists the running game (or discards it if it ended).
    pub fn save_current(&mut self) {
        if let Screen::Play(s) = &self.screen {
            if s.game.finished() {
                self.store.delete(s.info.id);
            } else {
                self.store.save(s.info.id, &s.game.save());
            }
            if let Some(score) = s.game.score() {
                self.store.record(s.info.id, score);
            }
        }
    }

    fn leave_game(&mut self) {
        self.save_current();
        self.screen = Screen::Menu;
    }

    /// Pauses the running game (no-op on the menu or if already paused). Used when the window is
    /// minimized or loses focus.
    pub fn pause(&mut self) {
        if let Screen::Play(s) = &mut self.screen
            && s.pause.is_none()
        {
            s.pause = Some(Pause { reason: Reason::User, sel: 0 });
            self.save_current();
        }
    }

    /// Claude finished (or needs permission): pause and show the banner.
    pub fn claude_done(&mut self) {
        self.banner = Some(self.time);
        if let Screen::Play(s) = &mut self.screen
            && s.pause.is_none()
        {
            s.pause = Some(Pause { reason: Reason::Claude, sel: 0 });
        }
        self.save_current();
    }

    pub fn update(&mut self, dt: f64, input: &Input) {
        let dt = dt.clamp(0.0, 0.25);
        self.time += dt;

        if let Some(t0) = self.banner {
            match self.mode {
                Mode::Auto if self.time - t0 >= DONE_LINGER => {
                    self.save_current();
                    self.quit = true;
                    return;
                }
                Mode::Manual if self.time - t0 > 0.3 && input.any() => {
                    // The key that dismisses the banner isn't also sent to the game.
                    self.banner = None;
                    return;
                }
                _ => {}
            }
            if self.mode == Mode::Auto {
                return;
            }
        }

        match &mut self.screen {
            Screen::Menu => self.update_menu(input),
            Screen::Play(_) => self.update_play(dt, input),
        }
    }

    fn update_menu(&mut self, input: &Input) {
        for k in input.pressed.clone() {
            if self.confirm_new {
                self.confirm_new = false;
                if k == Key::Char('y') {
                    let info = &CATALOG[self.menu_sel];
                    self.store.delete(info.id);
                    self.open(info, true);
                    return;
                }
                continue;
            }
            match k {
                Key::Up | Key::Char('w') | Key::Char('k') => {
                    self.menu_sel = (self.menu_sel + CATALOG.len() - 1) % CATALOG.len()
                }
                Key::Down | Key::Char('s') | Key::Char('j') => self.menu_sel = (self.menu_sel + 1) % CATALOG.len(),
                Key::Enter | Key::Space => {
                    self.open(&CATALOG[self.menu_sel], false);
                    return;
                }
                Key::Char('n') => {
                    let info = &CATALOG[self.menu_sel];
                    if self.store.has_save(info.id) {
                        self.confirm_new = true;
                    } else {
                        self.open(info, true);
                        return;
                    }
                }
                Key::Char(c @ '1'..='9') => {
                    let i = c as usize - '1' as usize;
                    if i < CATALOG.len() {
                        self.menu_sel = i;
                    }
                }
                Key::Esc | Key::Char('q') => self.quit = true,
                _ => {}
            }
        }
    }

    fn update_play(&mut self, dt: f64, input: &Input) {
        let Screen::Play(s) = &mut self.screen else {
            return;
        };
        if let Some(p) = &mut s.pause {
            let mut action = None;
            for k in &input.pressed {
                match k {
                    Key::Esc | Key::Char('p') => action = Some(0),
                    Key::Up | Key::Char('w') => p.sel = (p.sel + PAUSE_ITEMS.len() - 1) % PAUSE_ITEMS.len(),
                    Key::Down | Key::Char('s') => p.sel = (p.sel + 1) % PAUSE_ITEMS.len(),
                    Key::Enter | Key::Space => action = Some(p.sel),
                    Key::Char('r') => action = Some(1),
                    Key::Char('m') | Key::Char('q') => action = Some(2),
                    _ => {}
                }
            }
            match action {
                Some(0) => {
                    s.pause = None;
                    s.countdown = if s.game.realtime() { 1.5 } else { 0.0 };
                }
                Some(1) => {
                    let info = s.info;
                    self.store.delete(info.id);
                    self.open(info, true);
                }
                Some(2) => self.leave_game(),
                _ => {}
            }
            return;
        }
        if input.was(Key::Esc) || input.char_pressed('p') {
            s.pause = Some(Pause { reason: Reason::User, sel: 0 });
            self.save_current();
            return;
        }
        if s.countdown > 0.0 {
            s.countdown -= dt;
            return;
        }
        s.game.update(dt, input);

        self.autosave += dt;
        self.score_check += dt;
        if self.score_check >= 1.0 {
            self.score_check = 0.0;
            if let Some(score) = s.game.score() {
                let id = s.info.id;
                if self.store.best(id).is_some_and(|b| score.beats(&b)) && self.new_best.is_none() {
                    self.new_best = Some(self.time);
                }
                self.store.record(id, score);
            }
        }
        if self.autosave >= 5.0 {
            self.autosave = 0.0;
            self.save_current();
        }
    }

    // ---------- Drawing ----------

    pub fn draw(&mut self, f: &mut Frame) {
        let area = f.area();
        f.buffer_mut().set_style(area, Style::new().bg(Color::Rgb(10, 11, 16)).fg(Color::Rgb(220, 222, 230)));
        if area.width < 20 || area.height < 4 {
            return;
        }
        let bar = Rect { height: 1, ..area };
        let body = Rect { y: area.y + 1, height: area.height - 1, ..area };
        self.draw_bar(f, bar);
        match &mut self.screen {
            Screen::Menu => {}
            Screen::Play(s) => s.game.draw(f, body),
        }
        if matches!(self.screen, Screen::Menu) {
            self.draw_menu(f, body);
        }
        self.draw_overlays(f, body);
    }

    fn draw_bar(&self, f: &mut Frame, bar: Rect) {
        f.buffer_mut().set_style(bar, Style::new().bg(Color::Rgb(18, 20, 28)));
        let mut left = vec![Span::styled(" ◆ sidequest ", Style::new().fg(ui::ACCENT).add_modifier(Modifier::BOLD))];
        if let Screen::Play(s) = &self.screen {
            left.push(Span::styled(format!("› {} ", s.info.title), Style::new().fg(Color::White)));
        }
        if self.new_best.is_some_and(|t| self.time - t < 3.0) {
            left.push(Span::styled(" ★ new best ", Style::new().fg(ui::WARN)));
        }
        f.render_widget(Paragraph::new(Line::from(left)), bar);

        let right = if self.banner.is_some() {
            Span::styled("✓ Claude is done ", Style::new().fg(ui::GOOD).add_modifier(Modifier::BOLD))
        } else if self.claude_busy > 0 {
            let spin = ["◐", "◓", "◑", "◒"][(self.time * 6.0) as usize % 4];
            let n = if self.claude_busy > 1 { format!(" ×{}", self.claude_busy) } else { String::new() };
            Span::styled(format!("{spin} Claude is working{n} "), Style::new().fg(ui::WARN))
        } else {
            Span::styled("○ Claude idle ", Style::new().fg(ui::FAINT))
        };
        let w = right.width() as u16;
        if bar.width > w + 30 {
            f.render_widget(Paragraph::new(right), Rect { x: bar.x + bar.width - w, width: w, ..bar });
        }
    }

    fn draw_menu(&self, f: &mut Frame, body: Rect) {
        let logo = ["┏━┓╻╺┳┓┏━╸┏━┓╻ ╻┏━╸┏━┓╺┳╸", "┗━┓┃ ┃┃┣╸ ┃┓┃┃ ┃┣╸ ┗━┓ ┃ ", "┗━┛╹╺┻┛┗━╸┗┻┛┗━┛┗━╸┗━┛ ╹ "];
        let width = 72.min(body.width.saturating_sub(2));
        let height = (logo.len() + 3 + CATALOG.len() * 3 + 2) as u16;
        let r = ui::centered(body, width, height.min(body.height));
        let mut lines: Vec<Line> = logo
            .iter()
            .enumerate()
            .map(|(i, l)| {
                Line::styled(*l, Style::new().fg([ui::ACCENT, Color::Rgb(150, 170, 255), Color::Rgb(190, 140, 255)][i]))
            })
            .collect();
        lines.push(Line::styled("games for while Claude is thinking", Style::new().fg(ui::DIM)));
        lines.push(Line::from(""));
        for (i, g) in CATALOG.iter().enumerate() {
            let sel = i == self.menu_sel;
            let marker = if sel { "▸ " } else { "  " };
            let mut row = vec![
                Span::styled(format!("{marker}{} ", i + 1), Style::new().fg(ui::FAINT)),
                Span::styled(
                    g.title,
                    Style::new()
                        .fg(if sel { Color::White } else { Color::Rgb(200, 204, 214) })
                        .add_modifier(Modifier::BOLD),
                ),
            ];
            if let Some(b) = self.store.best(g.id) {
                row.push(Span::styled(
                    format!("   best {}", format_score(g.id, b.value, b.better)),
                    Style::new().fg(ui::DIM),
                ));
            }
            if self.store.has_save(g.id) {
                row.push(Span::styled("   ▶ continue", Style::new().fg(ui::GOOD)));
            }
            let style = if sel { Style::new().bg(Color::Rgb(34, 38, 54)) } else { Style::new() };
            lines.push(Line::from(row).style(style).left_aligned());
            lines.push(
                Line::styled(format!("     {}", ui_trunc(g.tagline, width as usize - 6)), Style::new().fg(ui::DIM))
                    .style(style)
                    .left_aligned(),
            );
            lines.push(Line::from(""));
        }
        let hint = if self.confirm_new {
            Line::styled("Start over and erase your saved run? y / n", Style::new().fg(ui::WARN))
        } else {
            Line::styled("↑↓ choose · enter play · n new game · esc close", Style::new().fg(ui::FAINT))
        };
        lines.push(hint);
        let para = Paragraph::new(lines).centered();
        f.render_widget(para, r);
    }

    fn draw_overlays(&self, f: &mut Frame, body: Rect) {
        let Screen::Play(s) = &self.screen else {
            return;
        };
        if let Some(p) = &s.pause {
            let title = match p.reason {
                Reason::Claude => "Claude is done",
                Reason::User => "Paused",
            };
            let color = if p.reason == Reason::Claude { ui::GOOD } else { ui::ACCENT };
            let mut lines = vec![];
            if p.reason == Reason::Claude {
                lines.push(Line::styled(
                    if self.mode == Mode::Auto {
                        "Saved. Taking you back to your terminal…"
                    } else {
                        "Your game is saved and paused."
                    },
                    Style::new().fg(ui::GOOD),
                ));
                lines.push(Line::from(""));
            }
            if !(p.reason == Reason::Claude && self.mode == Mode::Auto) {
                for (i, item) in PAUSE_ITEMS.iter().enumerate() {
                    let st =
                        if i == p.sel { Style::new().fg(Color::Black).bg(color) } else { Style::new().fg(ui::DIM) };
                    lines.push(Line::styled(format!("  {item}  "), st));
                }
                if self.window_hints {
                    lines.push(Line::from(""));
                    lines.push(Line::styled("⌘M minimize · click away and it pauses", Style::new().fg(ui::FAINT)));
                }
            }
            ui::modal(f, body, title, color, lines);
        } else if s.countdown > 0.0 {
            let n = s.countdown.ceil() as u32;
            let r = ui::centered(body, 24, 3);
            f.render_widget(Clear, r);
            f.render_widget(
                Paragraph::new(vec![
                    Line::styled(format!("Resuming in {n}…"), Style::new().fg(ui::WARN).add_modifier(Modifier::BOLD)),
                    Line::styled("esc for menu", Style::new().fg(ui::FAINT)),
                ])
                .centered()
                .block(ui::panel("", ui::WARN)),
                r,
            );
        }
    }
}

fn ui_trunc(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(max.saturating_sub(1)).collect();
        t.push('…');
        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn arcade(mode: Mode, start: Option<&str>) -> (Arcade, tempfile::TempDir) {
        let d = tempfile::tempdir().unwrap();
        (Arcade::new(Store::open(d.path().to_path_buf()), mode, start), d)
    }

    fn keys(ks: &[Key]) -> Input {
        Input { pressed: ks.to_vec(), ..Input::default() }
    }

    fn render(a: &mut Arcade, w: u16, h: u16) -> String {
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        t.draw(|f| a.draw(f)).unwrap();
        t.backend().buffer().content().iter().map(|c| c.symbol()).collect()
    }

    #[test]
    fn menu_opens_games_and_esc_quits() {
        let (mut a, _d) = arcade(Mode::Manual, None);
        assert!(render(&mut a, 100, 34).contains("FATHOM"));
        a.update(0.016, &keys(&[Key::Down, Key::Enter]));
        assert_eq!(a.current_game(), Some("snake"));
        a.update(0.016, &keys(&[Key::Esc]));
        assert!(a.paused());
        a.update(0.016, &keys(&[Key::Char('m')]));
        assert_eq!(a.current_game(), None);
        assert!(a.store.has_save("snake"), "leaving saves the run");
        a.update(0.016, &keys(&[Key::Esc]));
        assert!(a.quit);
    }

    #[test]
    fn claude_done_pauses_and_auto_mode_closes() {
        let (mut a, _d) = arcade(Mode::Auto, Some("snake"));
        assert_eq!(a.current_game(), Some("snake"));
        a.claude_busy = 1;
        a.claude_done();
        assert!(a.paused());
        let screen = render(&mut a, 100, 34);
        assert!(screen.contains("Claude is done"));
        for _ in 0..100 {
            a.update(0.02, &Input::default());
        }
        assert!(a.quit, "auto window closes after the banner");
        assert!(a.store.has_save("snake"));
    }

    #[test]
    fn manual_mode_banner_waits_for_a_key_and_resume_counts_down() {
        let (mut a, _d) = arcade(Mode::Manual, Some("snake"));
        a.claude_done();
        for _ in 0..100 {
            a.update(0.02, &Input::default());
        }
        assert!(!a.quit);
        a.update(0.02, &keys(&[Key::Space]));
        a.update(0.02, &keys(&[Key::Esc])); // resume from the pause menu
        assert!(a.paused(), "resume starts with a countdown for real-time games");
        for _ in 0..100 {
            a.update(0.02, &Input::default());
        }
        assert!(!a.paused());
    }

    #[test]
    fn auto_mode_resumes_last_game_with_countdown() {
        let d = tempfile::tempdir().unwrap();
        {
            let mut a = Arcade::new(Store::open(d.path().to_path_buf()), Mode::Manual, Some("invaders"));
            a.update(0.1, &Input::default());
            a.save_current();
        }
        let a = Arcade::new(Store::open(d.path().to_path_buf()), Mode::Auto, None);
        assert_eq!(a.current_game(), Some("invaders"));
        assert!(a.paused(), "countdown before play resumes");
    }

    #[test]
    fn pause_only_affects_running_games() {
        let (mut a, _d) = arcade(Mode::Manual, None);
        a.pause();
        assert_eq!(a.current_game(), None, "pausing on the menu does nothing");
        let (mut a, _d) = arcade(Mode::Manual, Some("2048"));
        assert!(!a.paused());
        a.pause();
        assert!(a.paused());
        a.window_hints = true;
        assert!(render(&mut a, 100, 34).contains("minimize"));
        a.pause(); // idempotent
        a.update(0.02, &keys(&[Key::Esc]));
        assert!(!a.paused(), "esc resumes as usual");
    }

    #[test]
    fn new_game_asks_before_erasing() {
        let (mut a, _d) = arcade(Mode::Manual, Some("2048"));
        a.update(0.02, &keys(&[Key::Esc]));
        a.update(0.02, &keys(&[Key::Char('m')]));
        let sel = CATALOG.iter().position(|g| g.id == "2048").unwrap();
        a.menu_sel = sel;
        a.update(0.02, &keys(&[Key::Char('n')]));
        assert!(a.confirm_new);
        assert!(render(&mut a, 100, 40).contains("erase your saved run"));
        a.update(0.02, &keys(&[Key::Char('x')]));
        assert!(!a.confirm_new);
        assert_eq!(a.current_game(), None);
    }

    #[test]
    fn every_game_renders_at_every_size() {
        for g in CATALOG {
            let (mut a, _d) = arcade(Mode::Manual, Some(g.id));
            for (w, h) in [(1, 1), (10, 3), (20, 4), (40, 12), (80, 24), (104, 34), (200, 60)] {
                render(&mut a, w, h);
            }
            for _ in 0..30 {
                a.update(0.05, &keys(&[Key::Space, Key::Right]));
                render(&mut a, 104, 34);
            }
        }
    }
}
