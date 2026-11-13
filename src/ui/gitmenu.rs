use tinsel::app::{Frame, Rect};
use tinsel::{paint, palette};

use super::{Colors, vline};
use crate::editor::Editor;
use crate::gitmenu::{GitMenu, Tab};

pub(super) fn draw(m: &GitMenu, e: &Editor, frame: &mut Frame, c: &Colors) -> Option<(usize, usize)> {
    let area = frame.area();
    let t = frame.time();
    frame.dim(area);
    let width = area.width.saturating_sub(6).clamp(24, 132);
    let height = area.height.saturating_sub(4).clamp(8, 34);
    let rise = ((1.0 - open) * 2.0).round() as usize;
    let rect = Rect::new(
        area.x + (area.width - width) / 2,
        (area.y + (area.height - height) / 3 + rise).min(area.y + area.height - height),
        width,
        height,
    );
    for under in [
        Rect::new(rect.x + 2, rect.y + rect.height, rect.width, 1),
        Rect::new(rect.x + rect.width, rect.y + 1, 2, rect.height),
    ] {
        frame.dim(under.intersect(area));
    }
    let status = match m.tab() {
        Tab::Files if changes.is_empty() => "clean".to_string(),
        Tab::Files => format!("{} changed", changes.len()),
        Tab::Log => "recent commits".to_string(),
        Tab::Branches => "branches".to_string(),
    };
    if inner.height < 6 || inner.width < 20 {
        return None;
    }
    let list_width = (inner.width * 2 / 5).clamp(20, 48).min(inner.width);
    let split = inner.x + list_width;
    let rule = |frame: &mut Frame, y: usize, joint: &str| {
        frame.print(
            Rect::new(inner.x, y, inner.width, 1),
            0,
            &paint(palette::LINE, &"─".repeat(inner.width)),
        );
        frame.print(Rect::new(split, y, 1, 1), 0, &paint(palette::LINE, joint));
    };
    rule(frame, inner.y + 1, "┬");
    let body_height = inner.height - 5;
    let body_y = inner.y + 2;
    vline(frame, split, body_y, body_height, &paint(palette::LINE, "│"));
    rule(frame, body_y + body_height, "┴");
    let list = Rect::new(inner.x, body_y, list_width, body_height);
    let pane = Rect::new(
        split + 2,
        body_y,
        (inner.x + inner.width).saturating_sub(split + 2),
        body_height,
    );
    match m.tab() {
        Tab::Files => files(m, frame, list, c),
        Tab::Log | Tab::Branches => entries(m, frame, list, c),
    }
    if open < 1.0 {
        frame.dim(rect);
    }
    at
}
