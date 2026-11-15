use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use tinsel::ColorMode;

use crate::editor::Editor;

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

impl Editor {
    pub fn preview_focused(&self) -> bool {
        self.previews.contains_key(&self.focus)
    }

    pub(crate) fn is_preview(&self, id: usize) -> bool {
        self.previews.contains_key(&id)
    }

}
