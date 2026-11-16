use tinsel::app::{Frame, Rect, hints};
use tinsel::{Rgb, bold, mix, paint, palette};

use super::{Colors, Drawn, LILAC, accent, diff_totals, display_path, draw_explorer, input, vline, windows};
use crate::editor::{Editor, Mode};

pub(super) fn draw(e: &Editor, frame: &mut Frame, c: &Colors) -> Drawn {
    let area = frame.area();
    let status = area.height >= 2;
    let stars = area.height >= 5;
    let head = area.height >= 6;
    let roomy = e.options.roomy && area.height >= 10;
    let panes = Rect::new(
        area.x,
        area.y,
        area.width,
        area.height - usize::from(status) - usize::from(stars),
    );
    let side_width = match e.explorer() {
        None => 0,
        Some(_) if panes.width < 56 => panes.width,
        Some(_) => (panes.width / 4).clamp(26, 38),
    };
    // a full-width sidebar has nothing to be ruled off from
    let ruled = side_width > 0 && side_width < panes.width;
    let column = Rect::new(panes.x + side_width, panes.y, panes.width - side_width, panes.height);
    let top = usize::from(head) + usize::from(roomy);
    let rows = panes.height.saturating_sub(top + usize::from(roomy));
    let margin = usize::from(column.width > 1);
    let text = Rect::new(column.x + margin, panes.y + top, column.width - margin, rows);
    let editor_focused = !e.explorer_focused();
    let lit = accent(e);

    let mut body = Rect::default();
    if text.width > 0 && text.height > 0 {
        body = windows(e, frame, text, c, editor_focused, false);
        if !editor_focused && !welcome && e.options.dim {
            let faded = Rect::new(column.x, text.y, column.width, text.height);
            frame.dim(faded);
            frame.tint(faded, c.app);
        }
    }
    if ruled {
        vline(frame, column.x - 1, panes.y, panes.height, &paint(c.faint, "┊"));
    }
    if let Some(x) = e.explorer().filter(|_| side_width > 0) {
        let list = if ruled {
            Rect::new(panes.x, panes.y, side_width - 1, rows + top)
        } else {
            Rect::new(panes.x, panes.y + top, side_width, rows)
        };
        draw_explorer(e, x, frame, list, c, lit);
        if editor_focused && e.options.dim {
            frame.dim(list);
        }
    }
    if head {
        // the open files start over the text when the sidebar has the corner
        let row = if ruled {
            Rect::new(column.x - 1, area.y, area.x + area.width - (column.x - 1), 1)
        } else {
            Rect::new(area.x, area.y, area.width, 1)
        };
        // the first star sits over the line numbers' last digit
        let start = match e.window_count() {
            _ => row.x + 2,
        };
        draw_top(e, frame, row, start, lit, c, !ruled);
    }
    let mut map = None;
    if stars {
        let y = area.y + area.height - 2;
        let row = Rect::new(area.x + 2, y, area.width.saturating_sub(4), 1);
    }
    let typing_at = if status {
        let fits = panes.height >= 6 && panes.width >= 30;
        let row = Rect::new(area.x, area.y + area.height - 1, area.width, 1);
    } else {
        None
    };
    Drawn {
        body,
        text,
        panes,
        typing_at,
    }
}

pub(super) fn branch_info(e: &Editor) -> Option<String> {
    let b = e.branch()?;
    let mut parts = vec![paint(LILAC, &b)];
    if let Some((ahead, behind)) = e.upstream().filter(|u| *u != (0, 0)) {
        let mut s = Vec::new();
        if ahead > 0 {
            s.push(format!("↑{ahead}"));
        }
        if behind > 0 {
            s.push(format!("↓{behind}"));
        }
        parts.push(paint(palette::MUTED, &s.join(" ")));
    }
    parts.extend(diff_totals(e));
    Some(parts.join("  "))
}

