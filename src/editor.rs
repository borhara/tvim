use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use tinsel::app::{Key, TextInput};

use crate::blame::Blamer;
use crate::buffer::{Buffer, Pos};
use crate::clipboard::{Clipboard, System};
use crate::command::{Act, Cmd, Motion, Op, Parse, Target, parse};
use crate::diff::{self, Hunk, Mark};
use crate::explorer::{Action, Explorer};
use crate::finder::{Finder, Pick};
use crate::git::{Base, Git, Tree};
use crate::gitmenu::{GitMenu, Want};
use crate::grep::{Grep, Jump};
use crate::jumps::{JumpList, Spot};
use crate::mapping::{Lookup, MapMode, Mapping, Maps};
use crate::motion::{self, Find};
use crate::object::{Span, object};
use crate::preview::Preview;
use crate::regex::Regex;
use crate::syntax::{self, State, Syntax};
use crate::text::{char_count, class, col_at, first_non_blank, indent, is_word, vcol};
use crate::window::{Node, Win};

mod block;
pub use block::Block;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Insert,
    Replace,
    Visual,
    VisualLine,
    VisualBlock,
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
            Mode::VisualBlock => "V-BLOCK",
        }
    }

    pub fn is_visual(self) -> bool {
        matches!(self, Mode::Visual | Mode::VisualLine | Mode::VisualBlock)
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
    // a block from block visual mode, one line of it per row
    pub block: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    Shell(String),
    Suspend,
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
    // milliseconds to wait for the rest of a mapping
    pub timeout_len: usize,
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
    pub wrap: bool,
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
            timeout_len: 1000,
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
            wrap: false,
        }
    }
}

impl Options {
    pub(crate) fn flag(&self, name: &str) -> Option<bool> {
        self.clone().flag_mut(name).map(|(_, on)| *on)
    }

    pub(crate) fn flag_mut(&mut self, name: &str) -> Option<(&'static str, &mut bool)> {
        Some(match name {
            "number" | "nu" => ("number", &mut self.number),
            "relativenumber" | "rnu" => ("relativenumber", &mut self.relative_number),
            "expandtab" | "et" => ("expandtab", &mut self.expand_tab),
            "ignorecase" | "ic" => ("ignorecase", &mut self.ignore_case),
            "smartcase" | "scs" => ("smartcase", &mut self.smart_case),
            "hlsearch" | "hls" => ("hlsearch", &mut self.highlight_search),
            "incsearch" | "is" => ("incsearch", &mut self.incremental_search),
            "autoindent" | "ai" => ("autoindent", &mut self.auto_indent),
            "syntax" | "syn" => ("syntax", &mut self.syntax),
            "cursorline" | "cul" => ("cursorline", &mut self.cursor_line),
            "indentguides" | "ig" => ("indentguides", &mut self.indent_guides),
            "autopairs" | "ap" => ("autopairs", &mut self.auto_pairs),
            "smooth" | "sm" => ("smooth", &mut self.smooth),
            "tabpreview" | "tp" => ("tabpreview", &mut self.tab_preview),
            "dim" => ("dim", &mut self.dim),
            "gitlens" | "gl" => ("gitlens", &mut self.gitlens),
            "roomy" | "ro" => ("roomy", &mut self.roomy),
            "zeropad" | "zp" => ("zeropad", &mut self.zero_pad),
            "wrap" => ("wrap", &mut self.wrap),
            _ => return None,
        })
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
    block: Option<block::BlockInsert>,
}

#[derive(Clone, Debug)]
pub(crate) struct Completion {
    start: Pos,
    prefix: String,
    words: Vec<String>,
    at: Option<usize>,
}

#[derive(Clone, Debug, Default)]
pub struct CmdLine {
    pub text: String,
    pub cursor: usize,
    pub prompt: char,
    browse: Option<usize>,
    origin: Pos,
    origin_top: usize,
    completing: Option<(Vec<crate::suggest::Suggestion>, usize)>,
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

#[derive(Default)]
struct Doc {
    id: usize,
    buf: Buffer,
    cursor: Pos,
    want: usize,
    last_visual: Option<(Pos, Pos, Mode)>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    version: u64,
    saved: u64,
    marks: HashMap<char, Pos>,
    file: FileInfo,
    saved_lines: Vec<String>,
    git: Git,
    top: usize,
    left: usize,
    states: RefCell<States>,
    diffs: RefCell<Option<DiffCache>>,
}

impl Doc {
    fn modified(&self) -> bool {
        self.version != self.saved
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BufferInfo {
    pub id: usize,
    pub name: String,
    pub path: Option<PathBuf>,
    pub modified: bool,
    pub current: bool,
}

#[derive(Default)]
pub(crate) struct States {
    version: u64,
    syntax: Option<Syntax>,
    hashes: Vec<u64>,
    states: Vec<State>,
}

fn line_hash(line: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    line.hash(&mut h);
    h.finish()
}

pub(crate) struct Text<'a> {
    pub buf: &'a Buffer,
    pub path: Option<&'a Path>,
    pub modified: bool,
    pub version: u64,
    git: &'a Git,
    saved_lines: &'a [String],
    states: &'a RefCell<States>,
    diffs: &'a RefCell<Option<DiffCache>>,
}

impl Text<'_> {
    pub fn syntax(&self, on: bool) -> Option<Syntax> {
        if !on {
            return None;
        }
        Syntax::detect(self.path?.file_name()?.to_str()?)
    }

    pub fn highlight(&self, row: usize, on: bool) -> Option<Vec<syntax::Kind>> {
        let syntax = self.syntax(on)?;
        let state = self.state_at(row, syntax);
        Some(syntax::highlight(self.buf.line(row), syntax, state).0)
    }

    fn state_at(&self, row: usize, syntax: Syntax) -> State {
        let mut cache = self.states.borrow_mut();
        let cache = &mut *cache;
        if cache.syntax != Some(syntax) {
            *cache = States {
                syntax: Some(syntax),
                ..States::default()
            };
        }
        if cache.version != self.version {
            // a line's state only depends on the lines above it, so keep everything above the first change
            let same = cache
                .hashes
                .iter()
                .zip(self.buf.lines())
                .take_while(|(h, l)| **h == line_hash(l))
                .count();
            cache.hashes.truncate(same);
            cache.states.truncate(same + 1);
            cache.version = self.version;
        }
        if cache.states.is_empty() {
            cache.states.push(State::Normal);
        }
        while cache.states.len() <= row.min(self.buf.len()) {
            let r = cache.states.len() - 1;
            let line = self.buf.line(r);
            cache.hashes.push(line_hash(line));
            cache.states.push(syntax::highlight(line, syntax, cache.states[r]).1);
        }
        cache.states[row.min(cache.states.len() - 1)]
    }

    fn diffed(&self) -> DiffCache {
        let key = (self.version, self.git.with(|f| f.seq));
        if self.diffs.borrow().as_ref().is_none_or(|c| c.0 != key) {
            let lines = self.buf.lines();
            let (hunks, untracked) = self.git.with(|f| {
                let hunks = match &f.base {
                    Base::Head(old) => diff::hunks(old, lines),
                    Base::Untracked if self.buf.is_empty() => Vec::new(),
                    Base::Untracked => vec![Hunk {
                        old: 0..0,
                        new: 0..lines.len(),
                    }],
                    Base::Outside if self.modified || self.saved_lines != lines => diff::hunks(self.saved_lines, lines),
                    Base::Outside => Vec::new(),
                };
                (hunks, matches!(f.base, Base::Untracked))
            });
            // the sidebar's ? already says the whole file is new
            let marks = if untracked {
                vec![None; lines.len()]
            } else {
                diff::marks_from(&hunks, lines.len())
            };
            *self.diffs.borrow_mut() = Some((key, hunks.into(), marks.into()));
        }
        self.diffs.borrow().clone().expect("just filled")
    }

    pub fn change_marks(&self) -> Rc<[Option<Mark>]> {
        self.diffed().2
    }
}

/// What one screen row of a window shows: part of line `row`, from character `col`, which
/// sits at screen column `left`, for `width` columns. Rows past the end have `row` past it too.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Slice {
    pub row: usize,
    pub col: usize,
    pub left: usize,
    pub width: usize,
    pub first: bool,
    pub last: bool,
}

pub(crate) struct View<'a> {
    pub id: usize,
    pub rect: tinsel::app::Rect,
    pub focused: bool,
    pub cursor: Pos,
    pub top: usize,
    pub left: usize,
    pub text: Text<'a>,
}

type RegexCache = (String, (bool, bool), Rc<Regex>);
// keyed on (buffer version, git answer)
type HitsCache = (usize, u64, Rc<Regex>, Rc<[Pos]>);
type DiffCache = ((u64, u64), Rc<[Hunk]>, Rc<[Option<Mark>]>);
pub(crate) type OutlineCache = ((usize, u64, Option<Syntax>), Vec<crate::ui::Section>);

