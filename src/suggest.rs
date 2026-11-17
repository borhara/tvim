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

pub fn suggestions(text: &str, options: &Options, buffers: &[String]) -> Vec<Suggestion> {
    let Some((cmd, arg)) = text.split_once(' ') else {
        if text
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_digit() || "%.$'".contains(c))
        {
            return Vec::new();
        }
        return COMMANDS
            .iter()
            .filter(|(name, short, _)| name.starts_with(text) || !short.is_empty() && short.starts_with(text))
            .map(|(name, _, desc)| Suggestion {
                label: name.to_string(),
                replace: name.to_string(),
                desc: desc.to_string(),
                value: String::new(),
            })
            .collect();
    };
    let word = arg.rsplit(' ').next().unwrap_or("");
    let head = &text[..text.len() - word.len()];
    match cmd {
        "set" | "se" => {
            let (no, stem) = match word.strip_prefix("no") {
                Some(rest) if !OPTIONS.iter().any(|(n, _, _)| n.starts_with(word)) => ("no", rest),
                _ => ("", word),
            };
            OPTIONS
                .iter()
                .filter(|(name, eq, _)| name.starts_with(stem) && (no.is_empty() || eq.is_empty()))
                .map(|(name, eq, desc)| Suggestion {
                    label: format!("{no}{name}"),
                    replace: format!("{head}{no}{name}{eq}"),
                    desc: desc.to_string(),
                    value: option_value(name, options),
                })
                .collect()
        }
        "b" | "buffer" => buffers
            .iter()
            .filter(|b| b.contains(word))
            .map(|b| Suggestion {
                label: b.clone(),
                replace: format!("{head}{b}"),
                desc: "open".into(),
                value: String::new(),
            })
            .collect(),
        "e" | "edit" | "r" | "read" | "w" | "write" | "Ex" | "Explore" | "Lex" => files(head, word),
        _ => Vec::new(),
    }
}

fn files(head: &str, word: &str) -> Vec<Suggestion> {
    let (dir, stem) = match word.rfind('/') {
        Some(i) => (&word[..=i], &word[i + 1..]),
        None => ("", word),
    };
    let look_in = if dir.is_empty() {
        ".".to_string()
    } else {
        crate::editor::expand_home(dir)
    };
    let read = std::fs::read_dir(look_in);
    let Ok(entries) = read else { return Vec::new() };
    let mut hits: Vec<(bool, String, u64)> = entries
        .filter_map(Result::ok)
        .filter_map(|e| {
            let name = e.file_name().to_str()?.to_string();
            if !name.starts_with(stem) || name.starts_with('.') && !stem.starts_with('.') {
                return None;
            }
            let meta = e.metadata().ok()?;
            Some((meta.is_dir(), name, meta.len()))
        })
        .collect();
    hits.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.to_lowercase().cmp(&b.1.to_lowercase())));
    hits.into_iter()
        .map(|(is_dir, name, size)| {
            let mark = if is_dir { "/" } else { "" };
            Suggestion {
                label: format!("{name}{mark}"),
                replace: format!("{head}{dir}{name}{mark}"),
                desc: if is_dir {
                    "folder".into()
                } else {
                    crate::explorer::human_size(size)
                },
                value: String::new(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(text: &str) -> Vec<String> {
        suggestions(text, &Options::default(), &["src/a.rs".into(), "README.md".into()])
            .into_iter()
            .map(|s| s.label)
            .collect()
    }

    #[test]
    fn commands_by_name_or_short_name() {
        assert_eq!(labels("wq"), ["wq", "wqall"]);
        assert_eq!(labels("se"), ["set"]);
        assert_eq!(labels("Ex"), ["Explore"]);
        assert!(labels("42").is_empty());
        assert!(labels("").len() > 10);
    }

    #[test]
    fn options_with_their_values() {
        let s = suggestions("set rel", &Options::default(), &[]);
        assert_eq!(s[0].label, "relativenumber");
        assert_eq!(s[0].replace, "set relativenumber");
        assert_eq!(s[0].value, "off");
        assert_eq!(suggestions("set ts", &Options::default(), &[]).len(), 0);
        assert_eq!(
            suggestions("set tab", &Options::default(), &[])[0].replace,
            "set tabstop="
        );
        assert_eq!(labels("set nonu"), ["nonumber"]);
        assert_eq!(labels("set roo"), ["roomy"]);
    }

    #[test]
    fn open_buffers() {
        assert_eq!(labels("b rs"), ["src/a.rs"]);
    }
}
