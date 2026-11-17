mod gitmenu;
mod overlays;
mod sky;
mod zen;

use std::cell::{Cell, RefCell};
use std::io::Write;
use std::path::Path;

use tinsel::anim::{Glide, Glider};
use tinsel::app::{self, App, CursorShape, Event, Flow, Frame, Rect, Sparkles, Toasts};
use tinsel::{ColorMode, Rgb, Tone, bold, color_mode, italic, mix, paint, paint_on, palette, underline};

use crate::buffer::Buffer;
use crate::buffer::Pos;
use crate::diff::Mark;
use crate::editor::{Editor, Effect, Mode, Range, View};
use crate::explorer::Explorer;
use crate::git::FileState;
use crate::syntax::{Kind, Syntax};
use crate::text::{caret, cell_width, char_count, is_word, vcol};

const HINT_DELAY: f64 = 0.4;
// how much the sidebar fades while the text has focus
const SIDE_DIM: f32 = 0.35;

pub struct Tvim {
    pub editor: Editor,
    toasts: Toasts,
    pending_since: Option<f64>,
}

impl Tvim {
    pub fn new(editor: Editor) -> Tvim {
        Tvim {
            editor,
            toasts: Toasts::new().life(3.0),
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
            let text = saved(&m.text);
            self.toasts.push(app::time(), Tone::Success, text);
            self.editor.clear_message();
        }
    }
}

impl App for Tvim {
    fn view(&self, frame: &mut Frame) {
        let hints = self.pending_since.is_some_and(|t| frame.time() - t >= HINT_DELAY);
        if let Some(t) = self.pending_since.filter(|_| !hints) {
            frame.redraw_at(t + HINT_DELAY);
        }
        draw_with(&self.editor, frame, hints);
        let area = frame.area();
        if area.height > 4 {
            let room = Rect::new(area.x, area.y, area.width.saturating_sub(1), area.height - 2);
            frame.render(room, &self.toasts);
        }
    }

    fn event(&mut self, event: &Event) -> Flow {
        match event {
            Event::Key(k) => {
                let (esc, k) = crate::keys::as_vim(*k);
                if let Some(esc) = esc {
                    self.editor.key(esc);
                }
                self.editor.key(k);
            }
            Event::Paste(text) => self.editor.paste(text),
            Event::Wake | Event::Tick => {
                self.editor.tick();
                return Flow::Continue;
            }
            _ => return Flow::Continue,
        }
        for effect in self.editor.take_effects() {
            run_effect(&mut self.editor, effect);
        }
        self.settle();
        if self.editor.should_quit() {
            Flow::Quit
        } else {
            Flow::Continue
        }
    }

