use std::path::Path;
use std::sync::{Arc, Mutex};

use tinsel::app::Waker;

use crate::finder::walk;
use crate::sync::lock;

const MAX_FILE: u64 = 1_000_000;
const MAX_TOTAL: usize = 100_000_000;

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