fn thousands(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

// drops the clock first, then crlf, length, language
fn corner(e: &Editor, branch: bool, room: usize, timed: bool, c: &Colors) -> String {
    let mut facts = Vec::new();
    let branch = if branch {
        Some(branch_info(e).unwrap_or_else(|| {
            let cwd = std::env::current_dir().map(|d| display_path(&d)).unwrap_or_default();
            paint(palette::MUTED, &cwd)
        }))
    } else {
        None
    };
    let (h, m) = crate::time::clock();
    let faint = mix(palette::MUTED, palette::LINE, 0.5);
    let clock = paint(mix(faint, c.app, 0.3), &format!("{h:02}:{m:02}"));
    let build = |facts: &[String], timed: bool| {
        let mut groups = Vec::new();
        if !facts.is_empty() {
            groups.push(facts.join(&paint(faint, " · ")));
        }
        groups.extend(branch.clone().filter(|b| !b.is_empty()));
        if timed {
            groups.push(clock.clone());
        }
        groups.join("    ")
    };
    while tinsel::text::width(&shown) > room {
        if !shown.ends_with(&clock) && facts.pop().is_none() {
            return String::new();
        }
        shown = build(&facts, false);
    }
    shown
}

fn draw_top(e: &Editor, frame: &mut Frame, row: Rect, start: usize, lit: Rgb, c: &Colors, branch: bool) {
    // the current file's star and name come before anything in the corner
    let own = names
        .iter()
        .find(|t| t.current)
        .map_or(12, |t| tinsel::text::width(&t.name) + 9);
    // frames from the running app always have a time; a still frame shows no clock
    let timed = frame.time() > 0.0;
    let right = corner(e, branch, (row.x + row.width).saturating_sub(start + own + 5), timed, c);
    let right_width = tinsel::text::width(&right);
    if names.is_empty() {
        frame.print(Rect::new(start, row.y, 4, 1), 0, &bold(&paint(palette::MUTED, "tvim")));
    }
    let end = (row.x + row.width).saturating_sub(2);
    let fits = right_width > 0 && start + right_width + 12 < end;
    if fits {
        frame.print(Rect::new(end - right_width, row.y, right_width, 1), 0, &right);
    }
    let room = end.saturating_sub(start + if fits { right_width + 3 } else { 0 });

    let sep = paint(c.faint, " ─── ");
    let width = |i: usize| tinsel::text::width(&names[i].name) + 7 + if names[i].modified { 2 } else { 0 };
    let current = names.iter().position(|t| t.current).unwrap_or(0);
    let (mut first, mut last) = (current, current);
    let mut used = if names.is_empty() { 0 } else { width(current) };
    loop {
        let mut grew = false;
        if last + 1 < names.len() && used + width(last + 1) + 2 <= room {
            last += 1;
            used += width(last);
            grew = true;
        }
        if first > 0 && used + width(first - 1) + 2 <= room {
            first -= 1;
            used += width(first);
            grew = true;
        }
        if !grew {
            break;
        }
    }
    if names.is_empty() {
        return;
    }
    let mut line = String::new();
    if first > 0 {
        line.push_str(&paint(palette::MUTED, "‹ "));
    }
    for (i, tab) in names.iter().enumerate().take(last + 1).skip(first) {
        if i > first {
            line.push_str(&sep);
        }
        line.push_str(&if tab.current {
            format!("{} {}", bold(&paint(lit, "✶")), bold(&paint(palette::TEXT, &tab.name)))
        } else {
            format!("{} {}", paint(palette::LINE, "⋆"), paint(palette::MUTED, &tab.name))
        });
        if tab.modified {
        }
    }
    if last + 1 < names.len() {
        line.push_str(&paint(palette::MUTED, " ›"));
    }
    frame.print(Rect::new(start, row.y, room, 1), 0, &line);
}

fn explorer_status(e: &Editor, frame: &mut Frame, row: Rect, mid: Rect, c: &Colors) -> Option<(usize, usize)> {
    let x = e.explorer().expect("in files");
    if let Some(m) = e.message() {
        frame.print(mid, 0, &message(m));
        return None;
    }
    if x.is_filtering() {
        let filter = x.filter().unwrap_or("");
        frame.print(mid, 0, &format!("{} {filter}", paint(palette::ROSE, "/")));
        return Some((mid.x + 2 + tinsel::text::width(filter), row.y));
    }
    let path = display_path(x.root());
    let keys = [
        ("enter", "open"),
        ("-", "up"),
        ("/", "filter"),
        (".", "hidden"),
        ("q", "hide"),
    ];
    let path = if tinsel::text::width(&full) <= mid.width {
        full
    } else {
    };
    let path_w = tinsel::text::width(&path);
    let keys = hints(&keys).to_string();
    let keys_w = tinsel::text::width(&keys);
    // the hints sit in the middle of the row, like the star line above, unless the path is in the way
    let centred = (row.x + (row.width.saturating_sub(keys_w)) / 2).max(mid.x + path_w + 3);
    frame.print(mid, 0, &path);
    if centred + keys_w <= mid.x + mid.width {
        frame.print(Rect::new(centred, row.y, keys_w, 1), 0, &keys);
    }
    None
}

fn typing_line(e: &Editor, frame: &mut Frame, mid: Rect) -> (usize, usize) {
    let cmd = e.cmdline();
    let mut room = mid.width;
    if e.mode() == Mode::Search && !cmd.text.is_empty() {
        let count = match e.search_count() {
            Some((n, total)) => paint(palette::MUTED, &format!("{n} of {total}")),
            None => paint(palette::RED, "no match"),
        };
        let w = tinsel::text::width(&count);
        if w + 6 < mid.width {
            frame.print(Rect::new(mid.x + mid.width - w, mid.y, w, 1), 0, &count);
            room = mid.width - w - 2;
        }
    }
    let label = format!("{} ", paint(palette::ROSE, &cmd.prompt.to_string()));
    input(frame, Rect::new(mid.x, mid.y, room, 1), &label, &cmd.text, cmd.cursor, None)
}
