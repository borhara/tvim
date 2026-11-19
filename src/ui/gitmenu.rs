use tinsel::app::{Frame, Modal, Panel, Rect};
use tinsel::{Rgb, Spinner, bold, italic, mix, paint, palette};

use super::{Colors, LILAC, icon, input};
use crate::gitmenu::{Entry, GitMenu, Group, Tab, Typing};

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

const SPIN_FPS: u32 = 30;

pub(super) fn draw(m: &GitMenu, frame: &mut Frame, c: &Colors) -> Option<(usize, usize)> {
    let area = frame.area();
    if !m.ready() || area.width < 24 || area.height < 8 {
        return None;
    }
    let t = frame.time();
    // the spinners turn while git works
    if m.working() {
        frame.animate_at(SPIN_FPS);
    }
    let width = area.width.saturating_sub(6).clamp(24, 132);
    let height = area.height.saturating_sub(4).clamp(8, 34);
    let rect = Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 3,
        width,
        height,
    );
    let changes = m.changes();
    let status = match m.tab() {
        Tab::Files if changes.is_empty() => "clean".to_string(),
        Tab::Files => format!("{} changed", changes.len()),
        Tab::Log => "recent commits".to_string(),
        Tab::Branches => "branches".to_string(),
        Tab::Graph => "graph".to_string(),
    };
    let panel = Panel::new(m.repo_name()).status(status).focused(true);
    let mut modal = Modal::new(panel).at(rect).background(c.bar);
    // a snapshot, at time 0, shows it settled
    if let Some(at) = m.opened().filter(|_| t > 0.0) {
        modal = modal.opened(at);
    }
    let inner = frame.modal(&modal);
    if inner.height < 6 || inner.width < 20 {
        return None;
    }

    header(m, frame, Rect::new(inner.x, inner.y, inner.width, 1), t);
    let is_graph = m.tab() == Tab::Graph;
    let graph_col_width = (inner.width * 3 / 10).clamp(18, 30);
    let list_width = if is_graph {
        graph_col_width
    } else {
        (inner.width * 2 / 5).clamp(20, 48).min(inner.width)
    };
    let split = inner.x + list_width;
    let body_height = inner.height - 5;
    let body_y = inner.y + 2;
    // a rule above and below the body, and one between list and pane joining them
    frame.hline(inner.x, inner.y + 1, inner.width, palette::LINE);
    frame.hline(inner.x, body_y + body_height, inner.width, palette::LINE);
    frame.vline(split, inner.y + 1, body_height + 2, palette::LINE);
    let list = Rect::new(inner.x, body_y, list_width, body_height);
    let pane = Rect::new(
        split + 2,
        body_y,
        (inner.x + inner.width).saturating_sub(split + 2),
        body_height,
    );
    match m.tab() {
        Tab::Files => {
            files(m, frame, list, c);
            right_pane(m, frame, pane, t);
        }
        Tab::Log | Tab::Branches => {
            entries(m, frame, list, c);
            right_pane(m, frame, pane, t);
        }
        Tab::Graph => {
            graph_view(m, frame, list, pane, c);
        }
    }
    let at = prompt(
        m,
        frame,
        Rect::new(inner.x, inner.y + inner.height - 2, inner.width, 1),
        t,
        c,
    );
    hints(m, frame, Rect::new(inner.x, inner.y + inner.height - 1, inner.width, 1));
    at
}

