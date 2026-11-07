use tinsel::app::{Frame, Panel, Rect};
use tinsel::{paint, palette};

use super::{Colors, icon, vline};
use crate::editor::Editor;
use crate::finder::Finder;

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

struct Picker {
    list: Rect,
    pane: Option<Rect>,
    at: (usize, usize),
}

// the finder's and grep's frame; split gives the list's width when there's room for a preview
fn picker(
    frame: &mut Frame,
    area: Rect,
    (max_width, max_height): (usize, usize),
    panel: Panel,
    query: &str,
    split: impl Fn(usize) -> Option<usize>,
    c: &Colors,
) -> Option<Picker> {
    if area.width < 20 || area.height < 6 {
        return None;
    }
    let width = area.width.saturating_sub(8).clamp(20, max_width);
    let height = area.height.saturating_sub(4).clamp(6, max_height);
    let rect = Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 3,
        width,
        height,
    );
    let panel = panel.footer("↑↓ choose · enter open · esc close").focused(true);
    let inner = float(frame, rect, panel, c);

    frame.print(inner, 0, &format!("{} {}", paint(palette::ROSE, "›"), query));
    let at = (inner.x + 2 + tinsel::text::width(query)).min(inner.x + inner.width.saturating_sub(1));
    frame.print(inner, 1, &paint(palette::LINE, &"─".repeat(inner.width)));

    let rows = Rect::new(inner.x, inner.y + 2, inner.width, inner.height.saturating_sub(2));
    let split = split(inner.width);
    let list = Rect::new(rows.x, rows.y, split.unwrap_or(inner.width), rows.height);
    let pane = split.map(|w| {
        vline(frame, list.x + w, rows.y, rows.height, &paint(palette::LINE, "│"));
        frame.print(Rect::new(list.x + w, inner.y + 1, 1, 1), 0, &paint(palette::LINE, "┬"));
        Rect::new(list.x + w + 2, rows.y, rows.width.saturating_sub(w + 2), rows.height)
    });
    Some(Picker {
        list,
        pane,
        at: (at, inner.y),
    })
}

pub(super) fn finder(f: &Finder, frame: &mut Frame, area: Rect, c: &Colors) -> Option<(usize, usize)> {
    let count = if f.cut_short() {
        format!("{} of {}+", f.match_count(), f.total())
    } else {
        format!("{} of {}", f.match_count(), f.total())
    };
    let p = picker(frame, area, (120, 26), panel, f.query(), |w| (w >= 80).then(|| w * 2 / 5), c)?;
    let list = p.list;
    if f.match_count() == 0 {
        let note = if f.total() == 0 && f.is_recent() {
            "nothing opened yet"
        } else if f.total() == 0 {
            "no files here"
        } else {
            "nothing matches"
        };
        return Some(p.at);
    }
    let top = f.top(list.height);
    for (i, (path, hits)) in f.matches().enumerate().skip(top).take(list.height) {
        let y = list.y + i - top;
        let selected = i == f.selected();
        if selected {
            frame.tint(Rect::new(list.x, y, list.width, 1), c.picked);
        }
        if f.is_recent() {
            continue;
        }
        let name_start = path.rfind('/').map_or(0, |b| path[..b].chars().count() + 1);
        let file = &path[path.rfind('/').map_or(0, |b| b + 1)..];
        let (glyph, color) = icon(file, false);
        let room = list.width.saturating_sub(5);
        let skip = (tinsel::text::width(path) + 1).saturating_sub(room);
        frame.print(Rect::new(list.x + 4, y, room, 1), 0, &shown);
    }
    if let Some(pane) = p.pane {
        if let Some((path, lines)) = f.preview() {
            let at = f.line(f.selected()).filter(|r| *r < lines.len()).map(|r| (r, 0, 0));
            preview_lines(&path, &lines, at, frame, pane, c);
        }
    }
    Some(p.at)
}
