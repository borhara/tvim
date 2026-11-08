use tinsel::app::{Frame, Panel, Rect};
use tinsel::{bold, mix, paint, palette};

use super::{Colors, icon, styled_line, vline, with_hits};
use crate::editor::Editor;
use crate::finder::Finder;
use crate::syntax::{self, State, Syntax};

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
        let shown = with_hits(path, |j| hits.contains(&j), |j, s| {
        });
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


fn preview_lines(
    path: &str,
    lines: &[String],
    at: Option<(usize, usize, usize)>,
    frame: &mut Frame,
    pane: Rect,
    c: &Colors,
) {
    let name = path.rsplit('/').next().unwrap_or(path);
    let syntax = Syntax::detect(name);
    let about = syntax.map_or("text".to_string(), |s| s.name().to_string());
    frame.print(
        pane,
        0,
        &format!(
            "{}  {}",
            bold(&paint(palette::SOFT, path)),
            paint(palette::MUTED, &about)
        ),
    );
    let rows = pane.height.saturating_sub(2);
    let row = at.map(|a| a.0);
    let first = row.map_or(0, |r| r.saturating_sub(rows / 3));
    let mut state = State::Normal;
    let number_width = (first + rows).to_string().len().max(3) + 1;
    for (i, line) in lines.iter().enumerate().take(first + rows) {
        let kinds = syntax.map(|s| {
            let (k, next) = syntax::highlight(line, s, state);
            state = next;
            k
        });
        if i < first {
            continue;
        }
        let y = pane.y + 2 + i - first;
        let w = pane.width.saturating_sub(number_width);
        let here = row == Some(i);
        if here {
            frame.tint(Rect::new(pane.x, y, pane.width, 1), mix(c.bar, palette::ROSE, 0.18));
        }
        if let Some((_, col, len)) = at.filter(|a| here && a.2 > 0) {
            let a = crate::text::vcol(line, col, 4);
            let b = crate::text::vcol(line, col + len, 4);
            if a < w {
                let tint = mix(c.bar, palette::AMBER, 0.55);
                frame.tint(
                    Rect::new(pane.x + number_width + a, y, (b - a).min(w - a).max(1), 1),
                    tint,
                );
            }
        }
        let label = format!("{:>w$} ", i + 1, w = number_width - 1);
        let n = if here {
            bold(&paint(palette::ROSE, &label))
        } else {
            paint(palette::LINE, &label)
        };
        frame.print(Rect::new(pane.x, y, number_width, 1), 0, &n);
        frame.print(
            Rect::new(pane.x + number_width, y, w, 1),
            0,
            &styled_line(line, kinds.as_deref(), 4, c),
        );
    }
}
