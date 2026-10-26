use std::io::Write;
use std::path::Path;

use tinsel::app::{self, App, Event, Flow, Frame, Rect};
use tinsel::{Rgb, mix, palette};

use crate::editor::{Editor, Mode};

const HINT_DELAY: f32 = 0.4;

pub struct Tvim {
    pub editor: Editor,
    pending_since: Option<f32>,
}

impl Tvim {
    pub fn new(editor: Editor) -> Tvim {
        Tvim {
            editor,
            pending_since: None,
        }
    }

    fn settle(&mut self) {
        self.pending_since = match (self.editor.pending().is_empty(), self.pending_since) {
            (true, _) => None,
            (false, None) => Some(app::time()),
            (false, since) => since,
        };
        let written = self
            .editor
            .message()
            .filter(|m| !m.error && m.text.ends_with(" written"));
        if let Some(m) = written {
            self.editor.clear_message();
        }
    }
}

impl App for Tvim {
    fn view(&self, frame: &mut Frame) {
        let hints = self.pending_since.is_some_and(|t| frame.time() - t >= HINT_DELAY);
        draw_with(&self.editor, frame, hints);
        let area = frame.area();
        if area.height > 4 {
            let room = Rect::new(area.x, area.y, area.width.saturating_sub(1), area.height - 2);
        }
    }

    fn event(&mut self, event: &Event) -> Flow {
        match event {
            Event::Key(k) => self.editor.key(*k),
            Event::Wake | Event::Tick => {
                self.editor.tick();
                return Flow::Continue;
            }
            _ => return Flow::Continue,
        }
        self.settle();
        if self.editor.should_quit() {
            Flow::Quit
        } else {
            Flow::Continue
        }
    }

    fn title(&self) -> Option<String> {
        let name = file_name(self.editor.file().path.as_deref(), "[No Name]");
        let modified = if self.editor.is_modified() { " ●" } else { "" };
        Some(format!("{name}{modified} · tvim"))
    }

    fn ctrl_c_quits(&self) -> bool {
        false
    }

    fn mouse(&self) -> bool {
        true
    }
}

fn file_name(path: Option<&Path>, or: &str) -> String {
    path.and_then(Path::file_name)
        .map_or_else(|| or.to_string(), |n| n.to_string_lossy().into_owned())
}

// dark colours; tinsel flips them on light terminals
struct Colors {
    app: Rgb,
    faint: Rgb,
    bar: Rgb,
    ink: Rgb,
    selection: Rgb,
    search: Rgb,
    current_match: Rgb,
    bracket: Rgb,
    guide: Rgb,
    scrollbar: Rgb,
    word: Rgb,
    trailing: Rgb,
    picked: Rgb,
}

fn colors() -> Colors {
    let app = Rgb(23, 18, 27);
    let bar = Rgb(44, 33, 51);
    Colors {
        app,
        faint: Rgb(77, 67, 79),
        bar,
        ink: Rgb(23, 18, 27),
        selection: mix(app, ORANGE, 0.34),
        search: mix(app, palette::AMBER, 0.28),
        current_match: mix(app, palette::AMBER, 0.6),
        bracket: mix(app, palette::ROSE, 0.5),
        guide: mix(app, palette::LINE, 0.4),
        scrollbar: mix(app, palette::ROSE, 0.4),
        word: mix(app, palette::LINE, 0.5),
        trailing: mix(app, palette::RED, 0.3),
        picked: mix(bar, palette::ROSE, 0.3),
    }
}

const ORANGE: Rgb = Rgb(240, 140, 70);

struct Drawn {
    body: Rect,
    text: Rect,
    panes: Rect,
    typing_at: Option<(usize, usize)>,
}

pub fn draw(e: &Editor, frame: &mut Frame) {
    draw_with(e, frame, true);
}

fn draw_with(e: &Editor, frame: &mut Frame, hints: bool) {
    let area = frame.area();
    if area.width == 0 || area.height == 0 {
        return;
    }
    let c = colors();
    let since_zen = e.anim.zen(e.zen(), frame.time());
    let d = if e.zen() {
        zen::draw(e, frame, &c)
    } else {
        sky::draw(e, frame, &c)
    };
    let mut typing_at = d.typing_at;
    if e.mode() == Mode::Command && d.panes.height >= 6 && d.panes.width >= 30 {
    }
    if let Some(f) = e.finder() {
        typing_at = overlays::finder(f, frame, d.panes, &c);
    }
        || e.finder().is_some()
        || matches!(e.mode(), Mode::Command | Mode::Search)
    if let Some((x, y, shape)) = cursor {
        frame.set_cursor(x, y, shape);
    }
}

fn display_path(path: &Path) -> String {
    let cwd = std::env::current_dir().ok();
    if let Some(rel) = cwd.as_deref().and_then(|d| path.strip_prefix(d).ok()) {
        return rel.display().to_string();
    }
    if let Ok(home) = std::env::var("HOME") {
        if let Ok(rel) = path.strip_prefix(&home) {
            return format!("~/{}", rel.display());
        }
    }
    path.display().to_string()
}
