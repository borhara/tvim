use tinsel::app::{Frame, Rect};
use tinsel::{paint, palette};

use super::{Colors, Drawn, input, preview_window, text_area};
use crate::editor::{Editor, Mode};
use crate::keys::show;

const WIDTH: usize = 90;

pub(super) fn draw(e: &Editor, frame: &mut Frame, c: &Colors) -> Drawn {
    let area = frame.area();
    frame.tint(area, c.app);
    let foot = usize::from(area.height >= 3);
    let head = usize::from(area.height >= 12);
    let width = (WIDTH + 1).min(area.width);
    let x = area.x + (area.width - width) / 2;
    let body = if e.preview_focused() {
        preview_window(e, e.focus, frame, text, c, true)
    } else {
        text_area(e, frame, text, c, true, false)
    };
    let panes = Rect::new(area.x, area.y, area.width, area.height - foot);
    let fits = panes.height >= 6 && panes.width >= 30;
    let typing_at = if foot == 1 {
        bottom_line(e, frame, Rect::new(x, area.y + area.height - 1, width - 1, 1), !fits)
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

fn bottom_line(e: &Editor, frame: &mut Frame, row: Rect, command_here: bool) -> Option<(usize, usize)> {
    let mode = e.mode();
    if mode == Mode::Search || mode == Mode::Command && command_here {
        let cmd = e.cmdline();
        let label = format!("{} ", paint(palette::ROSE, &cmd.prompt.to_string()));
        return Some(input(frame, row, &label, &cmd.text, cmd.cursor, None));
    }
    if let Some(m) = e.message().filter(|_| mode != Mode::Command) {
        let color = if m.error { palette::RED } else { palette::MUTED };
        frame.print(row, 0, &paint(color, &m.text));
    }
    let mut right: String = e.pending().iter().map(|k| show(*k)).collect();
    if let Some(r) = e.recording() {
        right = format!("● @{r} {right}");
    }
    let w = tinsel::text::width(&right);
    if w > 0 && w + 2 < row.width {
        let shown = paint(palette::MUTED, right.trim_end());
        frame.print(Rect::new(row.x + row.width - w, row.y, w, 1), 0, &shown);
    }
    None
}
