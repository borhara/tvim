use tinsel::app::{Frame, Rect, hints};
use tinsel::format::plural;
use tinsel::{Rgb, bold, mix, paint, palette};

use super::{
    Colors, Drawn, LILAC, SIDE_DIM, accent, crumb_scope, diff_totals, display_path, draw_explorer, file_name, input,
    outline, tab_names, twinkle, windows,
};
use crate::diff::Mark;
use crate::editor::{Editor, Message, Mode};
use crate::keys::show;

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
        let welcome = e.is_blank_slate() && e.mode() == Mode::Normal;
        if !editor_focused && !welcome && e.options.dim {
            let faded = Rect::new(column.x, text.y, column.width, text.height);
            frame.fade(faded);
            frame.tint(faded, c.app);
        }
    }
    if ruled {
        let rule = paint(c.faint, "┊");
        for y in panes.y..panes.y + panes.height {
            frame.print(Rect::new(column.x - 1, y, 1, 1), 0, &rule);
        }
    }
    if let Some(x) = e.explorer().filter(|_| side_width > 0) {
        let list = if ruled {
            Rect::new(panes.x, panes.y, side_width - 1, rows + top)
        } else {
            Rect::new(panes.x, panes.y + top, side_width, rows)
        };
        draw_explorer(e, x, frame, list, c, lit);
        if editor_focused && e.options.dim {
            frame.fade_by(list, SIDE_DIM);
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
            1 if !e.is_blank_slate() && e.gutter() > 4 && body.x > row.x + 6 => body.x - 4,
            _ => row.x + 2,
        };
        draw_top(e, frame, row, start, lit, c, !ruled);
        if roomy && !e.is_blank_slate() {
            let sky = Rect::new(start, area.y + 1, (column.x + column.width).saturating_sub(start + 2), 1);
            let clock = frame.ambient_time() * 0.4;
            super::sparkles(frame, sky, clock, 0.15);
        }
    }
    let mut map = None;
    if stars {
        let y = area.y + area.height - 2;
        let row = Rect::new(area.x + 2, y, area.width.saturating_sub(4), 1);
        map = star_line(e, frame, row, lit, c);
    }
    let typing_at = if status {
        let fits = panes.height >= 6 && panes.width >= 30;
        let row = Rect::new(area.x, area.y + area.height - 1, area.width, 1);
        draw_status(e, frame, row, !fits, c, map)
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

// drops the clock first, then crlf, length, language
fn corner(e: &Editor, branch: bool, room: usize, timed: bool, c: &Colors) -> String {
    let mut facts = Vec::new();
    if !e.is_blank_slate() {
        if let Some(lang) = e.syntax() {
            let color = super::icon(&file_name(e.file().path.as_deref(), ""), false).1;
            facts.push(paint(color, lang.name()));
        }
        let lines = e.buffer().len();
        facts.push(paint(
            palette::MUTED,
            &format!("{} {}", tinsel::format::grouped(lines as u64), if lines == 1 { "line" } else { "lines" }),
        ));
        // only the unusual line ending is worth a mention
        if e.file().crlf {
            facts.push(paint(palette::MUTED, "crlf"));
        }
    }
    let branch = if branch {
        Some(branch_info(e).unwrap_or_else(|| {
            let cwd = std::env::current_dir().map(|d| display_path(&d)).unwrap_or_default();
            paint(palette::MUTED, &cwd)
        }))
    } else {
        None
    };
    let faint = mix(palette::MUTED, palette::LINE, 0.5);
    let clock = paint(mix(faint, c.app, 0.3), &tinsel::format::local_clock());
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
    let mut shown = build(&facts, timed && !e.is_blank_slate());
    while tinsel::text::width(&shown) > room {
        if !shown.ends_with(&clock) && facts.pop().is_none() {
            return String::new();
        }
        shown = build(&facts, false);
    }
    shown
}

fn draw_top(e: &Editor, frame: &mut Frame, row: Rect, start: usize, lit: Rgb, c: &Colors, branch: bool) {
    let names = if e.is_blank_slate() { Vec::new() } else { tab_names(e) };
    // the current file's star and name come before anything in the corner
    let own = names
        .iter()
        .find(|t| t.current)
        .map_or(12, |t| tinsel::text::width(&t.name) + 9);
    // frames from the running app always have a time; a still frame shows no clock
    let timed = frame.time() > 0.0;
    let right = corner(e, branch, (row.x + row.width).saturating_sub(start + own + 5), timed, c);
    if timed {
        // the clock turns over on the minute
        frame.redraw_in(crate::time::to_next_minute());
    }
    let right_width = tinsel::text::width(&right);
    if names.is_empty() {
        frame.print(Rect::new(start, row.y, 4, 1), 0, &bold(&paint(palette::MUTED, "tvim")));
    }
    let end = (row.x + row.width).saturating_sub(2);
    let fits = right_width > 0 && start + right_width + 12 < end;
    if fits {
        frame.print_right(Rect::new(row.x, row.y, end - row.x, 1), 0, &right);
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
            let dot = if tab.current {
                palette::AMBER
            } else {
                mix(palette::AMBER, palette::MUTED, 0.5)
            };
            line.push_str(&paint(dot, " ●"));
        }
    }
    if last + 1 < names.len() {
        line.push_str(&paint(palette::MUTED, " ›"));
    }
    frame.print(Rect::new(start, row.y, room, 1), 0, &line);
}

