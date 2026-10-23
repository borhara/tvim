use std::cell::Cell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::buffer::{Buffer, Pos};
use crate::clipboard::Clipboard;
use crate::motion::Find;
use crate::syntax;
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

pub struct Editor {
    pub(crate) buf: Buffer,
    pub(crate) cursor: Pos,
    pub(crate) want: usize,
    pub(crate) mode: Mode,
    pub(crate) anchor: Pos,
    last_visual: Option<(Pos, Pos, Mode)>,
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
    pub fn file(&self) -> &FileInfo {
        &self.file
    }

    pub fn name(&self) -> String {
        display_name(self.file.path.as_deref())
    }

}

fn display_name(path: Option<&Path>) -> String {
    path.map_or_else(|| "[No Name]".to_string(), |p| p.display().to_string())
}
