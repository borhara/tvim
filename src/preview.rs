use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use tinsel::ColorMode;

use crate::editor::{Editor, Text};
use crate::syntax::Syntax;
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

}
