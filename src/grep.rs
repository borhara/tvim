use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use tinsel::app::{Key, Waker};

use crate::finder::{follow, walk};
use crate::sync::{lock, wait_until};

const MAX_FILE: u64 = 1_000_000;
const MAX_TOTAL: usize = 100_000_000;
const MAX_HITS: usize = 2_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hit {
    pub file: usize,
    pub row: usize,
    pub col: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Jump {
    Nothing,
    Open {
        path: PathBuf,
        row: usize,
        col: usize,
        query: String,
    },
    Close,
}

struct File {
    path: String,
    text: String,
    // the same bytes with ASCII lowered, so offsets line up
    lower: String,
    starts: Vec<usize>,
}

impl File {
    fn line(&self, row: usize) -> &str {
        let start = self.starts[row];
        let end = self.starts.get(row + 1).map_or(self.text.len(), |e| e - 1);
        self.text[start..end].trim_end_matches(['\r', '\n'])
    }
}

#[derive(Default)]
struct Loaded {
    files: Vec<File>,
    done: bool,
    cut_short: bool,
}

pub struct Grep {
    root: PathBuf,
    loaded: Arc<Mutex<Loaded>>,
    query: String,
    hits: Vec<Hit>,
    searched: usize,
    capped: bool,
    selected: usize,
    top: Cell<usize>,
}

impl Grep {
    pub fn new(root: &Path, query: &str) -> Grep {
        let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        let loaded = Arc::new(Mutex::new(Loaded::default()));
        let shared = loaded.clone();
        let from = root.clone();
        let waker = Waker::new();
        std::thread::spawn(move || read_all(&from, &shared, &waker));
        let mut g = Grep {
            root,
            loaded,
            query: query.to_string(),
            hits: Vec::new(),
            searched: 0,
            capped: false,
            selected: 0,
            top: Cell::new(0),
        };
        g.refresh();
        g
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Loaded> {
        lock(&self.loaded)
    }

    pub fn wait(&mut self) {
        wait_until(|| self.lock().done);
        self.refresh();
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn hits(&self) -> &[Hit] {
        &self.hits
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn loading(&self) -> bool {
        !self.lock().done
    }

    pub fn capped(&self) -> bool {
        self.capped || self.lock().cut_short
    }

    pub fn file_count(&self) -> usize {
        self.lock().files.len()
    }

    pub fn files_hit(&self) -> usize {
        self.hits.chunk_by(|a, b| a.file == b.file).count()
    }

    pub fn path(&self, hit: &Hit) -> String {
        self.lock().files[hit.file].path.clone()
    }

    pub fn line(&self, hit: &Hit) -> String {
        self.lock().files[hit.file].line(hit.row).to_string()
    }

    pub fn lines(&self, hit: &Hit, first: usize, count: usize) -> Vec<String> {
        let loaded = self.lock();
        let f = &loaded.files[hit.file];
        (first..(first + count).min(f.starts.len()))
            .map(|r| f.line(r).to_string())
            .collect()
    }

    fn ignores_case(&self) -> bool {
        !self.query.chars().any(char::is_uppercase)
    }
    pub fn refresh(&mut self) -> bool {
        let before = self.hits.len();
        let query = self.query.trim().to_string();
        if query.is_empty() || self.capped {
            self.searched = self.file_count();
            return false;
        }
        let ic = self.ignores_case();
        let needle = if ic { query.to_ascii_lowercase() } else { query };
        let loaded = lock(&self.loaded);
        for (i, f) in loaded.files.iter().enumerate().skip(self.searched) {
            let hay = if ic { &f.lower } else { &f.text };
            let mut from = 0;
            while let Some(at) = hay[from..].find(&needle) {
                let at = from + at;
                let row = f.starts.partition_point(|&s| s <= at) - 1;
                let col = f.text[f.starts[row]..at].chars().count();
                if self.hits.len() == MAX_HITS {
                    self.capped = true;
                    self.searched = loaded.files.len();
                    return self.hits.len() != before;
                }
                self.hits.push(Hit { file: i, row, col });
                // one hit per line
                from = f.starts.get(row + 1).copied().unwrap_or(hay.len());
            }
        }
        self.searched = loaded.files.len();
        self.hits.len() != before
    }

    fn search_again(&mut self) {
        self.hits.clear();
        self.searched = 0;
        self.capped = false;
        self.selected = 0;
        self.top.set(0);
        self.refresh();
    }

    pub fn top(&self, rows: usize) -> usize {
        follow(&self.top, self.selected, rows)
    }

    pub fn key(&mut self, key: Key) -> Jump {
        let last = self.hits.len().saturating_sub(1);
        match key {
            Key::Esc | Key::Ctrl('c') => return Jump::Close,
            Key::Enter => {
                return match self.hits.get(self.selected) {
                    Some(h) => Jump::Open {
                        path: self.root.join(self.path(h)),
                        row: h.row,
                        col: h.col,
                        query: self.query.trim().to_string(),
                    },
                    None => Jump::Nothing,
                };
            }
            Key::Down | Key::Ctrl('n') | Key::Ctrl('j') | Key::Tab => self.selected = (self.selected + 1).min(last),
            Key::Up | Key::Ctrl('p') | Key::Ctrl('k') | Key::BackTab => self.selected = self.selected.saturating_sub(1),
            Key::PageDown | Key::Ctrl('d') => self.selected = (self.selected + 10).min(last),
            Key::PageUp => self.selected = self.selected.saturating_sub(10),
            Key::Char(c) => {
                self.query.push(c);
                self.search_again();
            }
            Key::Backspace | Key::Ctrl('h') => {
                if self.query.pop().is_none() {
                    return Jump::Close;
                }
                self.search_again();
            }
            Key::Ctrl('w') => {
                let keep = self
                    .query
                    .trim_end()
                    .rfind(|c: char| !c.is_alphanumeric() && c != '_')
                    .map_or(0, |i| i + 1);
                self.query.truncate(keep);
                self.search_again();
            }
            Key::Ctrl('u') => {
                self.query.clear();
                self.search_again();
            }
            _ => {}
        }
        Jump::Nothing
    }

    pub fn paste(&mut self, text: &str) {
        self.query.push_str(text.lines().next().unwrap_or(""));
        self.search_again();
    }
}

fn read_all(root: &Path, loaded: &Arc<Mutex<Loaded>>, waker: &Waker) {
    let (paths, cut_short) = walk(root);
    let mut total = 0;
    let mut batch = Vec::new();
    let flush = |batch: &mut Vec<File>, done: bool, cut: bool| {
        let mut l = lock(loaded);
        l.files.append(batch);
        l.done = done;
        l.cut_short |= cut;
        drop(l);
        waker.wake();
    };
    for rel in paths {
        // stop once the search is closed
        if Arc::strong_count(loaded) == 1 {
            return;
        }
        let path = root.join(&rel);
        if std::fs::metadata(&path).map_or(true, |m| m.len() > MAX_FILE) {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else { continue };
        // skip binaries
        if bytes.contains(&0) {
            continue;
        }
        let Ok(text) = String::from_utf8(bytes) else { continue };
        total += text.len();
        let mut starts = vec![0];
        starts.extend(text.match_indices('\n').map(|(i, _)| i + 1).filter(|&i| i < text.len()));
        let lower = text.to_ascii_lowercase();
        batch.push(File {
            path: rel,
            text,
            lower,
            starts,
        });
        if batch.len() >= 200 {
            flush(&mut batch, false, false);
        }
        if total > MAX_TOTAL {
            flush(&mut batch, true, true);
            return;
        }
    }
    flush(&mut batch, true, cut_short);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("tvim-grep-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::create_dir_all(root.join("target")).unwrap();
        std::fs::write(root.join("src/a.rs"), "fn alpha() {}\nlet Beta = 2;\nalpha();\n").unwrap();
        std::fs::write(root.join("notes.md"), "# alpha notes\n").unwrap();
        std::fs::write(root.join("target/built.rs"), "alpha\n").unwrap();
        std::fs::write(root.join("logo.png"), [0u8, 159, 146, 150]).unwrap();
        root
    }

    fn grep(root: &Path, query: &str) -> Grep {
        let mut g = Grep::new(root, query);
        g.wait();
        g
    }

    #[test]
    fn finds_every_line_in_every_file() {
        let g = grep(&project("all"), "alpha");
        let found: Vec<(String, usize, usize)> = g.hits().iter().map(|h| (g.path(h), h.row, h.col)).collect();
        assert_eq!(
            found,
            [
                ("notes.md".into(), 0, 2),
                ("src/a.rs".into(), 0, 3),
                ("src/a.rs".into(), 2, 0)
            ]
        );
        assert_eq!(g.files_hit(), 2);
        assert_eq!(g.line(&g.hits()[1]), "fn alpha() {}");
        assert_eq!(
            g.line(&g.hits()[2]),
            "alpha();",
            "the last line comes without its line break"
        );
        assert!(!g.loading());
    }

    #[test]
    fn typing_narrows_and_enter_jumps() {
        let mut g = grep(&project("keys"), "");
        assert!(g.hits().is_empty());
        for c in "alpha(".chars() {
            g.key(Key::Char(c));
        }
        assert_eq!(g.hits().len(), 2);
        g.key(Key::Down);
        match g.key(Key::Enter) {
            Jump::Open { path, row, col, query } => {
                assert!(path.ends_with("src/a.rs"));
                assert_eq!((row, col, query.as_str()), (2, 0, "alpha("));
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(g.key(Key::Esc), Jump::Close);
    }

}