fn header(m: &GitMenu, frame: &mut Frame, row: Rect, t: f64) {
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

fn working(what: &str, t: f64) -> String {
    format!(
        "{} {}",
        Spinner::Sparkle.frame(t),
        paint(palette::BLUSH, &format!("{what}…"))
    )
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

fn graph_view(m: &GitMenu, frame: &mut Frame, graph_pane: Rect, commits_pane: Rect, c: &Colors) {
    let all = m.entries();
    if all.is_empty() {
        let note = if m.busy().is_some() {
            "  reading…"
        } else {
            "  nothing here yet"
        };
        frame.print(graph_pane, 0, &paint(palette::MUTED, note));
        return;
    }
    let top = (m.selected() + 1).saturating_sub(graph_pane.height);
    for (i, en) in all.iter().enumerate().skip(top).take(graph_pane.height) {
        let y = graph_pane.y + i - top;
        let left_row = Rect::new(graph_pane.x, y, graph_pane.width, 1);
        let right_row = Rect::new(commits_pane.x, y, commits_pane.width, 1);
        if i == m.selected() {
            mark_row(frame, left_row, c);
            frame.tint(right_row, mix(c.bar, palette::ROSE, 0.22));
        }

        let col0 = if i == m.selected() {
            paint(palette::ROSE, "▌")
        } else {
            " ".to_string()
        };

        let indicator = if en.current {
            paint(palette::ROSE, "✦")
        } else if !en.branch.is_empty() {
            paint(palette::ROSE, "⎇")
        } else {
            " ".to_string()
        };

        let graph_text = style_graph_chars(&en.detail);
        let mut left_text = format!("{col0}{indicator} {graph_text}");

        if !en.branch.is_empty() {
            let used = tinsel::text::width(&left_text) + 1;
            if left_row.width > used {
                let avail = left_row.width - used;
                let branch_label = if tinsel::text::width(&en.branch) > avail {
                    tinsel::text::truncate(&en.branch, avail)
                } else {
                    en.branch.clone()
                };
                left_text.push(' ');
                left_text.push_str(&italic(&paint(LILAC, &branch_label)));
            }
        }

        frame.print(left_row, 0, &left_text);

        let commit_text = format_commit_entry(en, commits_pane.width);
        frame.print(right_row, 0, &commit_text);
    }
}

fn format_commit_entry(en: &Entry, max_width: usize) -> String {
    let colored_id = bold(&paint(palette::AMBER, &en.id));
    if en.title.is_empty() {
        return colored_id;
    }

    let meta = if !en.author.is_empty() && !en.date.is_empty() {
        format!("({}, {})", en.author, en.date)
    } else if !en.author.is_empty() {
        format!("({})", en.author)
    } else if !en.date.is_empty() {
        format!("({})", en.date)
    } else {
        String::new()
    };

    let id_w = tinsel::text::width(&en.id);
    let meta_w = tinsel::text::width(&meta);

    if max_width == 0 {
        let colored_meta = if meta.is_empty() {
            String::new()
        } else {
            format!(" {}", paint(palette::MUTED, &meta))
        };
        return format!("{colored_id} {}{colored_meta}", paint(palette::TEXT, &en.title));
    }

    let rem = max_width.saturating_sub(id_w + 1);
    if rem == 0 {
        return colored_id;
    }

    let title_w = tinsel::text::width(&en.title);
    if !meta.is_empty() && rem > meta_w + 6 {
        let avail_for_title = rem.saturating_sub(meta_w + 1);
        let truncated_title = if title_w > avail_for_title {
            tinsel::text::truncate(&en.title, avail_for_title)
        } else {
            en.title.clone()
        };
        let colored_meta = paint(palette::MUTED, &meta);
        format!("{colored_id} {} {colored_meta}", paint(palette::TEXT, &truncated_title))
    } else {
        let truncated_title = if title_w > rem {
            tinsel::text::truncate(&en.title, rem)
        } else {
            en.title.clone()
        };
        format!("{colored_id} {}", paint(palette::TEXT, &truncated_title))
    }
}

fn style_graph_chars(s: &str) -> String {
    s.chars()
        .map(|ch| match ch {
            '●' => bold(&paint(palette::ROSE, "●")),
            '│' => paint(palette::LINE, "│"),
            '─' => paint(palette::LINE, "─"),
            '╭' => paint(palette::LINE, "╭"),
            '╮' => paint(palette::LINE, "╮"),
            '╯' => paint(palette::LINE, "╯"),
            '╰' => paint(palette::LINE, "╰"),
            '├' => paint(palette::LINE, "├"),
            '┤' => paint(palette::LINE, "┤"),
            '┬' => paint(palette::LINE, "┬"),
            '┴' => paint(palette::LINE, "┴"),
            '┼' => paint(palette::LINE, "┼"),
            ' ' => " ".to_string(),
            _ => ch.to_string(),
        })
        .collect()
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

fn right_pane(m: &GitMenu, frame: &mut Frame, pane: Rect, t: f64) {
    if pane.width < 4 || pane.height < 2 {
        return;
    }
    let rows = pane.height - 1;
    let print_lines = |frame: &mut Frame, lines: &[String], from: usize, style: &dyn Fn(&str) -> String| {
        for (i, l) in lines.iter().skip(from).take(rows).enumerate() {
            frame.print(pane, i + 1, &style(l));
        }
    };
    if let Some(o) = m.output() {
        let badge = if o.code == 0 {
            paint(palette::SAGE, "✓ exit 0")
        } else {
            paint(palette::RED, &format!("✕ exit {}", o.code))
        };
        let title = format!(
            "{} {}",
            paint(palette::ROSE, "$"),
            bold(&paint(palette::TEXT, &o.title))
        );
        frame.print(pane, 0, &title);
        let w = tinsel::text::width(&badge);
        if w + tinsel::text::width(&title) + 2 <= pane.width {
            frame.print(Rect::new(pane.x + pane.width - w, pane.y, w, 1), 0, &badge);
        }
        print_lines(frame, &o.lines, o.scroll, &|l| paint(palette::TEXT, l));
        return;
    }
    let title = match m.tab() {
        Tab::Files => m.current().map(|ch| {
            let what = match ch.group {
                Group::Staged => "staged",
                Group::Changes => "not staged",
                Group::Untracked => "untracked",
            };
            format!(
                "{}  {}",
                bold(&paint(palette::TEXT, &ch.path)),
                italic(&paint(palette::MUTED, what))
            )
        }),
        Tab::Log | Tab::Branches | Tab::Graph => m.entries().get(m.selected()).map(|en| {
            let detail = if en.detail.is_empty() {
                String::new()
            } else {
                format!("  {}", en.detail)
            };
            format!(
                "{}{}",
                bold(&paint(palette::TEXT, &en.id)),
                italic(&paint(palette::MUTED, &detail))
            )
        }),
    };
    let Some(title) = title else { return };
    frame.print(pane, 0, &title);
    match m.peek() {
        Some([]) => {
            frame.print(pane, 1, &paint(palette::MUTED, "no differences"));
        }
        Some(lines) => print_lines(frame, lines, m.peek_scroll(), &diff_line),
        None if m.working() => {
            let wait = format!("{} {}", Spinner::Sparkle.frame(t), paint(palette::MUTED, "loading…"));
            frame.print(pane, 1, &wait);
        }
        None => {}
    }
}

fn prompt(m: &GitMenu, frame: &mut Frame, row: Rect, t: f64, c: &Colors) -> Option<(usize, usize)> {
    if let Some(ch) = m.confirming() {
        let text = format!(
            "{} {}{}   {} {}",
            bold(&paint(palette::RED, "discard")),
            paint(palette::TEXT, &format!("changes to {}", ch.path)),
            paint(palette::TEXT, "?"),
            bold(&paint(palette::ROSE, "y")),
            paint(palette::MUTED, "yes · any other key keeps them")
        );
        frame.print(row, 0, &text);
        return None;
    }
    if let Some((kind, text, at)) = m.typing() {
        let label = match kind {
            Typing::Commit => format!("{} {} ", paint(palette::SAGE, "✎"), paint(palette::SAGE, "message ›")),
            Typing::Command => format!("{} ", bold(&paint(palette::ROSE, "git ›"))),
        };
        frame.tint(row, mix(c.bar, palette::ROSE, 0.1));
        return Some(input(frame, row, &label, &text, at, Some(palette::TEXT)));
    }
    let text = match (m.note(), m.busy()) {
        (_, Some(what)) => working(what, t),
        (Some(n), _) if n.error => paint(palette::RED, &format!("✕ {}", n.text)),
        (Some(n), _) => paint(palette::SAGE, &format!("✓ {}", n.text)),
        (None, None) => italic(&paint(palette::MUTED, ": runs any git command here")),
    };
    frame.print(row, 0, &text);
    None
}

fn hints(m: &GitMenu, frame: &mut Frame, row: Rect) {
    let keys: &[(&str, &str)] = if m.typing().is_some() {
        &[("⏎", "run"), ("esc", "cancel"), ("↑↓", "history")]
    } else if m.output().is_some() {
        &[
            ("j/k", "scroll"),
            ("esc", "close output"),
            (":", "git …"),
            ("r", "refresh"),
        ]
    } else {
        match m.tab() {
            Tab::Files => &[
                ("j/k", "move"),
                ("s", "stage"),
                ("a", "all"),
                ("u", "unstage all"),
                ("d", "discard"),
                ("c", "commit"),
                ("⏎", "open"),
                ("l", "log"),
                ("b", "branches"),
                ("p/P", "pull/push"),
                (":", "git …"),
                ("r", "refresh"),
                ("q", "close"),
            ],
            Tab::Log => &[
                ("j/k", "move"),
                ("J/K", "scroll"),
                ("l", "files"),
                ("b", "branches"),
                (":", "git …"),
                ("q", "close"),
            ],
            Tab::Branches => &[
                ("j/k", "move"),
                ("⏎", "switch"),
                ("b", "files"),
                ("l", "log"),
                (":", "git …"),
                ("q", "close"),
            ],
            Tab::Graph => &[
                ("j/k", "move"),
                ("t", "files"),
                ("l", "log"),
                ("b", "branches"),
                (":", "git …"),
                ("q", "close"),
            ],
        }
    };
    let keys = if m.typing().is_some_and(|(k, _, _)| k == Typing::Commit) {
        &keys[..2]
    } else {
        keys
    };
    let mut line = String::new();
    let mut used = 0;
    for (k, d) in keys {
        let w = tinsel::text::width(k) + 1 + tinsel::text::width(d) + 2;
        if used + w > row.width {
            break;
        }
        line.push_str(&format!(
            "{} {}  ",
            bold(&paint(palette::ROSE, k)),
            paint(palette::MUTED, d)
        ));
        used += w;
    }
    frame.print(row, 0, &line);
}
