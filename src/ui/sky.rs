use tinsel::app::{Frame, Rect};
use tinsel::{paint, palette};

use super::input;
use crate::editor::{Editor, Mode};

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
