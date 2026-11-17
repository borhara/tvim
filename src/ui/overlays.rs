use tinsel::app::{Frame, Panel, Rect};
use tinsel::{bold, mix, paint, palette};

use super::{Colors, file_name, icon, input, styled_line, vline, with_hits};
use crate::editor::Editor;
use crate::finder::Finder;
use crate::grep::Grep;
use crate::syntax::{self, State, Syntax};

pub(super) fn float(frame: &mut Frame, rect: Rect, panel: Panel, c: &Colors) -> Rect {
    let inner = panel.inner(rect);
    frame.render(rect, panel);
    frame.tint(rect, c.bar);
    inner
}

pub(super) fn palette(e: &Editor, frame: &mut Frame, area: Rect, c: &Colors) -> Option<(usize, usize)> {
    let cmd = e.cmdline();
    let error = e.message().filter(|m| m.error).map(|m| m.text.clone());
    let width = area.width.saturating_sub(8).clamp(30.min(area.width), 76);
    let list_rows = suggestions.len().min(8).min(area.height.saturating_sub(10));
    let height = 3 + if list_rows > 0 { list_rows + 1 } else { 0 } + usize::from(error.is_some());
    let rect = Rect::new(
        area.x + (area.width - width) / 2,
        area.y + area.height.saturating_sub(height) / 4,
        width,
        height,
    );
    let panel = Panel::new("command")
        .footer("tab complete · ↑↓ history · esc close")
        .focused(true);
    let inner = float(frame, rect, panel, c);

    let label = format!("{} ", paint(palette::ROSE, ":"));
    let at = input(frame, Rect::new(inner.x, inner.y, inner.width, 1), &label, &cmd.text, cmd.cursor, None);
    let mut row = 1;
    if let Some(err) = &error {
        frame.print(inner, row, &paint(palette::RED, &format!("✕ {err}")));
        row += 1;
    }
    if list_rows > 0 {
        frame.print(inner, row, &paint(palette::LINE, &"─".repeat(inner.width)));
        row += 1;
        let typed = cmd.text.rsplit(' ').next().unwrap_or("");
        let first = picked.map_or(0, |p| (p + 1).saturating_sub(list_rows));
        let label_width = suggestions
            .iter()
            .map(|s| tinsel::text::width(&s.label))
            .max()
            .unwrap_or(0)
            .min(24);
        for (i, s) in suggestions.iter().enumerate().skip(first).take(list_rows) {
            let y = inner.y + row + i - first;
            let selected = picked == Some(i);
            if selected {
                frame.tint(Rect::new(inner.x, y, inner.width, 1), c.picked);
            }
            let marker = marker(selected);
            let matched = if picked.is_none() && s.label.starts_with(typed) {
                typed.len()
            } else {
                0
            };
            let (head, tail) = s.label.split_at(matched.min(s.label.len()));
            let pad = " ".repeat(label_width.saturating_sub(tinsel::text::width(&s.label)));
            let label = format!(
                "{}{}{pad}",
                bold(&paint(palette::ROSE, head)),
                paint(palette::TEXT, tail)
            );
            let line = format!("{marker}{label}   {}", paint(palette::MUTED, &s.desc));
            frame.print(Rect::new(inner.x, y, inner.width, 1), 0, &line);
            if !s.value.is_empty() {
                let color = match s.value.as_str() {
                    "on" => palette::SAGE,
                    "off" => palette::MUTED,
                    _ => palette::AMBER,
                };
                let v = paint(color, &s.value);
                let w = tinsel::text::width(&v);
                frame.print(Rect::new(inner.x + inner.width - w, y, w, 1), 0, &v);
            }
        }
    }
    Some(at)
}

