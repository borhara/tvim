use tinsel::app::{Frame, Rect};

use super::{Colors, Drawn, preview_window, text_area};
use crate::editor::Editor;

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
