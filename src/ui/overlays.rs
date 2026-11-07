use tinsel::app::{Frame, Panel, Rect};

use super::Colors;
use crate::editor::Editor;

pub(super) fn completion(e: &Editor, frame: &mut Frame, area: Rect, at: (usize, usize), c: &Colors) {
    let Some((prefix, words, picked)) = e.completion() else {
        return;
    };
    let (cx, cy) = at;
    let below = (area.y + area.height).saturating_sub(cy + 1);
    let above = cy.saturating_sub(area.y);
    let room = if below >= 3 || below >= above { below } else { above };
    let rows = words.len().min(8).min(room.saturating_sub(2));
    if rows == 0 || area.width < 10 {
        return;
    }
    let word_width = words.iter().map(|w| tinsel::text::width(w)).max().unwrap_or(0).min(40);
    let width = (word_width + 6).min(area.width);
    let height = rows + 2;
    let shown = picked.map_or(prefix, |i| words[i].as_str());
    let x = cx
        .saturating_sub(tinsel::text::width(shown) + 4)
        .clamp(area.x, area.x + area.width - width);
    let y = if room == below { cy + 1 } else { cy - height };
    let mut panel = Panel::new("").focused(true);
    if words.len() > rows {
        panel = panel.footer(format!("{}/{}", picked.map_or(0, |i| i + 1), words.len()));
    }
    let first = picked.map_or(0, |p| (p + 1).saturating_sub(rows));
    for (i, w) in words.iter().enumerate().skip(first).take(rows) {
        let line = Rect::new(inner.x, inner.y + i - first, inner.width, 1);
        let selected = picked == Some(i);
        if selected {
            frame.tint(line, c.picked);
        }
        let (head, tail) = w.split_at(prefix.len().min(w.len()));
        let tail = if selected {
        } else {
        };
    }
}
