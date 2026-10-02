//! Play in any terminal through crossterm.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use anyhow::Result;
use ratatui::crossterm::event::{
    self, Event as TermEvent, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, KeyboardEnhancementFlags,
    PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use ratatui::crossterm::{execute, terminal};

use crate::arcade::Arcade;
use crate::claude::{self, Watcher};
use crate::input::{Input, Key};

pub fn map_key(k: &KeyEvent) -> Option<Key> {
    Some(match k.code {
        KeyCode::Up => Key::Up,
        KeyCode::Down => Key::Down,
        KeyCode::Left => Key::Left,
        KeyCode::Right => Key::Right,
        KeyCode::Enter => Key::Enter,
        KeyCode::Esc => Key::Esc,
        KeyCode::Tab | KeyCode::BackTab => Key::Tab,
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Char(' ') => Key::Space,
        KeyCode::Char(c) => Key::Char(c.to_ascii_lowercase()),
        _ => return None,
    })
}

pub fn run(mut arcade: Arcade) -> Result<()> {
    let mut terminal = ratatui::try_init()?;
    // Ghostty, kitty, WezTerm and foot report key releases, which makes held keys precise.
    let enhanced = matches!(terminal::supports_keyboard_enhancement(), Ok(true))
        && execute!(
            std::io::stdout(),
            PushKeyboardEnhancementFlags(
                KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES | KeyboardEnhancementFlags::REPORT_EVENT_TYPES
            )
        )
        .is_ok();
    let mut watcher = Watcher::new(claude::events_path(), claude::now_ms());
    let result = (|| -> Result<()> {
        let mut held: HashMap<Key, Instant> = HashMap::new();
        let mut last = Instant::now();
        let mut last_poll = Instant::now();
        let frame = Duration::from_millis(16);
        loop {
            let start = Instant::now();
            let mut input = Input { held_reliable: enhanced, ..Input::default() };
            while let Some(left) = frame.checked_sub(start.elapsed()) {
                if !event::poll(left)? {
                    break;
                }
                if let TermEvent::Key(k) = event::read()? {
                    let Some(key) = map_key(&k) else { continue };
                    if k.modifiers.contains(KeyModifiers::CONTROL) && key == Key::Char('c') {
                        arcade.quit = true;
                    }
                    if k.modifiers.contains(KeyModifiers::SHIFT)
                        || matches!(k.code, KeyCode::Char(c) if c.is_ascii_uppercase())
                    {
                        input.shift = true;
                    }
                    match k.kind {
                        KeyEventKind::Press => {
                            input.pressed.push(key);
                            held.insert(key, Instant::now());
                        }
                        KeyEventKind::Repeat => {
                            if !enhanced {
                                input.pressed.push(key);
                            }
                            held.insert(key, Instant::now());
                        }
                        KeyEventKind::Release => {
                            held.remove(&key);
                        }
                    }
                }
            }
            if !enhanced {
                held.retain(|_, t| t.elapsed() < Duration::from_millis(120));
            }
            input.held = held.keys().copied().collect();
            if last_poll.elapsed() > Duration::from_millis(250) {
                last_poll = Instant::now();
                super::pump_claude(&mut watcher, &mut arcade);
            }
            let dt = last.elapsed().as_secs_f64();
            last = Instant::now();
            arcade.update(dt, &input);
            terminal.draw(|f| arcade.draw(f))?;
            if arcade.quit {
                return Ok(());
            }
        }
    })();
    arcade.save_current();
    if enhanced {
        let _ = execute!(std::io::stdout(), PopKeyboardEnhancementFlags);
    }
    ratatui::restore();
    result
}
