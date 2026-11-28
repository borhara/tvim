//! The editor: buffers, modes and everything a key can do, without drawing any of it. The
//! types and the getters the UI reads live here; the work is split by what it's about:
//!
//! - `normal`, `insert`, `visual`, `block` and `cmdline`: the modes, and the keys in each
//! - `input`: where keys come in, and mappings
//! - `operators` and `repeat`: changing text, registers, dot and macros
//! - `undo`, `search` and `jumps`: history, searching, the jump list and marks
//! - `buffers` and `view`: open files, and how they sit on the screen
//! - `lsp`, `git` and `pickers`: language servers, the git menu, and the overlays

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use tinsel::app::Key;

use crate::blame::Blamer;
use crate::buffer::{Buffer, Pos};
use crate::clipboard::{Clipboard, System};
use crate::diff::{self, Hunk, Mark};
use crate::explorer::Explorer;
use crate::finder::Finder;
use crate::git::{Base, Git, Tree};
use crate::gitmenu::GitMenu;
use crate::grep::Grep;
use crate::jumps::JumpList;
use crate::languages::Languages;
use crate::mapping::Maps;
use crate::motion::{self, Find};
use crate::preview::Preview;
use crate::regex::Regex;
use crate::servers::Servers;
use crate::syntax::{self, State, Syntax};
use crate::window::{Node, Win};

mod block;
mod buffers;
mod cmdline;
mod conflicts;
mod diffmode;
mod editorconfig;
mod folds;
mod git;
mod help;
mod history;
mod input;
mod insert;
mod jumps;
mod lsp;
mod multi;
mod normal;
mod operators;
mod pickers;
mod quickfix;
mod repeat;
mod search;
mod spelling;
mod surround;
mod undo;
mod view;
mod visual;
pub use block::Block;
pub(crate) use surround::{can_add as can_surround, can_find as can_unsurround};

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
    pub lsp: bool,
    // a language server's completions show as you type
    pub auto_complete: bool,
    // the language server tidies a file before it's written
    pub format_on_save: bool,
    // words spelled wrong are underlined, in prose and comments
    pub spell: bool,
    pub roomy: bool,
    pub zero_pad: bool,
    pub wrap: bool,
    pub linebreak: bool,
    // how wide gq makes lines, and typing past it breaks them; 0 for 79 and no breaking
    pub text_width: usize,
    pub undo_file: bool,
    pub update_check: bool,
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
            lsp: true,
            auto_complete: true,
            format_on_save: false,
            spell: true,
            roomy: true,
            zero_pad: false,
            wrap: false,
            linebreak: false,
            text_width: 0,
            undo_file: true,
            update_check: true,
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
            "lsp" => ("lsp", &mut self.lsp),
            "autocomplete" | "acp" => ("autocomplete", &mut self.auto_complete),
            "formatonsave" | "fos" => ("formatonsave", &mut self.format_on_save),
            "spell" => ("spell", &mut self.spell),
            "roomy" | "ro" => ("roomy", &mut self.roomy),
            "zeropad" | "zp" => ("zeropad", &mut self.zero_pad),
            "wrap" => ("wrap", &mut self.wrap),
            "linebreak" | "lbr" => ("linebreak", &mut self.linebreak),
            "undofile" | "udf" => ("undofile", &mut self.undo_file),
            "updatecheck" | "uc" => ("updatecheck", &mut self.update_check),
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
    // read as one empty line, which a buffer with nothing in it still writes back
    pub blank_line: bool,
    // a picture, shown rather than edited; the buffer is left empty
    pub image: Option<tinsel::image::Image>,
    // the name of text that isn't a file, as a.rs @ HEAD, to read but not change
    pub scratch: Option<String>,
    // what the project's .editorconfig says about it
    pub editorconfig: crate::editorconfig::Settings,
    // when it was last changed on disk, as of reading or writing it, to see when something
    // else changes it
    pub mtime: Option<std::time::SystemTime>,
}

#[derive(Clone, Debug, Default)]
struct Snapshot {
    // shared with the history, which holds the same text
    lines: Rc<[String]>,
    cursor: Pos,
    version: u64,
    marks: HashMap<char, Pos>,
}

// the text as a change left it, and when, which a file's kept history doesn't know
#[derive(Clone, Debug, Default)]
struct Moment {
    lines: Rc<[String]>,
    cursor: Pos,
    when: Option<i64>,
    // its version of the text, and that of the text the change was made to
    version: u64,
    from: Option<u64>,
}

#[derive(Clone, Debug, Default)]
struct Dot {
    count: Option<usize>,
    register: Option<char>,
    inputs: Vec<Input>,
    // a change made in visual mode repeats on as much text from the cursor
    visual: Option<VisualSize>,
}