    fn title(&self) -> Option<String> {
        if let Some(x) = self.editor.explorer().filter(|_| self.editor.explorer_focused()) {
            return Some(format!("{} · tvim", display_path(x.root())));
        }
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

fn saved(message: &str) -> String {
    let (path, rest) = match (message.find('"'), message.rfind('"')) {
        (Some(a), Some(b)) if b > a => (&message[a + 1..b], &message[b + 1..]),
        _ => ("", message),
    };
    let name = Path::new(path).file_name().map_or(path.into(), |n| n.to_string_lossy());
    let mut parts = vec![format!("saved {name}")];
    let rest = rest.trim().trim_end_matches("written").trim();
    for word in rest.split([' ', ',']).filter(|w| !w.is_empty()) {
        parts.push(match word {
            "[New]" => "new".to_string(),
            w if w.ends_with('L') => plural(&w[..w.len() - 1], "line"),
            w if w.ends_with('B') => plural(&w[..w.len() - 1], "byte"),
            w => w.to_string(),
        });
    }
    parts.join(" · ")
}

fn file_name(path: Option<&Path>, or: &str) -> String {
    path.and_then(Path::file_name)
        .map_or_else(|| or.to_string(), |n| n.to_string_lossy().into_owned())
}

// keeps the end of a path, which is where the file's name is
fn cut_left(s: &str, room: usize) -> String {
    if tinsel::text::width(s) <= room {
        return s.to_string();
    }
    let mut used = 1;
    let start = s
        .char_indices()
        .rev()
        .take_while(|(_, ch)| {
            used += tinsel::text::char_width(*ch);
            used <= room
        })
        .last()
        .map_or(s.len(), |(i, _)| i);
    format!("…{}", &s[start..])
}

fn plural(n: &str, what: &str) -> String {
    if n == "1" {
        format!("1 {what}")
    } else {
        format!("{n} {what}s")
    }
}

fn run_effect(editor: &mut Editor, effect: Effect) {
    match effect {
        Effect::Shell(cmd) => {
            let shell = std::env::var("SHELL").unwrap_or_else(|_| "sh".into());
            let result = app::suspend(|| {
                let mut out = std::io::stdout();
                let _ = writeln!(out);
                let status = std::process::Command::new(&shell).arg("-c").arg(&cmd).status();
                let _ = write!(out, "\n{}", paint(palette::MUTED, "press enter to go back to tvim"));
                let _ = out.flush();
                let _ = std::io::stdin().read_line(&mut String::new());
                status
            });
            match result {
                Ok(Ok(status)) if status.success() => {}
                Ok(Ok(status)) => editor.error(format!("shell returned {}", status.code().unwrap_or(-1))),
                Ok(Err(e)) => editor.error(format!("E282: Can't run {shell}: {e}")),
                Err(e) => editor.error(format!("Couldn't take the terminal back: {e}")),
            }
        }
        Effect::Suspend => suspend(editor),
    }
}

#[cfg(unix)]
fn suspend(editor: &mut Editor) {
    let pid = std::process::id().to_string();
    let stopped = app::suspend(|| std::process::Command::new("kill").args(["-TSTP", &pid]).status());
    if !matches!(stopped, Ok(Ok(s)) if s.success()) {
        editor.error("Couldn't suspend");
    }
}

#[cfg(not(unix))]
fn suspend(editor: &mut Editor) {
    editor.error("Suspending isn't supported here");
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

const LILAC: Rgb = Rgb(196, 166, 238);
const BLUE: Rgb = Rgb(141, 182, 242);
const PEACH: Rgb = Rgb(246, 184, 150);
const ORANGE: Rgb = Rgb(240, 140, 70);

fn paint_kind(kind: Kind, s: &str, c: &Colors) -> String {
    use Kind::*;
    let quiet = mix(palette::SOFT, palette::MUTED, 0.45);
    match kind {
        Plain => s.to_string(),
        Keyword => paint(palette::ROSE, s),
        Function => paint(LILAC, s),
        Type => paint(PEACH, s),
        Str => paint(palette::SAGE, s),
        Escape => paint(palette::PETAL, s),
        Number | Constant => paint(palette::AMBER, s),
        Comment => italic(&paint(palette::MUTED, s)),
        Todo => bold(&paint_on(c.ink, palette::AMBER, s)),
        Macro => paint(palette::BLUSH, s),
        Attribute => paint(mix(LILAC, palette::MUTED, 0.45), s),
        Punct => paint(quiet, s),
        Operator => paint(palette::SOFT, s),
        Key => paint(palette::BLUSH, s),
        Label => italic(&paint(palette::PETAL, s)),
        Heading => bold(&paint(palette::ROSE, s)),
        Strong => bold(s),
        Emphasis => italic(s),
        Code => paint(palette::SAGE, s),
        Link => underline(&paint(palette::BLUSH, s)),
        Url => underline(&paint(palette::MUTED, s)),
        ListMark => paint(palette::AMBER, s),
        Quote => italic(&paint(palette::SOFT, s)),
    }
}

fn styled_line(line: &str, kinds: Option<&[Kind]>, ts: usize, c: &Colors) -> String {
    let mut out = String::new();
    let mut run = String::new();
    let mut run_kind = Kind::Plain;
    let mut v = 0;
    for (i, ch) in line.chars().enumerate() {
        let w = cell_width(ch, v, ts);
        let kind = kinds.and_then(|k| k.get(i).copied()).unwrap_or(Kind::Plain);
        if kind != run_kind && !run.is_empty() {
            out.push_str(&paint_kind(run_kind, &run, c));
            run.clear();
        }
        run_kind = kind;
        match ch {
            '\t' => run.extend(std::iter::repeat_n(' ', w)),
            ch if ch.is_control() => {
                run.push('^');
                run.push(caret(ch));
            }
            ch => run.push(ch),
        }
        v += w;
    }
    if !run.is_empty() {
        out.push_str(&paint_kind(run_kind, &run, c));
    }
    out
}

fn mode_color(mode: Mode) -> Rgb {
    match mode {
        Mode::Normal => palette::ROSE,
        Mode::Command | Mode::Search => palette::BLUSH,
        Mode::Insert => palette::SAGE,
        Mode::Replace => palette::RED,
        Mode::Visual | Mode::VisualLine => palette::AMBER,
    }
}

fn accent(e: &Editor) -> Rgb {
    let mode = e.mode();
    let base = if e.explorer_focused() && mode != Mode::Command {
        palette::ROSE
    } else {
        mode_color(mode)
    };
    mix(base, palette::BLUSH, 0.3)
}

// a few dots at a time fade in and out, each lasting 3-6s
fn twinkle(i: usize, n: usize, t: f64) -> f32 {
    if t <= 0.0 || n == 0 {
        return 0.0;
    }
    let noise = |a: u32, b: u32| {
        (a.wrapping_mul(2_654_435_761) ^ b.wrapping_mul(40_503)).wrapping_mul(2_246_822_519) as f32 / u32::MAX as f32
    };
    let stars = (n / 14).max(1) as u32;
    let mut lit: f32 = 0.0;
    for s in 0..stars {
        let life = 3.0 + 3.0 * noise(s, 1) as f64;
        let age = t / life + noise(s, 2) as f64;
        let (round, p) = (age.floor() as u64 as u32, age.fract() as f32);
        let place = (noise(s.wrapping_mul(7919) ^ round.wrapping_mul(104_729), 3) * n as f32) as usize;
        if place.min(n - 1) == i {
            lit = lit.max((p * std::f32::consts::PI).sin().powi(3));
        }
    }
    lit
}

fn icon(name: &str, dir: bool) -> (&'static str, Rgb) {
    if dir {
        return ("▸", BLUE);
    }
    let lower = name.to_ascii_lowercase();
    match crate::syntax::extension(name).as_str() {
        // ⊛ for Rust's cog and ≡ for settings: both are in JetBrains Mono, unlike ⚙
        "rs" => ("⊛", palette::BERRY),
        "py" | "pyi" => ("●", palette::SAGE),
        "js" | "mjs" | "jsx" | "ts" | "tsx" => ("●", palette::AMBER),
        "go" | "lua" => ("●", LILAC),
        "c" | "h" | "cc" | "cpp" | "hpp" | "java" | "swift" | "kt" => ("●", PEACH),
        "md" | "markdown" | "txt" | "rst" => ("◆", LILAC),
        "toml" | "yaml" | "yml" | "json" | "ini" | "cfg" | "conf" => ("≡", palette::AMBER),
        "lock" => ("◇", palette::MUTED),
        "sh" | "bash" | "zsh" | "fish" => ("$", palette::SAGE),
        "png" | "jpg" | "jpeg" | "gif" | "svg" | "webp" => ("▣", palette::BLUSH),
        _ if lower.starts_with("license") || lower.starts_with("readme") => ("○", palette::SOFT),
        _ if lower.starts_with('.') => ("·", palette::MUTED),
        _ => ("·", palette::SOFT),
    }
}

const LOGO: [&str; 2] = ["▀█▀ █ █ █ █▀▄▀█ ✶", " █  ▀▄▀ █ █ ▀ █  "];

// on opening, the logo spells tinselvim; then insel squeezes into the gap and fades while t and
// vim glide together into LOGO, in one smooth move. each glyph is its two rows and whether it stays
const INTRO: [(&str, &str, bool); 10] = [
    ("▀█▀", " █ ", true),
    ("█", "█", false),
    ("█▄ █", "█ ▀█", false),
    ("█▀", "▄█", false),
    ("█▀▀", "██▄", false),
    ("█  ", "█▄▄", false),
    ("█ █", "▀▄▀", true),
    ("█", "█", true),
    ("█▀▄▀█", "█ ▀ █", true),
    ("✶", " ", true),
];
const INTRO_HOLD: f64 = 1.0;
const INTRO_CLOSE: f64 = 1.0;
const INTRO_END: f64 = INTRO_HOLD + INTRO_CLOSE;

// the logo's two rows mid-intro, each cell with how solid it is
type IntroRows = [Vec<(char, f32)>; 2];

// the intro's rows and the column they start at; None once it has settled (and at time 0, so
// tests and snapshots see the plain logo)
fn intro(t: f64, area: Rect) -> Option<(usize, IntroRows)> {
    if t <= 0.0 || t >= INTRO_END {
        return None;
    }
    let width = |s: &str| s.chars().count();
    let p = ((t - INTRO_HOLD) / INTRO_CLOSE).clamp(0.0, 1.0) as f32;
    let p = p * p * (3.0 - 2.0 * p);
    // insel is gone from sight a little before its room is
    let fade = (p / 0.8).min(1.0);
    let solid = 1.0 - fade * fade * (3.0 - 2.0 * fade);

    let room = |keep: bool| INTRO.iter().filter(|g| g.2 == keep).map(|g| width(g.0) + 1).sum::<usize>();
    let fin = room(true) - 1;
    let squeezed = room(false) as f32 * (1.0 - p);
    // centred on where the logo will sit, so t only moves right and vim only left
    let centre = (area.width.saturating_sub(fin) / 2) as f32 + fin as f32 / 2.0;
    let x0 = centre - (fin as f32 + squeezed) / 2.0;

    let mut cells = Vec::new();
    let (mut kept, mut dropped) = (0.0, 0.0);
    for &(top, bottom, keep) in &INTRO {
        for (row, glyph) in [top, bottom].into_iter().enumerate() {
            for (i, ch) in glyph.chars().enumerate() {
                let (x, a) = if keep {
                    (x0 + kept + dropped * (1.0 - p) + i as f32, 1.0)
                } else {
                    (x0 + kept + (dropped + i as f32) * (1.0 - p), solid)
                };
                if ch != ' ' && a > 0.0 {
                    cells.push((x.round().max(0.0) as usize, row, ch, a, keep));
                }
            }
        }
        let w = (width(top) + 1) as f32;
        if keep {
            kept += w;
        } else {
            dropped += w;
        }
    }
    let lo = cells.iter().map(|c| c.0).min()?;
    let hi = cells.iter().map(|c| c.0).max()?;
    let mut rows = [vec![(' ', 1.0); hi - lo + 1], vec![(' ', 1.0); hi - lo + 1]];
    // squeezed cells share columns; the letters that stay always win
    for (x, row, ch, a, keep) in cells {
        let cell = &mut rows[row][x - lo];
        if keep || cell.0 == ' ' {
            *cell = (ch, a);
        }
    }
    Some((area.x + lo, rows))
}

// a row of the intro painted with the logo's gradient, cells fading into the background
fn paint_intro(row: &[(char, f32)], shift: f32, bg: Rgb) -> String {
    let n = row.len().saturating_sub(1).max(1) as f32;
    row.iter()
        .enumerate()
        .map(|(i, &(ch, a))| {
            let g = (i as f32 / n + shift).rem_euclid(2.0);
            let color = tinsel::style::sample(&tinsel::style::RAMP, if g > 1.0 { 2.0 - g } else { g });
            match color_mode() {
                ColorMode::None if a < 0.5 => " ".to_string(),
                _ => paint(mix(bg, color, a), ch.encode_utf8(&mut [0; 4])),
            }
        })
        .collect()
}

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
    // the window's margin too, which tinsel sends again once the intro settles
    frame.set_background(c.app);
    let since_zen = e.anim.zen(e.zen(), frame);
    let d = if e.zen() {
        zen::draw(e, frame, &c)
    } else {
        sky::draw(e, frame, &c)
    };
    if !e.zen() && since_zen < FADE {
        let (t, end) = (d.text, area.x + area.width);
        let below = t.y + t.height;
        for r in [
            Rect::new(area.x, area.y, area.width, t.y - area.y),
            Rect::new(area.x, below, area.width, (area.y + area.height).saturating_sub(below)),
            Rect::new(area.x, t.y, t.x - area.x, t.height),
            Rect::new(t.x + t.width, t.y, end.saturating_sub(t.x + t.width), t.height),
        ] {
            frame.fade(r);
        }
    }
    let mut typing_at = d.typing_at;
    if e.mode() == Mode::Command && d.panes.height >= 6 && d.panes.width >= 30 {
        typing_at = overlays::palette(e, frame, d.panes, &c);
    }
    if hints && !e.explorer_focused() {
        overlays::hint_strip(e, frame, d.text, &c);
    }
    if let Some((x, y, _)) = text_cursor(e, e.mode(), d.body).filter(|_| e.completion().is_some()) {
        overlays::completion(e, frame, d.panes, (x, y), &c);
    }
    if let Some(f) = e.finder() {
        typing_at = overlays::finder(f, frame, d.panes, &c);
    }
    if let Some(g) = e.grep_picker() {
        typing_at = overlays::grep(g, frame, d.panes, &c);
    }
    if let Some(m) = e.git_menu() {
        typing_at = gitmenu::draw(m, e, frame, &c);
    }

    let typing = e.git_menu().is_some()
        || e.finder().is_some()
        || e.grep_picker().is_some()
        || matches!(e.mode(), Mode::Command | Mode::Search)
        || e.explorer_focused();
    let cursor = if typing {
        typing_at.map(|(x, y)| (x, y, CursorShape::Bar))
    } else if e.is_blank_slate() || d.body.width == 0 || e.preview_focused() {
        None
    } else {
        text_cursor(e, e.mode(), d.body).filter(|_| bar_on_guide(e).is_none())
    };
    if let Some((x, y, shape)) = cursor {
        frame.set_cursor(x, y, shape);
        frame.set_cursor_color(mode_color(e.mode()));
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
    (col % e.options.shift_width.max(1) == 0 && col < resolved_indent(buf, ts, p.row)).then_some(col)
}

fn text_cursor(e: &Editor, mode: Mode, body: Rect) -> Option<(usize, usize, CursorShape)> {
    let p = e.cursor();
    let line = e.buffer().line(p.row);
    let ts = e.options.tab_stop;
    let start = vcol(line, p.col, ts);
    // vim draws the block cursor at the end of a tab
    let col = match (mode, line.chars().nth(p.col)) {
        (Mode::Insert, _) | (_, None) => start,
        (_, Some(ch)) => (start + cell_width(ch, start, ts)).saturating_sub(1),
    };
    let shape = match mode {
        Mode::Insert => CursorShape::Bar,
        Mode::Replace => CursorShape::Underline,
        _ => CursorShape::Block,
    };
    let row = p.row.checked_sub(e.anim.shown()).filter(|r| *r < body.height)?;
    let x = col.checked_sub(e.left()).filter(|x| *x < body.width)?;
    Some((body.x + x, body.y + row, shape))
}

fn text_area(e: &Editor, frame: &mut Frame, area: Rect, c: &Colors, focused: bool, scrollbar: bool) -> Rect {
    // set_screen reserves a scrollbar column and two status rows
    e.set_screen(area.width, area.height + 2);
    e.scroll();
    e.anim.follow(e, frame);
    let (gutter, body) = split_gutter(area, e.gutter());
    e.anim.text_at(body.x);
    if e.is_blank_slate() && e.mode() == Mode::Normal {
        draw_welcome(frame, area, c);
    } else {
        let v = View {
            id: e.focus,
            rect: area,
            focused: true,
            cursor: e.cursor(),
            top: e.anim.shown(),
            left: e.left(),
            text: e.shown_text(),
        };
        draw_text(e, &v, frame, gutter, body, c, focused);
        if scrollbar && area.width > 0 {
            draw_scrollbar(
                e,
                &v,
                frame,
                Rect::new(area.x + area.width - 1, area.y, 1, area.height),
                c,
                focused,
            );
        }
    }
    body
}

fn windows(e: &Editor, frame: &mut Frame, area: Rect, c: &Colors, focused: bool, scrollbar: bool) -> Rect {
    e.set_region(area.width, area.height);
    if e.window_count() == 1 {
        return text_area(e, frame, area, c, focused, scrollbar);
    }
    let accent = if focused { mode_color(e.mode()) } else { palette::LINE };
    let mut body = Rect::default();
    for v in e.views(area) {
        let r = v.rect;
        if r.width == 0 || r.height == 0 {
            continue;
        }
        let text = Rect::new(r.x, r.y + 1, r.width, r.height - 1);
        let preview = e.is_preview(v.id);
        if preview {
            let b = preview_window(e, v.id, frame, text, c, v.focused && focused);
            if v.focused {
                body = b;
            }
        } else if v.focused {
            body = text_area(e, frame, text, c, focused, true);
        } else {
            other_window(e, &v, frame, text, c);
        }
        let lit = v.focused && focused;
        let scope = if lit && !preview { crumb_scope(e) } else { Vec::new() };
        window_title(&v, lit, frame, Rect::new(r.x, r.y, r.width, 1), accent, preview, &scope);
        if r.x + r.width < area.x + area.width {
            vline(frame, r.x + r.width, r.y, r.height, &paint(palette::LINE, "│"));
        }
    }
    body
}

// scrolled to keep the cursor in view; returns the cursor's cell
fn input(frame: &mut Frame, row: Rect, label: &str, text: &str, cursor: usize, ink: Option<Rgb>) -> (usize, usize) {
    let before: String = text.chars().take(cursor).collect();
    let at = tinsel::text::width(label) + tinsel::text::width(&before);
    let skip = (at + 1).saturating_sub(row.width);
    let text = ink.map_or_else(|| text.to_string(), |ink| paint(ink, text));
    frame.print_from(row, 0, &format!("{label}{text}"), skip);
    (row.x + at - skip, row.y)
}

fn vline(frame: &mut Frame, x: usize, y: usize, height: usize, glyph: &str) {
    for y in y..y + height {
        frame.print(Rect::new(x, y, 1, 1), 0, glyph);
    }
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

fn other_window(e: &Editor, v: &View, frame: &mut Frame, area: Rect, c: &Colors) {
    let (gutter, body) = split_gutter(area, e.gutter_for(v.text.buf.len()));
    draw_text(e, v, frame, gutter, body, c, false);
    if area.width > 0 {
        let bar = Rect::new(area.x + area.width - 1, area.y, 1, area.height);
        draw_scrollbar(e, v, frame, bar, c, false);
    }
}

fn preview_window(e: &Editor, id: usize, frame: &mut Frame, area: Rect, c: &Colors, focused: bool) -> Rect {
    let body = Rect::new(area.x, area.y, area.width.saturating_sub(1), area.height);
    if body.width == 0 || body.height == 0 {
        return body;
    }
    let Some((lines, top)) = e.preview_lines(id, body.width, body.height) else {
        return body;
    };
    let t = frame.time();
    let shown = match e.previews.get(&id) {
        Some(p) => glide(&p.glide, top, body.height, e.options.smooth && t > 0.0, frame),
        None => top,
    };
    if lines.is_empty() {
        frame.print(body, 0, &paint(palette::MUTED, "  nothing to show yet"));
    }
    for (i, line) in lines.iter().skip(shown).take(body.height).enumerate() {
        frame.print(body, i, line);
    }
    draw_thumb(
        frame,
        Rect::new(body.x + body.width, body.y, 1, body.height),
        shown,
        lines.len(),
        thumb_color(e, focused, c),
    );
    body
}

fn thumb_color(e: &Editor, focused: bool, c: &Colors) -> Rgb {
    if focused {
        mix(c.app, mode_color(e.mode()), 0.6)
    } else {
        c.scrollbar
    }
}

fn window_title(
    v: &View,
    lit: bool,
    frame: &mut Frame,
    row: Rect,
    accent: Rgb,
    preview: bool,
    scope: &[(String, Rgb)],
) {
    let name = file_name(v.text.path, "untitled");
    let name = if preview { format!("preview · {name}") } else { name };
    let line = if lit { accent } else { palette::LINE };
    let mut label = if lit {
        format!(" {} {} ", paint(accent, "✶"), bold(&paint(palette::TEXT, &name)))
    } else {
        format!(" {} ", paint(palette::MUTED, &name))
    };
    if v.text.modified && !preview {
        label.push_str(&paint(palette::AMBER, "● "));
    }
    let sep = paint(palette::LINE, "› ");
    let room = row.width.saturating_sub(6 + tinsel::text::width(&label));
    let mut parts: Vec<String> = scope
        .iter()
        .map(|(t, color)| format!("{sep}{} ", paint(*color, t)))
        .collect();
    while !parts.is_empty() && parts.iter().map(|p| tinsel::text::width(p)).sum::<usize>() > room {
        parts.remove(0);
    }
    label.push_str(&parts.concat());
    let rest = row.width.saturating_sub(2 + tinsel::text::width(&label));
    frame.print(
        row,
        0,
        &format!("{}{label}{}", paint(line, "──"), paint(line, &"─".repeat(rest))),
    );
}

fn draw_scrollbar(e: &Editor, v: &View, frame: &mut Frame, bar: Rect, c: &Colors, focused: bool) {
    let lines = v.text.buf.len();
    let rows = bar.height;
    if rows == 0 || lines <= rows {
        return;
    }
    draw_thumb(frame, bar, v.top, lines, thumb_color(e, focused, c));
    if lines > 20_000 {
        return;
    }
    let mut mark = |row: usize| {
        frame.print(Rect::new(bar.x, bar.y + row * rows / lines, 1, 1), 0, &paint(palette::AMBER, "▐"));
    };
    if std::ptr::eq(v.text.buf, e.buffer()) {
        for p in e.search_hits().iter().flat_map(|h| h.iter()) {
            mark(p.row);
        }
    } else if let Some(pat) = e.search_pattern() {
        for (row, line) in v.text.buf.lines().iter().enumerate() {
            if !e.matches_in(line, pat).is_empty() {
                mark(row);
            }
        }
    }
}

fn draw_thumb(frame: &mut Frame, bar: Rect, top: usize, lines: usize, color: Rgb) {
    let rows = bar.height;
    if rows == 0 || lines <= rows {
        return;
    }
    let size = (rows * rows / lines).clamp(1, rows);
    let start = (top * rows / lines).min(rows - size);
    for i in start..start + size {
        frame.print(Rect::new(bar.x, bar.y + i, 1, 1), 0, &paint(color, "▐"));
    }
}

fn draw_welcome(frame: &mut Frame, area: Rect, c: &Colors) {
    if color_mode() != ColorMode::None {
        sparkles(frame, area, frame.ambient_time(), 0.012);
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
    let t = frame.time();
    let intro = intro(t, area);
    if intro.is_some() {
        frame.animate();
    }
    for (i, l) in LOGO.iter().enumerate() {
        let shift = i as f32 * 0.15;
        match &intro {
            Some((x, rows)) => {
                let r = Rect::new(*x, y, rows[i].len(), 1).intersect(area);
                frame.print(r, 0, &paint_intro(&rows[i], shift, c.app));
            }
            None => line(frame, y, tinsel::text::width(l), &tinsel::style::gradient(l, &tinsel::style::RAMP, shift)),
        }
        y += 1;
    }
    y += 1;
    line(frame, y, tagline.len(), &paint(palette::SOFT, tagline));
    y += 3;
    for (key, what) in menu {
        let keys = paint_on(
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

fn indent_width(line: &str, ts: usize) -> Option<usize> {
    let ind = crate::text::indent(line);
    (ind.len() < line.len()).then(|| vcol(line, char_count(ind), ts))
}

fn resolved_indent(buf: &Buffer, ts: usize, row: usize) -> usize {
    if let Some(w) = indent_width(buf.line(row), ts) {
        return w;
    }
    let up = (row.saturating_sub(200)..row)
        .rev()
        .find_map(|r| indent_width(buf.line(r), ts));
    let down = (row + 1..buf.len().min(row + 200)).find_map(|r| indent_width(buf.line(r), ts));
    match (up, down) {
        (Some(a), Some(b)) => a.min(b),
        _ => 0,
    }
}

fn scope(buf: &Buffer, row: usize, ts: usize, sw: usize) -> Option<(usize, usize, usize)> {
    let inner = resolved_indent(buf, ts, row);
    if inner < sw {
        return None;
    }
    let last = buf.last_row();
    let (mut top, mut bottom) = (row, row);
    while top > 0 && row - top < 2000 && resolved_indent(buf, ts, top - 1) >= inner {
        top -= 1;
    }
    while bottom < last && bottom - row < 2000 && resolved_indent(buf, ts, bottom + 1) >= inner {
        bottom += 1;
    }
    Some((inner - sw, top, bottom))
}

// v.focused alone means only that this is the focused window
fn draw_text(e: &Editor, v: &View, frame: &mut Frame, gutter: Rect, body: Rect, c: &Colors, focused: bool) {
    let buf = v.text.buf;
    let ts = e.options.tab_stop;
    let sw = e.options.shift_width.max(1);
    let (top, left, cursor, active) = (v.top, v.left, v.cursor, v.focused);
    let time = frame.time();
    let mode = if active { e.mode() } else { Mode::Normal };
    let selection = e.selection().filter(|_| active);
    let pattern = e.search_pattern();
    let marks = v.text.change_marks();
    let word = (mode == Mode::Normal && focused)
        .then(|| word_at(buf.line(cursor.row), cursor.col))
        .flatten();
    let current_match = (mode == Mode::Search).then_some(cursor);
    let bracket = e
        .matching_bracket()
        .filter(|_| active && matches!(mode, Mode::Normal | Mode::Insert));
    let colored = color_mode() != ColorMode::None;
    let guides = e.options.indent_guides && colored;
    let scope = if guides && active && !mode.is_visual() {
        scope(buf, cursor.row, ts, sw)
    } else {
        None
    };
    let accent = if active { mode_color(mode) } else { palette::SOFT };
    let bar = if active && focused { bar_on_guide(e) } else { None };
    let lit = mix(accent, palette::BLUSH, 0.3);
    let lens = if active && focused { e.lens() } else { None };
    if active && focused && lens.is_none() {
        e.anim.lens_gone();
    }

    let mut sky_from = None;
    for i in 0..body.height {
        let row = top + i;
        let y = body.y + i;
        if row >= buf.len() {
            // sky starts a row past the end
            if row > buf.len() && !e.zen() && sky_from.is_none() {
                sky_from = Some(y);
            }
            continue;
        }
        let line = buf.line(row);
        let on_cursor = row == cursor.row;

        let glow = if active { e.anim.glow(row, time) } else { 0.0 };
        let band = if on_cursor && active && e.options.cursor_line {
            0.07
        } else {
            0.0
        };
        let glow = if mode.is_visual() { 0.0 } else { 0.3 * glow };
        if glow + band > 0.0 {
            frame.tint(
                Rect::new(gutter.x, y, gutter.width + body.width, 1),
                mix(c.app, accent, glow + band),
            );
        }
        if gutter.width > 4 {
            let width = gutter.width - 4;
            let n = if e.options.relative_number && !on_cursor {
                row.abs_diff(cursor.row)
            } else {
                row + 1
            };
            let digits = if e.options.zero_pad && (on_cursor || !e.options.relative_number) {
                v.text.buf.len().to_string().len().max(2)
            } else {
                1
            };
            let label = if e.options.relative_number && on_cursor && e.options.number {
                format!("{n:<width$}")
            } else {
                format!("{:>width$}", format!("{n:0>digits$}"))
            };
            let label = if on_cursor && active {
                bold(&paint(lit, &label))
            } else {
                paint(c.faint, &label)
            };
            frame.print(Rect::new(gutter.x + 1, y, width, 1), 0, &label);
            if on_cursor && active && e.options.cursor_line {
                frame.print(Rect::new(gutter.x + width + 2, y, 1, 1), 0, &bold(&paint(lit, "›")));
            }
            if let Some(Some(mark)) = marks.get(row) {
                let color = match mark {
                    Mark::Added => palette::SAGE,
                    Mark::Changed => palette::AMBER,
                    Mark::Removed | Mark::RemovedAbove => palette::RED,
                };
                frame.print(Rect::new(gutter.x, y, 1, 1), 0, &bold(&paint(color, "·")));
            }
        }

        let tint = |frame: &mut Frame, from: usize, to: usize, color: Rgb| {
            let a = vcol(line, from, ts);
            let b = vcol(line, to, ts).max(a + 1);
            let (a, b) = (a.max(left) - left, b.saturating_sub(left).min(body.width));
            if a < b {
                frame.tint(Rect::new(body.x + a, y, b - a, 1), color);
            }
        };
        if let Some((from, to)) = selection.and_then(|sel| selected_cols(&sel, row, char_count(line))) {
            tint(frame, from, to, c.selection);
        }
        if let Some(pat) = pattern {
            for (from, to) in e.matches_in(line, pat) {
                let color = if current_match == Some(Pos::new(row, from)) {
                    c.current_match
                } else {
                    c.search
                };
                tint(frame, from, to, color);
            }
        }
        if let Some(b) = bracket.filter(|b| b.row == row) {
            tint(frame, b.col, b.col + 1, c.bracket);
        }
        if let Some(w) = &word {
            for (at, end) in whole_words(line, w) {
                tint(frame, at, end, c.word);
            }
        }
        let trimmed = char_count(line.trim_end_matches([' ', '\t']));
        let len = char_count(line);
        if trimmed < len && trimmed > 0 && !(on_cursor && matches!(mode, Mode::Insert | Mode::Replace)) {
            tint(frame, trimmed, len, c.trailing);
        }

        let kinds = v.text.highlight(row, e.options.syntax);
        let shown = styled_line(line, kinds.as_deref(), ts, c);
        frame.print_from(Rect::new(body.x, y, body.width, 1), 0, &shown, left);

        if colored {
            tab_arrows(frame, Rect::new(body.x, y, body.width, 1), line, left, ts, c.guide);
        }

        if guides {
            let width = resolved_indent(buf, ts, row);
            let mut col = 0;
            while col < width {
                if col >= left && col - left < body.width {
                    let in_scope = scope.is_some_and(|(sc, t, b)| sc == col && row >= t && row <= b);
                    let color = if in_scope { mix(c.app, accent, 0.45) } else { c.guide };
                    let shown = if on_cursor && bar == Some(col) {
                        bold(&paint(accent, "┃"))
                    } else {
                        paint(color, "│")
                    };
                    frame.print(Rect::new(body.x + col - left, y, 1, 1), 0, &shown);
                }
                col += sw;
            }
        }

        if let Some(text) = lens.as_deref().filter(|_| on_cursor) {
            draw_lens(e, frame, Rect::new(body.x, y, body.width, 1), line, left, text, c);
        }
    }
    if let Some(y) = sky_from {
        let width = body.x + body.width - gutter.x;
        let clock = frame.ambient_time();
        sparkles(frame, Rect::new(gutter.x, y, width, body.y + body.height - y), clock, 0.012);
    }
}

fn selected_cols(sel: &Range, row: usize, len: usize) -> Option<(usize, usize)> {
    if row < sel.start.row || row > sel.end.row {
        return None;
    }
    if sel.end.row == row && sel.end.col == 0 && row > sel.start.row && !sel.linewise {
        return None;
    }
    let (from, to) = if sel.linewise {
        (0, len + 1)
    } else {
        let from = if row == sel.start.row { sel.start.col } else { 0 };
        let to = if row == sel.end.row { sel.end.col } else { len + 1 };
        (from, to)
    };
    Some((from, to.max(from + usize::from(len == 0))))
}

fn whole_words<'a>(line: &'a str, w: &'a str) -> impl Iterator<Item = (usize, usize)> + 'a {
    let whole = |i: Option<usize>| i.and_then(|i| line.chars().nth(i)).is_none_or(|ch| !is_word(ch));
    crate::editor::find_all(line, w, false)
        .into_iter()
        .map(|at| (at, at + char_count(w)))
        .filter(move |&(at, end)| whole(at.checked_sub(1)) && whole(Some(end)))
}

fn tab_arrows(frame: &mut Frame, row: Rect, line: &str, left: usize, ts: usize, ink: Rgb) {
    let lead = crate::text::first_non_blank(line);
    for (i, ch) in line.chars().enumerate().skip(lead) {
        if ch == '\t' {
            let at = vcol(line, i, ts);
            if at >= left && at - left < row.width {
                frame.print(Rect::new(row.x + at - left, row.y, 1, 1), 0, &paint(ink, "→"));
            }
        }
    }
}

fn draw_lens(e: &Editor, frame: &mut Frame, row: Rect, line: &str, left: usize, text: &str, c: &Colors) {
    const GAP: usize = 4;
    let end = vcol(line, char_count(line), e.options.tab_stop);
    let Some(x) = (end + GAP).checked_sub(left) else { return };
    let room = row.width.saturating_sub(x);
    let short = text.split(" · ").next().unwrap_or(text);
    let Some(shown) = [text, short].into_iter().find(|t| tinsel::text::width(t) <= room) else {
        return;
    };
    let key = format!("{}:{}", e.buffer_id, e.cursor().row);
    let fade = e.anim.lens(&key, frame);
    if fade <= 0.0 {
        return;
    }
    let color = mix(c.app, mix(palette::MUTED, palette::BLUSH, 0.15), fade);
    let w = tinsel::text::width(shown);
    frame.print(Rect::new(row.x + x, row.y, w, 1), 0, &italic(&paint(color, shown)));
}

fn word_at(line: &str, col: usize) -> Option<String> {
    let chars: Vec<char> = line.chars().collect();
    if !chars.get(col).is_some_and(|c| is_word(*c)) {
        return None;
    }
    let start = (0..col).rev().find(|&i| !is_word(chars[i])).map_or(0, |i| i + 1);
    let end = (col..chars.len()).find(|&i| !is_word(chars[i])).unwrap_or(chars.len());
    Some(chars[start..end].iter().collect())
}

fn explorer_header(e: &Editor, x: &Explorer, frame: &mut Frame, area: Rect) {
    let root = x.root();
    let name = file_name(Some(root), &root.display().to_string());
    let span = area.width.saturating_sub(4);
    let name = tinsel::text::truncate(&name, span.saturating_sub(6));
    let named = tinsel::text::width(&name) + 2;
    let rule = span.saturating_sub(named + 2);
    let ink = palette::LINE;
    let head = format!(
        "{}{}{}",
        paint(ink, &format!("╭{}", "─".repeat(rule / 2))),
        format_args!(" {} ", bold(&paint(palette::TEXT, &name))),
        paint(ink, &format!("{}╮", "─".repeat(rule - rule / 2))),
    );
    frame.print(Rect::new(area.x + 2, area.y, span, 1), 0, &head);
    if let Some(info) = sky::branch_info(e).filter(|i| tinsel::text::width(i) <= span) {
        let w = tinsel::text::width(&info);
        frame.print(Rect::new(area.x + 2 + (span - w) / 2, area.y + 1, w, 1), 0, &info);
    }
}

fn draw_explorer(e: &Editor, x: &Explorer, frame: &mut Frame, area: Rect, c: &Colors, lit: Rgb) {
    if area.height == 0 || area.width < 6 {
        return;
    }
    let tree = e.tree();
    let mut list = area;
    if area.height >= 5 {
        explorer_header(e, x, frame, area);
        list = Rect::new(area.x, area.y + 2, area.width, area.height - 2);
    }
    let shown = x.visible();
    if shown.is_empty() {
        let note = match x.filter() {
            Some(f) if !f.is_empty() => format!("nothing matches “{f}”"),
            _ => "this folder is empty".to_string(),
        };
        frame.print(list, 0, &format!("    {}", paint(palette::MUTED, &note)));
        return;
    }
    let top = x.top(list.height);
    let current = e.file().path.as_ref().and_then(|p| p.canonicalize().ok());
    let focused = e.explorer_focused();
    for (i, (entry, hits)) in shown.iter().enumerate().skip(top).take(list.height) {
        let y = list.y + i - top;
        let row = Rect::new(list.x, y, list.width, 1);
        let selected = focused && i == x.selected();
        let is_current = !entry.dir && current.as_deref() == Some(entry.path.as_path());
        let tint = match (selected, is_current) {
            (true, true) => 0.11,
            (true, false) => 0.07,
            (false, true) => 0.05,
            _ => 0.0,
        };
        if tint > 0.0 {
            // clear of the screen's edges, which the terminal stretches into its margins
            let inside = Rect::new(row.x + 1, y, row.width.saturating_sub(2), 1);
            frame.tint(inside, mix(c.app, palette::ROSE, tint));
        }
        let state = tree.with(|t| {
            if entry.dir {
                (t.dir_changed(&entry.path) || t.file(&entry.path).is_some()).then_some(FileState::Modified)
            } else {
                t.file(&entry.path)
            }
        });
        let (glyph, ink, base) = match state {
            _ if is_current => ("✶", lit, lit),
            Some(_) if entry.dir => ("◇", palette::AMBER, palette::SOFT),
            None if entry.dir => ("◇", c.faint, palette::SOFT),
            Some(s) => ("⋆", file_state_color(s), palette::SOFT),
            None => ("·", c.faint, palette::MUTED),
        };
        let base = if is_current {
            base
        } else if selected {
            palette::TEXT
        } else if entry.name.starts_with('.') {
            mix(base, c.app, 0.3)
        } else {
            base
        };
        let name = with_hits(&entry.name, |j| hits.contains(&j), |_, s| paint(base, s));
        let (glyph, name) = if is_current {
            (bold(&paint(ink, glyph)), bold(&name))
        } else {
            (paint(ink, glyph), name)
        };
        let guides = paint(c.faint, &"│ ".repeat(entry.depth));
        frame.print(
            Rect::new(row.x, y, row.width - 1, 1),
            0,
            &format!("  {guides}{glyph} {name}"),
        );
        if selected {
            frame.print(Rect::new(list.x, y, 1, 1), 0, &paint(palette::ROSE, "▎"));
        }
    }
}

fn with_hits(text: &str, hit: impl Fn(usize) -> bool, other: impl Fn(usize, &str) -> String) -> String {
    let mut out = String::new();
    let mut buf = [0; 4];
    for (j, ch) in text.chars().enumerate() {
        let s = ch.encode_utf8(&mut buf);
        out.push_str(&if hit(j) {
            bold(&paint(palette::ROSE, s))
        } else {
            other(j, s)
        });
    }
    out
}

fn file_state_color(state: FileState) -> Rgb {
    match state {
        FileState::Modified => palette::AMBER,
        FileState::Added => palette::SAGE,
        FileState::Untracked => palette::MUTED,
        FileState::Deleted => palette::RED,
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

struct Tab {
    name: String,
    current: bool,
    modified: bool,
}

fn tab_names(e: &Editor) -> Vec<Tab> {
    let all = e.buffers();
    let base = |b: &crate::editor::BufferInfo| file_name(b.path.as_deref(), "untitled");
    all.iter()
        .map(|b| {
            let name = base(b);
            let clash = all.iter().filter(|o| base(o) == name).count() > 1;
            let shown = match (&b.path, clash) {
                (Some(p), true) => p
                    .parent()
                    .and_then(|d| d.file_name())
                    .map_or(name.clone(), |d| format!("{}/{name}", d.to_string_lossy())),
                _ => name,
            };
            Tab {
                name: shown,
                current: b.current,
                modified: b.modified,
            }
        })
        .collect()
}

fn diff_totals(e: &Editor) -> Option<String> {
    let (mut added, mut changed, mut removed) = (0, 0, 0);
    for h in e.change_hunks().iter() {
        let (old, new) = (h.old.len(), h.new.len());
        let both = old.min(new);
        changed += both;
        added += new - both;
        removed += old - both;
    }
    let parts: Vec<String> = [
        (added, "+", palette::SAGE),
        (changed, "~", palette::AMBER),
        (removed, "-", palette::RED),
    ]
    .into_iter()
    .filter(|(n, _, _)| *n > 0)
    .map(|(n, sign, color)| paint(color, &format!("{sign}{n}")))
    .collect();
    (!parts.is_empty()).then(|| parts.join(" "))
}

fn crumb_scope(e: &Editor) -> Vec<(String, Rgb)> {
    let syntax = e.syntax();
    let buf = e.buffer();
    let row = e.cursor().row;
    let mut found = Vec::new();
    if syntax == Some(Syntax::Markdown) {
        let mut level = usize::MAX;
        for r in (0..=row).rev() {
            let line = buf.line(r);
            let hashes = line.chars().take_while(|c| *c == '#').count();
            if hashes > 0 && hashes < level && line.chars().nth(hashes) == Some(' ') {
                found.push((format!("# {}", line[hashes..].trim()), palette::ROSE));
                level = hashes;
                if level == 1 {
                    break;
                }
            }
        }
    } else if syntax.is_some() {
        let ts = e.options.tab_stop;
        let mut limit = indent_width(buf.line(row), ts).unwrap_or(usize::MAX).saturating_add(1);
        for r in (row.saturating_sub(3000)..=row).rev() {
            let line = buf.line(r);
            let Some(ind) = indent_width(line, ts) else { continue };
            if ind >= limit {
                continue;
            }
            if let Some(symbol) = symbol(line.trim()) {
                found.push(symbol);
                limit = ind;
                if ind == 0 {
                    break;
                }
            }
        }
    }
    found.reverse();
    found
}

// a top-level item of the file, named on the row under the star line
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Section {
    pub start: usize,
    pub end: usize,
    pub label: String,
}

fn outline(e: &Editor) -> std::cell::Ref<'_, Vec<Section>> {
    let syntax = e
        .file()
        .path
        .as_ref()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .and_then(Syntax::detect);
    let key = (e.buffer_id, e.version(), syntax);
    if e.outline.borrow().as_ref().is_none_or(|(k, _)| *k != key) {
        let found = sections(e.buffer().lines(), syntax);
        *e.outline.borrow_mut() = Some((key, found));
    }
    std::cell::Ref::map(e.outline.borrow(), |o| &o.as_ref().expect("just filled").1)
}

fn sections(lines: &[String], syntax: Option<Syntax>) -> Vec<Section> {
    let blank = |r: usize| lines[r].trim().is_empty();
    // an item's stretch stops at its last non-blank line
    let trim = |start: usize, mut end: usize| {
        while end > start && blank(end) {
            end -= 1;
        }
        end
    };
    match syntax {
        Some(Syntax::Markdown) => {
            let mut heads = Vec::new();
            let mut fenced = false;
            for (r, line) in lines.iter().enumerate() {
                if line.trim_start().starts_with("```") || line.trim_start().starts_with("~~~") {
                    fenced = !fenced;
                }
                let hashes = line.chars().take_while(|c| *c == '#').count();
                if !fenced && (1..=6).contains(&hashes) && line[hashes..].starts_with(' ') {
                    heads.push((r, hashes, line[hashes..].trim().to_string()));
                }
            }
            let mut top = heads.iter().map(|h| h.1).min().unwrap_or(0);
            // a lone title says nothing the file name doesn't, so then it's the level below
            if heads.iter().filter(|h| h.1 == top).count() == 1 {
                top = heads.iter().map(|h| h.1).filter(|l| *l > top).min().unwrap_or(top);
            }
            heads
                .iter()
                .filter(|h| h.1 == top)
                .map(|(r, _, label)| {
                    let next = heads
                        .iter()
                        .find(|h| h.0 > *r && h.1 <= top)
                        .map_or(lines.len(), |h| h.0);
                    Section {
                        start: *r,
                        end: trim(*r, next - 1),
                        label: label.clone(),
                    }
                })
                .collect()
        }
        Some(Syntax::Toml) => {
            let heads: Vec<(usize, String)> = lines
                .iter()
                .enumerate()
                .filter(|(_, l)| l.starts_with('['))
                .filter_map(|(r, l)| l.rfind(']').map(|end| (r, l[..=end].to_string())))
                .collect();
            heads
                .iter()
                .enumerate()
                .map(|(i, (r, label))| {
                    let next = heads.get(i + 1).map_or(lines.len(), |h| h.0);
                    Section {
                        start: *r,
                        end: trim(*r, next - 1),
                        label: label.clone(),
                    }
                })
                .collect()
        }
        Some(Syntax::Yaml | Syntax::Json) | None => Vec::new(),
        Some(_) => code_sections(lines, &trim),
    }
}

fn code_sections(lines: &[String], trim: &dyn Fn(usize, usize) -> usize) -> Vec<Section> {
    let top = |r: usize| lines[r].starts_with(|c: char| !c.is_whitespace());
    let blank = |r: usize| lines[r].trim().is_empty();
    let import = |r: usize| {
        let l = lines[r].trim_start_matches("pub ").trim_start_matches("pub(crate) ");
        ["use ", "import ", "from ", "#include", "require"]
            .iter()
            .any(|w| l.starts_with(w))
    };
    // `}` ends an item, but `) -> T {` closing a long signature opens its body
    let closer = |r: usize| {
        let l = lines[r].trim_end();
        l.starts_with(['}', ')', ']']) && !l.ends_with(['{', '(', '[']) || l == "end"
    };
    let lead = |r: usize| ["#[", "//", "@"].iter().any(|w| lines[r].starts_with(w));
    let mut out: Vec<Section> = Vec::new();
    let (mut body_seen, mut use_seen, mut mods) = (false, false, false);
    let mut r = 0;
    while r < lines.len() {
        if !top(r) {
            r += 1;
            continue;
        }
        let is_use = import(r) && !use_seen && !body_seen;
        let Some(label) = (if is_use {
            Some("use".to_string())
        } else {
            symbol(lines[r].trim_end()).map(|s| s.0)
        }) else {
            r += 1;
            continue;
        };
        let floor = out.last().map_or(0, |s| s.end + 1);
        let mut start = r;
        while start > floor && lead(start - 1) {
            start -= 1;
        }
        let mut k = r + 1;
        while k < lines.len() {
            let more = lines[k].starts_with([')', ']', '{']) || lines[k].starts_with("where");
            if blank(k)
                || !top(k)
                || is_use && (import(k) || closer(k) || lines[k].starts_with("//"))
                || more && !closer(k)
            {
                k += 1;
                continue;
            }
            if closer(k) {
                k += 1;
            }
            break;
        }
        let end = trim(r, k - 1);
        let one_mod = label.starts_with("mod ") && end == r && lines[r].trim_end().ends_with(';');
        match out.last_mut() {
            // a run of `mod x;` lines is one item
            Some(prev) if one_mod && mods => {
                prev.end = end;
                prev.label = "mod".into();
            }
            _ => out.push(Section { start, end, label }),
        }
        mods = one_mod;
        use_seen |= is_use;
        body_seen |= !is_use && !one_mod;
        r = k;
    }
    out
}

fn symbol(line: &str) -> Option<(String, Rgb)> {
    let mut rest = line;
    for prefix in [
        "pub(crate) ",
        "pub(super) ",
        "pub ",
        "export ",
        "async ",
        "unsafe ",
        "static ",
        "default ",
        "const ",
    ] {
        if let Some(r) = rest.strip_prefix(prefix) {
            rest = r;
        }
    }
    let name = |s: &str| -> String { s.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect() };
    for (kw, mark, color) in [
        ("fn ", "ƒ ", LILAC),
        ("def ", "ƒ ", LILAC),
        ("function ", "ƒ ", LILAC),
        ("func ", "ƒ ", LILAC),
        ("struct ", "", PEACH),
        ("enum ", "", PEACH),
        ("trait ", "", PEACH),
        ("class ", "", PEACH),
        ("interface ", "", PEACH),
        ("mod ", "", PEACH),
    ] {
        if let Some(r) = rest.strip_prefix(kw) {
            // a Go method: func (r *T) Name(
            let r = if kw == "func " && r.starts_with('(') {
                r.split_once(") ").map_or(r, |x| x.1)
            } else {
                r
            };
            let n = name(r);
            if n.is_empty() {
                return None;
            }
            let label = if mark.is_empty() {
                format!("{kw}{n}")
            } else {
                format!("{mark}{n}")
            };
            return Some((label, color));
        }
    }
    if let Some(r) = rest.strip_prefix("impl").filter(|r| r.starts_with([' ', '<'])) {
        let r = r.trim_start_matches(|c: char| c == '<' || c.is_whitespace());
        let head: String = r
            .split('{')
            .next()
            .unwrap_or("")
            .replace(" where", "")
            .trim()
            .to_string();
        let head = match head.split_once("> ") {
            Some((_, h)) => h.to_string(),
            None => head,
        };
        if !head.is_empty() {
            return Some((format!("impl {head}"), PEACH));
        }
    }
    None
}

// MARK: - Motion

const FRAME: f64 = 1.0 / 60.0;
const PULSE: f64 = 0.4;
const SLIDE: f64 = 0.18;
const FADE: f64 = 0.15;
// scrolls this small just snap, so holding j or k never lags
const SNAP: f64 = 2.0;
const LENS_WAIT: f64 = 0.35;
const LENS_FADE: f64 = 0.25;
const OPEN: f64 = 0.2;

// what moves on screen, on tinsel's glides; each asks the frame for more while it moves, and a
// frame at time 0 (tests, snapshots) always draws settled
pub(crate) struct Anim {
    scroll: Glider,
    shown: Cell<usize>,
    landed: Cell<bool>,
    pulse: Cell<Option<(usize, f64)>>,
    zen: Cell<Option<(bool, f64)>>,
    slide: Cell<Option<Glide>>,
    text_x: Cell<usize>,
    pub(crate) star: Glider,
    lens: RefCell<Option<(String, f64)>>,
}

impl Default for Anim {
    fn default() -> Self {
        Anim {
            scroll: Glider::new().snap(SNAP),
            shown: Cell::default(),
            landed: Cell::default(),
            pulse: Cell::default(),
            zen: Cell::default(),
            slide: Cell::default(),
            text_x: Cell::default(),
            star: Glider::new().snap(SNAP),
            lens: RefCell::default(),
        }
    }
}

// a glider's place for a frame: gliding when smooth scrolling is on, there at once when not
pub(crate) fn glide(g: &Glider, to: usize, span: usize, smooth: bool, frame: &mut Frame) -> usize {
    let shown = if smooth {
        g.follow(to as f64, span as f64, frame)
    } else {
        g.jump(to as f64, span as f64)
    };
    shown.round().max(0.0) as usize
}

// asks for frames until `until`
fn moving(frame: &mut Frame, until: f64) {
    if frame.time() > 0.0 && frame.time() < until {
        frame.animate();
    }
}

impl Anim {
    pub(crate) fn reset(&self) {
        self.scroll.reset();
        self.pulse.set(None);
    }

    pub(crate) fn land(&self) {
        self.landed.set(true);
    }

    fn follow(&self, e: &Editor, frame: &mut Frame) -> usize {
        let t = frame.time();
        let (top, rows) = (e.top(), e.text_rows());
        let on = e.options.smooth && t > 0.0;
        let shown = glide(&self.scroll, top, rows, on, frame);
        let end = self.scroll.glide().map(|g| g.end());
        if self.landed.take() && on && !e.mode().is_visual() {
            let start = end.unwrap_or(t);
            self.pulse.set(Some((e.cursor().row, start)));
        }
        if !on {
            self.pulse.set(None);
        }
        self.shown.set(shown);
        if let Some((_, start)) = self.pulse.get() {
            moving(frame, start + PULSE);
        }
        shown
    }

    pub(crate) fn shown(&self) -> usize {
        self.shown.get()
    }

    fn glow(&self, row: usize, t: f64) -> f32 {
        match self.pulse.get() {
            Some((r, start)) if r == row => {
                let left = 1.0 - ((t - start) / PULSE).clamp(0.0, 1.0) as f32;
                left * left
            }
            _ => 0.0,
        }
    }

    fn zen(&self, zen: bool, frame: &mut Frame) -> f64 {
        let t = frame.time();
        let since = match self.zen.get() {
            Some((was, at)) if was == zen => at,
            Some(_) if t > 0.0 => {
                self.slide
                    .set(zen.then(|| Glide::new(self.text_x.get() as f64, 0.0, t - FRAME, SLIDE)));
                t
            }
            _ => f64::NEG_INFINITY,
        };
        self.zen.set(Some((zen, since)));
        moving(frame, since + SLIDE.max(FADE));
        if t > 0.0 { t - since } else { f64::INFINITY }
    }

    fn column_x(&self, x: usize, t: f64) -> usize {
        match self.slide.get().filter(|g| t > 0.0 && g.moving(t)) {
            Some(g) => g.towards(x as f64).at(t).round().max(0.0) as usize,
            None => x,
        }
    }

    fn lens(&self, key: &str, frame: &mut Frame) -> f32 {
        let t = frame.time();
        if t <= 0.0 {
            return 1.0;
        }
        let mut seen = self.lens.borrow_mut();
        let since = match seen.as_ref() {
            Some((k, at)) if k == key => *at,
            _ => {
                *seen = Some((key.to_string(), t));
                t
            }
        };
        moving(frame, since + LENS_WAIT + LENS_FADE);
        ((t - since - LENS_WAIT) / LENS_FADE).clamp(0.0, 1.0) as f32
    }

    fn lens_gone(&self) {
        self.lens.replace(None);
    }

    fn opening(&self, opened: Option<f64>, frame: &mut Frame) -> f32 {
        let t = frame.time();
        let Some(at) = opened.filter(|_| t > 0.0) else {
            return 1.0;
        };
        moving(frame, at + OPEN);
        let p = ((t - at) / OPEN).clamp(0.0, 1.0) as f32;
        1.0 - (1.0 - p).powi(3)
    }

    fn text_at(&self, x: usize) {
        self.text_x.set(x);
    }
}

// the stars twinkle a few times a second, on the ambient clock, so they rest while unfocused
const TWINKLE_FPS: u32 = 15;

fn sparkles(frame: &mut Frame, area: Rect, clock: f64, density: f32) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    frame.animate_at(TWINKLE_FPS);
    let mut sky = Frame::new(area.width, area.height).at(clock);
    sky.render(sky.area(), Sparkles::new().density(density).wandering());
    for (i, line) in sky.snapshot().lines().iter().enumerate() {
        frame.print(Rect::new(area.x, area.y + i, area.width, 1), 0, line);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_intro_squeezes_insel_away_into_the_logo() {
        let area = Rect::new(0, 0, 80, 24);
        let text = |row: &[(char, f32)]| row.iter().map(|c| c.0).collect::<String>().trim_end().to_string();
        let at = |t: f64| intro(t, area).map(|(x, rows)| (x, rows.map(|r| text(&r))));
        assert_eq!(at(0.0), None, "a snapshot sees the plain logo");
        assert_eq!(at(INTRO_END), None);

        let (_, start) = at(0.1).unwrap();
        assert_eq!(start[0], "▀█▀ █ █▄ █ █▀ █▀▀ █   █ █ █ █▀▄▀█ ✶", "tinselvim");
        assert_eq!(start[1], " █  █ █ ▀█ ▄█ ██▄ █▄▄ ▀▄▀ █ █ ▀ █");

        // one smooth move: t only ever right, the star only ever left, insel only ever fainter
        let (mut t_was, mut star_was, mut solid_was) = (0, usize::MAX, 1.0);
        let mut t = 0.01;
        while t < INTRO_END {
            let (x, rows) = intro(t, area).unwrap();
            let star = x + rows[0].iter().position(|c| c.0 == '✶').unwrap();
            // how solid insel's faintest cell is; once it's all gone, nothing of it is left at all
            let faded = rows.iter().flatten().filter(|c| c.0 != ' ' && c.1 < 1.0).map(|c| c.1);
            let solid = match faded.fold(None, |m: Option<f32>, a| Some(m.map_or(a, |m| m.min(a)))) {
                Some(a) => a,
                None if t > INTRO_HOLD + 0.1 => 0.0,
                None => 1.0,
            };
            assert!(x >= t_was && star <= star_was && solid <= solid_was, "at {t}: {rows:?}");
            (t_was, star_was, solid_was) = (x, star, solid);
            t += 0.01;
        }
        let (_, rows) = intro(INTRO_HOLD + INTRO_CLOSE * 0.5, area).unwrap();
        assert!(rows[0].iter().any(|c| c.1 > 0.0 && c.1 < 1.0), "halfway, insel is half faded");

        let (x, end) = at(INTRO_END - 0.0001).unwrap();
        assert_eq!(x, (80 - 17) / 2, "it lands where the logo is drawn");
        assert_eq!(end, LOGO.map(|l| l.trim_end().to_string()));
    }

    #[test]
    fn written_messages_read_as_a_sentence() {
        assert_eq!(saved("\"src/a.rs\" 3L, 20B written"), "saved a.rs · 3 lines · 20 bytes");
        assert_eq!(saved("\"x\" [New] 1L, 2B written"), "saved x · new · 1 line · 2 bytes");
        assert_eq!(saved("\"x\" 0L, 0B written"), "saved x · 0 lines · 0 bytes");
    }

    #[test]
    fn long_paths_keep_their_end_in_columns() {
        assert_eq!(cut_left("src/main.rs", 20), "src/main.rs");
        assert_eq!(cut_left("src/main.rs", 8), "…main.rs");
        assert_eq!(cut_left("文書/メモ.md", 8), "…メモ.md");
        assert_eq!(cut_left("文書/メモ.md", 6), "…モ.md");
        assert_eq!(cut_left("文書/メモ.md", 5), "….md");
        assert_eq!(cut_left("abc", 0), "…");
    }

    #[test]
    fn symbols() {
        assert_eq!(symbol("pub fn scroll(&self) {").unwrap().0, "ƒ scroll");
        assert_eq!(symbol("impl Editor {").unwrap().0, "impl Editor");
        assert_eq!(symbol("def run(self):").unwrap().0, "ƒ run");
        assert_eq!(symbol("func (s *Server) Start() {").unwrap().0, "ƒ Start");
        assert_eq!(symbol("pub struct Pos {").unwrap().0, "struct Pos");
        assert_eq!(symbol("let x = 1;"), None);
    }

    #[test]
    fn breadcrumbs_follow_the_cursor_in() {
        let path = std::env::temp_dir().join(format!("tvim-crumb-{}.rs", std::process::id()));
        std::fs::write(
            &path,
            "impl Editor {\n    pub fn scroll(&self) {\n        let x = 1;\n    }\n}\n",
        )
        .unwrap();
        let mut e = Editor::open(&path).unwrap();
        e.keys(&crate::keys::parse("3G"));
        let crumbs: Vec<String> = crumb_scope(&e).into_iter().map(|c| c.0).collect();
        assert_eq!(crumbs, ["impl Editor", "ƒ scroll"]);
    }

    fn outline_of(text: &str, syntax: Syntax) -> Vec<(usize, usize, String)> {
        let lines: Vec<String> = text.lines().map(str::to_string).collect();
        sections(&lines, Some(syntax))
            .into_iter()
            .map(|s| (s.start, s.end, s.label))
            .collect()
    }

    #[test]
    fn the_top_level_items_of_code() {
        let rust = "mod a;\nmod b;\n\nuse std::io;\nuse x::{\n    y,\n};\n\n#[derive(Debug)]\npub struct S {\n    a: u8,\n}\n\nconst N: u8 = 1;\n\nfn long(\n    a: u8,\n) -> u8 {\n    a\n}\n\nimpl S {\n    fn f() {}\n}\n";
        assert_eq!(
            outline_of(rust, Syntax::Rust),
            [
                (0, 1, "mod".to_string()),
                (3, 6, "use".into()),
                (8, 11, "struct S".into()),
                (15, 19, "ƒ long".into()),
                (21, 23, "impl S".into()),
            ],
            "mod lines run together, attributes join their item, constants are bare"
        );
        let py = "import os\nfrom x import y\n\n@cache\ndef run():\n    pass\n\n\nclass A:\n    x = 1\n";
        assert_eq!(
            outline_of(py, Syntax::Python),
            [
                (0, 1, "use".to_string()),
                (3, 5, "ƒ run".into()),
                (8, 9, "class A".into())
            ]
        );
        assert_eq!(symbol("implement = 1"), None);
    }

    #[test]
    fn headings_and_sections() {
        let md = "# Title\n\n## One\ntext\n### Deeper\n\n## Two\n```\n## not a heading\n```\n";
        assert_eq!(
            outline_of(md, Syntax::Markdown),
            [(2, 4, "One".to_string()), (6, 9, "Two".into())],
            "a lone title gives way to the level below"
        );
        let md = "# A\nx\n## a\n# B\n";
        assert_eq!(
            outline_of(md, Syntax::Markdown),
            [(0, 2, "A".to_string()), (3, 3, "B".into())]
        );
        let toml = "name = 1\n\n[package]\nx = 1\n\n[[bin]]\ny = 2\n";
        assert_eq!(
            outline_of(toml, Syntax::Toml),
            [(2, 3, "[package]".to_string()), (5, 6, "[[bin]]".into())]
        );
    }

    #[test]
    fn icons_by_kind() {
        assert_eq!(icon("main.rs", false).0, "⊛");
        assert_eq!(icon("Cargo.toml", false).0, "≡");
        assert_eq!(icon("README.md", false).0, "◆");
        assert_eq!(icon("src", true).0, "▸");
    }
}