fn marker(selected: bool) -> String {
    if selected {
        paint(palette::ROSE, "› ")
    } else {
        "  ".into()
    }
}

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
    let inner = float(frame, Rect::new(x, y, width, height), panel, c);
    let first = picked.map_or(0, |p| (p + 1).saturating_sub(rows));
    for (i, w) in words.iter().enumerate().skip(first).take(rows) {
        let line = Rect::new(inner.x, inner.y + i - first, inner.width, 1);
        let selected = picked == Some(i);
        if selected {
            frame.tint(line, c.picked);
        }
        let marker = marker(selected);
        let (head, tail) = w.split_at(prefix.len().min(w.len()));
        let tail = if selected {
            bold(&paint(palette::PETAL, tail))
        } else {
            paint(palette::BLUSH, tail)
        };
        frame.print(line, 0, &format!("{marker}{}{tail}", bold(&paint(palette::ROSE, head))));
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
    let panel = Panel::new(f.title()).status(count);
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
        frame.print(list, 0, &paint(palette::MUTED, note));
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
            recent_row(f, i, path, hits, frame, Rect::new(list.x, y, list.width, 1));
            continue;
        }
        let name_start = path.rfind('/').map_or(0, |b| path[..b].chars().count() + 1);
        let file = &path[path.rfind('/').map_or(0, |b| b + 1)..];
        let (glyph, color) = icon(file, false);
        let shown = with_hits(path, |j| hits.contains(&j), |j, s| {
            paint(if j < name_start { palette::MUTED } else { palette::TEXT }, s)
        });
        frame.print(
            Rect::new(list.x, y, list.width, 1),
            0,
            &format!("{}{} ", marker(selected), paint(color, glyph)),
        );
        let room = list.width.saturating_sub(5);
        let skip = (tinsel::text::width(path) + 1).saturating_sub(room);
        frame.print_from(Rect::new(list.x + 4, y, room, 1), 0, &shown, skip);
    }
    if let Some(pane) = p.pane {
        if let Some((path, lines)) = f.preview() {
            let at = f.line(f.selected()).filter(|r| *r < lines.len()).map(|r| (r, 0, 0));
            preview_lines(&path, &lines, at, frame, pane, c);
        }
    }
    Some(p.at)
}

// like the finder's rows, but the name first, its folder after, and the line it was left at
fn recent_row(f: &Finder, i: usize, path: &str, hits: &[usize], frame: &mut Frame, row: Rect) {
    let selected = i == f.selected();
    let split = path.rfind('/').map_or(0, |b| b + 1);
    let (dir, file) = path.split_at(split);
    let name_start = dir.chars().count();
    let (glyph, color) = icon(file, false);
    let at = f.line(i).map_or(String::new(), |r| format!(":{}", r + 1));
    let name = with_hits(file, |j| hits.contains(&(name_start + j)), |_, s| {
        if selected {
            bold(&paint(palette::TEXT, s))
        } else {
            paint(palette::SOFT, s)
        }
    });
    let dir = dir.trim_end_matches('/');
    let home = std::env::var("HOME").ok().filter(|h| !h.is_empty());
    let dir = match home.as_deref().and_then(|h| std::path::Path::new(dir).strip_prefix(h).ok()) {
        Some(rel) if rel.as_os_str().is_empty() => "~".to_string(),
        Some(rel) => format!("~/{}", rel.display()),
        None => dir.to_string(),
    };
    let room = row.width.saturating_sub(at.len() + 7 + tinsel::text::width(file));
    let dir = super::cut_left(&dir, room);
    let mark = if selected {
        paint(palette::ROSE, "▎")
    } else {
        " ".into()
    };
    frame.print(
        row,
        0,
        &format!("{mark} {} {name} {}", paint(color, glyph), paint(palette::MUTED, &dir)),
    );
    let at_x = row.x + row.width.saturating_sub(at.len() + 1);
    frame.print(Rect::new(at_x, row.y, at.len(), 1), 0, &paint(palette::MUTED, &at));
}


