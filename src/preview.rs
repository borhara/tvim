use std::cell::{Cell, Ref, RefCell};
use std::collections::HashMap;

use tinsel::ColorMode;
use tinsel::app::Key;

use crate::command::{Act, Motion, Parse, parse};
use crate::editor::{Editor, Text};
use crate::syntax::Syntax;
use tinsel::anim::Glider;
use crate::window::{MIN_COLS, MIN_ROWS, Win};

// narrower than this and the preview goes underneath
const SIDE_BY_SIDE: usize = 80;

pub(crate) struct Preview {
    pub buffer: usize,
    pub source: usize,
    top: Cell<usize>,
    // scrolled by hand: stays put until the source moves or changes
    pinned: Cell<bool>,
    seen: Cell<Option<(u64, usize)>>,
    size: Cell<Option<(usize, usize)>>,
    render: RefCell<Render>,
    pub glide: Glider,
}

impl Preview {
    fn new(buffer: usize, source: usize) -> Preview {
        Preview {
            buffer,
            source,
            top: Cell::new(0),
            pinned: Cell::new(false),
            seen: Cell::new(None),
            size: Cell::new(None),
            render: RefCell::default(),
            // a scroll this small snaps, as the text's does
            glide: Glider::new().snap(2.0),
        }
    }
}

#[derive(Default)]
struct Render {
    key: Option<(u64, usize, usize, ColorMode)>,
    lines: Vec<String>,
    spans: Vec<Span>,
    // keyed by block source, so typing only re-renders that block
    done: HashMap<String, Vec<String>>,
}

struct Span {
    row: usize,
    rows: usize,
    line: usize,
    lines: usize,
}

impl Render {
    fn build(&mut self, src: &[String], width: usize) {
        let mut old = std::mem::take(&mut self.done);
        self.lines.clear();
        self.spans.clear();
        for (a, b) in blocks(src) {
            let text = src[a..b].join("\n");
            let out = match old.remove(&text).or_else(|| self.done.get(&text).cloned()) {
                Some(out) => out,
                None => tinsel::markdown::wrapped(&text, width).into_lines(),
            };
            if self.lines.last().is_some_and(|l| !l.is_empty()) {
                self.lines.push(String::new());
            }
            self.spans.push(Span {
                row: a,
                rows: b - a,
                line: self.lines.len(),
                lines: out.len(),
            });
            self.lines.extend(out.iter().cloned());
            self.done.insert(text, out);
        }
    }

    fn line_for(&self, row: usize) -> usize {
        let i = self.spans.partition_point(|s| s.row <= row);
        let Some(s) = i.checked_sub(1).map(|i| &self.spans[i]) else {
            return 0;
        };
        if row >= s.row + s.rows {
            return s.line + s.lines;
        }
        s.line + (row - s.row) * s.lines / s.rows
    }
}

// split at blank lines outside fences and around headings, where tinsel restarts anyway
fn blocks(lines: &[String]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    let mut fence: Option<&str> = None;
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim();
        if let Some(f) = fence {
            if t.starts_with(f) {
                fence = None;
            }
            continue;
        }
        let hashes = t.chars().take_while(|&c| c == '#').count();
        let heading = (1..=6).contains(&hashes) && t[hashes..].starts_with(' ');
        if t.is_empty() || heading {
            if let Some(s) = start.take() {
                out.push((s, i));
            }
            if heading {
                out.push((i, i + 1));
            }
            continue;
        }
        fence = ["```", "~~~"].into_iter().find(|f| t.starts_with(f));
        start.get_or_insert(i);
    }
    if let Some(s) = start {
        out.push((s, lines.len()));
    }
    out
}

fn is_markdown(text: &Text) -> bool {
    let name = text.path.and_then(|p| p.file_name()).and_then(|n| n.to_str());
    name.and_then(Syntax::detect) == Some(Syntax::Markdown)
}

impl Editor {
    pub fn preview_focused(&self) -> bool {
        self.previews.contains_key(&self.focus)
    }

    pub(crate) fn is_preview(&self, id: usize) -> bool {
        self.previews.contains_key(&id)
    }

