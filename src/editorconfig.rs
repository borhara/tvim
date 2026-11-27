//! A project's `.editorconfig`: how its files are indented, and how they end.

use std::path::Path;

/// What the `.editorconfig` files above a file say about it, each None where they don't.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Settings {
    /// indent_style: true for spaces, false for tabs
    pub spaces: Option<bool>,
    pub indent_size: Option<usize>,
    pub tab_width: Option<usize>,
    pub trim_trailing_whitespace: Option<bool>,
    pub insert_final_newline: Option<bool>,
}

impl Settings {
    pub fn is_empty(&self) -> bool {
        *self == Settings::default()
    }

    // a property, as `key = value`, or nothing if it's not one of ours or can't be read
    fn set(&mut self, key: &str, value: &str) {
        let flag = || match value {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        };
        match key {
            "indent_style" => {
                self.spaces = match value {
                    "space" => Some(true),
                    "tab" => Some(false),
                    _ => None,
                }
            }
            // `tab` takes it from tab_width, worked out once everything's read
            "indent_size" if value == "tab" => self.indent_size = Some(0),
            "indent_size" => self.indent_size = value.parse().ok().filter(|&n| n > 0),
            "tab_width" => self.tab_width = value.parse().ok().filter(|&n| n > 0),
            "trim_trailing_whitespace" => self.trim_trailing_whitespace = flag(),
            "insert_final_newline" => self.insert_final_newline = flag(),
            _ => {}
        }
    }

    /// How wide a tab shows, as tab_width says or else indent_size.
    pub fn tab_stop(&self) -> Option<usize> {
        self.tab_width.or(self.indent_size.filter(|&n| n > 0))
    }

    /// How far an indent goes, as indent_size says, or tab_width for `tab`.
    pub fn shift_width(&self) -> Option<usize> {
        match self.indent_size {
            Some(0) => self.tab_width,
            size => size,
        }
    }
}

/// The settings for `path`, from each `.editorconfig` above it up to one that says
/// `root = true`, the nearest winning.
pub fn for_file(path: &Path) -> Settings {
    let path = crate::paths::absolute(path);
    let mut files = Vec::new();
    let mut dir = path.parent();
    while let Some(d) = dir {
        if let Ok(text) = std::fs::read_to_string(d.join(".editorconfig")) {
            let root = is_root(&text);
            files.push((d.to_path_buf(), text));
            if root {
                break;
            }
        }
        dir = d.parent();
    }
    let mut out = Settings::default();
    for (dir, text) in files.iter().rev() {
        let Ok(rel) = path.strip_prefix(dir) else { continue };
        apply(&mut out, text, &rel.to_string_lossy().replace('\\', "/"));
    }
    out
}

fn is_root(text: &str) -> bool {
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            return false;
        }
        if let Some((k, v)) = line.split_once('=')
            && k.trim().eq_ignore_ascii_case("root")
        {
            return v.trim().eq_ignore_ascii_case("true");
        }
    }
    false
}

/// What one file's sections say about the file at `rel` from its folder.
pub fn apply(out: &mut Settings, text: &str, rel: &str) {
    let mut on = false;
    for line in text.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if let Some(section) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            on = section_matches(section, rel);
        } else if on && let Some((k, v)) = line.split_once('=') {
            out.set(&k.trim().to_lowercase(), &v.trim().to_lowercase());
        }
    }
}

// a section's glob: without a slash it matches the name in any folder, with one from the top
fn section_matches(glob: &str, rel: &str) -> bool {
    let glob = match glob.strip_prefix('/') {
        Some(g) => g.to_string(),
        None if glob.contains('/') => glob.to_string(),
        None => format!("**/{glob}"),
    };
    expand(&glob).iter().any(|g| matches(g.as_bytes(), rel.as_bytes()))
}

// {a,b} and {1..3} as every glob they stand for
fn expand(glob: &str) -> Vec<String> {
    let Some(open) = glob.find('{') else {
        return vec![glob.to_string()];
    };
    let mut depth = 0;
    let Some(close) = glob[open..].char_indices().find_map(|(i, c)| {
        depth += match c {
            '{' => 1,
            '}' => -1,
            _ => 0,
        };
        (depth == 0).then_some(open + i)
    }) else {
        return vec![glob.to_string()];
    };
    let (before, inside, after) = (&glob[..open], &glob[open + 1..close], &glob[close + 1..]);
    let range = inside
        .split_once("..")
        .and_then(|(a, b)| Some((a.parse::<i64>().ok()?, b.parse::<i64>().ok()?)));
    let options: Vec<String> = match range {
        Some((a, b)) if b.abs_diff(a) <= 1000 => (a.min(b)..=a.max(b)).map(|n| n.to_string()).collect(),
        _ => split_top(inside),
    };
    options
        .iter()
        .flat_map(|o| expand(&format!("{before}{o}{after}")))
        .collect()
}

// at the commas not inside a further {}
fn split_top(s: &str) -> Vec<String> {
    let (mut out, mut part, mut depth) = (Vec::new(), String::new(), 0);
    for c in s.chars() {
        match c {
            '{' => depth += 1,
            '}' => depth -= 1,
            ',' if depth == 0 => {
                out.push(std::mem::take(&mut part));
                continue;
            }
            _ => {}
        }
        part.push(c);
    }
    out.push(part);
    out
}

