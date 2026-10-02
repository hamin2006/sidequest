//! FATHOM (placeholder while the core is built).

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::widgets::Paragraph;
use serde_json::Value;

use super::Game;
use crate::input::Input;

#[derive(Default)]
pub struct Fathom;

impl Fathom {
    pub fn new() -> Self {
        Fathom
    }

    pub fn load(_v: &Value) -> Option<Box<dyn Game>> {
        Some(Box::new(Fathom))
    }
}

impl Game for Fathom {
    fn update(&mut self, _dt: f64, _input: &Input) {}

    fn draw(&mut self, f: &mut Frame, area: Rect) {
        f.render_widget(Paragraph::new("FATHOM is coming"), area);
    }

    fn save(&self) -> Value {
        Value::Null
    }
}