    pub(crate) fn toggle_preview(&mut self) {
        if self.preview_focused() {
            return self.close_preview(self.focus);
        }
        let mine = self
            .previews
            .iter()
            .find(|(_, p)| p.source == self.focus && p.buffer == self.buffer_id)
            .map(|(&id, _)| id);
        if let Some(id) = mine {
            return self.close_preview(id);
        }
        if !is_markdown(&self.shown_text()) {
            return self.info("Only markdown files have a preview");
        }
        self.sync_windows();
        let area = self.region();
        let Some((_, r)) = self.window_rects(area).into_iter().find(|(id, _)| *id == self.focus) else {
            return;
        };
        let across = r.width >= SIDE_BY_SIDE || r.height < MIN_ROWS * 5;
        let fits = if across {
            r.width > MIN_COLS * 2
        } else {
            r.height >= MIN_ROWS * 2
        };
        if !fits {
            return self.error("E36: Not enough room");
        }
        self.save_window();
        let Some(copy) = self.wins.iter().find(|w| w.id == self.focus).cloned() else {
            return;
        };
        let id = self.next_win;
        self.next_win += 1;
        self.wins.push(Win { id, ..copy });
        self.layout.split(area, self.focus, id, across, true);
        self.previews.insert(id, Preview::new(self.buffer_id, self.focus));
        self.fit_screen();
        self.sync_windows();
    }

    pub(crate) fn close_preview(&mut self, id: usize) {
        let Some(p) = self.previews.remove(&id) else {
            return;
        };
        if id == self.focus {
            self.close_window();
            self.focus_window(p.source);
        } else {
            self.layout.remove(id);
            self.wins.retain(|w| w.id != id);
            self.fit_screen();
        }
    }

    pub(crate) fn tab_focus(&mut self) {
        let preview = self
            .previews
            .iter()
            .find(|(_, p)| p.source == self.focus)
            .map(|(&id, _)| id);
        match preview.filter(|_| self.options.tab_preview) {
            Some(id) => self.focus_window(id),
            None => self.toggle_explorer_focus(),
        }
    }

    pub(crate) fn leave_preview(&mut self) {
        if let Some(source) = self.previews.get(&self.focus).map(|p| p.source) {
            self.focus_window(source);
        }
    }

    pub(crate) fn prune_previews(&mut self) {
        let wins = &self.wins;
        self.previews.retain(|id, _| wins.iter().any(|w| w.id == *id));
        let orphans: Vec<usize> = self
            .previews
            .iter()
            .filter(|(_, p)| !self.wins.iter().any(|w| w.id == p.source))
            .map(|(&id, _)| id)
            .collect();
        for id in orphans {
            self.close_preview(id);
        }
    }

    fn source_view(&self, p: &Preview) -> Option<(usize, usize)> {
        let top = if p.source == self.focus {
            (self.buffer_id == p.buffer).then(|| self.top.get())?
        } else {
            self.wins.iter().find(|w| w.id == p.source && w.buffer == p.buffer)?.top
        };
        let rows = self
            .window_rects(self.region())
            .into_iter()
            .find(|(id, _)| *id == p.source)
            .map_or(self.text_rows(), |(_, r)| r.height.saturating_sub(1));
        Some((top, rows))
    }

