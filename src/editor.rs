use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::buffer::{Buffer, Pos};
use crate::clipboard::{Clipboard, System};
use crate::motion::Find;
use crate::syntax;
use crate::text::{char_count, col_at, first_non_blank, vcol};
use crate::window::Node;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Insert,
    Replace,
    Visual,
    VisualLine,
    Command,
    Search,
}

impl Mode {
    pub fn name(self) -> &'static str {
        match self {
            Mode::Normal | Mode::Command | Mode::Search => "NORMAL",
            Mode::Insert => "INSERT",
            Mode::Replace => "REPLACE",
            Mode::Visual => "VISUAL",
            Mode::VisualLine => "V-LINE",
        }
    }

    pub fn is_visual(self) -> bool {
        matches!(self, Mode::Visual | Mode::VisualLine)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub text: String,
    pub error: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options {
    pub number: bool,
    pub relative_number: bool,
    pub expand_tab: bool,
    pub tab_stop: usize,
    pub shift_width: usize,
    pub ignore_case: bool,
    pub smart_case: bool,
    pub highlight_search: bool,
    pub incremental_search: bool,
    pub auto_indent: bool,
    pub scroll_off: usize,
    pub syntax: bool,
    pub cursor_line: bool,
    pub indent_guides: bool,
    pub auto_pairs: bool,
    pub smooth: bool,
    pub tab_preview: bool,
    pub dim: bool,
    pub gitlens: bool,
    pub roomy: bool,
    pub zero_pad: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            number: true,
            relative_number: false,
            expand_tab: true,
            tab_stop: 4,
            shift_width: 4,
            ignore_case: true,
            smart_case: true,
            highlight_search: true,
            incremental_search: true,
            auto_indent: true,
            scroll_off: 3,
            syntax: true,
            cursor_line: true,
            indent_guides: true,
            auto_pairs: true,
            smooth: true,
            tab_preview: false,
            dim: true,
            gitlens: true,
            roomy: true,
            zero_pad: false,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileInfo {
    pub path: Option<PathBuf>,
    pub crlf: bool,
    pub final_newline: bool,
    pub new: bool,
}

#[derive(Clone, Debug, Default)]
struct Snapshot {
    lines: Vec<String>,
    cursor: Pos,
    version: u64,
}

const MAX_UNDO: usize = 1000;

pub struct Editor {
    pub(crate) buf: Buffer,
    pub(crate) cursor: Pos,
    pub(crate) want: usize,
    pub(crate) mode: Mode,
    pub(crate) anchor: Pos,
    last_visual: Option<(Pos, Pos, Mode)>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    version: u64,
    saved: u64,
    versions: u64,
    changing: bool,
    cmd_history: Vec<String>,
    search_history: Vec<String>,
    pub(crate) last_search: Option<String>,
    pub(crate) last_substitute: Option<(String, String)>,
    search_backward: bool,
    pub(crate) search_shown: bool,
    pub(crate) message: Option<Message>,
    last_find: Option<Find>,
    marks: HashMap<char, Pos>,
    replaying: bool,
    insert_replay: bool,
    last_macro: Option<char>,
    macro_depth: usize,
    pub(crate) file: FileInfo,
    pub options: Options,
    pub(crate) top: Cell<usize>,
    pub(crate) left: Cell<usize>,
    pub(crate) screen: Cell<(usize, usize)>,
    zen: Option<bool>,
    quit: bool,
    clipboard: Box<dyn Clipboard>,
    explorer_focused: bool,
    saved_lines: Vec<String>,
    git_history: Vec<String>,
    pub(crate) no_jumps: bool,
    current: usize,
    pub(crate) buffer_id: usize,
    next_id: usize,
    pub(crate) layout: Node,
    pub(crate) focus: usize,
    pub(crate) next_win: usize,
    // for shifting other windows on this buffer
    pub(crate) shadow: Option<(usize, Vec<String>)>,
    pub(crate) region: Cell<Option<(usize, usize)>>,
}

impl Default for Editor {
    fn default() -> Self {
        Editor::new()
    }
}

impl Editor {
    pub fn new() -> Editor {
        Editor {
            buf: Buffer::new(),
            cursor: Pos::default(),
            want: 0,
            mode: Mode::Normal,
            pending: Vec::new(),
            anchor: Pos::default(),
            last_visual: None,
            registers: HashMap::new(),
            undo: Vec::new(),
            redo: Vec::new(),
            version: 0,
            saved: 0,
            versions: 0,
            changing: false,
            cmd_history: Vec::new(),
            search_history: Vec::new(),
            last_search: None,
            last_substitute: None,
            search_backward: false,
            search_shown: false,
            message: None,
            last_find: None,
            marks: HashMap::new(),
            dot: None,
            building: None,
            replaying: false,
            insert_replay: false,
            completion: None,
            recording: None,
            macros: HashMap::new(),
            last_macro: None,
            macro_depth: 0,
            file: FileInfo {
                final_newline: true,
                ..FileInfo::default()
            },
            options: Options::default(),
            top: Cell::new(0),
            left: Cell::new(0),
            screen: Cell::new((80, 24)),
            anim: Default::default(),
            zen: None,
            effects: Vec::new(),
            quit: false,
            clipboard: Box::new(System),
            explorer: None,
            explorer_focused: false,
            finder: None,
            grep: None,
            saved_lines: vec![String::new()],
            git_menu: None,
            git_history: Vec::new(),
            states: RefCell::default(),
            marks_cache: RefCell::new(None),
            regex_cache: RefCell::new(None),
            hits_cache: RefCell::new(None),
            no_jumps: false,
            current: 0,
            buffer_id: 1,
            next_id: 2,
            recent: Vec::new(),
            outline: RefCell::new(None),
            layout: Node::Leaf(1),
            focus: 1,
            next_win: 2,
            shadow: None,
            region: Cell::new(None),
            previews: HashMap::new(),
        }
    }

    pub fn with_text(text: &str) -> Editor {
        let mut e = Editor::new();
        e.buf = Buffer::from_text(text);
        e.saved_lines = e.buf.lines().to_vec();
        e
    }

    pub fn open(path: &Path) -> std::io::Result<Editor> {
        let mut e = Editor::new();
        e.load(path)?;
        Ok(e)
    }

    pub fn buffer(&self) -> &Buffer {
        &self.buf
    }

    pub fn text(&self) -> String {
        self.buf.text()
    }

    pub fn cursor(&self) -> Pos {
        self.cursor
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn message(&self) -> Option<&Message> {
        self.message.as_ref()
    }

    pub fn is_modified(&self) -> bool {
        self.version != self.saved
    }

    pub fn should_quit(&self) -> bool {
        self.quit
    }

    pub fn file(&self) -> &FileInfo {
        &self.file
    }

    pub fn name(&self) -> String {
        display_name(self.file.path.as_deref())
    }

    pub fn clear_message(&mut self) {
        self.message = None;
    }

    pub(crate) fn clamp(&mut self) {
        self.cursor.row = self.cursor.row.min(self.buf.last_row());
        let max = self.max_col(self.buf.line_len(self.cursor.row));
        self.cursor.col = self.cursor.col.min(max);
    }

    fn max_col(&self, len: usize) -> usize {
        match self.mode {
            Mode::Insert | Mode::Replace => len,
            _ => len.saturating_sub(1),
        }
    }

    pub(crate) fn touch(&mut self) {
        if !self.changing {
            self.undo.push(self.snapshot());
            if self.undo.len() > MAX_UNDO {
                self.undo.remove(0);
            }
            self.redo.clear();
            self.changing = true;
        }
        self.versions += 1;
        self.version = self.versions;
    }

    pub(crate) fn end_change(&mut self) {
        if self.changing {
            self.changing = false;
            if self.undo.last().is_some_and(|s| s.lines == self.buf.lines()) {
                let s = self.undo.pop().expect("just checked");
                self.version = s.version;
            }
        }
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            lines: self.buf.lines().to_vec(),
            cursor: self.cursor,
            version: self.version,
        }
    }

    fn restore(&mut self, s: Snapshot) {
        let changed = (0..self.buf.len().max(s.lines.len()))
            .find(|&r| self.buf.lines().get(r) != s.lines.get(r))
            .unwrap_or(0);
        self.buf.replace_all(s.lines);
        self.version = s.version;
        let row = changed.min(self.buf.last_row());
        self.cursor = if s.cursor.row == row {
            s.cursor
        } else {
            self.first_non_blank_pos(row)
        };
        self.mode = Mode::Normal;
    }

    pub(crate) fn info(&mut self, text: impl Into<String>) {
        self.message = Some(Message {
            text: text.into(),
            error: false,
        });
    }

    pub(crate) fn error(&mut self, text: impl Into<String>) {
        self.message = Some(Message {
            text: text.into(),
            error: true,
        });
    }

    pub(crate) fn first_non_blank_pos(&self, row: usize) -> Pos {
        Pos::new(row, first_non_blank(self.buf.line(row)))
    }

    fn clamp_pos(&self, p: Pos) -> Pos {
        let row = p.row.min(self.buf.last_row());
        Pos::new(row, p.col.min(self.buf.line_len(row).saturating_sub(1)))
    }

    fn set_want(&mut self) {
        self.want = vcol(self.buf.line(self.cursor.row), self.cursor.col, self.options.tab_stop);
    }

    fn col_for_want(&self, row: usize) -> usize {
        let line = self.buf.line(row);
        let max = self.max_col(char_count(line));
        if self.want == usize::MAX {
            return max;
        }
        col_at(line, self.want, self.options.tab_stop).min(max)
    }

    pub(crate) fn set_quit(&mut self) {
        self.quit = true;
    }

    pub(crate) fn set_cursor(&mut self, p: Pos) {
        self.cursor = p;
        self.set_want();
    }

}

fn display_name(path: Option<&Path>) -> String {
    path.map_or_else(|| "[No Name]".to_string(), |p| p.display().to_string())
}
