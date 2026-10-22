use crate::syntax;

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
