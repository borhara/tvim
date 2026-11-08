use tinsel::app::{Frame, Rect, hints};
use tinsel::{paint, palette};

use super::{Colors, display_path, input};
use crate::editor::{Editor, Mode};

fn explorer_status(e: &Editor, frame: &mut Frame, row: Rect, mid: Rect, c: &Colors) -> Option<(usize, usize)> {
    let x = e.explorer().expect("in files");
    if let Some(m) = e.message() {
        frame.print(mid, 0, &message(m));
        return None;
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
