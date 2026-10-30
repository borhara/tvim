use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use tinsel::app::Key;

use crate::buffer::{Buffer, Pos};
use crate::clipboard::{Clipboard, System};
use crate::command::{Act, Cmd, Motion, Op, Parse, Target, parse};
use crate::diff::Hunk;
use crate::motion::{self, Find};
use crate::object::{Span, object};
use crate::syntax;
use crate::text::{char_count, class, col_at, first_non_blank, indent, is_word, vcol};
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
pub enum Input {
    Key(Key),
    Paste(String),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Register {
    pub text: String,
    pub linewise: bool,
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

#[derive(Clone, Debug, Default)]
struct Dot {
    count: Option<usize>,
    register: Option<char>,
    inputs: Vec<Input>,
}

#[derive(Clone, Debug, Default)]
struct InsertSession {
    count: usize,
    open: Option<bool>,
    typed: Vec<Input>,
    replaced: Vec<Option<char>>,
    auto_indent: Option<usize>,
    register_next: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Exclusive,
    Inclusive,
    Linewise,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Range {
    pub start: Pos,
    pub end: Pos,
    pub linewise: bool,
}

impl Range {
    pub fn lines(first: usize, last: usize) -> Self {
        Range {
            start: Pos::new(first, 0),
            end: Pos::new(last, 0),
            linewise: true,
        }
    }

    // a charwise range ending at column 0 takes nothing from that line
    pub fn last_row(&self) -> usize {
        if !self.linewise && self.end.col == 0 && self.end.row > self.start.row {
            self.end.row - 1
        } else {
            self.end.row
        }
    }
}

const MAX_UNDO: usize = 1000;

pub struct Editor {
    pub(crate) buf: Buffer,
    pub(crate) cursor: Pos,
    pub(crate) want: usize,
    pub(crate) mode: Mode,
    pub(crate) pending: Vec<Key>,
    pub(crate) anchor: Pos,
    last_visual: Option<(Pos, Pos, Mode)>,
    registers: HashMap<char, Register>,
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
    dot: Option<Dot>,
    building: Option<Dot>,
    replaying: bool,
    insert_replay: bool,
    insert: InsertSession,
    recording: Option<(char, Vec<Input>)>,
    macros: HashMap<char, Vec<Input>>,
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
            insert: InsertSession::default(),
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

    pub fn set_clipboard(&mut self, clipboard: Box<dyn Clipboard>) {
        self.clipboard = clipboard;
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

    pub fn pending(&self) -> &[Key] {
        &self.pending
    }

    pub fn is_modified(&self) -> bool {
        self.version != self.saved
    }

    pub fn should_quit(&self) -> bool {
        self.quit
    }

    pub fn recording(&self) -> Option<char> {
        self.recording.as_ref().map(|r| r.0)
    }

    pub fn file(&self) -> &FileInfo {
        &self.file
    }

    pub fn register(&self, name: char) -> Option<&Register> {
        self.registers.get(&name.to_ascii_lowercase())
    }

    pub fn name(&self) -> String {
        display_name(self.file.path.as_deref())
    }

    pub fn selection(&self) -> Option<Range> {
        self.mode.is_visual().then(|| self.visual_range())
    }

    pub fn clear_message(&mut self) {
        self.message = None;
    }

    pub fn set_screen(&self, width: usize, height: usize) {
        self.screen.set((width, height));
    }

    pub fn text_rows(&self) -> usize {
        self.screen.get().1.saturating_sub(2).max(1)
    }

    pub fn gutter(&self) -> usize {
        self.gutter_for(self.buf.len())
    }

    pub(crate) fn gutter_for(&self, lines: usize) -> usize {
        if self.zen() || !(self.options.number || self.options.relative_number) {
            return 0;
        }
        lines.checked_ilog10().map_or(1, |d| d as usize + 1).max(3) + 4
    }

    pub fn text_cols(&self) -> usize {
        self.screen.get().0.saturating_sub(self.gutter() + 1).max(1)
    }

    pub fn top(&self) -> usize {
        self.top.get()
    }

    pub fn left(&self) -> usize {
        self.left.get()
    }

    fn scroll_off(&self) -> usize {
        self.options.scroll_off.min(self.text_rows().saturating_sub(1) / 2)
    }

    pub fn scroll(&self) {
        let rows = self.text_rows();
        let so = self.scroll_off();
        let row = self.cursor.row;
        let mut top = self.top.get().min(self.buf.last_row());
        if row < top + so {
            top = row.saturating_sub(so);
        }
        let below = (row + so).min(self.buf.last_row());
        if below >= top + rows {
            top = below + 1 - rows;
        }
        self.top.set(top);

        let cols = self.text_cols();
        let line = self.buf.line(row);
        let start = vcol(line, self.cursor.col, self.options.tab_stop);
        let end = vcol(line, self.cursor.col + 1, self.options.tab_stop).max(start + 1);
        let mut left = self.left.get();
        if start < left {
            left = start;
        }
        if end > left + cols {
            left = end - cols;
        }
        self.left.set(left);
    }

    pub fn keys(&mut self, keys: &[Key]) {
        for &k in keys {
            self.key(k);
        }
    }

    pub fn key(&mut self, key: Key) {
        self.input(Input::Key(key));
    }

    fn input(&mut self, input: Input) {
        if self.git_menu.is_some() {
        }
        if self.finder.is_some() {
        }
        if self.grep.is_some() {
        }
        if self.mode == Mode::Normal && self.previews.contains_key(&self.focus) {
            if let Input::Key(k) = input {
            }
            return;
        }
        if let Some((_, keys)) = &mut self.recording {
            if !self.replaying && self.macro_depth == 0 && !self.insert_replay {
                keys.push(input.clone());
            }
        }
        match self.mode {
            Mode::Normal | Mode::Visual | Mode::VisualLine => self.normal(input),
            Mode::Insert | Mode::Replace => self.insert_input(input),
        }
        if matches!(self.mode, Mode::Normal | Mode::Visual | Mode::VisualLine) && self.pending.is_empty() {
            self.end_change();
        }
        self.clamp();
        // a preview sent to another buffer by :e is an ordinary window now
        if self
            .previews
            .get(&self.focus)
            .is_some_and(|p| p.buffer != self.buffer_id)
        {
            self.previews.remove(&self.focus);
        }
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

    fn undo(&mut self, count: usize) {
        for n in 0..count {
            let Some(s) = self.undo.pop() else {
                if n == 0 {
                    self.info("Already at oldest change");
                }
                return;
            };
            let now = self.snapshot();
            self.redo.push(now);
            self.restore(s);
        }
    }

    fn redo(&mut self, count: usize) {
        for n in 0..count {
            let Some(s) = self.redo.pop() else {
                if n == 0 {
                    self.info("Already at newest change");
                }
                return;
            };
            let now = self.snapshot();
            self.undo.push(now);
            self.restore(s);
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

    fn store(&mut self, register: Option<char>, text: String, linewise: bool, yank: bool) {
        let r = Register { text, linewise };
        // @ plays the new text, not an older recording
        if let Some(c) = register.filter(char::is_ascii_alphabetic) {
            self.macros.remove(&c.to_ascii_lowercase());
        }
        match register {
            Some('_') => return,
            Some(c @ ('+' | '*')) => {
                let mut t = r.text.clone();
                if linewise {
                    t.push('\n');
                }
                if !self.clipboard.set(&t) {
                    self.error(format!("E354: No clipboard to copy to (register {c})"));
                }
            }
            Some(c) if c.is_ascii_uppercase() => {
                let entry = self.registers.entry(c.to_ascii_lowercase()).or_default();
                if entry.linewise || linewise && !entry.text.is_empty() {
                    entry.text.push('\n');
                }
                entry.text.push_str(&r.text);
                entry.linewise |= linewise;
                let all = entry.clone();
                self.registers.insert('"', all);
                return;
            }
            Some(c) if c.is_ascii_lowercase() => {
                self.registers.insert(c, r.clone());
            }
            _ => {}
        }
        self.registers.insert(if yank { '0' } else { '1' }, r.clone());
        if !yank && !linewise && !r.text.contains('\n') {
            self.registers.insert('-', r.clone());
        }
        self.registers.insert('"', r);
    }

    fn fetch(&mut self, register: Option<char>) -> Option<Register> {
        match register.unwrap_or('"') {
            c @ ('+' | '*') => {
                let Some(text) = self.clipboard.get() else {
                    self.error(format!("E353: Nothing in register {c}"));
                    return None;
                };
                let linewise = text.ends_with('\n');
                Some(Register {
                    text: text.strip_suffix('\n').unwrap_or(&text).to_string(),
                    linewise,
                })
            }
            '/' => self.last_search.clone().map(|text| Register { text, linewise: false }),
            c => {
                let r = self.registers.get(&c.to_ascii_lowercase()).cloned();
                if r.is_none() {
                    self.error(format!("E353: Nothing in register {c}"));
                }
                r
            }
        }
    }

    fn normal(&mut self, input: Input) {
        let key = match input {
            Input::Paste(text) => {
                self.pending.clear();
                if !self.mode.is_visual() {
                }
                return;
            }
            Input::Key(k) => k,
        };
        if self.pending.is_empty() && !matches!(key, Key::Char(':' | '/' | '?')) {
            self.message = None;
        }
        if key == Key::Char('q') && self.pending.is_empty() && self.recording.is_some() {
            self.stop_recording();
            return;
        }
        if matches!(key, Key::Esc | Key::Ctrl('c')) {
            let had = !self.pending.is_empty();
            self.pending.clear();
            if self.mode.is_visual() {
                self.leave_visual();
            } else if key == Key::Ctrl('c') && !had {
                self.info("Type  :qa!  and press Enter to abandon all changes and exit tvim");
            }
            return;
        }
        self.pending.push(key);
        match parse(&self.pending, self.mode.is_visual()) {
            Parse::More => {}
            Parse::Invalid => self.pending.clear(),
            Parse::Done(cmd) => {
                self.pending.clear();
                if self.mode.is_visual() {
                    self.run_visual(cmd);
                } else {
                    self.run(cmd);
                }
            }
        }
    }

    fn is_change(cmd: &Cmd) -> bool {
        match cmd.act {
            Act::Operate(op, _) => op != Op::Yank,
            Act::Key(Key::Char(c)) => "xXsSDCpPJ~&iaIAoOR".contains(c),
            Act::Key(Key::Ctrl('a' | 'x')) | Act::Key(Key::Delete) => true,
            Act::With('r', _) | Act::G('J' | 'p' | 'P' | 'i') | Act::Hunk('r') => true,
            _ => false,
        }
    }

    fn run(&mut self, cmd: Cmd) {
        let change = Self::is_change(&cmd) && !self.replaying;
        if change {
            self.building = Some(Dot {
                count: cmd.count,
                register: cmd.register,
                inputs: cmd.keys.iter().map(|&k| Input::Key(k)).collect(),
            });
        }
        let n = cmd.n();
        match cmd.act {
            Act::Move(m) => self.move_cursor(m, cmd.count),
            Act::Operate(op, target) => self.operate(op, target, cmd.count, cmd.register),
            Act::Key(k) => self.normal_key(k, cmd.count, cmd.register),
            Act::With(c, ch) => self.normal_with(c, ch, n),
            Act::G('t') => match cmd.count {
            },
            Act::G(c) => self.normal_g(c, n, cmd.register),
            Act::Z(k) => self.normal_z(k, cmd.count),
            Act::Select { .. } => {}
        }
        if change && !matches!(self.mode, Mode::Insert | Mode::Replace) {
            self.dot = self.building.take();
        }
    }

    fn normal_key(&mut self, key: Key, count: Option<usize>, register: Option<char>) {
        let n = count.unwrap_or(1);
        let op = |e: &mut Self, op: Op, m: Motion| e.operate(op, Target::Motion(m), count, register);
        match key {
            Key::Char('x') | Key::Delete => {
                if self.buf.line_len(self.cursor.row) > 0 {
                    op(self, Op::Delete, Motion::Right)
                }
            }
            Key::Char('X') => op(self, Op::Delete, Motion::Left),
            Key::Char('s') => {
                if self.buf.line_len(self.cursor.row) == 0 {
                    self.begin_insert(1, None);
                } else {
                    op(self, Op::Change, Motion::Right)
                }
            }
            Key::Char('S') => self.operate(Op::Change, Target::Lines, count, register),
            Key::Char('D') => op(self, Op::Delete, Motion::LineEnd),
            Key::Char('C') => op(self, Op::Change, Motion::LineEnd),
            Key::Char('Y') => op(self, Op::Yank, Motion::LineEnd),
            Key::Char('p') => self.put(register, n, false),
            Key::Char('P') => self.put(register, n, true),
            Key::Char('J') => self.join(self.cursor.row, n.max(2), true),
            Key::Char('~') => self.toggle_case_chars(n),
            Key::Char('u' | 'U') => self.undo(n),
            Key::Ctrl('r') => self.redo(n),
            Key::Char('.') => self.repeat(count),
            Key::Char('i') => self.begin_insert(n, None),
            Key::Char('a') => {
                if self.buf.line_len(self.cursor.row) > 0 {
                    self.cursor.col += 1;
                }
                self.begin_insert(n, None);
            }
            Key::Char('I') => {
                self.cursor = self.first_non_blank_pos(self.cursor.row);
                self.begin_insert(n, None);
            }
            Key::Char('A') => {
                self.cursor.col = self.buf.line_len(self.cursor.row);
                self.begin_insert(n, None);
            }
            Key::Char('o') => self.open_line(false, n),
            Key::Char('O') => self.open_line(true, n),
            Key::Char('R') => {
                self.begin_insert(n, None);
                self.mode = Mode::Replace;
            }
            Key::Char('v') => self.enter_visual(Mode::Visual),
            Key::Char('V') => self.enter_visual(Mode::VisualLine),
            Key::Char('&') => self.ex("s"),
            Key::Ctrl('a') => self.increment(n as i64),
            Key::Ctrl('x') => self.increment(-(n as i64)),
            Key::Ctrl('d') => self.scroll_half(true, count),
            Key::Ctrl('u') => self.scroll_half(false, count),
            Key::Ctrl('f') | Key::PageDown => self.scroll_page(true, n),
            Key::Ctrl('b') | Key::PageUp => self.scroll_page(false, n),
            Key::Ctrl('e') => self.scroll_lines(n as isize),
            Key::Ctrl('y') => self.scroll_lines(-(n as isize)),
            Key::Ctrl('g') => self.file_info(),
            Key::Ctrl('l') => self.message = None,
            _ => {}
        }
    }

    fn normal_with(&mut self, c: char, ch: char, n: usize) {
        match c {
            'r' => self.replace_chars(ch, n),
            'm' => {
                if ch.is_ascii_lowercase() || ch == '\'' || ch == '`' {
                    self.marks.insert(if ch == '`' { '\'' } else { ch }, self.cursor);
                }
            }
            'q' => {
                if ch.is_ascii_alphanumeric() || ch == '"' {
                    self.recording = Some((ch, Vec::new()));
                }
            }
            'Z' if ch == 'Z' => self.ex("x"),
            'Z' => self.ex("q!"),
            _ => {}
        }
    }

    fn normal_g(&mut self, c: char, n: usize, register: Option<char>) {
        match c {
            'v' => {
                if let Some((anchor, cursor, mode)) = self.last_visual {
                    self.anchor = self.clamp_pos(anchor);
                    self.cursor = self.clamp_pos(cursor);
                    self.mode = mode;
                }
            }
            'J' => self.join(self.cursor.row, n.max(2), false),
            'p' | 'P' => {
                let Some(r) = self.fetch(register) else { return };
                let end = self.put_register(&r, n, c == 'P');
                self.cursor = if r.linewise {
                    Pos::new((end.row + 1).min(self.buf.last_row()), 0)
                } else {
                    end
                };
                self.set_want();
            }
            'i' => {
                if let Some(&p) = self.marks.get(&'^') {
                    let row = p.row.min(self.buf.last_row());
                    self.cursor = Pos::new(row, p.col.min(self.buf.line_len(row)));
                }
                self.begin_insert(n, None);
            }
            _ => {}
        }
    }

    fn normal_z(&mut self, key: Key, count: Option<usize>) {
        if let Some(n) = count {
            self.cursor.row = (n - 1).min(self.buf.last_row());
        }
        let rows = self.text_rows();
        let so = self.scroll_off();
        let row = self.cursor.row;
        let top = match key {
            Key::Char('z' | '.') => row.saturating_sub(rows / 2),
            Key::Char('t') | Key::Enter => row.saturating_sub(so),
            _ => (row + so + 1).saturating_sub(rows),
        };
        self.top.set(top);
        if matches!(key, Key::Enter | Key::Char('.' | '-')) {
            self.cursor = self.first_non_blank_pos(row);
        }
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

    fn move_cursor(&mut self, m: Motion, count: Option<usize>) {
        let Some((to, _)) = self.target(m, count, false) else {
            return;
        };
        if jump {
        }
        self.cursor = to;
        match m {
            Motion::Up | Motion::Down => {}
            Motion::LineEnd => self.want = usize::MAX,
            _ => self.set_want(),
        }
        if !self.mode.is_visual() {
            self.clamp();
        }
        if jump {
            self.land(matches!(m, Motion::SearchNext(_)));
        }
    }

    pub(crate) fn target(&mut self, m: Motion, count: Option<usize>, for_op: bool) -> Option<(Pos, Kind)> {
        use Kind::*;
        let n = count.unwrap_or(1);
        let p = self.cursor;
        let b = &self.buf;
        let len = b.line_len(p.row);
        let fnb = |row: usize| Pos::new(row, first_non_blank(b.line(row)));
        let repeat = |step: fn(&Buffer, Pos, bool) -> Option<Pos>, big: bool| {
            let mut at = p;
            for _ in 0..n {
                match step(b, at, big) {
                    Some(q) => at = q,
                    None => break,
                }
            }
            (at != p).then_some(at)
        };
        Some(match m {
        })
    }

    fn range_to(&self, to: Pos, kind: Kind) -> Range {
        let from = self.cursor;
        let (a, b) = if to < from { (to, from) } else { (from, to) };
        match kind {
            Kind::Linewise => Range::lines(a.row, b.row),
            Kind::Inclusive => Range {
                start: a,
                end: Pos::new(b.row, (b.col + 1).min(self.buf.line_len(b.row))),
                linewise: false,
            },
            Kind::Exclusive => {
                // vim's exclusive motion rules, see :h exclusive
                if b.col == 0 && b.row > a.row {
                    if a.col <= first_non_blank(self.buf.line(a.row)) {
                        return Range::lines(a.row, b.row - 1);
                    }
                    let row = b.row - 1;
                    return Range {
                        start: a,
                        end: Pos::new(row, self.buf.line_len(row)),
                        linewise: false,
                    };
                }
                Range {
                    start: a,
                    end: b,
                    linewise: false,
                }
            }
        }
    }

    fn operate(&mut self, op: Op, target: Target, count: Option<usize>, register: Option<char>) {
        let n = count.unwrap_or(1);
        let range = match target {
            Target::Lines => {
                let last = self.cursor.row + n - 1;
                if last > self.buf.last_row() && n > 1 && matches!(op, Op::Delete | Op::Change | Op::Yank) {
                    return;
                }
                Range::lines(self.cursor.row, last.min(self.buf.last_row()))
            }
            Target::Object { kind, inner } => {
                let Some(span) = object(&self.buf, self.cursor, kind, inner, n) else {
                    return;
                };
                Range {
                    start: span.start,
                    end: span.end,
                    linewise: span.linewise,
                }
            }
            Target::Motion(m) => {
                // cw acts like ce, see :h cw
                let m = match (op, m) {
                    (Op::Change, Motion::WordForward(big))
                        if self.buf.char_at(self.cursor).is_some_and(|c| !c.is_whitespace()) =>
                    {
                        return self.change_word(big, n, register);
                    }
                    _ => m,
                };
                let Some((to, kind)) = self.target(m, count, true) else {
                    return;
                };
                self.range_to(to, kind)
            }
        };
        // 3>> is three lines, not three shifts
        let times = if matches!(op, Op::Indent | Op::Dedent) { 1 } else { n };
        self.apply(op, range, register, times);
    }

    fn change_word(&mut self, big: bool, n: usize, register: Option<char>) {
        let p = self.cursor;
        let at_end = |e: &Self, q: Pos| {
            let c = motion::class_at(&e.buf, q, big);
            let next = motion::class_at(&e.buf, Pos::new(q.row, q.col + 1), big);
            next != c || next.is_none()
        };
        let mut end = p;
        for i in 0..n {
            if i == 0 && at_end(self, end) {
                continue;
            }
            match motion::word_end(&self.buf, end, big) {
                Some(q) => end = q,
                None => break,
            }
        }
        let range = Range {
            start: p,
            end: Pos::new(end.row, (end.col + 1).min(self.buf.line_len(end.row))),
            linewise: false,
        };
        self.apply(Op::Change, range, register, 1);
    }

    pub(crate) fn apply(&mut self, op: Op, r: Range, register: Option<char>, count: usize) {
        match op {
            Op::Yank => {
                let text = self.range_text(r);
                let lines = r.end.row - r.start.row + 1;
                self.store(register, text, r.linewise, true);
                if r.linewise {
                    if lines > 2 {
                        self.info(format!("{lines} lines yanked"));
                    }
                    self.cursor.row = r.start.row.min(self.cursor.row);
                } else {
                    self.cursor = r.start;
                }
            }
            Op::Delete => {
                if r.linewise {
                    let text = self.range_text(r);
                    self.store(register, text, true, false);
                    self.touch();
                    self.buf.delete_lines(r.start.row, r.end.row);
                    let row = r.start.row.min(self.buf.last_row());
                    self.cursor = self.first_non_blank_pos(row);
                    let gone = r.end.row - r.start.row + 1;
                    if gone > 2 {
                        self.info(format!("{gone} fewer lines"));
                    }
                } else {
                    if r.start >= r.end {
                        return;
                    }
                    let text = self.buf.slice(r.start, r.end);
                    self.store(register, text, false, false);
                    self.touch();
                    self.buf.delete(r.start, r.end);
                    self.cursor = r.start;
                }
                self.set_want();
            }
            Op::Change => {
                if r.linewise {
                    let text = self.range_text(r);
                    self.store(register, text, true, false);
                    self.touch();
                    let keep = if self.options.auto_indent {
                        indent(self.buf.line(r.start.row)).to_string()
                    } else {
                        String::new()
                    };
                    self.cursor = Pos::new(r.start.row, char_count(&keep));
                    self.buf.replace_lines(r.start.row, r.end.row, vec![keep]);
                } else {
                    let text = self.buf.slice(r.start, r.end);
                    self.store(register, text, false, false);
                    self.touch();
                    self.buf.delete(r.start, r.end);
                    self.cursor = r.start;
                }
                self.begin_insert(1, None);
            }
            Op::Indent | Op::Dedent => {
                self.touch();
                let times = count.max(1);
                for row in r.start.row..=r.last_row() {
                    let line = self.buf.line(row).to_string();
                    if line.is_empty() {
                        continue;
                    }
                    let width = vcol(&line, first_non_blank(&line), self.options.tab_stop);
                    let sw = self.options.shift_width.max(1);
                    let new = if op == Op::Indent {
                        width + sw * times
                    } else {
                        width.saturating_sub(sw * times)
                    };
                    let rest = line.trim_start_matches([' ', '\t']);
                    self.buf.set_line(row, format!("{}{rest}", self.make_indent(new)));
                }
                self.cursor = self.first_non_blank_pos(r.start.row);
                let lines = r.end.row - r.start.row + 1;
                if lines > 2 {
                    let sign = if op == Op::Indent { '>' } else { '<' };
                    self.info(format!(
                        "{lines} lines {sign}ed {times} time{}",
                        if times == 1 { "" } else { "s" }
                    ));
                }
            }
            Op::Lower | Op::Upper | Op::ToggleCase => {
                self.touch();
                let (from, to) = self.char_span(r);
                let text = self.buf.slice(from, to);
                let changed: String = text
                    .chars()
                    .map(|c| match op {
                        Op::Lower => single(c.to_lowercase()).unwrap_or(c),
                        Op::Upper => single(c.to_uppercase()).unwrap_or(c),
                        _ => toggle(c),
                    })
                    .collect();
                self.buf.delete(from, to);
                self.buf.insert(from, &changed);
                self.cursor = from;
            }
            Op::Comment => {
                let name = self
                    .file
                    .path
                    .as_ref()
                    .and_then(|p| p.file_name()?.to_str())
                    .unwrap_or("");
                let (open, close) = crate::comment::markers(name);
                let last = r.last_row().min(self.buf.last_row());
                let lines = crate::comment::toggle(&self.buf.lines()[r.start.row..=last], open, close);
                self.touch();
                for (row, line) in (r.start.row..).zip(lines) {
                    self.buf.set_line(row, line);
                }
                self.cursor = self.first_non_blank_pos(r.start.row);
            }
        }
    }

    fn char_span(&self, r: Range) -> (Pos, Pos) {
        if r.linewise {
            (Pos::new(r.start.row, 0), Pos::new(r.end.row, self.buf.line_len(r.end.row)))
        } else {
            (r.start, r.end)
        }
    }

    fn range_text(&self, r: Range) -> String {
        if r.linewise {
            self.buf.lines()[r.start.row..=r.end.row.min(self.buf.last_row())].join("\n")
        } else {
            self.buf.slice(r.start, r.end)
        }
    }

    fn make_indent(&self, width: usize) -> String {
        if self.options.expand_tab {
            " ".repeat(width)
        } else {
            let ts = self.options.tab_stop.max(1);
            format!("{}{}", "\t".repeat(width / ts), " ".repeat(width % ts))
        }
    }

    fn put(&mut self, register: Option<char>, count: usize, before: bool) {
        let Some(r) = self.fetch(register) else { return };
        let end = self.put_register(&r, count, before);
        if !r.linewise && !r.text.contains('\n') {
            self.cursor = Pos::new(end.row, end.col.saturating_sub(1));
        }
        self.set_want();
    }

    fn put_register(&mut self, r: &Register, count: usize, before: bool) -> Pos {
        self.touch();
        if r.linewise {
            let mut lines = Vec::new();
            for _ in 0..count {
                lines.extend(r.text.split('\n').map(str::to_string));
            }
            let added = lines.len();
            let at = if before { self.cursor.row } else { self.cursor.row + 1 };
            self.buf.insert_lines(at, lines);
            self.cursor = self.first_non_blank_pos(at);
            return Pos::new(at + added - 1, 0);
        }
        let text = r.text.repeat(count);
        let len = self.buf.line_len(self.cursor.row);
        let at = if before || len == 0 {
            self.cursor
        } else {
            Pos::new(self.cursor.row, (self.cursor.col + 1).min(len))
        };
        let end = self.buf.insert(at, &text);
        self.cursor = at;
        end
    }

    fn join(&mut self, row: usize, lines: usize, spaces: bool) {
        if row >= self.buf.last_row() {
            return;
        }
        self.touch();
        let last = (row + lines - 1).min(self.buf.last_row());
        let mut col = 0;
        for _ in row..last {
            let next = self.buf.delete_lines(row + 1, row + 1).remove(0);
            let mut line = self.buf.line(row).to_string();
            let next = if spaces { next.trim_start().to_string() } else { next };
            if spaces {
                if !next.is_empty() && !next.starts_with(')') && !line.ends_with([' ', '\t']) && !line.is_empty() {
                    line.push(' ');
                }
                col = char_count(&line).saturating_sub(usize::from(!next.is_empty() && line.ends_with(' ')));
            } else {
                col = char_count(&line);
            }
            line.push_str(&next);
            self.buf.set_line(row, line);
        }
        self.cursor = Pos::new(row, col);
    }

    fn toggle_case_chars(&mut self, n: usize) {
        let len = self.buf.line_len(self.cursor.row);
        if len == 0 {
            return;
        }
        let end = (self.cursor.col + n).min(len);
        let r = Range {
            start: self.cursor,
            end: Pos::new(self.cursor.row, end),
            linewise: false,
        };
        self.apply(Op::ToggleCase, r, None, 1);
        self.cursor.col = end.min(len - 1);
    }

    fn replace_chars(&mut self, ch: char, n: usize) {
        let p = self.cursor;
        let len = self.buf.line_len(p.row);
        if p.col + n > len {
            return;
        }
        self.touch();
        self.buf.delete(p, Pos::new(p.row, p.col + n));
        if ch == '\n' {
            self.buf.insert(p, "\n");
            self.cursor = Pos::new(p.row + 1, 0);
        } else {
            let text: String = std::iter::repeat_n(ch, n).collect();
            self.buf.insert(p, &text);
            self.cursor.col = p.col + n - 1;
        }
    }

    fn increment(&mut self, by: i64) {
        let line: Vec<char> = self.buf.line(self.cursor.row).chars().collect();
        let mut i = self.cursor.col.min(line.len());
        while i > 0 && i < line.len() && line[i].is_ascii_digit() && line[i - 1].is_ascii_digit() {
            i -= 1;
        }
        let Some(start) = (i..line.len()).find(|&j| line[j].is_ascii_digit()) else {
            return;
        };
        let mut end = start;
        while end < line.len() && line[end].is_ascii_digit() {
            end += 1;
        }
        let negative = start > 0 && line[start - 1] == '-';
        let from = if negative { start - 1 } else { start };
        let digits: String = line[start..end].iter().collect();
        let Ok(value) = digits.parse::<i64>() else { return };
        let value = if negative { -value } else { value };
        let new = value.saturating_add(by);
        let text = if digits.len() > 1 && digits.starts_with('0') {
            let width = digits.len();
            if new < 0 {
                format!("-{:0width$}", -new)
            } else {
                format!("{new:0width$}")
            }
        } else {
            new.to_string()
        };
        self.touch();
        let row = self.cursor.row;
        self.buf.delete(Pos::new(row, from), Pos::new(row, end));
        self.buf.insert(Pos::new(row, from), &text);
        self.cursor = Pos::new(row, from + char_count(&text) - 1);
    }

    fn repeat(&mut self, count: Option<usize>) {
        let Some(dot) = self.dot.clone() else { return };
        let mut inputs = Vec::new();
        if let Some(r) = dot.register {
            inputs.push(Input::Key(Key::Char('"')));
            inputs.push(Input::Key(Key::Char(r)));
        }
        if let Some(n) = count.or(dot.count) {
            inputs.extend(n.to_string().chars().map(|c| Input::Key(Key::Char(c))));
        }
        inputs.extend(dot.inputs.iter().cloned());
        self.replaying = true;
        self.end_change();
        for i in inputs {
            self.input(i);
        }
        if matches!(self.mode, Mode::Insert | Mode::Replace) {
            self.input(Input::Key(Key::Esc));
        }
        self.replaying = false;
        if let Some(d) = self.dot.as_mut() {
            if count.is_some() {
                d.count = count;
            }
        }
    }

    fn stop_recording(&mut self) {
        if let Some((name, mut keys)) = self.recording.take() {
            // drop the q that ended the recording
            keys.pop();
            let text: String = keys
                .iter()
                .map(|i| match i {
                    Input::Key(k) => crate::keys::notation(*k),
                    Input::Paste(t) => t.clone(),
                })
                .collect();
            // qA adds to what a already holds
            let (mut text, mut keys) = (text, keys);
            if name.is_ascii_uppercase() {
                let lower = name.to_ascii_lowercase();
                let old = self.registers.get(&lower).map_or(String::new(), |r| r.text.clone());
                let mut old_keys = self.macros.get(&lower).cloned().unwrap_or_else(|| {
                    crate::keys::parse(&old).into_iter().map(Input::Key).collect()
                });
                old_keys.append(&mut keys);
                keys = old_keys;
                text = old + &text;
            }
            let name = name.to_ascii_lowercase();
            if name.is_ascii_alphabetic() {
                self.registers.insert(name, Register { text, linewise: false });
            }
            self.macros.insert(name, keys);
        }
    }

    fn begin_insert(&mut self, count: usize, open: Option<bool>) {
        self.set_want();
        self.mode = Mode::Insert;
        self.insert = InsertSession {
            count,
            open,
            ..InsertSession::default()
        };
    }

    fn open_line(&mut self, above: bool, count: usize) {
        self.touch();
        self.open_one(above);
        self.begin_insert(count, Some(above));
        self.insert.auto_indent = Some(self.cursor.row);
    }

    fn open_one(&mut self, above: bool) {
        let line = self.buf.line(self.cursor.row).to_string();
        let mut ind = if self.options.auto_indent {
            indent(&line).to_string()
        } else {
            String::new()
        };
        let deeper = if above {
            line.trim_start().starts_with(['}', ']', ')'])
        } else {
            line.trim_end().ends_with(['{', '[', '('])
        };
        if self.options.auto_indent && deeper {
            let width = vcol(&ind, char_count(&ind), self.options.tab_stop);
            ind = self.make_indent(width + self.options.shift_width);
        }
        let row = if above { self.cursor.row } else { self.cursor.row + 1 };
        self.buf.insert_lines(row, vec![ind.clone()]);
        self.cursor = Pos::new(row, char_count(&ind));
    }

    fn insert_input(&mut self, input: Input) {
        if let Some(b) = self.building.as_mut() {
            b.inputs.push(input.clone());
        }
        if !matches!(input, Input::Key(Key::Esc | Key::Ctrl('c'))) {
            self.insert.typed.push(input.clone());
        }
        let key = match input {
            Input::Paste(text) => {
                self.touch();
                self.insert.auto_indent = None;
                self.cursor = self.buf.insert(self.cursor, &text);
                return;
            }
            Input::Key(k) => k,
        };
        if self.insert.register_next {
            self.insert.register_next = false;
            if let Key::Char(c) = key {
                if let Some(r) = self.fetch(Some(c)) {
                    let mut text = r.text;
                    if r.linewise {
                        text.push('\n');
                    }
                    self.touch();
                    self.cursor = self.buf.insert(self.cursor, &text);
                }
            }
            return;
        }
        match key {
            Key::Esc | Key::Ctrl('c') => self.leave_insert(),
            Key::Char(c) => self.type_paired(c),
            Key::Tab => {
                if self.options.expand_tab {
                    let v = vcol(self.buf.line(self.cursor.row), self.cursor.col, self.options.tab_stop);
                    let sw = self.options.shift_width.max(1);
                    let spaces = " ".repeat(sw - v % sw);
                    self.type_str(&spaces);
                } else {
                    self.type_char('\t');
                }
            }
            Key::Enter | Key::Ctrl('j') | Key::Ctrl('m') => self.newline(),
            Key::Backspace | Key::Ctrl('h') => self.backspace(),
            Key::Delete => {
                let p = self.cursor;
                let len = self.buf.line_len(p.row);
                if p.col < len {
                    self.touch();
                    self.buf.delete(p, Pos::new(p.row, p.col + 1));
                } else if p.row < self.buf.last_row() {
                    self.touch();
                    self.buf.delete(p, Pos::new(p.row + 1, 0));
                }
            }
            Key::Ctrl('w') => self.delete_word_before(),
            Key::Ctrl('u') => {
                let p = self.cursor;
                if p.col == 0 {
                    return self.backspace();
                }
                let fnb = first_non_blank(self.buf.line(p.row));
                let from = if p.col > fnb { fnb } else { 0 };
                self.touch();
                self.buf.delete(Pos::new(p.row, from), p);
                self.cursor.col = from;
            }
            Key::Ctrl('r') => self.insert.register_next = true,
            Key::Ctrl('t') | Key::Ctrl('d') => {
                let row = self.cursor.row;
                let before = self.buf.line_len(row);
                let r = Range::lines(row, row);
                let col = self.cursor.col;
                let op = if key == Key::Ctrl('t') { Op::Indent } else { Op::Dedent };
                if self.buf.line(row).is_empty() && op == Op::Indent {
                    self.touch();
                    let ind = self.make_indent(self.options.shift_width);
                    self.buf.set_line(row, ind);
                } else {
                    self.apply(op, r, None, 1);
                }
                let after = self.buf.line_len(row);
                self.cursor = Pos::new(row, (col + after).saturating_sub(before));
            }
            Key::Left => self.cursor.col = self.cursor.col.saturating_sub(1),
            Key::Right => self.cursor.col += 1,
            Key::Up => {
                if self.cursor.row > 0 {
                    self.cursor.row -= 1;
                    self.cursor.col = self.col_for_want(self.cursor.row);
                }
                return;
            }
            Key::Down => {
                if self.cursor.row < self.buf.last_row() {
                    self.cursor.row += 1;
                    self.cursor.col = self.col_for_want(self.cursor.row);
                }
                return;
            }
            Key::Home => self.cursor.col = 0,
            Key::End => self.cursor.col = self.buf.line_len(self.cursor.row),
            Key::PageUp => self.scroll_page(false, 1),
            Key::PageDown => self.scroll_page(true, 1),
            _ => {}
        }
        self.clamp();
        self.set_want();
    }

    fn type_char(&mut self, c: char) {
        self.touch();
        self.insert.auto_indent = None;
        let p = self.cursor;
        if self.mode == Mode::Replace && p.col < self.buf.line_len(p.row) {
            let old = self.buf.char_at(p);
            self.buf.delete(p, Pos::new(p.row, p.col + 1));
            self.insert.replaced.push(old);
        } else if self.mode == Mode::Replace {
            self.insert.replaced.push(None);
        }
        let mut s = [0u8; 4];
        self.cursor = self.buf.insert(p, c.encode_utf8(&mut s));
        if matches!(c, '}' | ']' | ')') && self.options.auto_indent && self.mode == Mode::Insert {
            self.outdent_closer(c);
        }
    }

    fn type_paired(&mut self, c: char) {
        if !self.options.auto_pairs || self.mode != Mode::Insert {
            return self.type_char(c);
        }
        let p = self.cursor;
        let next = self.buf.char_at(p);
        let prev = p
            .col
            .checked_sub(1)
            .and_then(|col| self.buf.char_at(Pos::new(p.row, col)));
        let quote = matches!(c, '"' | '\'' | '`');
        if (quote || matches!(c, ')' | ']' | '}')) && next == Some(c) {
            self.cursor.col += 1;
            return;
        }
        let close = match c {
            '(' => ')',
            '[' => ']',
            '{' => '}',
            _ if quote => c,
            _ => return self.type_char(c),
        };
        if next.is_some_and(is_word) || quote && prev.is_some_and(is_word) {
            return self.type_char(c);
        }
        self.type_char(c);
        self.buf.insert(self.cursor, &close.to_string());
    }

    fn outdent_closer(&mut self, c: char) {
        let row = self.cursor.row;
        if self.buf.line(row).trim() != c.encode_utf8(&mut [0; 4]) {
            return;
        }
        let open = match c {
            '}' => '{',
            ']' => '[',
            _ => '(',
        };
        let at = Pos::new(row, self.cursor.col - 1);
        let Some(o) = motion::find_open(&self.buf, at, open, c) else {
            return;
        };
        let ind = indent(self.buf.line(o.row)).to_string();
        self.buf.set_line(row, format!("{ind}{c}"));
        self.cursor.col = char_count(&ind) + 1;
    }

    fn type_str(&mut self, s: &str) {
        for c in s.chars() {
            self.type_char(c);
        }
    }

    fn newline(&mut self) {
        self.touch();
        // clear an unused autoindent but carry it to the next line
        let kept = self.clear_unused_indent();
        let p = self.cursor;
        let left: String = self.buf.line(p.row).chars().take(p.col).collect();
        let mut ind = match kept {
            Some(k) => k,
            None if self.options.auto_indent => indent(&left).to_string(),
            None => String::new(),
        };
        if self.options.auto_indent && left.trim_end().ends_with(['{', '[', '(', ':']) {
            ind = self.make_indent(vcol(&ind, char_count(&ind), self.options.tab_stop) + self.options.shift_width);
        }
        if self.mode == Mode::Replace {
            self.insert.replaced.clear();
        }
        let keep = char_count(left.trim_end_matches([' ', '\t']));
        if keep < p.col && !left.trim().is_empty() {
            self.buf.delete(Pos::new(p.row, keep), p);
            self.cursor.col = keep;
        }
        let end = self.buf.insert(self.cursor, &format!("\n{ind}"));
        let rest = self.buf.line(end.row).to_string();
        let tail = &rest[crate::text::byte_at(&rest, end.col)..];
        let trimmed = tail.trim_start_matches([' ', '\t']);
        let closer = trimmed.starts_with(['}', ']', ')']);
        if trimmed.len() != tail.len() {
            let head: String = rest.chars().take(end.col).collect();
            self.buf.set_line(end.row, format!("{head}{trimmed}"));
        }
        self.cursor = end;
        self.insert.auto_indent = Some(end.row);
        if closer && self.options.auto_indent && left.trim_end().ends_with(['{', '[', '(']) {
            let outer = indent(&left).to_string();
            let line = self.buf.line(end.row).to_string();
            let head: String = line.chars().take(end.col).collect();
            let tail: String = line.chars().skip(end.col).collect();
            self.buf.set_line(end.row, head);
            self.buf.insert_lines(end.row + 1, vec![format!("{outer}{tail}")]);
        }
    }

    fn clear_unused_indent(&mut self) -> Option<String> {
        let row = self.cursor.row;
        let line = self.buf.line(row);
        if self.insert.auto_indent != Some(row) || !line.trim().is_empty() || line.is_empty() {
            return None;
        }
        let line = line.to_string();
        self.buf.set_line(row, String::new());
        self.cursor.col = 0;
        Some(line)
    }

    fn backspace(&mut self) {
        let p = self.cursor;
        if self.mode == Mode::Replace {
            if p.col == 0 {
                return;
            }
            let back = Pos::new(p.row, p.col - 1);
            if let Some(old) = self.insert.replaced.pop() {
                self.touch();
                self.buf.delete(back, p);
                if let Some(c) = old {
                    self.buf.insert(back, &c.to_string());
                }
            }
            self.cursor = back;
            return;
        }
        if p.col == 0 {
            if p.row == 0 {
                return;
            }
            self.touch();
            let len = self.buf.line_len(p.row - 1);
            self.buf.delete(Pos::new(p.row - 1, len), p);
            self.cursor = Pos::new(p.row - 1, len);
            return;
        }
        self.touch();
        let pair = |a, b| {
            matches!(
                (a, b),
                ('(', ')') | ('[', ']') | ('{', '}') | ('"', '"') | ('\'', '\'') | ('`', '`')
            )
        };
        if let (true, Some(a), Some(b)) = (
            self.options.auto_pairs,
            self.buf.char_at(Pos::new(p.row, p.col - 1)),
            self.buf.char_at(p),
        ) {
            if pair(a, b) {
                self.buf.delete(Pos::new(p.row, p.col - 1), Pos::new(p.row, p.col + 1));
                self.cursor.col -= 1;
                return;
            }
        }
        let mut before = self.buf.line(p.row).chars().take(p.col);
        let blank = before.next() == Some(' ') && before.all(|c| c == ' ');
        let from = if self.options.expand_tab && blank {
            let sw = self.options.shift_width.max(1);
            p.col - (if p.col % sw == 0 { sw } else { p.col % sw })
        } else {
            p.col - 1
        };
        self.buf.delete(Pos::new(p.row, from), p);
        self.cursor.col = from;
    }

    fn delete_word_before(&mut self) {
        let p = self.cursor;
        if p.col == 0 {
            return self.backspace();
        }
        let chars: Vec<char> = self.buf.line(p.row).chars().collect();
        let mut i = p.col;
        while i > 0 && chars[i - 1].is_whitespace() {
            i -= 1;
        }
        if i > 0 {
            let k = class(chars[i - 1], false);
            while i > 0 && class(chars[i - 1], false) == k {
                i -= 1;
            }
        }
        self.touch();
        self.buf.delete(Pos::new(p.row, i), p);
        self.cursor.col = i;
    }

    fn leave_insert(&mut self) {
        self.completion = None;
        let count = self.insert.count;
        if count > 1 && !self.insert_replay {
            let typed = std::mem::take(&mut self.insert.typed);
            let open = self.insert.open;
            let building = self.building.take();
            self.insert_replay = true;
            for _ in 1..count {
                if open.is_some() {
                    self.open_one(false);
                }
                for i in &typed {
                    self.insert_input(i.clone());
                }
            }
            self.insert_replay = false;
            self.building = building;
        }
        self.clear_unused_indent();
        self.marks.insert('^', self.cursor);
        self.mode = Mode::Normal;
        self.cursor.col = self.cursor.col.saturating_sub(1);
        self.clamp();
        self.set_want();
        if let Some(b) = self.building.take() {
            self.dot = Some(b);
        }
        self.end_change();
    }

    fn enter_visual(&mut self, mode: Mode) {
        self.anchor = self.cursor;
        self.mode = mode;
    }

    fn leave_visual(&mut self) {
        let r = self.visual_range();
        self.last_visual = Some((self.anchor, self.cursor, self.mode));
        self.marks.insert('<', r.start);
        let last = r.last_row();
        self.marks.insert(
            '>',
            if r.linewise {
                Pos::new(last, 0)
            } else if last < r.end.row {
                Pos::new(last, self.buf.line_len(last).saturating_sub(1))
            } else {
                Pos::new(last, r.end.col.saturating_sub(1))
            },
        );
        self.mode = Mode::Normal;
        self.clamp();
    }

    pub(crate) fn visual_range(&self) -> Range {
        let (a, b) = if self.cursor < self.anchor {
            (self.cursor, self.anchor)
        } else {
            (self.anchor, self.cursor)
        };
        if self.mode == Mode::VisualLine {
            return Range::lines(a.row, b.row);
        }
        let len = self.buf.line_len(b.row);
        let end = if len == 0 && b.row < self.buf.last_row() {
            Pos::new(b.row + 1, 0)
        } else {
            Pos::new(b.row, (b.col + 1).min(len))
        };
        Range {
            start: a,
            end,
            linewise: false,
        }
    }

    fn run_visual(&mut self, cmd: Cmd) {
        let n = cmd.n();
        match cmd.act {
            Act::Move(m) => self.move_cursor(m, cmd.count),
            Act::Select { kind, inner } => {
                let Some(Span { start, end, linewise }) = object(&self.buf, self.cursor, kind, inner, n) else {
                    return;
                };
                if linewise {
                    self.mode = Mode::VisualLine;
                    self.anchor = start;
                    self.cursor = Pos::new(end.row, 0);
                } else if start < end {
                    self.anchor = start;
                    self.cursor = if end.col == 0 && end.row > start.row {
                        Pos::new(end.row - 1, self.buf.line_len(end.row - 1))
                    } else {
                        Pos::new(end.row, end.col - 1)
                    };
                }
            }
            Act::Operate(op, _) => {
                let r = self.visual_range();
                self.leave_visual();
                self.apply(op, r, cmd.register, n);
                if op == Op::Yank {
                    self.cursor = r.start;
                }
                // repeats as gcc over as many lines
                if op == Op::Comment && !self.replaying {
                    self.dot = Some(Dot {
                        count: Some(r.last_row() - r.start.row + 1),
                        register: None,
                        inputs: crate::keys::parse("gcc").into_iter().map(Input::Key).collect(),
                    });
                }
            }
            Act::Key(Key::Char(c)) => self.visual_key(c, cmd.register, n),
            Act::Key(Key::Ctrl(c @ ('a' | 'x'))) => {
                let r = self.visual_range();
                self.leave_visual();
                for row in r.start.row..=r.last_row() {
                    self.cursor = Pos::new(
                        row,
                        if row == r.start.row && !r.linewise {
                            r.start.col
                        } else {
                            0
                        },
                    );
                    self.increment(if c == 'a' { n as i64 } else { -(n as i64) });
                }
                self.cursor = r.start;
            }
            Act::With('r', ch) => {
                let r = self.visual_range();
                self.leave_visual();
                self.touch();
                let (from, to) = self.char_span(r);
                let text = self.buf.slice(from, to);
                let new: String = text.chars().map(|c| if c == '\n' { c } else { ch }).collect();
                self.buf.delete(from, to);
                self.buf.insert(from, &new);
                self.cursor = from;
            }
            Act::With(c, ch) => self.normal_with(c, ch, n),
            Act::G(c) => self.normal_g(c, n, cmd.register),
            Act::Z(k) => self.normal_z(k, cmd.count),
            _ => {}
        }
    }

    fn visual_key(&mut self, c: char, register: Option<char>, n: usize) {
        let r = self.visual_range();
        let lines = Range::lines(r.start.row, r.last_row());
        match c {
            'v' | 'V' => {
                let want = if c == 'v' { Mode::Visual } else { Mode::VisualLine };
                if self.mode == want {
                    self.leave_visual();
                } else {
                    self.mode = want;
                }
            }
            'o' | 'O' => std::mem::swap(&mut self.anchor, &mut self.cursor),
            'x' => {
                self.leave_visual();
                self.apply(Op::Delete, r, register, n);
            }
            'X' | 'D' => {
                self.leave_visual();
                self.apply(Op::Delete, lines, register, n);
            }
            'Y' => {
                self.leave_visual();
                self.apply(Op::Yank, lines, register, n);
            }
            's' => {
                self.leave_visual();
                self.apply(Op::Change, r, register, n);
            }
            'S' | 'R' | 'C' => {
                self.leave_visual();
                self.apply(Op::Change, lines, register, n);
            }
            'J' => {
                self.leave_visual();
                self.join(r.start.row, (r.last_row() - r.start.row + 1).max(2), true);
            }
            'p' | 'P' => {
                let Some(reg) = self.fetch(register) else { return };
                self.leave_visual();
                // p puts the replaced text in "; P leaves registers alone
                self.apply(Op::Delete, r, if c == 'P' { Some('_') } else { None }, 1);
                let text = if reg.linewise {
                    vec![reg.text.as_str(); n].join("\n")
                } else {
                    reg.text.repeat(n)
                };
                if r.linewise {
                    let row = r.start.row;
                    let lines: Vec<String> = text.split('\n').map(str::to_string).collect();
                    let emptied = self.buf.is_empty();
                    self.buf.insert_lines(row.min(self.buf.len()), lines);
                    if emptied {
                        let last = self.buf.last_row();
                        self.buf.delete_lines(last, last);
                    }
                    self.cursor = self.first_non_blank_pos(row.min(self.buf.last_row()));
                } else if reg.linewise {
                    let at = r.start;
                    self.buf.insert(at, &format!("\n{text}\n"));
                    self.cursor = Pos::new(at.row + 1, 0);
                } else {
                    let end = self.buf.insert(r.start, &text);
                    self.cursor = Pos::new(end.row, end.col.saturating_sub(1));
                }
            }
            ':' => {
                self.leave_visual();
            }
            'I' => {
                self.leave_visual();
                self.cursor = if r.linewise {
                    self.first_non_blank_pos(r.start.row)
                } else {
                    r.start
                };
                self.begin_insert(1, None);
            }
            'A' => {
                self.leave_visual();
                self.cursor = if r.linewise || r.last_row() < r.end.row {
                    Pos::new(r.last_row(), self.buf.line_len(r.last_row()))
                } else {
                    r.end
                };
                self.begin_insert(1, None);
            }
            _ => {}
        }
    }

    fn scroll_half(&mut self, down: bool, count: Option<usize>) {
        let n = count.unwrap_or(self.text_rows() / 2).max(1);
        let max_top = self.buf.last_row();
        let top = self.top.get();
        if down {
            if self.cursor.row >= self.buf.last_row() {
                return;
            }
            self.top
                .set((top + n).min(max_top.saturating_sub(self.text_rows().saturating_sub(1)).max(top)));
            self.cursor.row = (self.cursor.row + n).min(self.buf.last_row());
        } else {
            if self.cursor.row == 0 {
                return;
            }
            self.top.set(top.saturating_sub(n));
            self.cursor.row = self.cursor.row.saturating_sub(n);
        }
        self.cursor = self.first_non_blank_pos(self.cursor.row);
        self.set_want();
    }

    fn scroll_page(&mut self, down: bool, n: usize) {
        let page = self.text_rows().saturating_sub(2).max(1) * n;
        let top = self.top.get();
        let so = self.scroll_off();
        if down {
            let new = (top + page).min(self.buf.last_row());
            self.top.set(new);
            self.cursor.row = (new + so).min(self.buf.last_row());
        } else {
            let new = top.saturating_sub(page);
            self.top.set(new);
            self.cursor.row = (new + self.text_rows()).saturating_sub(1 + so).min(self.buf.last_row());
        }
        self.cursor = self.first_non_blank_pos(self.cursor.row);
        self.set_want();
    }

    fn scroll_lines(&mut self, by: isize) {
        let top = self.top.get();
        let new = if by > 0 {
            (top + by as usize).min(self.buf.last_row())
        } else {
            top.saturating_sub((-by) as usize)
        };
        self.top.set(new);
        let so = self.scroll_off();
        let rows = self.text_rows();
        let low = if new == 0 { 0 } else { new + so };
        let high = (new + rows).saturating_sub(1 + so).max(low);
        let row = self
            .cursor
            .row
            .clamp(low.min(self.buf.last_row()), high.min(self.buf.last_row()));
        if row != self.cursor.row {
            self.cursor.row = row;
            self.cursor.col = self.col_for_want(row);
        }
    }

    pub(crate) fn load(&mut self, path: &Path) -> std::io::Result<()> {
        if path.is_dir() {
            return Ok(());
        }
        let display = path.display().to_string();
        match std::fs::read(path) {
            Ok(bytes) => {
                let size = bytes.len();
                let text = String::from_utf8(bytes).map_err(|_| {
                    std::io::Error::new(std::io::ErrorKind::InvalidData, format!("{display} isn't UTF-8 text"))
                })?;
                let lf = text.matches('\n').count();
                let crlf = lf > 0 && text.matches("\r\n").count() == lf;
                let text = if crlf { text.replace("\r\n", "\n") } else { text };
                let final_newline = text.is_empty() || text.ends_with('\n');
                let body = text.strip_suffix('\n').unwrap_or(&text);
                self.buf = Buffer::from_text(body);
                self.file = FileInfo {
                    path: Some(path.to_path_buf()),
                    crlf,
                    final_newline,
                    new: false,
                };
                let lines = if text.is_empty() { 0 } else { self.buf.len() };
                let mut notes = String::new();
                if crlf {
                    notes.push_str(" [dos]");
                }
                if !final_newline {
                    notes.push_str(" [noeol]");
                }
                self.info(format!("\"{display}\"{notes} {lines}L, {size}B"));
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                self.buf = Buffer::new();
                self.file = FileInfo {
                    path: Some(path.to_path_buf()),
                    crlf: false,
                    final_newline: true,
                    new: true,
                };
                self.info(format!("\"{display}\" [New]"));
            }
            Err(e) => return Err(e),
        }
        self.cursor = Pos::default();
        self.saved_lines = self.buf.lines().to_vec();
        self.undo.clear();
        self.redo.clear();
        self.versions += 1;
        self.version = self.versions;
        self.saved = self.version;
        self.top.set(0);
        self.left.set(0);
        self.marks.clear();
        self.explorer_focused = false;
        self.mode = Mode::Normal;
        Ok(())
    }

    pub fn contents(&self, rows: Option<(usize, usize)>) -> String {
        let nl = if self.file.crlf { "\r\n" } else { "\n" };
        let lines = match rows {
            Some((a, b)) => &self.buf.lines()[a..=b],
            None => self.buf.lines(),
        };
        let mut out = lines.join(nl);
        let whole = rows.is_none();
        // keep the file's original trailing newline, or lack of one
        if !whole || self.file.final_newline && !self.buf.is_empty() {
            out.push_str(nl);
        }
        out
    }

    pub(crate) fn write(&mut self, path: Option<&str>, force: bool, rows: Option<(usize, usize)>) -> bool {
        let target = match path {
            Some(p) => PathBuf::from(expand_home(p)),
            None => match &self.file.path {
                Some(p) => p.clone(),
                None => {
                    self.error("E32: No file name");
                    return false;
                }
            },
        };
        let ours = self.file.path.as_ref() == Some(&target);
        if !ours && !force && target.exists() {
            self.error("E13: File exists (add ! to override)");
            return false;
        }
        if ours && rows.is_some() && !force {
            self.error("E140: Use ! to write partial buffer");
            return false;
        }
        let contents = self.contents(rows);
        let existed = target.exists();
        if let Err(e) = std::fs::write(&target, &contents) {
            self.error(format!("E212: Can't open file for writing: {e}"));
            return false;
        }
        let new = !existed;
        if self.file.path.is_none() {
            self.file.path = Some(target.clone());
        }
        if self.file.path.as_ref() == Some(&target) && rows.is_none() {
            self.saved = self.version;
            self.saved_lines = self.buf.lines().to_vec();
            *self.marks_cache.borrow_mut() = None;
            self.file.new = false;
        }
        let lines = if contents.is_empty() {
            0
        } else {
            rows.map_or(self.buf.len(), |(a, b)| b - a + 1)
        };
        let new = if new { " [New]" } else { "" };
        self.info(format!(
            "\"{}\"{new} {lines}L, {}B written",
            target.display(),
            contents.len()
        ));
        true
    }

    fn file_info(&mut self) {
        let lines = self.buf.len();
        let pct = (self.cursor.row + 1) * 100 / lines;
        let modified = if self.is_modified() { " [Modified]" } else { "" };
        self.info(format!(
            "\"{}\"{modified} {lines} line{} --{pct}%--",
            self.name(),
            if lines == 1 { "" } else { "s" }
        ));
    }

    pub(crate) fn set_quit(&mut self) {
        self.quit = true;
    }

    pub(crate) fn set_cursor(&mut self, p: Pos) {
        self.cursor = p;
        self.set_want();
    }

}

fn toggle(c: char) -> char {
    if c.is_lowercase() {
        single(c.to_uppercase()).unwrap_or(c)
    } else if c.is_uppercase() {
        single(c.to_lowercase()).unwrap_or(c)
    } else {
        c
    }
}

fn single(mut chars: impl Iterator<Item = char>) -> Option<char> {
    let c = chars.next()?;
    chars.next().is_none().then_some(c)
}

fn display_name(path: Option<&Path>) -> String {
    path.map_or_else(|| "[No Name]".to_string(), |p| p.display().to_string())
}

pub(crate) fn expand_home(path: &str) -> String {
    match (path.strip_prefix("~/"), std::env::var("HOME")) {
        (Some(rest), Ok(home)) => format!("{home}/{rest}"),
        _ => path.to_string(),
    }
}
