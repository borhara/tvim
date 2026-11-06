mod sky;

use std::io::Write;
use std::path::Path;

use tinsel::app::{self, App, Event, Flow, Frame, Rect, Sparkles};
use tinsel::{ColorMode, Rgb, bold, color_mode, italic, mix, paint, paint_bg, palette, underline};

use crate::buffer::Buffer;
use crate::buffer::Pos;
use crate::diff::Mark;
use crate::editor::{Editor, Effect, Mode, Range, View};
use crate::syntax::Kind;
use crate::text::{caret, cell_width, char_count, vcol};

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

fn run_effect(editor: &mut Editor, effect: Effect) {
    match effect {
        Effect::Shell(cmd) => {
            let shell = std::env::var("SHELL").unwrap_or_else(|_| "sh".into());
            let result = app::suspend(|| {
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
        Todo => bold(&paint_bg(c.ink, palette::AMBER, s)),
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
    (col % e.options.shift_width.max(1) == 0 && col < resolved_indent(buf, ts, p.row)).then_some(col)
}

fn text_area(e: &Editor, frame: &mut Frame, area: Rect, c: &Colors, focused: bool, scrollbar: bool) -> Rect {
    // set_screen reserves a scrollbar column and two status rows
    e.set_screen(area.width, area.height + 2);
    e.scroll();
    e.anim.follow(e, frame.time());
    let (gutter, body) = split_gutter(area, e.gutter());
    body
}

fn windows(e: &Editor, frame: &mut Frame, area: Rect, c: &Colors, focused: bool, scrollbar: bool) -> Rect {
    e.set_region(area.width, area.height);
    if e.window_count() == 1 {
        return text_area(e, frame, area, c, focused, scrollbar);
    }
    let mut body = Rect::default();
    for v in e.views(area) {
        let r = v.rect;
        if r.width == 0 || r.height == 0 {
            continue;
        }
        let text = Rect::new(r.x, r.y + 1, r.width, r.height - 1);
        if preview {
            if v.focused {
                body = b;
            }
        } else if v.focused {
            body = text_area(e, frame, text, c, focused, true);
        } else {
            other_window(e, &v, frame, text, c);
        }
        let lit = v.focused && focused;
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
    frame.print(row, 0, &format!("{label}{text}"));
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

fn thumb_color(e: &Editor, focused: bool, c: &Colors) -> Rgb {
    if focused {
    } else {
        c.scrollbar
    }
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
    let bar = if active && focused { bar_on_guide(e) } else { None };
    let lens = if active && focused { e.lens() } else { None };
    if active && focused && lens.is_none() {
    }

    let clock = e.anim.clock();
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
        }
        let trimmed = char_count(line.trim_end_matches([' ', '\t']));
        let len = char_count(line);
        if trimmed < len && trimmed > 0 && !(on_cursor && matches!(mode, Mode::Insert | Mode::Replace)) {
            tint(frame, trimmed, len, c.trailing);
        }

        let kinds = v.text.highlight(row, e.options.syntax);
        let shown = styled_line(line, kinds.as_deref(), ts, c);
        frame.print(Rect::new(body.x, y, body.width, 1), 0, &shown);

        if colored {
        }

        if guides {
            let width = resolved_indent(buf, ts, row);
            let mut col = 0;
            while col < width {
                if col >= left && col - left < body.width {
                    let in_scope = scope.is_some_and(|(sc, t, b)| sc == col && row >= t && row <= b);
                    let shown = if on_cursor && bar == Some(col) {
                    } else {
                        paint(color, "│")
                    };
                    frame.print(Rect::new(body.x + col - left, y, 1, 1), 0, &shown);
                }
                col += sw;
            }
        }

        if let Some(text) = lens.as_deref().filter(|_| on_cursor) {
        }
    }
    if let Some(y) = sky_from {
        let width = body.x + body.width - gutter.x;
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