/// How much a visual selection held, to select as much again: lines, and either characters
/// on a single line, the column it ended at on its last line, or a block's width.
#[derive(Clone, Copy, Debug)]
struct VisualSize {
    mode: Mode,
    lines: usize,
    cols: usize,
    to_end: bool,
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
    // where the text the choices replace starts, and what it was before any went in
    from: Pos,
    typed: String,
    // the word before the cursor, which the choices are matched against
    prefix: String,
    choices: Vec<Choice>,
    at: Option<usize>,
    // shown as the server answered while typing, with nothing put in until one is chosen, and
    // what it said, to match again as more is typed
    auto: Option<Vec<crate::languages::Item>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Asking {
    // by Ctrl-n, or by Ctrl-p to start from the end
    Typed { back: bool },
    // by typing, to show what's offered without choosing any
    Auto,
}

// the closed folds, outermost first, for the version of the text and the first lines closed
type FoldCache = (u64, Vec<usize>, Vec<(usize, usize)>);

// the changes between the windows compared, for their buffers and versions
type DiffPairCache = ((usize, u64, usize, u64), Rc<[Hunk]>);

// the merge conflicts in a buffer, for its id and the version of its text
type ConflictCache = ((usize, u64), Vec<crate::conflict::Conflict>);

// what the menu by the cursor is choosing between, to do once one is chosen
#[derive(Clone, Debug)]
pub(crate) enum Choosing {
    // and each open file's version when they were asked for, to check their edits against
    Actions(Vec<crate::languages::Action>, Vec<(usize, u64)>),
    // words to put in place of the one from one place to another
    Spelling(Pos, Pos, Vec<String>),
    // states the text has been in, by their place in its history
    History(Vec<usize>),
}

// one of the things offered to type, a word from the open files or what a server suggests
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Choice {
    pub label: String,
    // what goes in, after the first `keep` characters of what was typed
    text: String,
    keep: usize,
    pub kind: Option<&'static str>,
    pub detail: String,
    extra: Vec<crate::languages::Edit>,
}

#[derive(Clone, Debug, Default)]
pub struct CmdLine {
    pub text: String,
    pub cursor: usize,
    pub prompt: char,
    browse: Option<usize>,
    origin: Pos,
    pub(crate) origin_top: usize,
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
    history: VecDeque<Moment>,
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
    closed: Vec<usize>,
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
    pub id: usize,
    pub buf: &'a Buffer,
    pub path: Option<&'a Path>,
    pub image: Option<&'a tinsel::image::Image>,
    pub scratch: Option<&'a str>,
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
        // a scratch buffer's name starts with the file it's a copy of
        let name = match (self.path, self.scratch) {
            (Some(p), _) => p.file_name()?.to_str()?,
            (None, name) => name?.split(' ').next()?,
        };
        Syntax::detect(name)
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
    // the last line of the closed fold this row shows, if it shows one
    pub fold: Option<usize>,
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
    // every state the text has been in, in the order it got there, undone ones too
    history: VecDeque<Moment>,
    version: u64,
    saved: u64,
    versions: u64,
    changing: bool,
    // above zero while :g runs, so the changes it makes undo as one
    pub(crate) grouped: usize,
    // where undo history is kept between runs, if anywhere
    undo_dir: Option<PathBuf>,
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
    // completions asked of the language server, to show when they come
    completion_asked: Option<Asking>,
    recording: Option<(char, Vec<Input>)>,
    macros: HashMap<char, Vec<Input>>,
    last_macro: Option<char>,
    macro_depth: usize,
    pub(crate) maps: Maps,
    pub(crate) servers: Servers,
    pub(crate) languages: Languages,
    // what the server said about the cursor's place, shown until the next key
    hover: Option<String>,
    pub(crate) menu: Option<(crate::menu::Menu, Choosing)>,
    pub(crate) quickfix: Option<crate::quickfix::Quickfix>,
    multi: Option<multi::Multi>,
    // the word list for spelling, None until it's wanted, and inside None if there's none
    dictionary: RefCell<Option<Option<crate::spell::Dictionary>>>,
    // the lines :s would change as it's typed, as they'd be, and where it replaces in each
    pub(crate) substitute_preview: Option<HashMap<usize, crate::ex::Replaced>>,
    // the first lines of the folds closed in this buffer
    closed: Vec<usize>,
    // the closed folds, outermost only, as of a version of the text and those closed
    fold_cache: RefCell<Option<FoldCache>>,
    conflict_cache: RefCell<Option<ConflictCache>>,
    // the call the cursor is in while typing, as the server described it
    signature: Option<crate::languages::Signature>,
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
    // your own expandtab, tabstop and shiftwidth, while a buffer's .editorconfig has its own
    own_indent: Option<(bool, usize, usize)>,
    // the windows :diffthis compares, the first two still open being the pair
    diff_wins: Vec<usize>,
    diff_cache: RefCell<Option<DiffPairCache>>,
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
            history: VecDeque::new(),
            version: 0,
            saved: 0,
            versions: 0,
            changing: false,
            grouped: 0,
            undo_dir: None,
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
            completion_asked: None,
            recording: None,
            macros: HashMap::new(),
            last_macro: None,
            macro_depth: 0,
            maps: Maps::default(),
            servers: Servers::default(),
            languages: Languages::default(),
            hover: None,
            menu: None,
            quickfix: None,
            multi: None,
            dictionary: RefCell::new(None),
            substitute_preview: None,
            closed: Vec::new(),
            fold_cache: RefCell::new(None),
            conflict_cache: RefCell::new(None),
            signature: None,
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
            own_indent: None,
            diff_wins: Vec::new(),
            diff_cache: RefCell::new(None),
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
            id: self.buffer_id,
            buf: &self.buf,
            path: self.file.path.as_deref(),
            image: self.file.image.as_ref(),
            scratch: self.file.scratch.as_deref(),
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
                id: d.id,
                buf: &d.buf,
                path: d.file.path.as_deref(),
                image: d.file.image.as_ref(),
                scratch: d.file.scratch.as_deref(),
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

