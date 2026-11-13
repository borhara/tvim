use tinsel::app::{Frame, Panel, Rect};
use tinsel::{Rgb, Spinner, bold, mix, paint, palette};

use super::{Colors, LILAC, icon, vline};
use crate::editor::Editor;
use crate::gitmenu::{GitMenu, Group, Tab};

fn group_color(g: Group) -> Rgb {
    match g {
        Group::Staged => palette::SAGE,
        Group::Changes => palette::AMBER,
        Group::Untracked => LILAC,
    }
}

fn letter_color(l: char) -> Rgb {
    match l {
        'A' => palette::SAGE,
        'D' => palette::RED,
        'R' | 'C' => palette::BLUSH,
        'U' => LILAC,
        _ => palette::AMBER,
    }
}

pub(super) fn draw(m: &GitMenu, e: &Editor, frame: &mut Frame, c: &Colors) -> Option<(usize, usize)> {
    let area = frame.area();
    if !m.ready() || area.width < 24 || area.height < 8 {
        return None;
    }
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
    let changes = m.changes();
    let status = match m.tab() {
        Tab::Files if changes.is_empty() => "clean".to_string(),
        Tab::Files => format!("{} changed", changes.len()),
        Tab::Log => "recent commits".to_string(),
        Tab::Branches => "branches".to_string(),
    };
    let panel = Panel::new(m.repo_name()).status(status).focused(true);
    if inner.height < 6 || inner.width < 20 {
        return None;
    }

    header(m, frame, Rect::new(inner.x, inner.y, inner.width, 1), t);
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

fn header(m: &GitMenu, frame: &mut Frame, row: Rect, t: f32) {
    let Some(s) = m.status() else { return };
    let mut text = format!(
        "{}  {}",
        paint(palette::ROSE, "⎇"),
        bold(&paint(palette::PETAL, &s.branch))
    );
    match s.upstream {
        Some((0, 0)) => text.push_str(&paint(palette::MUTED, "  up to date")),
        Some((ahead, behind)) => {
            if ahead > 0 {
                text.push_str(&paint(palette::SAGE, &format!("  ↑{ahead}")));
            }
            if behind > 0 {
                text.push_str(&paint(palette::AMBER, &format!("  ↓{behind}")));
            }
        }
        None => text.push_str(&paint(palette::MUTED, "  no upstream")),
    }
    frame.print(row, 0, &text);
    let right = match m.busy() {
        Some(what) => working(what, t),
        None => {
            let count = |g: Group| s.changes.iter().filter(|c| c.group == g).count();
            [Group::Staged, Group::Changes, Group::Untracked]
                .into_iter()
                .filter(|g| count(*g) > 0)
                .map(|g| {
                    let what = match g {
                        Group::Staged => "staged",
                        Group::Changes => "changed",
                        Group::Untracked => "untracked",
                    };
                    paint(group_color(g), &format!("{} {what}", count(g)))
                })
                .collect::<Vec<_>>()
                .join(&paint(palette::LINE, " · "))
        }
    };
    let w = tinsel::text::width(&right);
    if w + tinsel::text::width(&text) + 2 <= row.width {
        frame.print(Rect::new(row.x + row.width - w, row.y, w, 1), 0, &right);
    }
}

fn working(what: &str, t: f32) -> String {
    format!("{} {}", Spinner::Sparkle.frame(t), paint(palette::BLUSH, &format!("{what}…")))
}

fn mark_row(frame: &mut Frame, row: Rect, c: &Colors) {
    frame.tint(row, mix(c.bar, palette::ROSE, 0.22));
    frame.print(Rect::new(row.x, row.y, 1, 1), 0, &paint(palette::ROSE, "▌"));
}

fn files(m: &GitMenu, frame: &mut Frame, list: Rect, c: &Colors) {
    let changes = m.changes();
    if changes.is_empty() {
        frame.print(list, 0, &paint(palette::MUTED, "  ✦ nothing to commit"));
        frame.print(list, 1, &paint(palette::MUTED, "    the tree is clean"));
        return;
    }
    // headers sit between the groups, so rows and changes aren't one to one
    let mut rows: Vec<(Option<usize>, String)> = Vec::new();
    for g in [Group::Staged, Group::Changes, Group::Untracked] {
        let n = changes.iter().filter(|ch| ch.group == g).count();
        if n == 0 {
            continue;
        }
        let color = group_color(g);
        rows.push((
            None,
            format!(
                " {} {} {}",
                paint(color, "▾"),
                bold(&paint(color, g.name())),
                paint(palette::MUTED, &n.to_string())
            ),
        ));
        for (i, ch) in changes.iter().enumerate().filter(|(_, ch)| ch.group == g) {
            let (dir, name) = match ch.path.rsplit_once('/') {
                Some((d, n)) => (format!(" {d}/"), n),
                None => (String::new(), ch.path.as_str()),
            };
            let (glyph, gc) = icon(name, false);
            let name = if ch.letter == 'D' {
                paint(palette::MUTED, name)
            } else {
                paint(palette::TEXT, name)
            };
            rows.push((
                Some(i),
                format!(
                    "   {} {} {name}{}",
                    bold(&paint(letter_color(ch.letter), &ch.letter.to_string())),
                    paint(gc, glyph),
                    paint(palette::MUTED, &dir)
                ),
            ));
        }
    }
    let at = rows.iter().position(|(i, _)| *i == Some(m.selected())).unwrap_or(0);
    let top = (at + 1).saturating_sub(list.height);
    // keep the group's header in sight when its first file is picked
    let top = if top > 0 && rows[top - 1].0.is_none() && top == at {
        top - 1
    } else {
        top
    };
    for (y, (i, text)) in rows
        .iter()
        .enumerate()
        .skip(top)
        .take(list.height)
        .map(|(n, r)| (n - top, r))
    {
        let row = Rect::new(list.x, list.y + y, list.width, 1);
        if *i == Some(m.selected()) {
            mark_row(frame, row, c);
        }
        frame.print(Rect::new(row.x + 1, row.y, row.width - 1, 1), 0, text);
    }
}

fn entries(m: &GitMenu, frame: &mut Frame, list: Rect, c: &Colors) {
    let all = m.entries();
    if all.is_empty() {
        let note = if m.busy().is_some() {
            "  reading…"
        } else {
            "  nothing here yet"
        };
        frame.print(list, 0, &paint(palette::MUTED, note));
        return;
    }
    let top = (m.selected() + 1).saturating_sub(list.height);
    for (i, en) in all.iter().enumerate().skip(top).take(list.height) {
        let row = Rect::new(list.x, list.y + i - top, list.width, 1);
        if i == m.selected() {
            mark_row(frame, row, c);
        }
        let text = if m.tab() == Tab::Log {
            format!(
                "  {} {}",
                paint(palette::AMBER, &en.id),
                paint(palette::TEXT, &en.title)
            )
        } else if en.current {
            format!(
                "  {} {}",
                paint(palette::ROSE, "✦"),
                bold(&paint(palette::PETAL, &en.id))
            )
        } else {
            format!("  {} {}", paint(palette::MUTED, "·"), paint(palette::BLUSH, &en.id))
        };
        frame.print(row, 0, &text);
    }
}

fn diff_line(l: &str) -> String {
    let head = [
        "diff ",
        "index ",
        "+++",
        "---",
        "new file",
        "deleted file",
        "similarity",
        "rename ",
    ];
    if head.iter().any(|h| l.starts_with(h)) {
        bold(&paint(palette::MUTED, l))
    } else if l.starts_with("@@") {
        paint(mix(palette::MUTED, LILAC, 0.45), l)
    } else if l.starts_with('+') {
        paint(palette::SAGE, l)
    } else if l.starts_with('-') {
        paint(palette::RED, l)
    } else if l.starts_with("commit ") {
        bold(&paint(palette::AMBER, l))
    } else if let Some((file, bar)) = l.split_once(" | ") {
        let bar: String = bar
            .chars()
            .map(|ch| match ch {
                '+' => paint(palette::SAGE, "+"),
                '-' => paint(palette::RED, "-"),
                _ => paint(palette::MUTED, &ch.to_string()),
            })
            .collect();
        format!("{}{}{bar}", paint(palette::TEXT, file), paint(palette::LINE, " | "))
    } else {
        paint(palette::SOFT, l)
    }
}