// * within a folder, ** across them, ? one character, [abc] and [!abc] one of or none of
fn matches(glob: &[u8], name: &[u8]) -> bool {
    match glob {
        [] => name.is_empty(),
        [b'*', b'*', b'/', rest @ ..] => {
            matches(rest, name) || (0..name.len()).any(|i| name[i] == b'/' && matches(rest, &name[i + 1..]))
        }
        [b'*', b'*', rest @ ..] => (0..=name.len()).any(|i| matches(rest, &name[i..])),
        [b'*', rest @ ..] => (0..=name.len())
            .take_while(|&i| i == 0 || name[i - 1] != b'/')
            .any(|i| matches(rest, &name[i..])),
        [b'?', rest @ ..] => name.first().is_some_and(|&c| c != b'/') && matches(rest, &name[1..]),
        [b'[', rest @ ..] if rest.contains(&b']') => {
            let end = rest.iter().position(|&c| c == b']').expect("checked");
            let (set, rest) = (&rest[..end], &rest[end + 1..]);
            let (not, set) = match set {
                [b'!', s @ ..] => (true, s),
                s => (false, s),
            };
            let Some(&c) = name.first() else { return false };
            in_set(set, c) != not && matches(rest, &name[1..])
        }
        [b'\\', c, rest @ ..] | [c, rest @ ..] => name.first() == Some(c) && matches(rest, &name[1..]),
    }
}

fn in_set(set: &[u8], c: u8) -> bool {
    let mut i = 0;
    while i < set.len() {
        if i + 2 < set.len() && set[i + 1] == b'-' {
            if (set[i]..=set[i + 2]).contains(&c) {
                return true;
            }
            i += 3;
        } else {
            if set[i] == c {
                return true;
            }
            i += 1;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(text: &str, rel: &str) -> Settings {
        let mut s = Settings::default();
        apply(&mut s, text, rel);
        s
    }

    #[test]
    fn globs_match_as_editorconfig_says() {
        assert!(section_matches("*", "a.rs"));
        assert!(section_matches("*.rs", "src/deep/a.rs"), "no slash: any folder");
        assert!(!section_matches("*.rs", "a.rsx"));
        assert!(section_matches("src/*.rs", "src/a.rs"));
        assert!(!section_matches("src/*.rs", "src/deep/a.rs"), "* stays in its folder");
        assert!(section_matches("src/**.rs", "src/deep/a.rs"));
        assert!(section_matches("/Makefile", "Makefile"));
        assert!(!section_matches("/Makefile", "sub/Makefile"));
        assert!(section_matches("*.{js,ts}", "x/b.ts"));
        assert!(section_matches("{package.json,.travis.yml}", ".travis.yml"));
        assert!(section_matches("file{1..3}.txt", "file2.txt"));
        assert!(!section_matches("file{1..3}.txt", "file4.txt"));
        assert!(section_matches("[Mm]akefile", "makefile"));
        assert!(section_matches("[!a-c]x", "dx"));
        assert!(!section_matches("[!a-c]x", "bx"));
        assert!(section_matches("?.c", "a.c"));
    }

    #[test]
    fn later_sections_win_and_others_are_left_alone() {
        let text = "root = true\n\n[*]\nindent_style = space\nindent_size = 4\nend_of_line = lf\n\n# go uses tabs\n[*.go]\nindent_style = tab\ntab_width = 8\n";
        let go = settings(text, "cmd/main.go");
        assert_eq!(go.spaces, Some(false));
        assert_eq!(go.tab_stop(), Some(8));
        assert_eq!(go.shift_width(), Some(4));
        let rs = settings(text, "a.rs");
        assert_eq!(
            (rs.spaces, rs.tab_stop(), rs.shift_width()),
            (Some(true), Some(4), Some(4))
        );
        assert!(settings("[*.md]\nindent_size = 2\n", "a.rs").is_empty());
    }

    #[test]
    fn indent_size_tab_takes_tab_width() {
        let s = settings("[*]\nindent_style = tab\nindent_size = tab\ntab_width = 3\n", "x");
        assert_eq!((s.tab_stop(), s.shift_width()), (Some(3), Some(3)));
        let s = settings(
            "[*]\nINDENT_SIZE = 2\ntrim_trailing_whitespace = true\ninsert_final_newline = false\n",
            "x",
        );
        assert_eq!(s.shift_width(), Some(2));
        assert_eq!(s.trim_trailing_whitespace, Some(true));
        assert_eq!(s.insert_final_newline, Some(false));
    }

    #[test]
    fn files_nearer_win_up_to_a_root() {
        let top = std::env::temp_dir().join(format!("tvim-ec-{}", std::process::id()));
        let deep = top.join("proj/src");
        std::fs::create_dir_all(&deep).unwrap();
        std::fs::write(top.join(".editorconfig"), "[*]\nindent_size = 8\ntab_width = 8\n").unwrap();
        std::fs::write(top.join("proj/.editorconfig"), "root = true\n[*]\nindent_size = 2\n").unwrap();
        std::fs::write(deep.join(".editorconfig"), "[*.rs]\nindent_style = tab\n").unwrap();
        let s = for_file(&deep.join("a.rs"));
        assert_eq!(s.shift_width(), Some(2));
        assert_eq!(s.spaces, Some(false));
        assert_eq!(s.tab_width, None, "nothing above the root counts");
        let _ = std::fs::remove_dir_all(&top);
    }
}
