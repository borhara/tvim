use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use tinsel::app::Waker;

use crate::finder::walk;
use crate::sync::lock;

const MAX_FILE: u64 = 1_000_000;
const MAX_TOTAL: usize = 100_000_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hit {
    pub file: usize,
    pub row: usize,
    pub col: usize,
}

struct File {
    path: String,
    text: String,
    // the same bytes with ASCII lowered, so offsets line up
    lower: String,
    starts: Vec<usize>,
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
        self.refresh();
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