    fn rendered<'a>(&self, p: &'a Preview, width: usize) -> Ref<'a, Render> {
        let text = self.text_of(p.buffer);
        let key = (text.version, text.buf.len(), width, tinsel::color_mode());
        if p.render.borrow().key != Some(key) {
            let mut r = p.render.borrow_mut();
            r.build(text.buf.lines(), width);
            r.key = Some(key);
        }
        p.render.borrow()
    }

    fn preview_top(&self, p: &Preview, width: usize, rows: usize) -> usize {
        let r = self.rendered(p, width);
        if let Some((top, shown)) = self.source_view(p) {
            let text = self.text_of(p.buffer);
            let at = (text.version, top);
            if p.seen.replace(Some(at)) != Some(at) {
                p.pinned.set(false);
            }
            if !p.pinned.get() {
                let end = top + shown >= text.buf.len();
                p.top.set(if end { usize::MAX } else { r.line_for(top) });
            }
        }
        let top = p.top.get().min(r.lines.len().saturating_sub(rows));
        p.top.set(top);
        top
    }

    pub(crate) fn preview_lines(&self, id: usize, width: usize, rows: usize) -> Option<(Ref<'_, [String]>, usize)> {
        let p = self.previews.get(&id)?;
        p.size.set(Some((width, rows)));
        let top = self.preview_top(p, width, rows);
        Some((Ref::map(self.rendered(p, width), |r| r.lines.as_slice()), top))
    }

    fn scroll_preview(&mut self, to: impl FnOnce(usize, usize) -> usize) {
        let Some(p) = self.previews.get(&self.focus) else {
            return;
        };
        let (width, rows) = p.size.get().unwrap_or_else(|| {
            let r = self
                .window_rects(self.region())
                .into_iter()
                .find(|(id, _)| *id == self.focus);
            r.map_or((80, 20), |(_, r)| {
                (r.width.saturating_sub(1), r.height.saturating_sub(1))
            })
        });
        let rows = rows.max(1);
        let top = self.preview_top(p, width, rows);
        p.top.set(to(top, rows));
        p.pinned.set(true);
    }

    pub(crate) fn preview_input(&mut self, key: Key) {
        if self.pending.is_empty() {
            if key != Key::Char(':') {
                self.message = None;
            }
            if matches!(key, Key::Char('q') | Key::Esc) {
                return self.close_preview(self.focus);
            }
            if key == Key::Tab {
                self.leave_preview();
                if self.explorer().is_some() {
                    self.toggle_explorer_focus();
                }
                return;
            }
        } else if matches!(key, Key::Esc | Key::Ctrl('c')) {
            return self.pending.clear();
        }
        self.pending.push(key);
        let cmd = match parse(&self.pending, false) {
            Parse::More => return,
            Parse::Invalid => return self.pending.clear(),
            Parse::Done(cmd) => cmd,
        };
        self.pending.clear();
        let n = cmd.n();
        let half = |rows: usize| cmd.count.unwrap_or(rows / 2).max(1);
        match cmd.act {
            Act::Move(Motion::Down) | Act::Key(Key::Ctrl('e')) => self.scroll_preview(|top, _| top + n),
            Act::Move(Motion::Up) | Act::Key(Key::Ctrl('y')) => self.scroll_preview(|top, _| top.saturating_sub(n)),
            Act::Move(Motion::FirstLine) => self.scroll_preview(|_, _| 0),
            Act::Move(Motion::LastLine) => self.scroll_preview(|_, _| usize::MAX),
            Act::Key(Key::Ctrl('d')) => self.scroll_preview(|top, rows| top + half(rows)),
            Act::Key(Key::Ctrl('u')) => self.scroll_preview(|top, rows| top.saturating_sub(half(rows))),
            Act::Key(Key::Ctrl('f') | Key::PageDown) => self.scroll_preview(|top, rows| top + rows * n),
            Act::Key(Key::Ctrl('b') | Key::PageUp) => {
                self.scroll_preview(|top, rows| top.saturating_sub(rows * n));
            }
            Act::Key(Key::Char(':')) => self.begin_cmdline(':', String::new()),
            Act::Window(k) => self.window_key(k, cmd.count),
            Act::Leader(c) => self.leader(c),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(s: &str) -> Vec<String> {
        s.lines().map(String::from).collect()
    }

    #[test]
    fn blocks_split_at_blanks_and_headings_but_not_in_fences() {
        let src = lines("# A\npara\nmore\n\n```\ncode\n\n# not\n```\n## B\n- x\n- y");
        assert_eq!(blocks(&src), vec![(0, 1), (1, 3), (4, 9), (9, 10), (10, 12)]);
    }

    #[test]
    fn rendering_by_block_matches_the_whole() {
        let src = "# Title\nSome *text* here.\n\n- one\n- two\n\n1. a\n\n2. b\n\n```rust\nfn x() {}\n\n```\n> quote\n## Sub\nend";
        tinsel::with_color_mode(ColorMode::None, || {
            let mut r = Render::default();
            r.build(&lines(src), 40);
            assert_eq!(r.lines, tinsel::markdown::wrapped(src, 40).into_lines());
        });
    }

    #[test]
    fn source_rows_map_into_their_block() {
        let src = lines("# One\n\nalpha\n\n# Two\n\nbeta");
        let mut r = Render::default();
        r.build(&src, 40);
        let at = |row| r.lines[r.line_for(row)].trim().to_string();
        assert!(at(0).contains("One"));
        assert!(at(2).contains("alpha"));
        assert!(at(4).contains("Two"));
        assert!(at(6).contains("beta"));
    }
}
