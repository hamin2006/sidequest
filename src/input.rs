//! Frontend-independent input: keys pressed this frame plus keys currently held.

use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    Up,
    Down,
    Left,
    Right,
    Enter,
    Esc,
    Space,
    Tab,
    Backspace,
    /// Printable character, lowercased for letters.
    Char(char),
}

#[derive(Debug, Clone, Default)]
pub struct Input {
    /// Keys that went down (or auto-repeated) this frame, in order.
    pub pressed: Vec<Key>,
    /// Keys being held right now.
    pub held: HashSet<Key>,
    /// Shift held (for "boost" style modifiers).
    pub shift: bool,
    /// The frontend reports real key-up events, so `held` is exact. Without this, `held` is only an
    /// estimate and games should step on `pressed` (which then includes auto-repeat).
    pub held_reliable: bool,
}

impl Input {
    pub fn was(&self, k: Key) -> bool {
        self.pressed.contains(&k)
    }

    pub fn any(&self) -> bool {
        !self.pressed.is_empty()
    }

    pub fn is_held(&self, k: Key) -> bool {
        self.held.contains(&k)
    }

    /// Direction pressed this frame (arrows or WASD), last one wins.
    pub fn dir_pressed(&self) -> Option<(i32, i32)> {
        self.pressed.iter().rev().find_map(|k| dir_of(*k))
    }

    /// Direction currently held (arrows or WASD), summed and clamped.
    pub fn dir_held(&self) -> (i32, i32) {
        let mut x = 0;
        let mut y = 0;
        for k in &self.held {
            if let Some((dx, dy)) = dir_of(*k) {
                x += dx;
                y += dy;
            }
        }
        (x.clamp(-1, 1), y.clamp(-1, 1))
    }

    pub fn char_pressed(&self, c: char) -> bool {
        self.pressed.contains(&Key::Char(c))
    }
}

pub fn dir_of(k: Key) -> Option<(i32, i32)> {
    match k {
        Key::Up | Key::Char('w') => Some((0, -1)),
        Key::Down | Key::Char('s') => Some((0, 1)),
        Key::Left | Key::Char('a') => Some((-1, 0)),
        Key::Right | Key::Char('d') => Some((1, 0)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directions() {
        let mut i = Input { pressed: vec![Key::Up, Key::Char('d')], ..Input::default() };
        assert_eq!(i.dir_pressed(), Some((1, 0)));
        i.held = [Key::Left, Key::Char('w'), Key::Char('a')].into_iter().collect();
        assert_eq!(i.dir_held(), (-1, -1));
        i.held = [Key::Left, Key::Right].into_iter().collect();
        assert_eq!(i.dir_held(), (0, 0));
    }
}