pub(super) fn grep(g: &Grep, frame: &mut Frame, area: Rect, c: &Colors) -> Option<(usize, usize)> {
    let hits = g.hits();
    let reading = if g.loading() {
        format!("reading {} files… ", g.file_count())
    } else {
        String::new()
    };
    let count = match (hits.len(), g.query().trim().is_empty()) {
        (_, true) if g.loading() => reading.trim_end().to_string(),
        (_, true) => format!("{} files", g.file_count()),
        (0, false) => format!("{reading}no matches"),
        (n, false) => {
            let more = if g.capped() { "+" } else { "" };
            format!("{reading}{n}{more} in {} files", g.files_hit())
        }
    };
    let root = file_name(Some(g.root()), "/");
    let panel = Panel::new(format!("search in {root}")).status(count);
    let split = |w: usize| (w >= 90 && !hits.is_empty()).then_some(w / 2);
    let p = picker(frame, area, (140, 30), panel, g.query(), split, c)?;
    let list = p.list;
    if hits.is_empty() {
        let note = if g.query().trim().is_empty() {
            "type to search every file"
        } else {
            "nothing matches"
        };
        frame.print(list, 0, &paint(palette::MUTED, note));
        return Some(p.at);
    }
    let query_len = g.query().trim().chars().count();
    let top = g.top(list.height);
    for (i, hit) in hits.iter().enumerate().skip(top).take(list.height) {
        let y = list.y + i - top;
        let selected = i == g.selected();
        if selected {
            frame.tint(Rect::new(list.x, y, list.width, 1), c.picked);
        }
        let path = g.path(hit);
        let path = path.as_str();
        let file = &path[path.rfind('/').map_or(0, |b| b + 1)..];
        let (glyph, color) = icon(file, false);
        let place = format!(
            "{}{}",
            paint(palette::SOFT, path),
            paint(palette::AMBER, &format!(":{}", hit.row + 1))
        );
        let place_width = tinsel::text::width(path) + 1 + (hit.row + 1).to_string().len();
        let line = g.line(hit);
        let line = line.as_str();
        let lead = line.len() - line.trim_start().len();
        let col = hit.col.saturating_sub(line[..lead].chars().count());
        let shown = with_hits(
            line.trim_start(),
            |j| j >= col && j < col + query_len,
            |_, s| paint(palette::TEXT, s),
        );
        frame.print(
            Rect::new(list.x, y, list.width, 1),
            0,
            &format!("{}{} {place}  ", marker(selected), paint(color, glyph)),
        );
        let x = list.x + 4 + place_width + 2;
        if x + 4 < list.x + list.width {
            // keep the match in view on a long line
            let room = list.x + list.width - x;
            let skip = (col + query_len + 2).saturating_sub(room);
            frame.print_from(Rect::new(x, y, room, 1), 0, &shown, skip);
        }
    }
    if let Some((pane, hit)) = p.pane.zip(hits.get(g.selected())) {
        let at = (hit.row, hit.col, query_len);
        let lines = g.lines(hit, 0, hit.row + pane.height);
        preview_lines(&g.path(hit), &lines, Some(at), frame, pane, c);
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

const MOTIONS: &[(&str, &str)] = &[
    ("w b e", "words"),
    ("0 ^ $", "line start, text, end"),
    ("j k", "lines down, up"),
    ("gg G", "the top, the end"),
    ("f t", "to a character"),
    ("{ }", "paragraphs"),
    ("%", "matching bracket"),
    ("n N", "next search match"),
];

const OBJECTS: &[(&str, &str)] = &[
    ("w W", "a word, a WORD"),
    ("p", "a paragraph"),
    ("\" ' `", "quoted text"),
    ("( b", "( parentheses )"),
    ("[", "[ brackets ]"),
    ("{ B", "{ braces }"),
    ("<", "< angle brackets >"),
];

const G: &[(&str, &str)] = &[
    ("gg", "to the top"),
    ("gt gT", "next, previous file"),
    ("ge", "back to a word's end"),
    ("g_", "last text on the line"),
    ("gv", "select again"),
    ("gu gU g~", "lower, upper, swap"),
    ("gc", "comment, uncomment"),
    ("gJ", "join, no spaces"),
    ("gp gP", "put, go past it"),
    ("gi", "type where you left off"),
];

const Z: &[(&str, &str)] = &[
    ("zz", "line to the middle"),
    ("zt", "line to the top"),
    ("zb", "line to the bottom"),
];

const WINDOW: &[(&str, &str)] = &[
    ("s v", "split, above or beside"),
    ("n", "split with an empty file"),
    ("h j k l", "the window that way"),
    ("w W", "next, previous window"),
    ("q", "close this window"),
    ("o", "only this window"),
    ("=", "make them all even"),
    ("+ -", "taller, shorter"),
    ("< >", "narrower, wider"),
];
