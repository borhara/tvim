use std::io::Write;
use std::path::Path;

use tinsel::app::{self, App, Event, Flow, Frame, Rect, Sparkles};
use tinsel::{ColorMode, Rgb, color_mode, mix, paint, paint_bg, palette};

use crate::editor::{Editor, Mode};
use crate::text::vcol;

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

const LOGO: [&str; 2] = ["▀█▀ █ █ █ █▀▄▀█ ✶", " █  ▀▄▀ █ █ ▀ █  "];

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

// the terminal draws a bar at a cell's left edge and guides sit mid-cell, so on a guide the
// guide itself becomes the cursor
fn bar_on_guide(e: &Editor) -> Option<usize> {
    if e.mode() != Mode::Insert || !e.options.indent_guides || color_mode() == ColorMode::None {
        return None;
    }
    let p = e.cursor();
    let buf = e.buffer();
    let ts = e.options.tab_stop;
    let col = vcol(buf.line(p.row), p.col, ts);
}

fn text_area(e: &Editor, frame: &mut Frame, area: Rect, c: &Colors, focused: bool, scrollbar: bool) -> Rect {
    // set_screen reserves a scrollbar column and two status rows
    e.set_screen(area.width, area.height + 2);
    e.scroll();
    e.anim.follow(e, frame.time());
    let (gutter, body) = split_gutter(area, e.gutter());
    body
}

fn split_gutter(area: Rect, gutter: usize) -> (Rect, Rect) {
    let gutter = Rect::new(area.x, area.y, gutter.min(area.width), area.height);
    let body = Rect::new(
        gutter.x + gutter.width,
        area.y,
        area.width.saturating_sub(gutter.width + 1),
        area.height,
    );
    (gutter, body)
}

fn draw_welcome(e: &Editor, frame: &mut Frame, area: Rect, c: &Colors) {
    if color_mode() != ColorMode::None {
        sparkles(frame, area, e.anim.clock(), 0.012);
    }
    let menu = [
        ("i", "start writing"),
        ("tab", "browse files"),
        ("space f", "find a file"),
        ("space r", "recent files"),
        ("space g", "search in files"),
        (":e file", "open a file"),
        (":q", "leave"),
    ];
    let key_width = menu.iter().map(|(k, _)| k.len()).max().unwrap_or(0) + 2;
    let menu_width = key_width + 2 + menu.iter().map(|(_, d)| d.len()).max().unwrap_or(0);
    let tagline = "a vim for your terminal, with a little sparkle";
    let version = format!("v{}  ·  drawn with tinsel", env!("CARGO_PKG_VERSION"));
    let height = LOGO.len() + 2 + 2 + 2 + 1 + menu.len();
    if area.height < height || area.width < tagline.len() + 4 {
        return;
    }
    let mut y = area.y + (area.height - height) / 2;
    let block_width = menu_width.max(tagline.len()) + 8;
    let x = area.x + area.width.saturating_sub(block_width) / 2;
    let clear = Rect::new(x, y.saturating_sub(1), block_width, height + 2).intersect(area);
    frame.clear(clear);
    let line = |frame: &mut Frame, y: usize, width: usize, text: &str| {
        let x = area.x + area.width.saturating_sub(width) / 2;
        frame.print(Rect::new(x, y, width, 1), 0, text);
    };
    for (i, l) in LOGO.iter().enumerate() {
        line(
            frame,
            y,
            tinsel::text::width(l),
            &tinsel::gradient(l, &tinsel::RAMP, i as f32 * 0.15),
        );
        y += 1;
    }
    y += 1;
    line(frame, y, tagline.len(), &paint(palette::SOFT, tagline));
    y += 3;
    for (key, what) in menu {
        let keys = paint_bg(
            palette::ROSE,
            mix(c.app, palette::ROSE, 0.16),
            &format!(" {key:<w$}", w = key_width - 1),
        );
        let text = format!("{keys}  {}", paint(palette::SOFT, what));
        line(frame, y, menu_width, &text);
        y += 1;
    }
    y += 2;
    line(
        frame,
        y,
        tinsel::text::width(&version),
        &paint(palette::MUTED, &version),
    );
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

// on tvim's clock so it rests while unfocused
fn sparkles(frame: &mut Frame, area: Rect, clock: f32, density: f32) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let mut sky = Frame::new(area.width, area.height).at(clock);
    sky.render(sky.area(), Sparkles::new().density(density));
    for (i, line) in sky.snapshot().lines().iter().enumerate() {
        frame.print(Rect::new(area.x, area.y + i, area.width, 1), 0, line);
    }
}