// the one mapping from a line to its dot on the map, so a change on the cursor's line is always
// under the star; the first and last lines are the end dots
fn dot(line: usize, total: usize, n: usize) -> usize {
    ((line * (n - 1) + (total - 1) / 2) / (total - 1).max(1)).min(n - 1)
}

// the whole row is sky; the file's map is the brighter stretch in its middle; returns the map's rect
fn star_line(e: &Editor, frame: &mut Frame, row: Rect, lit: Rgb, c: &Colors) -> Option<Rect> {
    if row.width == 0 {
        return None;
    }
    let total = e.buffer().len().max(1);
    let n = if row.width < 8 { 0 } else { total.min(row.width) };
    let map = Rect::new(row.x + (row.width - n) / 2, row.y, n, 1);
    for x in row.x..row.x + row.width {
        if x >= map.x && x < map.x + map.width {
            continue;
        }
        // dots with no lines behind them stay still
        frame.print(
            Rect::new(x, row.y, 1, 1),
            0,
            &paint(mix(palette::LINE, c.app, 0.82), "·"),
        );
    }
    (n > 0).then(|| {
        constellation(e, frame, map, lit, c);
        map
    })
}

// one dot per line when the file fits, so a short file looks short and j moves one dot;
// otherwise each dot is an even share of the file
fn constellation(e: &Editor, frame: &mut Frame, area: Rect, accent: Rgb, c: &Colors) {
    let total = e.buffer().len().max(1);
    let n = area.width;
    let bar = c.app;
    let marks = e.change_marks();
    let mut mark: Vec<Option<Mark>> = vec![None; n];
    for line in 0..e.buffer().len() {
        if let Some(m) = marks.get(line).copied().flatten() {
            mark[dot(line, total, n)].get_or_insert(m);
        }
    }
    let mut hit = vec![false; n];
    if total <= 20_000 {
        for p in e.search_hits().iter().flat_map(|h| h.iter()) {
            hit[dot(p.row, total, n)] = true;
        }
    }
    // matches everywhere would drown the map, so then they only tint the dots
    let sparse = hit.iter().filter(|h| **h).count() * 3 <= n;
    let now = frame.time();
    // thousandths of the way through the file: steady when the width changes, and fine enough
    // that a one-line move in a short file still glides
    let at = e.cursor().row * 1000 / (total - 1).max(1);
    let shown = super::glide(&e.anim.star, at, 1000, e.options.smooth && now > 0.0, frame);
    let here = ((shown * (n - 1) + 500) / 1000).min(n - 1);
    let sparkle = if e.mode().is_visual() {
        [mix(palette::AMBER, bar, 0.45), palette::AMBER, palette::SHINE]
    } else {
        [palette::BERRY, palette::ROSE, palette::SHINE]
    };
    frame.animate_at(super::TWINKLE_FPS);
    let t = frame.ambient_time();
    for i in 0..n {
        let tw = twinkle(i, n, t);
        let star = |rest: Rgb, ramp: [Rgb; 3]| {
            if tw >= 0.08 {
                (["⋆", "✧", "✦"][((tw * 3.0) as usize).min(2)], tinsel::style::sample(&ramp, tw))
            } else {
                ("·", mix(rest, bar, 0.3))
            }
        };
        let tinted = |c: Rgb| [mix(c, bar, 0.45), c, palette::SHINE];
        let flare = |color: Rgb| mix(mix(color, bar, 0.2), palette::SHINE, 0.55 * tw);
        let (ch, ink) = if i == here {
            ("✶", accent)
        } else if hit[i] && sparse {
            ("◇", flare(palette::BLUSH))
        } else if let Some(m) = mark[i] {
            match m {
                Mark::Added => star(palette::SAGE, tinted(palette::SAGE)),
                Mark::Changed => star(palette::AMBER, tinted(palette::AMBER)),
                Mark::Removed | Mark::RemovedAbove => ("×", flare(palette::RED)),
            }
        } else if hit[i] {
            ("·", palette::BLUSH)
        } else {
            star(mix(palette::LINE, palette::SOFT, 0.35), sparkle)
        };
        let ch = if i == here {
            bold(&paint(ink, ch))
        } else {
            paint(ink, ch)
        };
        frame.print(Rect::new(area.x + i, area.y, 1, 1), 0, &ch);
    }
}

