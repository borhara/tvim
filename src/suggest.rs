use crate::editor::Options;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Suggestion {
    pub label: String,
    pub replace: String,
    pub desc: String,
    pub value: String,
}

pub const COMMANDS: &[(&str, &str, &str)] = &[
    ("write", "w", "save the file"),
    ("quit", "q", "close this window, or tvim"),
    ("wq", "", "save and close"),
    ("xit", "x", "save if changed, then close"),
    ("edit", "e", "open a file"),
    ("find", "fin", "find a file by name"),
    ("recent", "rec", "files you opened lately"),
    ("Explore", "Ex", "browse this file's folder"),
    ("Lexplore", "Lex", "show or hide the sidebar"),
    ("bnext", "bn", "next open file"),
    ("bprevious", "bp", "previous open file"),
    ("buffer", "b", "switch to an open file"),
    ("bdelete", "bd", "close this file"),
    ("ls", "", "list open files"),
    ("split", "sp", "split the window, one above the other"),
    ("vsplit", "vs", "split the window side by side"),
    ("new", "", "split, with an empty file above"),
    ("vnew", "vne", "split, with an empty file beside"),
    ("only", "on", "close every other window"),
    ("close", "clo", "close this window"),
    ("wall", "wa", "save every file"),
    ("qall", "qa", "close tvim"),
    ("wqall", "wqa", "save everything and close"),
    ("set", "se", "change an option"),
    ("substitute", "s", "replace text"),
    ("nohlsearch", "noh", "clear search highlights"),
    ("read", "r", "put a file's text below"),
    ("registers", "reg", "show registers"),
    ("marks", "", "show marks"),
    ("normal", "norm", "run keys on each line"),
    ("undo", "u", "undo"),
    ("redo", "red", "redo"),
    ("shell", "sh", "open a shell"),
    ("zen", "", "only the text, centred"),
    ("git", "", "the git menu: stage, commit, log, branches"),
    ("preview", "prev", "show or hide a markdown preview"),
    ("help", "h", "a quick reminder"),
    ("version", "ve", "show the version"),
];

const OPTIONS: &[(&str, &str, &str)] = &[
    ("number", "", "line numbers"),
    ("relativenumber", "", "number lines from the cursor"),
    ("cursorline", "", "mark the cursor's line with ›"),
    ("indentguides", "", "lines down each indent"),
    ("syntax", "", "colour code"),
    ("expandtab", "", "tab types spaces"),
    ("tabstop", "=", "how wide a tab shows"),
    ("shiftwidth", "=", "how far >> and << move"),
    ("ignorecase", "", "search ignores case"),
    ("smartcase", "", "unless it has capitals"),
    ("hlsearch", "", "highlight matches"),
    ("incsearch", "", "search as you type"),
    ("autoindent", "", "new lines keep the indent"),
    ("autopairs", "", "brackets and quotes close themselves"),
    ("scrolloff", "=", "lines kept around the cursor"),
    ("smooth", "", "glide when the view jumps"),
    ("tabpreview", "", "tab moves into the markdown preview"),
    ("dim", "", "fade whichever of text and sidebar is not in use"),
    ("gitlens", "", "who last changed the cursor's line, and when"),
    ("roomy", "", "a twinkling row under the top line and a blank one above the stars"),
    ("zeropad", "", "line numbers start with zeros, as in 01"),
];

fn option_value(name: &str, o: &Options) -> String {
    match name {
        "tabstop" => o.tab_stop.to_string(),
        "shiftwidth" => o.shift_width.to_string(),
        "scrolloff" => o.scroll_off.to_string(),
        _ => o.flag(name).map_or_else(String::new, |on| if on { "on" } else { "off" }.to_string()),
    }
}
