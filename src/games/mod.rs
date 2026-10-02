//! The games. Each one only knows how to update from input, draw into a ratatui frame and serialize itself.

pub mod fathom;
pub mod invaders;
pub mod mines;
pub mod snake;
pub mod twenty48;

use ratatui::Frame;
use ratatui::layout::Rect;
use serde_json::Value;

use crate::input::Input;
use crate::store::Score;

pub trait Game {
    /// Advance by `dt` seconds. Only called while the game is running (not paused).
    fn update(&mut self, dt: f64, input: &Input);
    fn draw(&mut self, f: &mut Frame, area: Rect);
    fn save(&self) -> Value;
    /// Current score worth recording as a best, if any.
    fn score(&self) -> Option<Score> {
        None
    }
    /// The game ended (lost or won) and the save should be discarded when leaving.
    fn finished(&self) -> bool {
        false
    }
    /// Real-time games get a short countdown when resuming so you don't die instantly.
    fn realtime(&self) -> bool {
        true
    }
}

pub struct GameInfo {
    pub id: &'static str,
    pub title: &'static str,
    pub tagline: &'static str,
    pub new: fn() -> Box<dyn Game>,
    pub load: fn(&Value) -> Option<Box<dyn Game>>,
}

fn load_as<T: Game + serde::de::DeserializeOwned + 'static>(v: &Value) -> Option<Box<dyn Game>> {
    serde_json::from_value::<T>(v.clone()).ok().map(|g| Box::new(g) as Box<dyn Game>)
}

pub static CATALOG: &[GameInfo] = &[
    GameInfo {
        id: "fathom",
        title: "FATHOM",
        tagline: "A sonar roguelike. Every ping lights the dark, and tells something where you are.",
        new: || Box::new(fathom::Fathom::new()),
        load: fathom::Fathom::load,
    },
    GameInfo {
        id: "snake",
        title: "Snake",
        tagline: "Eat, grow, don't bite yourself. Gets faster as you go.",
        new: || Box::new(snake::Snake::new()),
        load: load_as::<snake::Snake>,
    },
    GameInfo {
        id: "invaders",
        title: "Space Invaders",
        tagline: "Hold the line against the marching waves.",
        new: || Box::new(invaders::Invaders::new()),
        load: load_as::<invaders::Invaders>,
    },
    GameInfo {
        id: "2048",
        title: "2048",
        tagline: "Slide and merge tiles. A calm one for short waits.",
        new: || Box::new(twenty48::Twenty48::new()),
        load: load_as::<twenty48::Twenty48>,
    },
    GameInfo {
        id: "mines",
        title: "Minesweeper",
        tagline: "Clear the field. Logic, not luck (first click is always safe).",
        new: || Box::new(mines::Mines::new()),
        load: load_as::<mines::Mines>,
    },
];

pub fn find(id: &str) -> Option<&'static GameInfo> {
    CATALOG.iter().find(|g| g.id == id)
}
