use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use tinsel::app::Key;

const SKIP: &[&str] = &["target", "node_modules", "__pycache__", "venv", "dist", "build"];

const MAX_FILES: usize = 50_000;
const MAX_TIME: Duration = Duration::from_millis(1500);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pick {
    Nothing,
    Open(PathBuf),
    Close,
}

pub struct Finder {
    root: PathBuf,
    files: Vec<String>,
    cut_short: bool,
    query: String,
    matches: Vec<(usize, Vec<usize>)>,
    selected: usize,
    top: Cell<usize>,
    preview: RefCell<Option<(String, Vec<String>)>>,
    title: String,
    // for recent files: the line each was left at, and j/k move until a filter is typed
    lines: Option<Vec<usize>>,
}

pub fn walk(root: &Path) -> (Vec<String>, bool) {
    let ignored = gitignore(root);
    let mut files = Vec::new();
    let started = Instant::now();
    let mut cut_short = false;
    let mut stack = vec![(root.to_path_buf(), 0usize)];
    while let Some((dir, depth)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let Ok(kind) = entry.file_type() else { continue };
            let Some(name) = entry.file_name().to_str().map(str::to_string) else {
                continue;
            };
            if ignored.iter().any(|p| p.matches(&name)) {
                continue;
            }
            if kind.is_dir() {
                if !SKIP.contains(&name.as_str()) && !name.starts_with('.') && depth < 32 {
                    stack.push((entry.path(), depth + 1));
                }
            } else if kind.is_file() && name != ".DS_Store" {
                if let Ok(rel) = entry.path().strip_prefix(root) {
                    files.push(rel.to_string_lossy().replace('\\', "/"));
                }
            }
        }
        if files.len() >= MAX_FILES || started.elapsed() > MAX_TIME {
            cut_short = true;
            break;
        }
    }
    files.sort_by(|a, b| {
        a.matches('/')
            .count()
            .cmp(&b.matches('/').count())
            .then_with(|| a.cmp(b))
    });
    (files, cut_short)
}

impl Finder {
    pub fn new(root: &Path, query: &str) -> Finder {
        let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        let (files, cut_short) = walk(&root);
        let name = root
            .file_name()
            .map_or_else(|| "/".into(), |n| n.to_string_lossy().into_owned());
        Finder::build(root, files, cut_short, query, format!("find in {name}"))
    }

