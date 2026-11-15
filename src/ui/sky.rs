use tinsel::app::{Frame, Rect, hints};
use tinsel::{paint, palette};

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
        // the first star sits over the line numbers' last digit
        let start = match e.window_count() {
            _ => row.x + 2,
        };
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