const MAX_UNDO: usize = 1000;
const MAX_HISTORY: usize = 100;

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
    pub(crate) cmd: CmdLine,
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
    pub(crate) completion: Option<Completion>,
    recording: Option<(char, Vec<Input>)>,
    macros: HashMap<char, Vec<Input>>,
    last_macro: Option<char>,
    macro_depth: usize,
    pub(crate) maps: Maps,
    // typed keys that may yet be the start of a mapping
    map_typed: Vec<Key>,
    map_depth: usize,
    // above zero while a mapping's keys are fed, which a macro doesn't record
    from_map: usize,
    // above zero while keys are fed as they are, past any mapping
    pub(crate) noremap: usize,
    pub(crate) file: FileInfo,
    pub options: Options,
    pub(crate) top: Cell<usize>,
    pub(crate) left: Cell<usize>,
    pub(crate) screen: Cell<(usize, usize)>,
    pub(crate) anim: crate::ui::Anim,
    zen: Option<bool>,
    effects: Vec<Effect>,
    quit: bool,
    clipboard: Box<dyn Clipboard>,
    explorer: Option<Explorer>,
    explorer_focused: bool,
    pub(crate) finder: Option<Finder>,
    grep: Option<Grep>,
    saved_lines: Vec<String>,
    git: Git,
    tree: Tree,
    blamer: Blamer,
    git_menu: Option<GitMenu>,
    git_history: Vec<String>,
    states: RefCell<States>,
    marks_cache: RefCell<Option<DiffCache>>,
    regex_cache: RefCell<Option<RegexCache>>,
    hits_cache: RefCell<Option<HitsCache>>,
    jumps: JumpList,
    pub(crate) no_jumps: bool,
    docs: Vec<Doc>,
    current: usize,
    pub(crate) buffer_id: usize,
    next_id: usize,
    pub(crate) recent: Vec<crate::session::Place>,
    // keyed on (buffer, version, syntax)
    pub(crate) outline: RefCell<Option<OutlineCache>>,
    // the focused window's cursor and scroll live in the fields above; wins has the rest
    pub(crate) wins: Vec<Win>,
    pub(crate) layout: Node,
    pub(crate) focus: usize,
    pub(crate) next_win: usize,
    // for shifting other windows on this buffer
    pub(crate) shadow: Option<(usize, Vec<String>)>,
    pub(crate) region: Cell<Option<(usize, usize)>>,
    pub(crate) previews: HashMap<usize, Preview>,
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
            cmd: CmdLine::default(),
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
            maps: Maps::default(),
            map_typed: Vec::new(),
            map_depth: 0,
            from_map: 0,
            noremap: 0,
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
            git: Git::default(),
            tree: Tree::default(),
            blamer: Blamer::default(),
            git_menu: None,
            git_history: Vec::new(),
            states: RefCell::default(),
            marks_cache: RefCell::new(None),
            regex_cache: RefCell::new(None),
            hits_cache: RefCell::new(None),
            jumps: JumpList::default(),
            no_jumps: false,
            docs: vec![Doc::default()],
            current: 0,
            buffer_id: 1,
            next_id: 2,
            recent: Vec::new(),
            outline: RefCell::new(None),
            wins: vec![Win {
                id: 1,
                buffer: 1,
                ..Win::default()
            }],
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

    pub fn cmdline(&self) -> &CmdLine {
        &self.cmd
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

    pub fn take_effects(&mut self) -> Vec<Effect> {
        std::mem::take(&mut self.effects)
    }

    pub fn name(&self) -> String {
        display_name(self.file.path.as_deref())
    }

    pub fn syntax(&self) -> Option<Syntax> {
        self.shown_text().syntax(self.options.syntax)
    }

    pub fn highlight(&self, row: usize) -> Option<Vec<syntax::Kind>> {
        self.shown_text().highlight(row, self.options.syntax)
    }

    pub(crate) fn shown_text(&self) -> Text<'_> {
        Text {
            buf: &self.buf,
            path: self.file.path.as_deref(),
            modified: self.is_modified(),
            version: self.version,
            git: &self.git,
            saved_lines: &self.saved_lines,
            states: &self.states,
            diffs: &self.marks_cache,
        }
    }

    pub(crate) fn text_of(&self, id: usize) -> Text<'_> {
        match self.buffer_at(id).map(|i| &self.docs[i]) {
            Some(d) if id != self.buffer_id => Text {
                buf: &d.buf,
                path: d.file.path.as_deref(),
                modified: d.modified(),
                version: d.version,
                git: &d.git,
                saved_lines: &d.saved_lines,
                states: &d.states,
                diffs: &d.diffs,
            },
            _ => self.shown_text(),
        }
    }

    pub(crate) fn has_buffer(&self, id: usize) -> bool {
        id == self.buffer_id || self.buffer_at(id).is_some()
    }

    pub(crate) fn buffer_at(&self, id: usize) -> Option<usize> {
        self.docs
            .iter()
            .enumerate()
            .position(|(i, d)| i != self.current && d.id == id)
    }

    pub fn change_marks(&self) -> Rc<[Option<Mark>]> {
        self.shown_text().change_marks()
    }

    pub fn change_hunks(&self) -> Rc<[Hunk]> {
        self.shown_text().diffed().1
    }

    pub fn branch(&self) -> Option<String> {
        self.git.with(|f| f.branch.clone())
    }

    pub fn upstream(&self) -> Option<(usize, usize)> {
        self.git.with(|f| f.upstream)
    }

    pub fn wait_for_git(&self) {
        self.git.wait();
        self.tree.wait();
    }

    pub fn tree(&self) -> &Tree {
        &self.tree
    }

    fn refresh_tree(&self) {
        if let Some(x) = &self.explorer {
            self.tree.refresh(x.root());
        }
    }

    pub(crate) fn version(&self) -> u64 {
        self.version
    }

    pub fn is_blank_slate(&self) -> bool {
        self.wins.len() == 1
            && self.file.path.is_none()
            && self.buf.is_empty()
            && self.version == 0
            && self.undo.is_empty()
    }

    pub fn selection(&self) -> Option<Range> {
        (self.mode.is_visual() && self.mode != Mode::VisualBlock).then(|| self.visual_range())
    }

    pub fn search_pattern(&self) -> Option<&str> {
        if self.mode == Mode::Search && self.options.incremental_search {
            return (!self.cmd.text.is_empty()).then_some(self.cmd.text.as_str());
        }
        if !(self.options.highlight_search && self.search_shown) {
            return None;
        }
        self.last_search.as_deref().filter(|p| !p.is_empty())
    }

    pub fn clear_message(&mut self) {
        self.message = None;
    }

    pub(crate) fn search_hits(&self) -> Option<Rc<[Pos]>> {
        let pat = self.search_pattern()?;
        if self.buf.len() > 50_000 {
            return None;
        }
        let re = self.regex(pat).ok()?;
        let mut cache = self.hits_cache.borrow_mut();
        let fresh = cache
            .as_ref()
            .is_some_and(|(id, v, r, _)| *id == self.buffer_id && *v == self.version && Rc::ptr_eq(r, &re));
        if !fresh {
            let hits = self
                .buf
                .lines()
                .iter()
                .enumerate()
                .flat_map(|(row, line)| re.spans(line).into_iter().map(move |(col, _)| Pos::new(row, col)))
                .collect();
            *cache = Some((self.buffer_id, self.version, re, hits));
        }
        cache.as_ref().map(|c| c.3.clone())
    }

    pub fn search_count(&self) -> Option<(usize, usize)> {
        let hits = self.search_hits()?;
        let current = hits.partition_point(|p| *p <= self.cursor);
        (!hits.is_empty()).then_some((current.max(1), hits.len()))
    }

    pub fn matching_bracket(&self) -> Option<Pos> {
        let c = self.buf.char_at(self.cursor)?;
        if !"()[]{}".contains(c) {
            return None;
        }
        motion::match_bracket(&self.buf, self.cursor)
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

    /// Where a line breaks into screen rows `width` wide: one row unless wrap is on.
    pub(crate) fn wrapped(&self, line: &str, width: usize) -> Vec<(usize, usize)> {
        if self.options.wrap {
            crate::text::wrap_rows(line, width, self.options.tab_stop, false)
        } else {
            vec![(0, 0)]
        }
    }

    /// The screen rows of a window `height` high showing `buf` from line `top`.
    pub(crate) fn slices(&self, buf: &Buffer, top: usize, left: usize, width: usize, height: usize) -> Vec<Slice> {
        let mut out = Vec::with_capacity(height);
        let mut row = top;
        while out.len() < height {
            if row >= buf.len() {
                out.push(Slice {
                    row,
                    col: 0,
                    left: 0,
                    width,
                    first: true,
                    last: true,
                });
            } else if !self.options.wrap {
                out.push(Slice {
                    row,
                    col: 0,
                    left,
                    width,
                    first: true,
                    last: true,
                });
            } else {
                let rows = self.wrapped(buf.line(row), width);
                for (k, &(col, at)) in rows.iter().enumerate() {
                    let end = rows.get(k + 1).map_or(at + width, |r| r.1);
                    out.push(Slice {
                        row,
                        col,
                        left: at,
                        width: (end - at).min(width),
                        first: k == 0,
                        last: k + 1 == rows.len(),
                    });
                }
            }
            row += 1;
        }
        out.truncate(height);
        out
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
        if self.options.wrap {
            self.left.set(0);
            return;
        }
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

    /// Types keys as they are, leaving mappings out.
    pub fn keys_noremap(&mut self, keys: &[Key]) {
        self.noremap += 1;
        self.keys(keys);
        self.noremap -= 1;
    }

    pub fn key(&mut self, key: Key) {
        self.input(Input::Key(key));
    }

    pub fn paste(&mut self, text: &str) {
        self.input(Input::Paste(text.to_string()));
    }

    fn input(&mut self, input: Input) {
        let Input::Key(key) = input else {
            self.resolve_mappings(true);
            return self.feed(input);
        };
        if self.noremap > 0 || self.replaying || self.maps.is_empty() {
            return self.feed(Input::Key(key));
        }
        self.map_typed.push(key);
        self.resolve_mappings(false);
    }

    /// Whether typed keys are being held back in case they start a mapping.
    pub fn mapping_pending(&self) -> bool {
        !self.map_typed.is_empty()
    }

    /// Stops waiting for a longer mapping and runs what the held keys come to.
    pub fn flush_mapping(&mut self) {
        self.resolve_mappings(true);
    }

    fn map_mode(&self) -> Option<MapMode> {
        let overlay = self.git_menu.is_some() || self.finder.is_some() || self.grep.is_some();
        if overlay || self.explorer_focused() && self.mode != Mode::Command {
            return None;
        }
        let fresh = crate::command::awaits_command(&self.pending);
        match self.mode {
            Mode::Normal if self.previews.contains_key(&self.focus) => None,
            Mode::Normal if fresh => Some(MapMode::Normal),
            m if m.is_visual() && fresh => Some(MapMode::Visual),
            Mode::Insert | Mode::Replace => Some(MapMode::Insert),
            Mode::Command | Mode::Search => Some(MapMode::Command),
            _ => None,
        }
    }

    // vim's rules, see :h map-typeahead: wait while a longer mapping could match, else take the
    // longest one that does, else let the first key through as it is
    fn resolve_mappings(&mut self, timed_out: bool) {
        while !self.map_typed.is_empty() {
            let Some(mode) = self.map_mode().filter(|m| self.maps.has_mode(*m)) else {
                let k = self.map_typed.remove(0);
                self.feed(Input::Key(k));
                continue;
            };
            let found =
                match self.maps.lookup(mode, &self.map_typed) {
                    Lookup::Prefix { .. } if !timed_out => return,
                    Lookup::Exact(m) | Lookup::Prefix { exact: Some(m) } => Some((self.map_typed.len(), m)),
                    Lookup::None | Lookup::Prefix { exact: None } => (1..self.map_typed.len()).rev().find_map(|i| {
                        match self.maps.lookup(mode, &self.map_typed[..i]) {
                            Lookup::Exact(m) | Lookup::Prefix { exact: Some(m) } => Some((i, m)),
                            _ => None,
                        }
                    }),
                };
            match found {
                Some((used, m)) => {
                    let rest: Vec<Key> = self.map_typed.drain(used..).collect();
                    self.map_typed.clear();
                    self.run_mapping(m);
                    self.map_typed.extend(rest);
                }
                None => {
                    let k = self.map_typed.remove(0);
                    self.feed(Input::Key(k));
                }
            }
        }
    }

    fn run_mapping(&mut self, m: Mapping) {
        if self.map_depth >= 100 {
            self.map_typed.clear();
            return self.error("E223: Recursive mapping");
        }
        if self.from_map == 0 && self.macro_depth == 0 && !self.insert_replay {
            if let Some((_, keys)) = &mut self.recording {
                keys.extend(m.lhs.iter().map(|&k| Input::Key(k)));
            }
        }
        self.map_depth += 1;
        self.from_map += 1;
        // nmap x xp doesn't map its own x again, see :h recursive_mapping
        let own = if m.rhs.starts_with(&m.lhs) { m.lhs.len() } else { 0 };
        for (i, &k) in m.rhs.iter().enumerate() {
            if m.noremap || i < own {
                self.resolve_mappings(true);
                self.noremap += 1;
                self.feed(Input::Key(k));
                self.noremap -= 1;
            } else {
                self.input(Input::Key(k));
            }
        }
        self.from_map -= 1;
        self.map_depth -= 1;
    }

    fn feed(&mut self, input: Input) {
        if self.git_menu.is_some() {
            return self.git_menu_input(input);
        }
        if self.finder.is_some() {
            return self.finder_input(input);
        }
        if self.grep.is_some() {
            return self.grep_input(input);
        }
        let filtering = self.explorer.as_ref().is_some_and(Explorer::is_filtering);
        if input == Input::Key(Key::Ctrl('p')) && self.explorer_focused() && !filtering && self.mode == Mode::Normal {
            return self.find_files(None);
        }
        if self.explorer_focused() && self.mode != Mode::Command {
            return self.explorer_input(input);
        }
        if self.mode == Mode::Normal && self.previews.contains_key(&self.focus) {
            if let Input::Key(k) = input {
                self.preview_input(k);
            }
            return;
        }
        if let Some((_, keys)) = &mut self.recording {
            if !self.replaying && self.macro_depth == 0 && !self.insert_replay && self.from_map == 0 {
                keys.push(input.clone());
            }
        }
        match self.mode {
            Mode::Normal | Mode::Visual | Mode::VisualLine | Mode::VisualBlock => self.normal(input),
            Mode::Insert | Mode::Replace => self.insert_input(input),
            Mode::Command | Mode::Search => self.cmdline_input(input),
        }
        if (self.mode == Mode::Normal || self.mode.is_visual()) && self.pending.is_empty() {
            self.end_change();
        }
        self.clamp();
        self.sync_windows();
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

    fn stash(&mut self) -> Doc {
        Doc {
            id: self.buffer_id,
            buf: std::mem::take(&mut self.buf),
            cursor: self.cursor,
            want: self.want,
            last_visual: self.last_visual.take(),
            undo: std::mem::take(&mut self.undo),
            redo: std::mem::take(&mut self.redo),
            version: self.version,
            saved: self.saved,
            marks: std::mem::take(&mut self.marks),
            file: std::mem::take(&mut self.file),
            saved_lines: std::mem::take(&mut self.saved_lines),
            git: std::mem::take(&mut self.git),
            top: self.top.get(),
            left: self.left.get(),
            states: std::mem::take(&mut self.states),
            diffs: std::mem::take(&mut self.marks_cache),
        }
    }

    fn unstash(&mut self, d: Doc) {
        self.buffer_id = d.id;
        self.buf = d.buf;
        self.cursor = d.cursor;
        self.want = d.want;
        self.last_visual = d.last_visual;
        self.undo = d.undo;
        self.redo = d.redo;
        self.version = d.version;
        self.saved = d.saved;
        self.marks = d.marks;
        self.file = d.file;
        self.saved_lines = d.saved_lines;
        self.git = d.git;
        self.git.probe(self.file.path.as_deref());
        self.top.set(d.top);
        self.left.set(d.left);
        self.anim.reset();
        self.changing = false;
        self.states = d.states;
        self.marks_cache = d.diffs;
        self.mode = Mode::Normal;
        self.pending.clear();
        self.completion = None;
    }

    pub fn buffers(&self) -> Vec<BufferInfo> {
        self.docs
            .iter()
            .enumerate()
            .map(|(i, d)| {
                let current = i == self.current;
                let (id, path, modified) = if current {
                    (self.buffer_id, self.file.path.clone(), self.is_modified())
                } else {
                    (d.id, d.file.path.clone(), d.modified())
                };
                BufferInfo {
                    id,
                    name: display_name(path.as_deref()),
                    path,
                    modified,
                    current,
                }
            })
            .collect()
    }

    pub fn places(&self) -> Vec<(PathBuf, Pos)> {
        let mut out = Vec::new();
        for (i, d) in self.docs.iter().enumerate() {
            let (path, pos) = if i == self.current {
                (&self.file.path, self.cursor)
            } else {
                (&d.file.path, d.cursor)
            };
            if let Some(p) = path {
                out.push((p.clone(), pos));
            }
        }
        out
    }

    pub fn buffer_index(&self) -> usize {
        self.current
    }

    pub(crate) fn switch_to(&mut self, index: usize) {
        if index == self.current || index >= self.docs.len() {
            return;
        }
        self.push_jump(self.cursor);
        let here = self.stash();
        self.docs[self.current] = here;
        let there = std::mem::take(&mut self.docs[index]);
        self.unstash(there);
        self.current = index;
        self.clamp();
        self.follow_file();
    }

    pub(crate) fn cycle_buffer(&mut self, by: isize) {
        let n = self.docs.len() as isize;
        let to = (self.current as isize + by).rem_euclid(n) as usize;
        self.switch_to(to);
    }

    fn disposable(&self) -> bool {
        let shown_elsewhere = self
            .wins
            .iter()
            .any(|w| w.id != self.focus && w.buffer == self.buffer_id);
        self.file.path.is_none()
            && self.buf.is_empty()
            && self.undo.is_empty()
            && !self.is_modified()
            && !shown_elsewhere
    }

    pub(crate) fn new_buffer(&mut self) {
        let here = self.stash();
        self.docs[self.current] = here;
        self.docs.push(Doc::default());
        self.current = self.docs.len() - 1;
        self.fresh_doc();
    }

    fn fresh_doc(&mut self) {
        let id = self.next_id;
        self.next_id += 1;
        self.unstash(Doc { id, ..Doc::default() });
        self.file.final_newline = true;
        self.saved_lines = vec![String::new()];
    }

    pub fn open_path(&mut self, path: &Path) -> std::io::Result<()> {
        if path.is_dir() {
            self.explore(path);
            return Ok(());
        }
        let canon = |p: &Path| crate::paths::canonical(p).ok();
        let want = canon(path);
        if want.is_some() {
            if self.file.path.as_deref().and_then(canon) == want {
                return Ok(());
            }
            let open = (0..self.docs.len())
                .find(|&i| i != self.current && self.docs[i].file.path.as_deref().and_then(canon) == want);
            if let Some(i) = open {
                self.switch_to(i);
                return Ok(());
            }
        }
        if self.disposable() {
            self.load(path)?;
            self.restore_place();
            return Ok(());
        }
        self.push_jump(self.cursor);
        let back = self.current;
        let here = self.stash();
        self.docs[self.current] = here;
        self.docs.push(Doc::default());
        self.current = self.docs.len() - 1;
        self.fresh_doc();
        if let Err(e) = self.load(path) {
            self.docs.pop();
            self.current = back;
            let there = std::mem::take(&mut self.docs[back]);
            self.unstash(there);
            return Err(e);
        }
        self.restore_place();
        Ok(())
    }

    fn open_or_report(&mut self, path: &Path) -> bool {
        let path = relative_to_cwd(path);
        match self.open_path(&path) {
            Ok(()) => true,
            Err(e) => {
                self.error(format!("\"{}\" {e}", path.display()));
                false
            }
        }
    }

    pub(crate) fn close_buffer(&mut self, force: bool) {
        if self.is_modified() && !force {
            let name = self.name();
            return self.error(format!(
                "E89: No write since last change for buffer \"{name}\" (add ! to override)"
            ));
        }
        self.leave_preview();
        self.remember_place();
        if self.docs.len() == 1 {
            self.fresh_doc();
            self.drop_orphan_windows();
            return;
        }
        let gone = self.current;
        let to = if gone + 1 < self.docs.len() { gone + 1 } else { gone - 1 };
        self.switch_to(to);
        self.docs.remove(gone);
        if gone < self.current {
            self.current -= 1;
        }
        self.drop_orphan_windows();
    }

    pub(crate) fn unsaved_elsewhere(&self) -> Option<String> {
        self.buffers()
            .into_iter()
            .find(|b| b.modified && !b.current)
            .map(|b| b.name)
    }

    pub(crate) fn write_all(&mut self) -> bool {
        let back = self.current;
        let mut ok = true;
        self.no_jumps = true;
        for i in 0..self.docs.len() {
            let modified = if i == self.current {
                self.is_modified()
            } else {
                self.docs[i].modified()
            };
            if modified {
                self.switch_to(i);
                ok &= self.write(None, false, None);
            }
        }
        self.switch_to(back);
        self.no_jumps = false;
        ok
    }

    pub fn grep_picker(&self) -> Option<&Grep> {
        self.grep.as_ref()
    }
    pub fn tick(&mut self) {
        if let Some(g) = self.grep.as_mut() {
            g.refresh();
        }
        self.git_menu_tick();
    }

    pub fn wait_for_grep(&mut self) {
        if let Some(g) = self.grep.as_mut() {
            g.wait();
        }
    }

    pub fn grep(&mut self, query: Option<&str>) {
        let root = cwd();
        self.grep_in(&root, query.unwrap_or(""));
    }

    pub fn grep_in(&mut self, root: &Path, query: &str) {
        self.reset_for_picker();
        self.grep = Some(Grep::new(root, query));
    }

    fn grep_input(&mut self, input: Input) {
        let Some(g) = self.grep.as_mut() else { return };
        let key = match input {
            Input::Paste(text) => return g.paste(&text),
            Input::Key(k) => k,
        };
        self.message = None;
        match g.key(key) {
            Jump::Nothing => {}
            Jump::Close => self.grep = None,
            Jump::Open { path, row, col, query } => {
                if self.open_or_report(&path) {
                    self.grep = None;
                    self.remember_jump();
                    self.cursor = Pos::new(row.min(self.buf.last_row()), col);
                    self.clamp();
                    self.set_want();
                    self.top.set(row.saturating_sub(self.text_rows() / 3));
                    self.land(true);
                    self.last_search = Some(crate::regex::escape(&query));
                    self.search_backward = false;
                    self.search_shown = true;
                }
            }
        }
    }

    pub fn pick_buffer_list(&mut self) {
        let root = cwd();
        let names: Vec<String> = self
            .buffers()
            .into_iter()
            .filter_map(|b| b.path)
            .map(|p| listed(&crate::paths::canonical(&p).unwrap_or(p)))
            .collect();
        self.pending.clear();
        self.finder = Some(Finder::from_list(&root, names, "open files"));
    }

    pub fn finder(&self) -> Option<&Finder> {
        self.finder.as_ref()
    }

    pub fn find_files(&mut self, query: Option<&str>) {
        let root = cwd();
        self.find_files_in(&root, query.unwrap_or(""));
    }

    pub fn find_files_in(&mut self, root: &Path, query: &str) {
        self.reset_for_picker();
        self.finder = Some(Finder::new(root, query));
    }

    fn finder_input(&mut self, input: Input) {
        let Some(f) = self.finder.as_mut() else { return };
        let key = match input {
            Input::Paste(text) => return f.paste(&text),
            Input::Key(k) => k,
        };
        self.message = None;
        match f.key(key) {
            Pick::Nothing => {}
            Pick::Close => self.finder = None,
            Pick::Open(path) => {
                if self.open_or_report(&path) {
                    self.finder = None;
                }
            }
        }
    }

    pub fn explorer(&self) -> Option<&Explorer> {
        self.explorer.as_ref()
    }

    pub fn explorer_focused(&self) -> bool {
        self.explorer.is_some() && self.explorer_focused && !self.zen()
    }

    pub fn zen(&self) -> bool {
        self.zen.is_some()
    }

    pub(crate) fn leave_zen(&mut self) {
        self.zen = None;
    }

    pub fn toggle_zen(&mut self) {
        match self.zen.take() {
            Some(focused) => self.explorer_focused = focused,
            None => {
                self.zen = Some(self.explorer_focused);
                self.explorer_focused = false;
            }
        }
    }

    pub(crate) fn file_dir(&self) -> PathBuf {
        self.file
            .path
            .as_deref()
            .map_or_else(|| PathBuf::from("."), |p| dir_of(p).to_path_buf())
    }

    // the working folder when the file is inside it, so its tree shows where the file sits
    fn sidebar_root(&self) -> PathBuf {
        let dir = self.file_dir();
        let cwd = std::env::current_dir().and_then(|d| crate::paths::canonical(&d)).ok();
        match (cwd, crate::paths::canonical(&dir).ok()) {
            (Some(cwd), Some(d)) if d.starts_with(&cwd) => cwd,
            _ => dir,
        }
    }

    fn current_file(&self) -> Option<PathBuf> {
        self.file.path.as_ref().and_then(|p| crate::paths::canonical(p).ok())
    }

    // keeps the open file showing in the sidebar, or moves the sidebar to its folder
    fn follow_file(&mut self) {
        let Some(path) = self.current_file() else { return };
        let Some(x) = self.explorer.as_mut() else { return };
        if !x.reveal(&path) {
            let dir = self.file_dir();
            let focused = self.explorer_focused;
            self.explore(&dir);
            self.explorer_focused = focused;
        }
    }

    pub fn toggle_explorer_focus(&mut self) {
        self.leave_zen();
        if self.explorer.is_none() {
            let dir = self.sidebar_root();
            self.explore(&dir);
        } else {
            self.explorer_focused = !self.explorer_focused;
        }
    }

    pub fn toggle_explorer(&mut self) {
        self.leave_zen();
        if self.explorer.take().is_none() {
            let dir = self.sidebar_root();
            self.explore(&dir);
        } else {
            self.explorer_focused = false;
        }
    }

    pub fn explore(&mut self, dir: &Path) {
        let current = self.current_file();
        match Explorer::open(dir, current.as_deref()) {
            Ok(x) => {
                self.explorer = Some(x);
                self.explorer_focused = true;
                self.mode = Mode::Normal;
                self.pending.clear();
                self.message = None;
                self.refresh_tree();
            }
            Err(e) => self.error(format!("\"{}\" {e}", dir.display())),
        }
    }

    pub fn explorer_rows(&self) -> usize {
        // less the sidebar's heading and the gap under it
        self.text_rows().saturating_sub(2).max(1)
    }

    fn explorer_input(&mut self, input: Input) {
        let rows = self.explorer_rows();
        let Some(x) = self.explorer.as_mut() else { return };
        let key = match input {
            Input::Paste(text) => return x.paste(&text),
            Input::Key(k) => k,
        };
        self.message = None;
        let root = x.root().to_path_buf();
        let action = x.key(key, rows);
        if self.explorer.as_ref().is_some_and(|x| x.root() != root) || matches!(key, Key::Char('R') | Key::Ctrl('l')) {
            self.refresh_tree();
        }
        match action {
            Action::Nothing => {}
            Action::Error(e) => self.error(e),
            Action::Command => self.begin_cmdline(':', String::new()),
            Action::Close => {
                self.explorer = None;
                self.explorer_focused = false;
            }
            Action::Leave => self.explorer_focused = false,
            Action::Open(path) => {
                self.open_or_report(&path);
            }
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
        let r = Register {
            text,
            linewise,
            block: false,
        };
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
                    block: false,
                })
            }
            '/' => self.last_search.clone().map(|text| Register {
                text,
                ..Register::default()
            }),
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
                    self.paste_normal(&text);
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
                Some(k) => self.switch_to(k.saturating_sub(1)),
                None => self.cycle_buffer(1),
            },
            Act::G('T') => self.cycle_buffer(-(n as isize)),
            Act::G(c) => self.normal_g(c, n, cmd.register),
            Act::Z(k) => self.normal_z(k, cmd.count),
            Act::Select { .. } => {}
            Act::Hunk(_) => self.reset_hunk(),
            Act::Window(k) => self.window_key(k, cmd.count),
            Act::Leader(c) => self.leader(c),
        }
        if change && !matches!(self.mode, Mode::Insert | Mode::Replace) {
            self.dot = self.building.take();
        }
    }

    pub(crate) fn leader(&mut self, c: char) {
        match c {
            'f' => self.find_files(None),
            'b' => self.pick_buffer_list(),
            'z' => self.toggle_zen(),
            'm' => self.toggle_preview(),
            'G' => self.open_git_menu(None),
            'r' => self.pick_recent(),
            _ => self.grep(None),
        }
    }

    fn reset_hunk(&mut self) {
        let row = self.cursor.row;
        let Some(h) = self
            .change_hunks()
            .iter()
            .find(|h| h.new.contains(&row) || h.new.is_empty() && h.row() == row)
            .cloned()
        else {
            return;
        };
        let old = self.git.with(|f| match &f.base {
            Base::Head(lines) => lines.get(h.old.clone()).map(<[String]>::to_vec),
            Base::Untracked => Some(Vec::new()),
            Base::Outside => self.saved_lines.get(h.old.clone()).map(<[String]>::to_vec),
        });
        let Some(old) = old else { return };
        self.touch();
        let mut lines = self.buf.lines().to_vec();
        lines.splice(h.new.clone(), old);
        self.buf.replace_all(lines);
        self.cursor = self.first_non_blank_pos(h.new.start.min(self.buf.last_row()));
        self.set_want();
        self.end_change();
    }

    pub fn lens(&self) -> Option<String> {
        let quiet = self.mode != Mode::Normal
            || !self.pending.is_empty()
            || self.explorer_focused()
            || self.preview_focused()
            || self.finder.is_some()
            || self.grep.is_some()
            || self.git_menu.is_some()
            || self.is_blank_slate();
        if !self.options.gitlens || quiet {
            return None;
        }
        let path = self.file.path.as_deref()?;
        let row = self.cursor.row;
        let head = self.git.with(|f| match &f.base {
            Base::Head(_) => f.head.clone().map(Some),
            Base::Untracked => Some(None),
            Base::Outside => None,
        })?;
        let Some(head) = head else {
            return Some("not committed yet".into());
        };
        let mut old = row;
        for h in self.change_hunks().iter() {
            if h.new.contains(&row) {
                return Some("not committed yet".into());
            }
            if h.new.end <= row {
                old = old + h.old.len() - h.new.len();
            }
        }
        let blame = self.blamer.get(path, &head)?;
        let commit = blame.line(old)?;
        Some(crate::blame::describe(commit, blame.me.as_deref(), crate::time::now()))
    }

    pub fn wait_for_blame(&self) {
        self.git.wait();
        let _ = self.lens();
        self.blamer.wait();
    }

    pub fn git_menu(&self) -> Option<&GitMenu> {
        self.git_menu.as_ref()
    }

    pub fn open_git_menu(&mut self, command: Option<&str>) {
        self.reset_for_picker();
        let start = match &self.file.path {
            Some(_) => self.file_dir(),
            None => cwd(),
        };
        let history = std::mem::take(&mut self.git_history);
        self.git_menu = Some(GitMenu::open(&start, history, command.map(str::to_string)));
    }

    fn close_git_menu(&mut self) {
        if let Some(m) = self.git_menu.take() {
            self.git_history = m.history;
        }
    }

    pub fn wait_for_git_menu(&mut self) {
        crate::sync::wait_until(|| {
            self.tick();
            !self.git_menu.as_ref().is_some_and(GitMenu::working)
        });
        self.git.wait();
    }

    fn git_menu_tick(&mut self) {
        let Some(m) = self.git_menu.as_mut() else { return };
        let changed = m.tick();
        if let Some(why) = m.failed().map(str::to_string) {
            self.close_git_menu();
            self.error(why);
            return;
        }
        if changed {
            self.after_git();
        }
        let early = match self.git_menu.as_mut() {
            Some(m) if m.ready() => std::mem::take(&mut m.early),
            _ => Vec::new(),
        };
        for k in early {
            if self.git_menu.is_some() {
                self.git_menu_input(Input::Key(k));
            }
        }
    }

    // the repo changed under us: redraw the gutter and branch, and reload a clean buffer
    fn after_git(&mut self) {
        self.git.probe(self.file.path.as_deref());
        self.refresh_tree();
        if self.is_modified() {
            return;
        }
        let Some(path) = self.file.path.clone() else { return };
        let Ok(text) = std::fs::read_to_string(&path) else {
            return;
        };
        if text == self.contents(None) {
            return;
        }
        let (cursor, top) = (self.cursor, self.top.get());
        if self.load(&path).is_ok() {
            self.cursor = cursor;
            self.top.set(top);
            self.clamp();
            self.message = None;
        }
    }

    fn git_menu_input(&mut self, input: Input) {
        let unsaved = self.is_modified()
            || self
                .docs
                .iter()
                .enumerate()
                .any(|(i, d)| i != self.current && d.modified());
        let Some(m) = self.git_menu.as_mut() else { return };
        let key = match input {
            Input::Paste(text) => return m.paste(&text),
            Input::Key(k) => k,
        };
        match m.key(key, unsaved) {
            Want::Nothing => {}
            Want::Close => self.close_git_menu(),
            Want::Open(path) => {
                if self.open_or_report(&path) {
                    self.close_git_menu();
                }
            }
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
            Key::Char(':') => self.begin_cmdline(':', String::new()),
            Key::Char('/') => self.begin_cmdline('/', String::new()),
            Key::Char('?') => self.begin_cmdline('?', String::new()),
            Key::Char('*') => self.search_word(true),
            Key::Char('#') => self.search_word(false),
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
            Key::Ctrl('z') => self.effects.push(Effect::Suspend),
            Key::Ctrl('o') => self.jump_older(n),
            Key::Ctrl('i') => self.jump_newer(n),
            // a terminal sends Tab for Ctrl-i
            Key::Tab if self.jumps.has_newer() => self.jump_newer(n),
            Key::Tab => self.tab_focus(),
            Key::Ctrl('p') => self.find_files(None),
            // terminals often keep Ctrl-q for themselves
            Key::Ctrl('v' | 'q') => self.enter_visual(Mode::VisualBlock),
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
            '@' => self.play_macro(ch, n),
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

    fn is_jump(m: Motion) -> bool {
        matches!(
            m,
            Motion::FirstLine
                | Motion::LastLine
                | Motion::Bracket
                | Motion::SearchNext(_)
                | Motion::Mark(..)
                | Motion::ScreenTop
                | Motion::ScreenMiddle
                | Motion::ScreenBottom
                | Motion::ParagraphForward
                | Motion::ParagraphBack
                | Motion::Hunk(_)
        )
    }

    fn move_cursor(&mut self, m: Motion, count: Option<usize>) {
        let Some((to, _)) = self.target(m, count, false) else {
            return;
        };
        let jump = Self::is_jump(m);
        if jump {
            self.jump_from(self.cursor);
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

    // searches always glow; other jumps only when they leave the screen
    pub(crate) fn land(&self, search: bool) {
        let top = self.top.get();
        if search || !(top..top + self.text_rows()).contains(&self.cursor.row) {
            self.anim.land();
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
            Motion::Left => {
                if p.col == 0 {
                    return None;
                }
                (Pos::new(p.row, p.col.saturating_sub(n)), Exclusive)
            }
            Motion::Right => {
                let max = if for_op || self.mode.is_visual() {
                    len
                } else {
                    len.saturating_sub(1)
                };
                if p.col >= max {
                    return None;
                }
                (Pos::new(p.row, (p.col + n).min(max)), Exclusive)
            }
            Motion::Up => {
                if p.row == 0 {
                    return None;
                }
                let row = p.row.saturating_sub(n);
                (Pos::new(row, self.col_for_want(row)), Linewise)
            }
            Motion::Down => {
                if p.row >= b.last_row() {
                    return None;
                }
                let row = (p.row + n).min(b.last_row());
                (Pos::new(row, self.col_for_want(row)), Linewise)
            }
            Motion::LineStart => (Pos::new(p.row, 0), Exclusive),
            Motion::FirstNonBlank => (fnb(p.row), Exclusive),
            Motion::LineEnd => {
                let row = (p.row + n - 1).min(b.last_row());
                (Pos::new(row, b.line_len(row).saturating_sub(1)), Inclusive)
            }
            Motion::LastNonBlank => {
                let row = (p.row + n - 1).min(b.last_row());
                let line = b.line(row);
                let end = char_count(line.trim_end_matches([' ', '\t']));
                (Pos::new(row, end.saturating_sub(1)), Inclusive)
            }
            Motion::WordForward(big) => {
                let mut at = p;
                let mut prev = p;
                for _ in 0..n {
                    prev = at;
                    let next = motion::word_forward(b, at, big);
                    if next == at {
                        break;
                    }
                    at = next;
                }
                if for_op {
                    // vim: dw on the last word stops at the end of the line
                    if at.row > prev.row && (at.col == 0 || at.col <= first_non_blank(b.line(at.row))) {
                        let mut row = at.row - 1;
                        while row > prev.row && b.line(row).trim().is_empty() {
                            row -= 1;
                        }
                        at = Pos::new(row, b.line_len(row));
                    }
                    if at == p {
                        return None;
                    }
                } else if at == p {
                    return None;
                } else if at.col >= b.line_len(at.row) && at.col > 0 {
                    at.col = b.line_len(at.row).saturating_sub(1);
                }
                (at, Exclusive)
            }
            Motion::WordBack(big) => (repeat(motion::word_back, big)?, Exclusive),
            Motion::WordEnd(big) => (repeat(motion::word_end, big)?, Inclusive),
            Motion::WordEndBack(big) => {
                let mut at = p;
                for _ in 0..n {
                    match motion::word_end_back(b, at, big) {
                        Some(q) => at = q,
                        None => {
                            at = Pos::new(0, 0);
                            break;
                        }
                    }
                }
                (at, Inclusive)
            }
            Motion::FirstLine => (fnb(count.map_or(0, |c| c - 1).min(b.last_row())), Linewise),
            Motion::LastLine => (fnb(count.map_or(b.last_row(), |c| c - 1).min(b.last_row())), Linewise),
            Motion::CountLine => (fnb((p.row + n - 1).min(b.last_row())), Linewise),
            Motion::NextLine => {
                if p.row >= b.last_row() {
                    return None;
                }
                (fnb((p.row + n).min(b.last_row())), Linewise)
            }
            Motion::PrevLine => {
                if p.row == 0 {
                    return None;
                }
                (fnb(p.row.saturating_sub(n)), Linewise)
            }
            Motion::Column => {
                let col = col_at(b.line(p.row), n - 1, self.options.tab_stop).min(len.saturating_sub(1));
                (Pos::new(p.row, col), Exclusive)
            }
            Motion::Find(f) => {
                self.last_find = Some(f);
                let col = motion::find_in_line(b.line(p.row), p.col, f, n, false)?;
                (Pos::new(p.row, col), if f.forward { Inclusive } else { Exclusive })
            }
            Motion::RepeatFind(reverse) => {
                let f = self.last_find?;
                let f = if reverse { f.reversed() } else { f };
                let col = motion::find_in_line(b.line(p.row), p.col, f, n, true)?;
                (Pos::new(p.row, col), if f.forward { Inclusive } else { Exclusive })
            }
            Motion::Bracket => {
                if let Some(c) = count {
                    let row = ((c.min(100) * b.len()).div_ceil(100)).saturating_sub(1);
                    return Some((fnb(row), Linewise));
                }
                (motion::match_bracket(b, p)?, Inclusive)
            }
            Motion::ParagraphForward => {
                let mut at = p;
                for _ in 0..n {
                    at = motion::paragraph_forward(b, at);
                }
                if !for_op && at.col > 0 {
                    at.col -= 1;
                }
                (at, Exclusive)
            }
            Motion::ParagraphBack => {
                let mut at = p;
                for _ in 0..n {
                    at = motion::paragraph_back(b, at);
                }
                (at, Exclusive)
            }
            Motion::ScreenTop | Motion::ScreenMiddle | Motion::ScreenBottom => {
                let top = self.top.get();
                let rows = self.text_rows();
                let bottom = (top + rows - 1).min(b.last_row());
                let so = self.scroll_off();
                let row = match m {
                    Motion::ScreenTop => {
                        let off = if top == 0 { 0 } else { so };
                        (top + (n - 1).max(off)).min(bottom)
                    }
                    Motion::ScreenBottom => {
                        let off = if bottom == b.last_row() { 0 } else { so };
                        bottom.saturating_sub((n - 1).max(off)).max(top)
                    }
                    _ => top + (bottom - top) / 2,
                };
                (fnb(row), Linewise)
            }
            Motion::SearchNext(reverse) => {
                let Some(pat) = self.last_search.clone() else {
                    self.error("E35: No previous regular expression");
                    return None;
                };
                let forward = self.search_backward == reverse;
                let mut at = p;
                for _ in 0..n {
                    at = self.find(&pat, at, forward, true)?;
                }
                self.search_shown = true;
                (at, Exclusive)
            }
            Motion::Hunk(forward) => {
                let rows = self.change_hunks().iter().map(Hunk::row).collect::<Vec<_>>();
                // like vim: no wrapping, and a big count stops at the last one
                let row = if forward {
                    rows.iter().filter(|&&r| r > p.row).take(n).last()
                } else {
                    rows.iter().rev().filter(|&&r| r < p.row).take(n).last()
                };
                (fnb((*row?).min(b.last_row())), Linewise)
            }
            Motion::Mark(c, exact) => {
                let c = if c == '`' { '\'' } else { c };
                let mut mark = self.marks.get(&c).copied();
                // '' where we already are goes to the last jump from somewhere else
                if c == '\'' && mark.is_none_or(|m| m.row == p.row && (!exact || m.col == p.col)) {
                    mark = self.jumps.latest_elsewhere(self.buffer_id, p.row).or(mark);
                }
                let Some(at) = mark else {
                    self.error("E20: Mark not set");
                    return None;
                };
                let at = self.clamp_pos(at);
                if exact {
                    (at, Exclusive)
                } else {
                    (fnb(at.row), Linewise)
                }
            }
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
                if Self::is_jump(m) {
                    self.jump_from(self.cursor);
                }
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
            (
                Pos::new(r.start.row, 0),
                Pos::new(r.end.row, self.buf.line_len(r.end.row)),
            )
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
        if !r.linewise && !r.block && !r.text.contains('\n') {
            self.cursor = Pos::new(end.row, end.col.saturating_sub(1));
        }
        self.set_want();
    }

    fn put_register(&mut self, r: &Register, count: usize, before: bool) -> Pos {
        if r.block {
            return self.put_block(r, count, before);
        }
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

    fn paste_normal(&mut self, text: &str) {
        self.touch();
        let len = self.buf.line_len(self.cursor.row);
        let at = Pos::new(self.cursor.row, (self.cursor.col + 1).min(len));
        let end = self.buf.insert(at, text);
        self.cursor = Pos::new(end.row, end.col.saturating_sub(1));
        self.end_change();
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

    fn play_macro(&mut self, name: char, n: usize) {
        let name = if name == '@' {
            match self.last_macro {
                Some(m) => m,
                None => {
                    self.error("E748: No previously used register");
                    return;
                }
            }
        } else {
            name
        };
        if name == ':' {
            self.last_macro = Some(name);
            if let Some(last) = self.cmd_history.last().cloned() {
                for _ in 0..n {
                    self.ex(&last);
                }
            }
            return;
        }
        let Some(inputs) = self.macros.get(&name.to_ascii_lowercase()).cloned().or_else(|| {
            self.registers
                .get(&name.to_ascii_lowercase())
                .map(|r| crate::keys::parse(&r.text).into_iter().map(Input::Key).collect())
        }) else {
            return;
        };
        if self.macro_depth > 20 {
            self.error("E169: Command too recursive");
            return;
        }
        self.last_macro = Some(name);
        self.macro_depth += 1;
        self.end_change();
        for _ in 0..n {
            for i in &inputs {
                self.input(i.clone());
            }
        }
        self.macro_depth -= 1;
    }

    fn stop_recording(&mut self) {
        if let Some((name, mut keys)) = self.recording.take() {
            // drop the q that ended the recording
            keys.pop();
            let text: String = keys
                .iter()
                .map(|i| match i {
                    Input::Key(k) => k.to_string(),
                    Input::Paste(t) => t.clone(),
                })
                .collect();
            // qA adds to what a already holds
            let (mut text, mut keys) = (text, keys);
            if name.is_ascii_uppercase() {
                let lower = name.to_ascii_lowercase();
                let old = self.registers.get(&lower).map_or(String::new(), |r| r.text.clone());
                let mut old_keys = self
                    .macros
                    .get(&lower)
                    .cloned()
                    .unwrap_or_else(|| crate::keys::parse(&old).into_iter().map(Input::Key).collect());
                old_keys.append(&mut keys);
                keys = old_keys;
                text = old + &text;
            }
            let name = name.to_ascii_lowercase();
            if name.is_ascii_alphabetic() {
                self.registers.insert(
                    name,
                    Register {
                        text,
                        ..Register::default()
                    },
                );
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
        if self.completion_input(&input) {
            return;
        }
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

    pub fn completion(&self) -> Option<(&str, &[String], Option<usize>)> {
        let c = self.completion.as_ref()?;
        Some((c.prefix.as_str(), c.words.as_slice(), c.at))
    }

    fn completion_input(&mut self, input: &Input) -> bool {
        let key = match input {
            Input::Key(k) => Some(*k),
            Input::Paste(_) => None,
        };
        if self.completion.is_some() {
            match key {
                Some(Key::Ctrl(c @ ('n' | 'p'))) => self.cycle_completion(c == 'p'),
                Some(Key::Ctrl('y')) => self.accept_completion(),
                Some(Key::Ctrl('e') | Key::Esc) => {
                    self.pick_completion(None);
                    self.completion = None;
                }
                _ => {
                    self.accept_completion();
                    return false;
                }
            }
            return true;
        }
        if self.mode == Mode::Insert && matches!(key, Some(Key::Ctrl('n' | 'p'))) {
            self.start_completion(key == Some(Key::Ctrl('p')));
            return true;
        }
        false
    }

    fn start_completion(&mut self, back: bool) {
        let p = self.cursor;
        let before: Vec<char> = self.buf.line(p.row).chars().take(p.col).collect();
        let len = before.iter().rev().take_while(|c| is_word(**c)).count();
        let prefix: String = before[before.len() - len..].iter().collect();
        let start = Pos::new(p.row, p.col - len);
        let others: Vec<&[String]> = (self.docs.iter().enumerate())
            .filter(|(i, _)| *i != self.current)
            .map(|(_, d)| d.buf.lines())
            .collect();
        let words = crate::complete::candidates(self.buf.lines(), start, &prefix, &others);
        if words.is_empty() {
            return self.error("Pattern not found");
        }
        self.completion = Some(Completion {
            start,
            prefix,
            words,
            at: None,
        });
        self.cycle_completion(back);
    }

    fn cycle_completion(&mut self, back: bool) {
        let Some(c) = &self.completion else { return };
        let n = c.words.len();
        let at = match (c.at, back) {
            (None, false) => Some(0),
            (None, true) => Some(n - 1),
            (Some(i), false) => (i + 1 < n).then_some(i + 1),
            (Some(i), true) => i.checked_sub(1),
        };
        self.pick_completion(at);
    }

    fn pick_completion(&mut self, at: Option<usize>) {
        let Some(c) = self.completion.as_mut() else { return };
        c.at = at;
        let text = at.map_or(&c.prefix, |i| &c.words[i]).clone();
        let start = c.start;
        self.touch();
        self.buf.delete(start, self.cursor);
        self.cursor = self.buf.insert(start, &text);
        self.insert.auto_indent = None;
        self.set_want();
    }

    // dot and counts replay the chosen word as typed text, not as another search
    fn accept_completion(&mut self) {
        let Some(c) = self.completion.take() else { return };
        let Some(i) = c.at else { return };
        let rest = c.words[i][c.prefix.len()..].to_string();
        if rest.is_empty() {
            return;
        }
        if let Some(b) = self.building.as_mut() {
            b.inputs.push(Input::Paste(rest.clone()));
        }
        self.insert.typed.push(Input::Paste(rest));
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
        self.finish_block_insert();
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
        if self.mode == Mode::VisualBlock {
            let blk = self.block();
            let (from, _) = self.block_cols(blk.top, &blk);
            let (_, to) = self.block_cols(blk.bottom, &blk);
            return Range {
                start: Pos::new(blk.top, from),
                end: Pos::new(blk.bottom, to),
                linewise: false,
            };
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
        if self.mode == Mode::VisualBlock && self.run_block(&cmd) {
            return;
        }
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
            Act::Key(Key::Ctrl('v' | 'q')) => self.switch_visual(Mode::VisualBlock),
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

    // v in visual mode leaves it, V switches to lines, and so on
    fn switch_visual(&mut self, want: Mode) {
        if self.mode == want {
            self.leave_visual();
        } else {
            self.mode = want;
        }
    }

    fn visual_key(&mut self, c: char, register: Option<char>, n: usize) {
        let r = self.visual_range();
        let lines = Range::lines(r.start.row, r.last_row());
        match c {
            'v' => self.switch_visual(Mode::Visual),
            'V' => self.switch_visual(Mode::VisualLine),
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
                self.begin_cmdline(':', "'<,'>".to_string());
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

    pub(crate) fn begin_cmdline(&mut self, prompt: char, text: String) {
        self.message = None;
        self.cmd = CmdLine {
            cursor: char_count(&text),
            text,
            prompt,
            browse: None,
            origin: self.cursor,
            origin_top: self.top.get(),
            completing: None,
        };
        self.mode = if prompt == ':' { Mode::Command } else { Mode::Search };
    }

    fn cmdline_input(&mut self, input: Input) {
        let key = match input {
            Input::Paste(text) => {
                let first = text.lines().next().unwrap_or("");
                let at = crate::text::byte_at(&self.cmd.text, self.cmd.cursor);
                self.cmd.text.insert_str(at, first);
                self.cmd.cursor += char_count(first);
                self.preview_search();
                return;
            }
            Input::Key(k) => k,
        };
        if !matches!(key, Key::Tab | Key::BackTab) {
            self.cmd.completing = None;
        }
        let edit = match key {
            Key::Esc | Key::Ctrl('c') => {
                self.cancel_cmdline();
                return;
            }
            Key::Enter | Key::Ctrl('j') => return self.finish_cmdline(),
            Key::Tab if self.mode == Mode::Command => return self.complete(false),
            Key::BackTab if self.mode == Mode::Command => return self.complete(true),
            Key::Backspace | Key::Ctrl('h') if self.cmd.text.is_empty() => {
                self.cancel_cmdline();
                return;
            }
            Key::Up | Key::Down => {
                self.browse_history(key == Key::Up);
                None
            }
            Key::Ctrl('h') => Some(Key::Backspace),
            Key::Ctrl('b') => Some(Key::Home),
            // vim's keys for the line; tinsel's others (Ctrl-A, Ctrl-K) mean something else here
            Key::Char(_)
            | Key::Backspace
            | Key::Delete
            | Key::Ctrl('w' | 'u' | 'e')
            | Key::Left
            | Key::Right
            | Key::Home
            | Key::End => Some(key),
            _ => None,
        };
        if let Some(key) = edit {
            let mut line = TextInput::new().word(is_word);
            line.focus(true);
            line.set_value(&self.cmd.text);
            line.set_cursor(self.cmd.cursor);
            line.edit(&tinsel::app::Event::Key(key));
            self.cmd.text = line.value();
            self.cmd.cursor = line.cursor();
        }
        self.preview_search();
    }

    fn browse_history(&mut self, back: bool) {
        let history = if self.mode == Mode::Command {
            &self.cmd_history
        } else {
            &self.search_history
        };
        if history.is_empty() {
            return;
        }
        let at = match (self.cmd.browse, back) {
            (None, true) => Some(history.len() - 1),
            (None, false) => return,
            (Some(i), true) => Some(i.saturating_sub(1)),
            (Some(i), false) if i + 1 < history.len() => Some(i + 1),
            (Some(_), false) => None,
        };
        self.cmd.browse = at;
        self.cmd.text = at.map(|i| history[i].clone()).unwrap_or_default();
        self.cmd.cursor = char_count(&self.cmd.text);
    }

    fn preview_search(&mut self) {
        if self.mode != Mode::Search || !self.options.incremental_search {
            return;
        }
        self.cursor = self.cmd.origin;
        self.top.set(self.cmd.origin_top);
        if self.cmd.text.is_empty() {
            return;
        }
        let pat = self.cmd.text.clone();
        let message = self.message.take();
        if let Some(p) = self.find(&pat, self.cmd.origin, self.cmd.prompt == '/', true) {
            self.cursor = p;
        }
        self.message = message;
    }

    fn cancel_cmdline(&mut self) {
        if self.mode == Mode::Search {
            self.cursor = self.cmd.origin;
            self.top.set(self.cmd.origin_top);
        }
        self.mode = Mode::Normal;
        self.cmd.text.clear();
        self.message = None;
    }

    fn finish_cmdline(&mut self) {
        let text = std::mem::take(&mut self.cmd.text);
        let prompt = self.cmd.prompt;
        self.mode = Mode::Normal;
        if prompt == ':' {
            remember(&mut self.cmd_history, &text);
            self.ex(&text);
        } else {
            let pat = if text.is_empty() {
                self.last_search.clone().unwrap_or_default()
            } else {
                text.clone()
            };
            if !text.is_empty() {
                remember(&mut self.search_history, &text);
            }
            if pat.is_empty() {
                self.error("E35: No previous regular expression");
                return;
            }
            self.cursor = self.cmd.origin;
            self.last_search = Some(pat.clone());
            self.search_backward = prompt == '?';
            self.search_shown = true;
            match self.find(&pat, self.cmd.origin, prompt == '/', true) {
                Some(p) => {
                    self.jump_from(self.cmd.origin);
                    self.cursor = p;
                    self.set_want();
                    self.land(true);
                    if self.message.is_none() {
                        self.info(format!("{prompt}{pat}"));
                    }
                }
                None => self.top.set(self.cmd.origin_top),
            }
        }
        self.clamp();
    }

    pub fn cmd_suggestions(&self) -> (Vec<crate::suggest::Suggestion>, Option<usize>) {
        if let Some((list, at)) = &self.cmd.completing {
            return (list.clone(), Some(*at));
        }
        let names: Vec<String> = self.buffers().into_iter().map(|b| b.name).collect();
        (crate::suggest::suggestions(&self.cmd.text, &self.options, &names), None)
    }

    fn complete(&mut self, back: bool) {
        let (list, at) = match self.cmd.completing.take() {
            Some((list, at)) => {
                let n = list.len();
                let at = if back { (at + n - 1) % n } else { (at + 1) % n };
                (list, at)
            }
            None => {
                let list = self.cmd_suggestions().0;
                if list.is_empty() {
                    return;
                }
                let at = if back { list.len() - 1 } else { 0 };
                (list, at)
            }
        };
        self.cmd.text = list[at].replace.clone();
        self.cmd.cursor = char_count(&self.cmd.text);
        // a folder: next Tab lists inside it
        if list.len() > 1 && !list[at].replace.ends_with('/') {
            self.cmd.completing = Some((list, at));
        }
    }

    pub(crate) fn regex(&self, pat: &str) -> Result<Rc<Regex>, String> {
        let case = (self.options.ignore_case, self.options.smart_case);
        let mut cache = self.regex_cache.borrow_mut();
        if let Some((p, c, re)) = cache.as_ref() {
            if p == pat && *c == case {
                return Ok(re.clone());
            }
        }
        let mut re = Regex::new(pat)?;
        re.set_case(case.0, case.1);
        let re = Rc::new(re);
        *cache = Some((pat.to_string(), case, re.clone()));
        Ok(re)
    }

    pub(crate) fn bad_pattern(&mut self, pat: &str, e: &str) {
        self.error(format!("E383: Invalid search string: {pat} ({e})"));
    }

    pub fn matches_in(&self, line: &str, pat: &str) -> Vec<(usize, usize)> {
        self.regex(pat).map(|re| re.spans(line)).unwrap_or_default()
    }

    pub(crate) fn find(&mut self, pat: &str, from: Pos, forward: bool, skip_here: bool) -> Option<Pos> {
        let re = match self.regex(pat) {
            Ok(re) => re,
            Err(e) => {
                self.bad_pattern(pat, &e);
                return None;
            }
        };
        let rows = self.buf.len();
        let hits =
            |e: &Self, row: usize| -> Vec<usize> { re.spans(e.buf.line(row)).into_iter().map(|m| m.0).collect() };
        let mut wrapped = false;
        if forward {
            for i in 0..=rows {
                let row = (from.row + i) % rows;
                if from.row + i >= rows {
                    wrapped = true;
                }
                let found = hits(self, row).into_iter().find(|&c| {
                    i > 0 && i < rows
                        || i == 0 && (c > from.col || !skip_here && c == from.col)
                        || i == rows && c <= from.col
                });
                if let Some(c) = found {
                    if wrapped {
                        self.info("search hit BOTTOM, continuing at TOP");
                    }
                    return Some(Pos::new(row, c));
                }
            }
        } else {
            for i in 0..=rows {
                let row = (from.row + rows * 2 - i) % rows;
                if i > from.row {
                    wrapped = true;
                }
                let found = hits(self, row).into_iter().rev().find(|&c| {
                    i > 0 && i < rows
                        || i == 0 && (c < from.col || !skip_here && c == from.col)
                        || i == rows && c >= from.col
                });
                if let Some(c) = found {
                    if wrapped {
                        self.info("search hit TOP, continuing at BOTTOM");
                    }
                    return Some(Pos::new(row, c));
                }
            }
        }
        self.error(format!("E486: Pattern not found: {pat}"));
        None
    }

    fn word_under_cursor(&self) -> Option<(usize, String)> {
        let chars: Vec<char> = self.buf.line(self.cursor.row).chars().collect();
        let mut col = self.cursor.col;
        while col < chars.len() && !is_word(chars[col]) {
            col += 1;
        }
        if col >= chars.len() {
            return None;
        }
        let mut start = col;
        while start > 0 && is_word(chars[start - 1]) {
            start -= 1;
        }
        let mut end = col;
        while end < chars.len() && is_word(chars[end]) {
            end += 1;
        }
        Some((start, chars[start..end].iter().collect()))
    }

    fn search_word(&mut self, forward: bool) {
        let Some((col, word)) = self.word_under_cursor() else {
            self.error("E348: No string under cursor");
            return;
        };
        let pat = format!("\\<{}\\>", crate::regex::escape(&word));
        self.last_search = Some(pat.clone());
        self.search_backward = !forward;
        self.search_shown = true;
        remember(&mut self.search_history, &pat);
        let start = self.cursor;
        self.message = None;
        // from the start of the word, so # doesn't land on it
        let Some(at) = self.find(&pat, Pos::new(start.row, col), forward, true) else {
            return;
        };
        self.jump_from(start);
        self.cursor = at;
        self.set_want();
        self.land(true);
        if self.message.is_none() {
            let prompt = if forward { '/' } else { '?' };
            self.info(format!("{prompt}{pat}"));
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
            self.explore(path);
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
        self.git.probe(self.file.path.as_deref());
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
        self.follow_file();
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
            self.git.probe(self.file.path.as_deref());
        }
        self.refresh_tree();
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

    pub(crate) fn push_effect(&mut self, e: Effect) {
        self.effects.push(e);
    }

    pub(crate) fn remember_jump(&mut self) {
        self.jump_from(self.cursor);
    }

    fn jump_from(&mut self, from: Pos) {
        self.marks.insert('\'', from);
        self.push_jump(from);
    }

    fn push_jump(&mut self, pos: Pos) {
        if self.no_jumps {
            return;
        }
        self.jumps.push(Spot { pos, ..self.here() });
    }

    fn here(&self) -> Spot {
        Spot {
            buffer: self.buffer_id,
            path: self.file.path.clone(),
            pos: self.cursor,
        }
    }

    fn jump_older(&mut self, mut n: usize) {
        let mut here = self.here();
        while let Some(spot) = self.jumps.back(here.clone(), n) {
            if self.go_to_spot(spot) {
                return;
            }
            here = self.here();
            n = 1;
        }
    }

    fn jump_newer(&mut self, mut n: usize) {
        while let Some(spot) = self.jumps.forward(n) {
            if self.go_to_spot(spot) {
                return;
            }
            n = 1;
        }
    }

    fn go_to_spot(&mut self, spot: Spot) -> bool {
        self.no_jumps = true;
        let found = if spot.buffer == self.buffer_id {
            true
        } else if let Some(i) = (0..self.docs.len()).find(|&i| i != self.current && self.docs[i].id == spot.buffer) {
            self.switch_to(i);
            true
        } else if let Some(path) = &spot.path {
            match self.open_path(path) {
                Ok(()) => true,
                Err(e) => {
                    self.error(format!("\"{}\" {e}", path.display()));
                    false
                }
            }
        } else {
            false
        };
        self.no_jumps = false;
        if found {
            self.leave_visual_if_any();
            self.mode = Mode::Normal;
            self.cursor = self.clamp_pos(spot.pos);
            self.set_want();
        }
        found
    }

    pub(crate) fn marks(&self) -> &HashMap<char, Pos> {
        &self.marks
    }

    pub(crate) fn set_cursor(&mut self, p: Pos) {
        self.cursor = p;
        self.set_want();
    }

    pub(crate) fn registers_list(&self) -> Vec<(char, Register)> {
        let mut all: Vec<(char, Register)> = self
            .registers
            .iter()
            .filter(|(k, _)| !k.is_control())
            .map(|(k, v)| (*k, v.clone()))
            .collect();
        all.sort_by_key(|(k, _)| {
            let rank = match k {
                '"' => 0,
                c if c.is_ascii_digit() => 1,
                '-' => 2,
                _ => 3,
            };
            (rank, *k)
        });
        all
    }

    pub(crate) fn reset_after_reload(&mut self) {
        self.mode = Mode::Normal;
    }

    pub(crate) fn leave_visual_if_any(&mut self) {
        if self.mode.is_visual() {
            self.leave_visual();
        }
    }

    fn reset_for_picker(&mut self) {
        self.pending.clear();
        self.message = None;
        self.leave_visual_if_any();
        self.mode = Mode::Normal;
    }

    pub(crate) fn dirty_guard(&mut self, force: bool) -> bool {
        if self.is_modified() && !force {
            self.error("E37: No write since last change (add ! to override)");
            return false;
        }
        true
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

fn remember(history: &mut Vec<String>, entry: &str) {
    if entry.is_empty() {
        return;
    }
    history.retain(|h| h != entry);
    history.push(entry.to_string());
    if history.len() > MAX_HISTORY {
        history.remove(0);
    }
}

pub fn find_all(line: &str, pat: &str, ignore_case: bool) -> Vec<usize> {
    let hay: Vec<char> = line.chars().collect();
    let needle: Vec<char> = pat.chars().collect();
    if needle.is_empty() || needle.len() > hay.len() {
        return Vec::new();
    }
    let same = |a: char, b: char| a == b || ignore_case && a.to_lowercase().eq(b.to_lowercase());
    let mut out = Vec::new();
    let mut i = 0;
    while i + needle.len() <= hay.len() {
        if needle.iter().enumerate().all(|(j, &c)| same(hay[i + j], c)) {
            out.push(i);
            i += needle.len();
        } else {
            i += 1;
        }
    }
    out
}

fn display_name(path: Option<&Path>) -> String {
    path.map_or_else(|| "[No Name]".to_string(), |p| p.display().to_string())
}

pub(crate) fn dir_of(path: &Path) -> &Path {
    path.parent()
        .filter(|d| !d.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
}

pub(crate) fn cwd() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

pub(crate) fn relative_to_cwd(path: &Path) -> PathBuf {
    let cwd = std::env::current_dir()
        .ok()
        .and_then(|d| crate::paths::canonical(&d).ok());
    match cwd.as_deref().and_then(|d| path.strip_prefix(d).ok()) {
        Some(rel) if !rel.as_os_str().is_empty() => rel.to_path_buf(),
        _ => path.to_path_buf(),
    }
}

/// A path as the finder lists it: relative to the working directory, with `/` between its
/// parts on every system, so its name is found the same way everywhere.
pub(crate) fn listed(path: &Path) -> String {
    slashed(&relative_to_cwd(path).display().to_string(), cfg!(windows))
}

/// On Windows, without any `\\?\` in front and with `/` between the parts.
fn slashed(path: &str, windows: bool) -> String {
    if !windows {
        return path.to_string();
    }
    crate::paths::plain(path).as_deref().unwrap_or(path).replace('\\', "/")
}

pub(crate) fn expand_home(path: &str) -> String {
    match (path.strip_prefix("~/"), std::env::var("HOME")) {
        (Some(rest), Ok(home)) => format!("{home}/{rest}"),
        _ => path.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::slashed;

    #[test]
    fn windows_paths_are_listed_with_slashes_and_without_their_prefix() {
        assert_eq!(slashed(r"\\?\C:\Users\me\a.rs", true), "C:/Users/me/a.rs");
        assert_eq!(slashed(r"\\?\UNC\server\share\a.rs", true), "//server/share/a.rs");
        assert_eq!(slashed(r"src\ui\mod.rs", true), "src/ui/mod.rs");
        assert_eq!(slashed(r"src\ui\mod.rs", false), r"src\ui\mod.rs");
    }
}