    fn build(root: PathBuf, files: Vec<String>, cut_short: bool, query: &str, title: String) -> Finder {
        let mut f = Finder {
            root,
            files,
            cut_short,
            query: query.to_string(),
            matches: Vec::new(),
            selected: 0,
            top: Cell::new(0),
            preview: RefCell::new(None),
            title,
            lines: None,
        };
        f.refresh();
        f
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn total(&self) -> usize {
        self.files.len()
    }

    pub fn is_recent(&self) -> bool {
        self.lines.is_some()
    }

    pub fn line(&self, nth: usize) -> Option<usize> {
        let (i, _) = self.matches.get(nth)?;
        self.lines.as_ref()?.get(*i).copied()
    }

    pub fn cut_short(&self) -> bool {
        self.cut_short
    }

    pub fn matches(&self) -> impl Iterator<Item = (&str, &[usize])> {
        self.matches
            .iter()
            .map(|(i, hits)| (self.files[*i].as_str(), hits.as_slice()))
    }

    pub fn preview(&self) -> Option<(String, Vec<String>)> {
        let (i, _) = self.matches.get(self.selected)?;
        let rel = &self.files[*i];
        // a recent file shows around the line it was left at, so read that far
        let (bytes_cap, lines_cap) = match self.line(self.selected) {
            Some(row) => (4 << 20, row + 200),
            None => (64 * 1024, 200),
        };
        let mut cache = self.preview.borrow_mut();
        if cache.as_ref().is_none_or(|(p, _)| p != rel) {
            let mut bytes = Vec::new();
            if let Ok(f) = std::fs::File::open(self.root.join(rel)) {
                use std::io::Read;
                let _ = f.take(bytes_cap).read_to_end(&mut bytes);
            }
            let text = match std::str::from_utf8(&bytes) {
                Ok(text) => Some(text),
                Err(e) => std::str::from_utf8(&bytes[..e.valid_up_to()]).ok().filter(|t| !t.is_empty()),
            };
            let lines = match text {
                Some(text) => text.lines().take(lines_cap).map(|l| l.replace('\r', "")).collect(),
                None => vec!["(not text)".to_string()],
            };
            *cache = Some((rel.clone(), lines));
        }
        cache.clone()
    }

    pub fn match_count(&self) -> usize {
        self.matches.len()
    }

    fn refresh(&mut self) {
        let query = needle(&self.query);
        let mut scored: Vec<(i64, usize, Vec<usize>)> = self
            .files
            .iter()
            .enumerate()
            .filter_map(|(i, f)| score(f, &query).map(|(s, hits)| (s, i, hits)))
            .collect();
        if self.lines.is_none() || !self.query.trim().is_empty() {
            scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        }
        self.matches = scored.into_iter().map(|(_, i, hits)| (i, hits)).collect();
        self.selected = 0;
        self.top.set(0);
    }

    pub fn top(&self, rows: usize) -> usize {
        follow(&self.top, self.selected, rows)
    }

    pub fn key(&mut self, key: Key) -> Pick {
        let last = self.matches.len().saturating_sub(1);
        let key = match key {
            Key::Char('j') if self.is_recent() && self.query.is_empty() => Key::Down,
            Key::Char('k') if self.is_recent() && self.query.is_empty() => Key::Up,
            k => k,
        };
        match key {
            Key::Esc | Key::Ctrl('c') => return Pick::Close,
            Key::Enter => {
                return match self.matches.get(self.selected) {
                    Some((i, _)) => Pick::Open(self.root.join(&self.files[*i])),
                    None => Pick::Nothing,
                };
            }
            Key::Down | Key::Ctrl('n') | Key::Ctrl('j') | Key::Tab => self.selected = (self.selected + 1).min(last),
            Key::Up | Key::Ctrl('p') | Key::Ctrl('k') | Key::BackTab => self.selected = self.selected.saturating_sub(1),
            Key::PageDown => self.selected = (self.selected + 10).min(last),
            Key::PageUp => self.selected = self.selected.saturating_sub(10),
            Key::Char(c) => {
                self.query.push(c);
                self.refresh();
            }
            Key::Backspace | Key::Ctrl('h') => {
                if self.query.pop().is_none() {
                    return Pick::Close;
                }
                self.refresh();
            }
            Key::Ctrl('w') => {
                let keep = self
                    .query
                    .trim_end()
                    .rfind(['/', ' ', '.', '_', '-'])
                    .map_or(0, |i| i + 1);
                self.query.truncate(keep);
                self.refresh();
            }
            Key::Ctrl('u') => {
                self.query.clear();
                self.refresh();
            }
            _ => {}
        }
        Pick::Nothing
    }

    pub fn paste(&mut self, text: &str) {
        self.query.push_str(text.lines().next().unwrap_or("").trim());
        self.refresh();
    }
}

// scrolls as little as it can to keep selected in view
pub(crate) fn follow(top: &Cell<usize>, selected: usize, rows: usize) -> usize {
    let rows = rows.max(1);
    let mut at = top.get();
    if selected < at {
        at = selected;
    }
    if selected >= at + rows {
        at = selected + 1 - rows;
    }
    top.set(at);
    at
}

fn needle(query: &str) -> Vec<char> {
    query
        .chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
}

fn score(path: &str, query: &[char]) -> Option<(i64, Vec<usize>)> {
    let chars: Vec<char> = path.chars().collect();
    if query.is_empty() {
        return Some((-(chars.len() as i64), Vec::new()));
    }
    let name_start = chars.iter().rposition(|c| *c == '/').map_or(0, |i| i + 1);
    let hits = subsequence(&chars[name_start..], query)
        .map(|h| h.into_iter().map(|i| i + name_start).collect::<Vec<_>>())
        .or_else(|| subsequence(&chars, query))?;
    let mut score: i64 = 0;
    for (n, &i) in hits.iter().enumerate() {
        score += 10;
        if i >= name_start {
            score += 12;
        }
        if n > 0 && hits[n - 1] + 1 == i {
            score += 18;
        }
        let boundary = i == 0
            || matches!(chars[i - 1], '/' | '_' | '-' | '.' | ' ')
            || chars[i].is_uppercase() && chars[i - 1].is_lowercase();
        if boundary {
            score += 20;
        }
        if i == name_start {
            score += 25;
        }
    }
    let spread = hits.last().unwrap_or(&0) - hits.first().unwrap_or(&0);
    score -= spread as i64;
    score -= chars.len() as i64 / 3;
    Some((score, hits))
}

fn subsequence(chars: &[char], query: &[char]) -> Option<Vec<usize>> {
    let lower: Vec<char> = chars.iter().map(|c| c.to_lowercase().next().unwrap_or(*c)).collect();
    // leftmost match, then pull letters right so they cluster
    let mut hits = Vec::with_capacity(query.len());
    let mut from = 0;
    for q in query {
        let at = (from..lower.len()).find(|&i| lower[i] == *q)?;
        hits.push(at);
        from = at + 1;
    }
    let mut limit = lower.len();
    for k in (0..hits.len()).rev() {
        let latest = (hits[k]..limit)
            .rev()
            .find(|&i| lower[i] == query[k])
            .unwrap_or(hits[k]);
        hits[k] = latest;
        limit = latest;
    }
    for k in (0..hits.len().saturating_sub(1)).rev() {
        if hits[k] + 1 != hits[k + 1] && hits[k + 1] > 0 && lower[hits[k + 1] - 1] == query[k] {
            let lowest = if k == 0 { 0 } else { hits[k - 1] + 1 };
            if hits[k + 1] > lowest {
                hits[k] = hits[k + 1] - 1;
            }
        }
    }
    Some(hits)
}

struct Ignore {
    prefix: String,
    suffix: String,
    wild: bool,
}

impl Ignore {
    fn matches(&self, name: &str) -> bool {
        if self.wild {
            name.len() >= self.prefix.len() + self.suffix.len()
                && name.starts_with(&self.prefix)
                && name.ends_with(&self.suffix)
        } else {
            name == self.prefix
        }
    }
}

fn gitignore(root: &Path) -> Vec<Ignore> {
    let Ok(text) = std::fs::read_to_string(root.join(".gitignore")) else {
        return Vec::new();
    };
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#') && !l.starts_with('!'))
        .filter_map(|l| {
            let l = l.trim_start_matches('/').trim_end_matches('/');
            // only bare names and single * globs
            if l.contains('/') || l.matches('*').count() > 1 || l.is_empty() {
                return None;
            }
            Some(match l.split_once('*') {
                Some((p, s)) => Ignore {
                    prefix: p.to_string(),
                    suffix: s.to_string(),
                    wild: true,
                },
                None => Ignore {
                    prefix: l.to_string(),
                    suffix: String::new(),
                    wild: false,
                },
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn best(files: &[&str], query: &str) -> String {
        let mut scored: Vec<(i64, &str)> = files
            .iter()
            .filter_map(|f| score(f, &needle(query)).map(|(s, _)| (s, *f)))
            .collect();
        scored.sort_by(|a, b| b.0.cmp(&a.0));
        scored.first().map(|x| x.1.to_string()).unwrap_or_default()
    }

    #[test]
    fn letters_in_order_match() {
        let score = |path: &str, query: &str| score(path, &needle(query));
        assert!(score("src/editor.rs", "edrs").is_some());
        assert!(score("src/editor.rs", "zz").is_none());
        assert_eq!(score("src/ui.rs", "UI").unwrap().1, [4, 5], "case is ignored");
        assert_eq!(
            score("a b", "a b").unwrap().1,
            [0, 2],
            "spaces in the query are ignored"
        );
    }

    #[test]
    fn matches_bunch_together() {
        assert_eq!(
            subsequence(&"xaxab".chars().collect::<Vec<_>>(), &['a', 'b']),
            Some(vec![3, 4])
        );
    }

    #[test]
    fn the_file_name_counts_most() {
        let files = ["src/editor.rs", "editor/src/main.rs", "docs/edit-notes.md"];
        assert_eq!(best(&files, "editor"), "src/editor.rs");
        assert_eq!(best(&files, "main"), "editor/src/main.rs");
        assert_eq!(best(&["tests/ui_test.rs", "src/ui.rs"], "ui"), "src/ui.rs");
        assert_eq!(best(&["src/some/deep/thing/mod.rs", "src/mod.rs"], "mod"), "src/mod.rs");
        assert_eq!(best(&["src/explorer.rs", "examples/x.rs"], "exp"), "src/explorer.rs");
    }

}