    /// The picture the open file is, if it's one.
    pub fn image(&self) -> Option<&tinsel::image::Image> {
        self.file.image.as_ref()
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

    pub fn matching_bracket(&self) -> Option<Pos> {
        let c = self.buf.char_at(self.cursor)?;
        if !"()[]{}".contains(c) {
            return None;
        }
        motion::match_bracket(&self.buf, self.cursor)
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
    pub fn tick(&mut self) {
        if let Some(g) = self.grep.as_mut() {
            g.refresh();
        }
        self.git_menu_tick();
        self.sync_languages();
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

    pub(crate) fn set_quit(&mut self) {
        self.quit = true;
    }

    pub(crate) fn push_effect(&mut self, e: Effect) {
        self.effects.push(e);
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
    use super::{Editor, Pos, slashed};
    use crate::keys::parse;
    use crate::languages::{Action, Answer, Ask, Edit, FileEdits, Item, Place, Problem, Said, Severity};
    use std::path::{Path, PathBuf};

    fn problem(start: (usize, usize), end: (usize, usize), severity: Severity, message: &str) -> Problem {
        Problem {
            start: Pos::new(start.0, start.1),
            end: Pos::new(end.0, end.1),
            severity,
            message: message.into(),
        }
    }

    fn with_problems(text: &str, problems: Vec<Problem>) -> Editor {
        let mut e = Editor::with_text(text);
        e.languages.found(e.buffer_id, problems);
        e
    }

    #[test]
    fn brackets_and_d_go_from_problem_to_problem() {
        let found = vec![
            problem((1, 4), (1, 5), Severity::Warning, "a"),
            problem((1, 4), (1, 9), Severity::Error, "the same place"),
            problem((3, 2), (3, 3), Severity::Error, "b"),
            problem((9, 9), (9, 9), Severity::Hint, "past the end, from older text"),
        ];
        let mut e = with_problems("zero\none two\ntwo\nthree four\nfive", found);
        e.keys(&parse("]d"));
        assert_eq!(e.cursor(), Pos::new(1, 4));
        e.keys(&parse("]d"));
        assert_eq!(e.cursor(), Pos::new(3, 2));
        e.keys(&parse("]d"));
        assert_eq!(e.cursor(), Pos::new(4, 3), "kept inside the text");
        e.keys(&parse("]d"));
        assert_eq!(e.cursor(), Pos::new(4, 3), "no wrapping round");
        e.keys(&parse("2[d"));
        assert_eq!(e.cursor(), Pos::new(1, 4));
        e.keys(&parse("``"));
        assert_eq!(e.cursor(), Pos::new(4, 3), "a jump, so `` goes back");
    }

    #[test]
    fn problems_pulled_into_shorter_text_are_visited_in_order() {
        let found = vec![
            problem((1, 1), (1, 2), Severity::Error, "a"),
            problem((5, 0), (5, 1), Severity::Error, "from when there were more lines"),
        ];
        let mut e = with_problems("ab\ncd", found);
        e.keys(&parse("]d"));
        assert_eq!(e.cursor(), Pos::new(1, 0));
        e.keys(&parse("]d"));
        assert_eq!(e.cursor(), Pos::new(1, 1));
    }

    #[test]
    fn the_worst_problem_on_the_line_is_the_one_shown() {
        let found = vec![
            problem((0, 0), (0, 1), Severity::Hint, "hint"),
            problem((0, 2), (1, 1), Severity::Warning, "spans two lines"),
        ];
        let mut e = with_problems("ab cd\nef\ngh", found);
        assert_eq!(e.problem_here().unwrap().message, "spans two lines");
        e.keys(&parse("j"));
        assert_eq!(e.problem_here().unwrap().message, "spans two lines");
        e.keys(&parse("j"));
        assert_eq!(e.problem_here(), None);
    }

    fn temp_file(name: &str, text: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("tvim-editor-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, text).unwrap();
        crate::paths::canonical(&path).unwrap()
    }

    // as the server would answer about where the cursor is now
    fn answer(e: &Editor, ask: Ask, said: Said) -> Answer {
        let others = e.docs.iter().enumerate().filter(|&(i, _)| i != e.current);
        Answer {
            id: e.buffer_id,
            version: e.version,
            at: e.cursor,
            versions: std::iter::once((e.buffer_id, e.version))
                .chain(others.map(|(_, d)| (d.id, d.version)))
                .collect(),
            read: Default::default(),
            ask,
            said,
        }
    }

    fn place(path: &Path, row: usize, col: usize) -> Place {
        Place {
            path: path.to_path_buf(),
            start: Pos::new(row, col),
            end: Pos::new(row, col + 1),
        }
    }

    fn item(label: &str, range: Option<((usize, usize), (usize, usize))>, text: &str) -> Item {
        Item {
            label: label.into(),
            text: text.into(),
            range: range.map(|(a, b)| (Pos::new(a.0, a.1), Pos::new(b.0, b.1))),
            filter: label.into(),
            kind: Some("method"),
            detail: String::new(),
            extra: Vec::new(),
        }
    }

    // Ctrl-n typed with a server that can complete things, which then answers with `items`
    fn completed(e: &mut Editor, items: Vec<Item>) {
        e.suggested(items);
    }

    #[test]
    fn a_servers_completions_are_matched_against_the_word_typed() {
        let mut e = Editor::with_text("s.P");
        e.keys(&parse("A"));
        let items = vec![
            item("len", None, "len"),
            item("map_parts", None, "map_parts"),
            item("push", None, "push"),
        ];
        completed(&mut e, items);
        let (_, choices, at) = e.completion().unwrap();
        let labels: Vec<&str> = choices.iter().map(|c| c.label.as_str()).collect();
        assert_eq!(labels, ["push", "map_parts"], "starting with it first, ignoring case");
        assert_eq!(at, Some(0));
        assert_eq!(e.buf.lines(), ["s.push"]);
        e.keys(&parse("<C-n>"));
        assert_eq!(e.buf.lines(), ["s.map_parts"]);
        e.keys(&parse("<C-e>"));
        assert_eq!(e.buf.lines(), ["s.P"], "and back to what was typed");
    }

    #[test]
    fn a_completion_replaces_the_range_the_server_gives_and_repeats_as_typed() {
        let mut e = Editor::with_text("s.p\ns.p");
        e.keys(&parse("A"));
        completed(&mut e, vec![item("push", Some(((0, 0), (0, 3))), "self.push")]);
        assert_eq!(e.buf.lines(), ["self.push", "s.p"]);
        e.keys(&parse("()<Esc>j."));
        assert_eq!(e.buf.lines(), ["self.push()", "self.push()"]);
    }

    #[test]
    fn a_completion_that_needs_an_import_brings_it_in_one_change() {
        let mut e = Editor::with_text("fn main() {\n    Has\n}");
        e.keys(&parse("jA"));
        let mut needing = item("HashMap", None, "HashMap");
        needing.extra = vec![Edit {
            start: Pos::new(0, 0),
            end: Pos::new(0, 0),
            text: "use std::collections::HashMap;\n\n".into(),
        }];
        completed(&mut e, vec![needing]);
        e.keys(&parse("<C-y>::new()"));
        assert_eq!(
            e.buf.lines(),
            [
                "use std::collections::HashMap;",
                "",
                "fn main() {",
                "    HashMap::new()",
                "}"
            ]
        );
        e.keys(&parse("<Esc>u"));
        assert_eq!(e.buf.lines(), ["fn main() {", "    Has", "}"], "undone together");
    }

    #[test]
    fn formatting_is_one_change_that_keeps_the_cursor_on_its_text() {
        let mut e = Editor::with_text("fn a(){\nlet x=1;\n}");
        e.keys(&parse("jfx"));
        let edits = vec![
            edit((0, 6), (0, 6), " "),
            edit((1, 0), (1, 0), "    "),
            edit((1, 5), (1, 6), " = "),
        ];
        let files = vec![FileEdits {
            path: PathBuf::new(),
            edits,
        }];
        let format = Ask::Format {
            lines: None,
            tab_size: 4,
            spaces: true,
        };
        e.answered(answer(&e, format.clone(), Said::Edits(files)));
        assert_eq!(e.buf.lines(), ["fn a() {", "    let x = 1;", "}"]);
        assert_eq!(e.cursor, Pos::new(1, 8));
        assert_eq!(e.message.as_ref().unwrap().text, "formatted");
        e.keys(&parse("u"));
        assert_eq!(e.buf.lines(), ["fn a(){", "let x=1;", "}"], "undone at once");
        e.answered(answer(&e, format, Said::Edits(Vec::new())));
        assert_eq!(e.message.as_ref().unwrap().text, "already formatted");
    }

    #[test]
    fn equals_asks_the_server_to_format_lines_and_says_when_there_is_none() {
        let mut e = Editor::with_text("a\n  b");
        e.keys(&parse("j$=k"));
        assert_eq!(e.cursor, Pos::new(0, 0), "to the first line, as = goes");
        assert_eq!(e.message.as_ref().unwrap().text, "no language server for this file");
        assert_eq!(e.buf.lines(), ["a", "  b"]);
    }

    #[test]
    fn formatonsave_without_a_server_writes_the_file_as_it_is() {
        let path = temp_file("fos.rs", "fn a(){}\n");
        let mut e = Editor::open(&path).unwrap();
        e.ex("set fos");
        e.keys(&parse("A x<Esc>"));
        e.ex("w");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "fn a(){} x\n");
        assert!(e.message.as_ref().unwrap().text.ends_with("written"));
    }

    // edits to `path`, or a command when there are none
    fn action(title: &str, path: &Path, edits: Vec<Edit>, command: Option<&str>) -> Action {
        Action {
            title: title.into(),
            kind: "quickfix".into(),
            preferred: false,
            disabled: None,
            edits: if edits.is_empty() {
                Vec::new()
            } else {
                vec![FileEdits {
                    path: path.to_path_buf(),
                    edits,
                }]
            },
            command: command.map(|c| crate::json::Json::object([("title", c.into()), ("command", c.into())])),
        }
    }

    #[test]
    fn a_chosen_code_action_makes_its_edits() {
        let path = temp_file("actions.rs", "use a;\nfn main() {}\n");
        let mut e = Editor::open(&path).unwrap();
        let ask = Ask::Actions {
            from: e.cursor,
            to: e.cursor,
        };
        let run = || action("Run", &path, Vec::new(), Some("run"));
        let remove = action("Remove unused import", &path, vec![edit((0, 0), (1, 0), "")], None);
        e.answered(answer(&e, ask.clone(), Said::Actions(vec![run(), remove])));
        let m = e.menu().expect("offered");
        assert_eq!(m.title, "code actions");
        assert_eq!(m.entries[1].label, "Remove unused import");
        assert_eq!(m.entries[1].hint, "quickfix");
        e.keys(&parse("j<CR>"));
        assert!(e.menu().is_none());
        assert_eq!(e.buf.lines(), ["fn main() {}"]);
        e.keys(&parse("u"));
        assert_eq!(e.buf.lines(), ["use a;", "fn main() {}"]);

        e.answered(answer(&e, ask.clone(), Said::Actions(vec![run()])));
        e.keys(&parse("1"));
        assert!(e.message.as_ref().unwrap().error, "no server to run it");
        e.answered(answer(&e, ask.clone(), Said::Actions(vec![run()])));
        e.keys(&parse("<Esc>"));
        assert!(e.menu().is_none());
        e.answered(answer(&e, ask, Said::Actions(Vec::new())));
        assert_eq!(e.message.as_ref().unwrap().text, "nothing to do here");
    }

    #[test]
    fn code_actions_for_a_selection_ask_about_all_of_it() {
        let mut e = Editor::with_text("one two\nthree");
        e.keys(&parse("wvj"));
        e.keys(&parse(" ca"));
        assert_eq!(e.message.as_ref().unwrap().text, "no language server for this file");
        let mut e = Editor::with_text("one two\nthree");
        e.keys(&parse("wvj"));
        assert_eq!(
            e.char_span(e.visual_range()),
            (Pos::new(0, 4), Pos::new(1, 5)),
            "the range asked about"
        );
    }

    #[test]
    fn a_signature_shows_while_typing_and_goes_with_insert_mode() {
        let mut e = Editor::with_text("");
        let sig = crate::languages::Signature {
            label: "f(a: u8)".into(),
            active: Some((2, 7)),
            doc: String::new(),
        };
        e.answered(answer(&e, Ask::Signature, Said::Signature(Some(sig.clone()))));
        assert!(e.signature().is_none(), "only while typing");
        e.keys(&parse("if("));
        e.answered(answer(&e, Ask::Signature, Said::Signature(Some(sig.clone()))));
        assert_eq!(e.signature(), Some(&sig));
        e.answered(answer(&e, Ask::Signature, Said::Signature(None)));
        assert!(e.signature().is_none(), "out of the call");
        e.answered(answer(&e, Ask::Signature, Said::Signature(Some(sig))));
        e.keys(&parse("<Esc>"));
        assert!(e.signature().is_none());
    }

    #[test]
    fn with_nothing_from_the_server_words_are_offered() {
        let mut e = Editor::with_text("plenty\npl");
        e.keys(&parse("jA"));
        completed(&mut e, vec![item("zzz", None, "zzz")]);
        assert_eq!(e.buf.lines(), ["plenty", "plenty"]);
        e.keys(&parse("<C-e>"));
        e.completion_asked = Some(super::Asking::Typed { back: false });
        let refused = answer(&e, Ask::Complete, Said::Refused("busy".into()));
        e.answered(refused);
        assert_eq!(e.buf.lines(), ["plenty", "plenty"], "nor when it refuses");
        assert!(e.message.is_none(), "which isn't worth saying");
    }

    fn labels(e: &Editor) -> Vec<String> {
        let (_, choices, _) = e.completion().expect("shown");
        choices.iter().map(|c| c.label.clone()).collect()
    }

    #[test]
    fn completions_shown_while_typing_go_in_only_when_chosen() {
        let mut e = Editor::with_text("");
        e.keys(&parse("ip"));
        e.suggested_as_typed(vec![item("print", None, "print"), item("push", None, "push")]);
        assert_eq!(e.buf.lines(), ["p"], "nothing put in yet");
        assert_eq!(e.completion().unwrap().2, None);
        e.keys(&parse("u"));
        assert_eq!(labels(&e), ["push"], "matched again as more is typed");
        e.keys(&parse("<C-n>"));
        assert_eq!(e.buf.lines(), ["push"]);
        e.keys(&parse("(<Esc>"));
        assert_eq!(e.buf.lines(), ["push()"], "and kept when typing carries on");
    }

    #[test]
    fn ctrl_y_takes_the_first_completion_shown_while_typing() {
        let mut e = Editor::with_text("x\nx");
        e.keys(&parse("ip"));
        e.suggested_as_typed(vec![item("print", None, "print"), item("push", None, "push")]);
        e.keys(&parse("<C-y>"));
        assert_eq!(e.buf.lines(), ["printx", "x"]);
        assert!(e.completion().is_none());
        e.keys(&parse("<Esc>j0."));
        assert_eq!(e.buf.lines(), ["printx", "printx"], "and . has it as typed");
    }

    #[test]
    fn enter_and_esc_go_on_as_usual_past_completions_shown_while_typing() {
        let mut e = Editor::with_text("");
        e.keys(&parse("ip"));
        e.suggested_as_typed(vec![item("print", None, "print")]);
        e.keys(&parse("<CR>"));
        assert_eq!(e.buf.lines(), ["p", ""], "a new line, nothing chosen");
        e.keys(&parse("p"));
        e.suggested_as_typed(vec![item("print", None, "print")]);
        e.keys(&parse("<Esc>"));
        assert_eq!(e.mode, super::Mode::Normal, "esc leaves insert mode");
        assert_eq!(e.buf.lines(), ["p", "p"]);
    }

    #[test]
    fn completions_that_only_repeat_the_word_typed_arent_shown() {
        let mut e = Editor::with_text("");
        e.keys(&parse("ipush"));
        e.suggested_as_typed(vec![item("push", None, "push")]);
        assert!(e.completion().is_none());
    }

    #[test]
    fn a_completion_answered_after_another_key_is_dropped() {
        let mut e = Editor::with_text("s.p");
        e.keys(&parse("A"));
        e.completion_asked = Some(super::Asking::Typed { back: false });
        let late = answer(&e, Ask::Complete, Said::Items(vec![item("push", None, "push")]));
        e.keys(&parse("<Left><Right>"));
        e.answered(late);
        assert!(e.completion().is_none());
    }

    #[test]
    fn a_definition_is_gone_to_and_ctrl_o_comes_back() {
        let a = temp_file("def-a.rs", "use b;\nfn main() {\n    helper();\n}\n");
        let b = temp_file("def-b.rs", "\n\npub fn helper() {}\n");
        let mut e = Editor::open(&a).unwrap();
        e.keys(&parse("jj4l"));
        let said = Said::Places(vec![place(&b, 2, 7)]);
        e.answered(answer(&e, Ask::Definition, said));
        assert_eq!(
            e.file.path.as_deref().map(|p| crate::paths::canonical(p).unwrap()),
            Some(b)
        );
        assert_eq!(e.cursor, Pos::new(2, 7));
        e.keys(&parse("<C-o>"));
        assert_eq!(
            e.file.path.as_deref().map(|p| crate::paths::canonical(p).unwrap()),
            Some(a.clone())
        );
        assert_eq!(e.cursor, Pos::new(2, 4));

        let said = Said::Places(vec![place(&a, 1, 3)]);
        e.answered(answer(&e, Ask::Definition, said));
        assert_eq!(e.cursor, Pos::new(1, 3), "in the same file");
        e.keys(&parse("<C-o>"));
        assert_eq!(e.cursor, Pos::new(2, 4));
    }

    #[test]
    fn answers_about_somewhere_since_left_are_dropped() {
        let a = temp_file("def-left.rs", "one\ntwo\nthree\n");
        let mut e = Editor::open(&a).unwrap();
        let late = answer(&e, Ask::Definition, Said::Places(vec![place(&a, 2, 0)]));
        e.keys(&parse("l"));
        e.answered(late.clone());
        assert_eq!(e.cursor, Pos::new(0, 1), "the cursor moved");
        e.keys(&parse("h"));
        e.keys(&parse("x"));
        e.answered(late);
        assert_eq!(e.cursor, Pos::new(0, 0), "the text changed");
    }

    #[test]
    fn references_are_listed_and_enter_goes_to_one() {
        let a = temp_file("refs-a.rs", "fn helper() {}\nfn main() {\n    helper();\n}\n");
        let b = temp_file("refs-b.rs", "use a::helper;\n\nfn more() { helper() }\n");
        let mut e = Editor::open(&a).unwrap();
        e.keys(&parse("w"));
        let said = Said::Places(vec![place(&a, 0, 3), place(&a, 2, 4), place(&b, 2, 12)]);
        e.answered(answer(&e, Ask::References, said));
        let g = e.grep_picker().expect("listed");
        assert_eq!(g.title(), Some("references to helper"));
        assert_eq!(g.hits().len(), 3);
        e.keys(&parse("<Down><Down><CR>"));
        assert!(e.grep_picker().is_none());
        assert_eq!(
            e.file.path.as_deref().map(|p| crate::paths::canonical(p).unwrap()),
            Some(b)
        );
        assert_eq!(e.cursor, Pos::new(2, 12));
        e.keys(&parse("n"));
        assert_eq!(e.cursor, Pos::new(0, 7), "n looks for the word");
    }

    #[test]
    fn references_are_listed_from_this_file_first_and_in_order() {
        let a = temp_file("order-a.rs", "x\nx\nx\n");
        let b = temp_file("order-b.rs", "x\nx\n");
        let mut e = Editor::open(&b).unwrap();
        let said = Said::Places(vec![
            place(&a, 2, 0),
            place(&b, 1, 0),
            place(&a, 0, 0),
            place(&b, 0, 0),
            place(&b, 0, 0),
        ]);
        e.answered(answer(&e, Ask::References, said));
        let g = e.grep_picker().unwrap();
        let listed: Vec<(String, usize)> = g.hits().iter().map(|h| (g.path(h), h.row)).collect();
        let name = |p: &Path| crate::editor::listed(p);
        assert_eq!(listed, [(name(&b), 0), (name(&b), 1), (name(&a), 0), (name(&a), 2)]);
    }

    #[test]
    fn a_file_not_saved_yet_is_gone_to_and_listed_as_it_is() {
        let path = unsaved_file("def-unsaved.py");
        let mut e = Editor::open(&path).unwrap();
        e.keys(&parse("idef f(): pass<CR>f()<Esc>"));
        e.answered(answer(&e, Ask::Definition, Said::Places(vec![place(&path, 0, 4)])));
        assert_eq!(e.cursor, Pos::new(0, 4));
        assert_eq!(e.buf.lines(), ["def f(): pass", "f()"]);
        assert_eq!(e.docs.len(), 1, "no second copy opened");
        let said = Said::Places(vec![place(&path, 0, 4), place(&path, 1, 0)]);
        e.answered(answer(&e, Ask::References, said));
        let g = e.grep_picker().unwrap();
        let lines: Vec<String> = g.hits().iter().map(|h| g.line(h)).collect();
        assert_eq!(lines, ["def f(): pass", "f()"]);
    }

    #[test]
    fn ctrl_o_comes_back_from_a_listed_place_in_another_file() {
        let a = temp_file("back-a.rs", "x\nx\nx\n");
        let b = temp_file("back-b.rs", "\n\n\nx\n");
        let mut e = Editor::open(&a).unwrap();
        e.keys(&parse("jj"));
        let said = Said::Places(vec![place(&a, 0, 0), place(&b, 3, 0)]);
        e.answered(answer(&e, Ask::References, said));
        e.keys(&parse("<C-n><CR>"));
        assert_eq!(e.cursor, Pos::new(3, 0));
        assert!(e.file.path.as_ref().unwrap().ends_with("back-b.rs"));
        e.keys(&parse("<C-o>"));
        assert!(e.file.path.as_ref().unwrap().ends_with("back-a.rs"));
        assert_eq!(e.cursor, Pos::new(2, 0));
    }

    #[test]
    fn files_the_answer_read_are_listed_without_reading_them_again() {
        let a = temp_file("read-a.rs", "x\n");
        let b = temp_file("read-b.rs", "on disk\n");
        let mut e = Editor::open(&a).unwrap();
        let mut said = answer(&e, Ask::References, Said::Places(vec![place(&b, 0, 0)]));
        said.read
            .insert(b.clone(), vec!["as the answer read it".into(), String::new()]);
        e.answered(said);
        let g = e.grep_picker().unwrap();
        assert_eq!(g.line(&g.hits()[0]), "as the answer read it");
    }

    #[test]
    fn symbols_are_listed_in_the_files_order_or_matching_what_was_asked() {
        let a = temp_file("syms-a.rs", "struct A;\nfn b() {}\n");
        let mut e = Editor::open(&a).unwrap();
        let said = Said::Places(vec![place(&a, 1, 3), place(&a, 0, 7)]);
        e.answered(answer(&e, Ask::Symbols(None), said));
        let g = e.grep_picker().expect("listed");
        assert_eq!(g.title(), Some("symbols in syms-a.rs"));
        let rows: Vec<usize> = g.hits().iter().map(|h| h.row).collect();
        assert_eq!(rows, [0, 1]);
        e.keys(&parse("<Esc>"));
        let said = Said::Places(vec![place(&a, 1, 3)]);
        e.answered(answer(&e, Ask::Symbols(Some("b".into())), said));
        assert_eq!(e.grep_picker().unwrap().title(), Some("symbols matching b"));
        e.keys(&parse("<Esc>"));
        e.answered(answer(&e, Ask::Symbols(None), Said::Places(Vec::new())));
        assert_eq!(e.message.as_ref().unwrap().text, "nothing defined here");
    }

    #[test]
    fn space_capital_s_asks_what_to_look_for_starting_from_the_word() {
        let mut e = Editor::with_text("let area = 1;");
        e.keys(&parse("w S"));
        assert_eq!(e.cmdline().text, "Symbols area");
    }

    #[test]
    fn ctrl_q_keeps_whats_listed_to_go_through_with_brackets_q() {
        let a = temp_file("qf-a.rs", "fn x() {}\nx();\n");
        let b = temp_file("qf-b.rs", "\n\nx();\n");
        let mut e = Editor::open(&a).unwrap();
        let said = Said::Places(vec![place(&a, 0, 3), place(&a, 1, 0), place(&b, 2, 0)]);
        e.answered(answer(&e, Ask::References, said));
        e.keys(&parse("<C-q>"));
        assert!(e.grep_picker().is_none());
        let q = e.quickfix().expect("kept");
        assert_eq!((q.title.as_str(), q.spots.len()), ("references to fn", 3));
        assert_eq!(q.spots[1].note, "x();");
        assert_eq!(
            e.message.as_ref().unwrap().text,
            "3 places in the quickfix list, ]q for the next"
        );

        e.keys(&parse("]q"));
        assert_eq!(e.cursor, Pos::new(0, 3), "the first to begin with");
        e.keys(&parse("2]q"));
        assert!(e.file.path.as_ref().unwrap().ends_with("qf-b.rs"));
        assert_eq!(e.cursor, Pos::new(2, 0));
        assert_eq!(e.message.as_ref().unwrap().text, "(3 of 3) x();");
        e.keys(&parse("]q"));
        assert_eq!(e.message.as_ref().unwrap().text, "E553: No more items");
        e.keys(&parse("[q"));
        assert_eq!(e.cursor, Pos::new(1, 0));
        e.ex("cc 1");
        assert_eq!(e.cursor, Pos::new(0, 3));
        e.ex("clast");
        assert_eq!(e.cursor, Pos::new(2, 0));
        e.ex("cfirst");
        e.ex("cn");
        assert_eq!(e.cursor, Pos::new(1, 0));

        e.ex("copen");
        let g = e.grep_picker().expect("listed again");
        assert_eq!(g.title(), Some("references to fn"));
        assert_eq!(g.hits().len(), 3);
        e.keys(&parse("<Down><Down><CR>"));
        e.keys(&parse("[q"));
        assert_eq!(e.cursor, Pos::new(1, 0), "on from the one chosen");
    }

    #[test]
    fn cdo_and_cfdo_run_a_command_through_the_quickfix_list() {
        let a = temp_file("cdo-a.rs", "old\nold\n");
        let b = temp_file("cdo-b.rs", "x old old\n");
        let mut e = Editor::open(&a).unwrap();
        let spot = |path: &Path, row, col| crate::quickfix::Spot {
            path: path.to_path_buf(),
            pos: Pos::new(row, col),
            note: String::new(),
        };
        e.keep_quickfix("t", vec![spot(&a, 0, 0), spot(&a, 1, 0), spot(&b, 0, 2)]);
        e.ex("cdo s/old/new/");
        assert!(e.file.path.as_ref().unwrap().ends_with("cdo-b.rs"), "left at the last");
        assert_eq!(e.buf.lines(), ["x new old"]);
        assert_eq!(e.message.as_ref().unwrap().text, "ran at 3 places, :wa saves them all");
        e.ex("cfdo %s/new/newer/g");
        assert_eq!(e.buf.lines(), ["x newer old"]);
        e.ex("b 1");
        assert_eq!(e.buf.lines(), ["newer", "newer"], "once in each file");
        e.ex("cdo s/nothing/x/");
        assert!(e.message.as_ref().unwrap().error, "stops at the first error");
    }

    #[test]
    fn cdo_follows_lines_an_earlier_run_put_in_or_took_out() {
        let a = temp_file("cdo-shift.rs", "x\ndrop\nx\nkeep\nx\n");
        let mut e = Editor::open(&a).unwrap();
        let spot = |row| crate::quickfix::Spot {
            path: a.clone(),
            pos: Pos::new(row, 0),
            note: String::new(),
        };
        e.keep_quickfix("t", vec![spot(1), spot(3)]);
        e.ex("cdo normal dd");
        assert_eq!(e.buf.lines(), ["x", "x", "x"], "both lines, where they'd moved to");
        e.keep_quickfix("t", vec![spot(0), spot(1), spot(2)]);
        e.ex("cdo s/x/a\\rb/");
        assert_eq!(e.buf.lines(), ["a", "b", "a", "b", "a", "b"]);
    }

    #[test]
    fn space_x_lists_problems_worst_first_and_keeps_them_to_go_through() {
        let a = temp_file("probs-a.rs", "a\nb\nc\n");
        let mut e = Editor::open(&a).unwrap();
        e.keys(&parse(" x"));
        assert_eq!(e.message.as_ref().unwrap().text, "no problems");
        let found = vec![
            problem((0, 0), (0, 1), Severity::Warning, "unused a\nmore"),
            problem((2, 0), (2, 1), Severity::Error, "c is wrong"),
        ];
        e.languages.found(e.buffer_id, found);
        e.keys(&parse(" x"));
        let g = e.grep_picker().expect("listed");
        assert_eq!(g.title(), Some("problems"));
        let notes: Vec<_> = g.hits().iter().map(|h| h.note.clone().unwrap()).collect();
        assert_eq!(notes, ["error: c is wrong", "warning: unused a"]);
        e.keys(&parse("<Esc>]q"));
        assert_eq!(e.cursor, Pos::new(2, 0));
        assert_eq!(e.message.as_ref().unwrap().text, "(1 of 2) error: c is wrong");
    }

    #[test]
    fn definitions_in_more_than_one_place_are_listed() {
        let a = temp_file("defs-a.rs", "fn f() {}\nfn f() {}\nf();\n");
        let mut e = Editor::open(&a).unwrap();
        e.keys(&parse("2j"));
        e.answered(answer(
            &e,
            Ask::Definition,
            Said::Places(vec![place(&a, 0, 3), place(&a, 1, 3)]),
        ));
        assert_eq!(e.grep_picker().unwrap().title(), Some("definitions of f"));
        e.answered(answer(&e, Ask::References, Said::Places(Vec::new())));
        assert_eq!(e.message.as_ref().unwrap().text, "no references found");
    }

    fn edit(start: (usize, usize), end: (usize, usize), text: &str) -> Edit {
        Edit {
            start: Pos::new(start.0, start.1),
            end: Pos::new(end.0, end.1),
            text: text.into(),
        }
    }

    #[test]
    fn a_rename_edits_every_file_and_leaves_them_unsaved() {
        let a = temp_file("ren-a.rs", "fn old() {}\nfn main() { old(); old(); }\n");
        let b = temp_file("ren-b.rs", "use a::old;\n");
        let mut e = Editor::open(&a).unwrap();
        e.keys(&parse("j$F("));
        let files = vec![
            FileEdits {
                path: a.clone(),
                edits: vec![
                    edit((1, 19), (1, 22), "brand_new"),
                    edit((0, 3), (0, 6), "brand_new"),
                    edit((1, 12), (1, 15), "brand_new"),
                ],
            },
            FileEdits {
                path: b.clone(),
                edits: vec![edit((0, 7), (0, 10), "brand_new")],
            },
        ];
        e.answered(answer(&e, Ask::Rename("brand_new".into()), Said::Edits(files)));
        assert_eq!(
            e.buf.lines(),
            ["fn brand_new() {}", "fn main() { brand_new(); brand_new(); }"]
        );
        assert_eq!(e.cursor, Pos::new(1, 34), "still on the second call's (");
        assert_eq!(
            e.file.path.as_deref().map(|p| crate::paths::canonical(p).unwrap()),
            Some(a.clone())
        );
        assert_eq!(
            e.message.as_ref().unwrap().text,
            "renamed 4 places in 2 files, :wa saves them all"
        );
        assert!(e.unsaved_elsewhere().unwrap().ends_with("ren-b.rs"));
        assert_eq!(std::fs::read_to_string(&b).unwrap(), "use a::old;\n", "not written");

        e.keys(&parse("u"));
        assert_eq!(
            e.buf.lines(),
            ["fn old() {}", "fn main() { old(); old(); }"],
            "undone at once"
        );
    }

    // a file named on the command line that isn't on disk yet
    fn unsaved_file(name: &str) -> PathBuf {
        let path = temp_file(name, "");
        std::fs::remove_file(&path).unwrap();
        path
    }

    #[test]
    fn a_rename_in_a_file_not_saved_yet_edits_it() {
        let path = unsaved_file("ren-unsaved.py");
        let mut e = Editor::open(&path).unwrap();
        e.keys(&parse("idef f(): pass<CR>f()<Esc>"));
        let files = vec![FileEdits {
            path: path.clone(),
            edits: vec![edit((0, 4), (0, 5), "g"), edit((1, 0), (1, 1), "g")],
        }];
        e.answered(answer(&e, Ask::Rename("g".into()), Said::Edits(files)));
        assert_eq!(e.buf.lines(), ["def g(): pass", "g()"]);
        assert_eq!(e.docs.len(), 1, "no second copy opened");
        assert_eq!(e.message.as_ref().unwrap().text, "renamed 2 places");
    }

    #[test]
    fn a_rename_says_which_files_it_couldnt_open() {
        let a = temp_file("ren-skip-a.rs", "fn old() {}\nold();\n");
        let b = a.with_file_name("ren-skip-b.rs");
        std::fs::write(&b, [0xff, 0xfe]).unwrap();
        let mut e = Editor::open(&a).unwrap();
        let files = vec![
            FileEdits {
                path: a.clone(),
                edits: vec![edit((0, 3), (0, 6), "new"), edit((1, 0), (1, 3), "new")],
            },
            FileEdits {
                path: b.clone(),
                edits: vec![edit((0, 0), (0, 1), "new")],
            },
        ];
        e.answered(answer(&e, Ask::Rename("new".into()), Said::Edits(files)));
        assert_eq!(e.buf.lines(), ["fn new() {}", "new();"]);
        let said = &e.message.as_ref().unwrap().text;
        assert!(said.starts_with("renamed 2 places, but couldn't open "), "{said}");
        assert!(said.contains("isn't UTF-8 text"), "{said}");
    }

    #[test]
    fn a_rename_is_dropped_when_a_file_it_edits_changed_while_waiting() {
        let a = temp_file("ren-stale-a.rs", "use b::old;\nfn main() { old(); }\n");
        let b = temp_file("ren-stale-b.rs", "pub fn old() {}\n");
        let mut e = Editor::open(&b).unwrap();
        e.ex(&format!("e {}", a.display()));
        let files = vec![
            FileEdits {
                path: a.clone(),
                edits: vec![edit((0, 7), (0, 10), "new")],
            },
            FileEdits {
                path: b.clone(),
                edits: vec![edit((0, 7), (0, 10), "new")],
            },
        ];
        let asked = answer(&e, Ask::Rename("new".into()), Said::Edits(files));
        e.ex("bp");
        e.keys(&parse("O// added above<Esc>"));
        e.ex("bn");
        e.answered(asked);
        let said = &e.message.as_ref().unwrap().text;
        assert!(
            said.ends_with("ren-stale-b.rs changed while renaming, so nothing was"),
            "{said}"
        );
        assert_eq!(e.buf.lines()[0], "use b::old;");
        e.ex("bp");
        assert_eq!(e.buf.lines(), ["// added above", "pub fn old() {}"]);
    }

    #[test]
    fn text_put_at_one_place_keeps_its_order() {
        let a = temp_file("ren-order.rs", "let x\n");
        let mut e = Editor::open(&a).unwrap();
        let files = vec![FileEdits {
            path: a.clone(),
            edits: vec![edit((0, 4), (0, 4), "A"), edit((0, 4), (0, 4), "B")],
        }];
        e.answered(answer(&e, Ask::Rename("x".into()), Said::Edits(files)));
        assert_eq!(e.buf.lines(), ["let ABx"]);
    }

    #[test]
    fn rename_with_no_name_asks_for_one() {
        let mut e = Editor::with_text("let count = 1;");
        e.keys(&parse("w"));
        e.keys(&parse("<Space>n"));
        assert_eq!(e.mode, super::Mode::Command);
        assert_eq!(e.cmd.text, "rename count");
        e.keys(&parse("<Esc>"));
        e.ex("rename total");
        assert_eq!(e.message.as_ref().unwrap().text, "no language server for this file");
    }

    #[test]
    fn nothing_found_or_no_server_is_said() {
        let mut e = Editor::with_text("a");
        e.answered(answer(&e, Ask::Definition, Said::Places(Vec::new())));
        assert_eq!(e.message.as_ref().unwrap().text, "no definition found");
        e.keys(&parse("gd"));
        assert_eq!(e.message.as_ref().unwrap().text, "no language server for this file");
    }

    #[test]
    fn windows_paths_are_listed_with_slashes_and_without_their_prefix() {
        assert_eq!(slashed(r"\\?\C:\Users\me\a.rs", true), "C:/Users/me/a.rs");
        assert_eq!(slashed(r"\\?\UNC\server\share\a.rs", true), "//server/share/a.rs");
        assert_eq!(slashed(r"src\ui\mod.rs", true), "src/ui/mod.rs");
        assert_eq!(slashed(r"src\ui\mod.rs", false), r"src\ui\mod.rs");
    }
}