fn tail(path: &str, room: usize) -> String {
    if room < 2 {
        return path.to_string();
    }
    tinsel::text::truncate_start(path, room)
}

fn path_parts(path: &str, c: &Colors) -> String {
    let (dir, name) = match path.rfind('/') {
        Some(i) => path.split_at(i + 1),
        None => ("", path),
    };
    format!("{}{}", paint(c.faint, dir), paint(palette::SOFT, name))
}

fn draw_status(
    e: &Editor,
    frame: &mut Frame,
    row: Rect,
    command_here: bool,
    c: &Colors,
    map: Option<Rect>,
) -> Option<(usize, usize)> {
    let mode = e.mode();
    let in_files = e.explorer_focused() && mode != Mode::Command;
    let mid_x = row.x + 2;

    let typing = mode == Mode::Search || mode == Mode::Command && command_here;
    let busy = in_files || typing || e.message().is_some();
    let mut parts = Vec::new();
    if let Some(r) = e.recording() {
        parts.push(paint(palette::RED, &format!("● rec @{r}")));
    }
    let pending: String = e.pending().iter().map(|k| show(*k)).collect();
    if !pending.is_empty() {
        parts.push(paint(palette::MUTED, &pending));
    }
    let place = if in_files {
        let x = e.explorer().expect("in files");
        let shown = x.visible().len();
        let at = (x.selected() + 1).min(shown);
        paint(palette::MUTED, &format!("{at} of {shown}"))
    } else {
        if !busy {
            if let Some((n, total)) = e.search_count() {
                parts.push(format!(
                    "{} {}",
                    paint(palette::AMBER, "/"),
                    paint(palette::MUTED, &format!("{n}/{total}"))
                ));
            }
            if let Some(sel) = e.selection() {
                let rows = sel.end.row - sel.start.row + 1;
                let size = if sel.linewise || rows > 1 {
                    plural(rows as u64, "line")
                } else {
                    plural((sel.end.col - sel.start.col) as u64, "character")
                };
                parts.push(paint(quiet(mode), &size));
            }
        }
        let p = e.cursor();
        let col = crate::text::vcol(e.buffer().line(p.row), p.col, e.options.tab_stop) + 1;
        if e.is_blank_slate() {
            String::new()
        } else {
            paint(quiet(mode), &format!("{}:{col}", p.row + 1))
        }
    };
    let end = (row.x + row.width).saturating_sub(2);
    let room = end.saturating_sub(mid_x);
    let mut right = String::new();
    for n in (0..=parts.len()).rev() {
        let mut all = parts[parts.len() - n..].to_vec();
        if !place.is_empty() {
            all.push(place.clone());
        }
        let text = all.join("    ");
        if tinsel::text::width(&text) + 6 <= room {
            right = text;
            break;
        }
    }
    if let Some(map) = map.filter(|_| !busy && !e.is_blank_slate()) {
        star_chart(e, frame, row, map, c, &right, !parts.is_empty());
        return None;
    }
    let right_width = tinsel::text::width(&right);
    if right_width > 0 {
        frame.print_right(Rect::new(row.x, row.y, end - row.x, 1), 0, &right);
    }
    let mid_end = end.saturating_sub(right_width + if right_width > 0 { 3 } else { 0 });
    if mid_end <= mid_x {
        return None;
    }
    let mid = Rect::new(mid_x, row.y, mid_end - mid_x, 1);

    if in_files {
        return explorer_status(e, frame, row, mid, c);
    }
    if typing {
        return Some(typing_line(e, frame, mid));
    }
    if let Some(m) = e.message().filter(|_| mode != Mode::Command) {
        frame.print(mid, 0, &message(m));
        return None;
    }
    if !e.is_blank_slate() {
        path_line(e, frame, mid, c);
    }
    None
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
    let full = path_parts(&path, c);
    let path = if tinsel::text::width(&full) <= mid.width {
        full
    } else {
        path_parts(&tail(&path, mid.width), c)
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

fn path_line(e: &Editor, frame: &mut Frame, mid: Rect, c: &Colors) {
    let path = e
        .file()
        .path
        .as_ref()
        .map_or_else(|| "untitled".to_string(), |p| display_path(p));
    // with several windows the focused one's title carries the scope
    let mut scope: Vec<String> = if e.window_count() == 1 {
        crumb_scope(e)
            .into_iter()
            .map(|(s, color)| {
                format!(
                    "{}{}",
                    paint(c.faint, " › "),
                    paint(mix(color, palette::MUTED, 0.35), &s)
                )
            })
            .collect()
    } else {
        Vec::new()
    };
    let width = |s: &[String]| s.iter().map(|p| tinsel::text::width(p)).sum::<usize>();
    // the folder gives way before the scope does, the scope before the file's name
    let name = tinsel::text::width(path.rsplit('/').next().unwrap_or(&path)) + 1;
    while !scope.is_empty() && name + width(&scope) > mid.width {
        scope.remove(0);
    }
    let mut line = path_parts(&tail(&path, mid.width - width(&scope)), c);
    line.push_str(&scope.concat());
    frame.print(mid, 0, &line);
}

struct Bracket {
    x: usize,
    width: usize,
    label: String,
    lit: bool,
}

// each top-level item spans the dots its lines map to; neighbours sharing a dot give way to the
// one the cursor is in, then to the one before them
fn brackets(e: &Editor, map: Rect) -> Vec<Bracket> {
    let total = e.buffer().len().max(1);
    let n = map.width;
    let items = outline(e);
    if items.is_empty() {
        return vec![Bracket {
            x: map.x,
            width: n,
            label: file_name(e.file().path.as_deref(), "untitled"),
            lit: true,
        }];
    }
    let row = e.cursor().row;
    let lit = items.iter().position(|s| (s.start..=s.end).contains(&row));
    let span = |i: usize| (dot(items[i].start, total, n), dot(items[i].end, total, n));
    let mut out = Vec::new();
    let mut free = 0;
    for i in 0..items.len() {
        let (mut a, mut b) = span(i);
        if lit != Some(i) {
            a = a.max(free);
            if let Some(edge) = lit.filter(|l| *l > i).and_then(|l| span(l).0.checked_sub(1)) {
                b = b.min(edge);
            }
            if a > b {
                continue;
            }
        }
        free = b + 1;
        out.push(Bracket {
            x: map.x + a,
            width: b - a + 1,
            label: items[i].label.clone(),
            lit: lit == Some(i),
        });
    }
    out
}

fn star_chart(e: &Editor, frame: &mut Frame, row: Rect, map: Rect, c: &Colors, right: &str, keep_right: bool) {
    let lit = accent(e);
    let end = (row.x + row.width).saturating_sub(2);
    let mut all = brackets(e, map);
    // a lone dot is too small to bracket unless the cursor is in it
    all.retain(|b| b.width > 1 || b.lit);
    // the item the cursor is in is always wide enough to name
    if let Some(i) = all.iter().position(|b| b.lit) {
        let want = tinsel::text::width(&all[i].label) + 4;
        if all[i].width < want {
            let right = (map.x + map.width).max(map.x + want);
            let middle = all[i].x + all[i].width / 2;
            let x = middle.saturating_sub(want / 2).clamp(map.x, right - want);
            let lit = Bracket {
                x,
                width: want,
                ..all.remove(i)
            };
            // anything underneath keeps whichever side of it is bigger
            let (lo, hi) = (x.saturating_sub(1), x + want + 1);
            all = all
                .into_iter()
                .filter_map(|mut b| {
                    let end = b.x + b.width;
                    if end <= lo || b.x >= hi {
                        return Some(b);
                    }
                    let (left, right) = (lo.saturating_sub(b.x), end.saturating_sub(hi));
                    if left >= right {
                        b.width = left;
                    } else {
                        (b.x, b.width) = (hi, right);
                    }
                    (b.width > 1).then_some(b)
                })
                .collect();
            all.push(lit);
            all.sort_by_key(|b| b.x);
        }
    }
    let w = tinsel::text::width(right);
    let limit = end.saturating_sub(w + 2);
    // line:col only shows where no label needs the room; recording, pending keys and counts do
    let clear = all.iter().all(|b| b.x + b.width <= limit);
    if w > 0 && (clear || keep_right) {
        // brackets running into the right end are cut short there rather than dropped
        all.retain(|b| b.x + 2 <= limit || b.lit && b.x < limit);
        for b in &mut all {
            b.width = b.width.min(limit.saturating_sub(b.x)).max(1);
        }
        frame.print_right(Rect::new(row.x, row.y, end - row.x, 1), 0, right);
    }
    let tone = tone(e.mode());
    for b in &all {
        let ink = match (b.lit, tone) {
            (true, Some(m)) => m,
            (true, None) => mix(lit, c.app, 0.45),
            (false, Some(m)) => mix(m, c.app, 0.55),
            (false, None) => c.faint,
        };
        let line = if b.width == 1 {
            "─".to_string()
        } else {
            format!("╰{}╯", "─".repeat(b.width - 2))
        };
        frame.print(Rect::new(b.x, row.y, b.width, 1), 0, &paint(ink, &line));
        let label_width = tinsel::text::width(&b.label) + 2;
        if label_width + 2 > b.width {
            continue;
        }
        let at = b.x + (b.width - label_width) / 2;
        let label = if b.lit {
            bold(&paint(lit, &b.label))
        } else {
            paint(quiet(e.mode()), &b.label)
        };
        frame.print(Rect::new(at, row.y, label_width, 1), 0, &format!(" {label} "));
    }
}

// insert, replace and visual colour the labels row; normal leaves it quiet
fn tone(mode: Mode) -> Option<Rgb> {
    matches!(mode, Mode::Insert | Mode::Replace | Mode::Visual | Mode::VisualLine).then(|| super::mode_color(mode))
}

fn quiet(mode: Mode) -> Rgb {
    tone(mode).map_or(palette::MUTED, |m| mix(m, palette::MUTED, 0.4))
}

fn message(m: &Message) -> String {
    if m.error {
        format!("{} {}", paint(palette::RED, "✕"), paint(palette::RED, &m.text))
    } else {
        format!("{} {}", paint(palette::BLUSH, "›"), m.text)
    }
}
