use tinsel::app::{Frame, Rect, hints};
use tinsel::{paint, palette};

use super::{Colors, LILAC, diff_totals, display_path, input};
use crate::editor::{Editor, Mode};

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
