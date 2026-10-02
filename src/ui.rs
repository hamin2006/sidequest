//! Drawing helpers shared by the arcade and the games.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, BorderType, Clear, Paragraph};

pub const ACCENT: Color = Color::Rgb(120, 200, 255);
pub const DIM: Color = Color::Rgb(120, 125, 140);
pub const FAINT: Color = Color::Rgb(70, 74, 88);
pub const GOOD: Color = Color::Rgb(110, 220, 140);
pub const WARN: Color = Color::Rgb(255, 196, 90);
pub const BAD: Color = Color::Rgb(255, 95, 95);
pub const BG_PANEL: Color = Color::Rgb(22, 24, 32);

/// A `w`×`h` rect centred in `area`, clipped to fit.
pub fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + (area.height - h) / 2,
        width: w,
        height: h,
    }
}

/// True (and draws a hint) if `area` is smaller than the game needs.
pub fn too_small(f: &mut Frame, area: Rect, w: u16, h: u16) -> bool {
    if area.width >= w && area.height >= h {
        return false;
    }
    let msg = format!("Make the window bigger ({w}×{h} needed)");
    let r = centered(area, (msg.chars().count() as u16).min(area.width), 1);
    f.render_widget(Paragraph::new(msg).style(Style::new().fg(DIM)), r);
    true
}

pub fn panel(title: &str, color: Color) -> Block<'static> {
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(color))
        .title(Line::styled(format!(" {title} "), Style::new().fg(color)))
        .style(Style::new().bg(BG_PANEL))
}

/// A centred modal box with the given lines.
pub fn modal(f: &mut Frame, area: Rect, title: &str, color: Color, lines: Vec<Line<'static>>) {
    let w = lines
        .iter()
        .map(|l| l.width() as u16)
        .max()
        .unwrap_or(10)
        .max(title.chars().count() as u16 + 4)
        + 4;
    let r = centered(area, w, lines.len() as u16 + 2);
    f.render_widget(Clear, r);
    f.render_widget(
        Paragraph::new(lines).block(panel(title, color)).centered(),
        r,
    );
}
