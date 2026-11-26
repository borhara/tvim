#![allow(dead_code)]

use tvim::clipboard::Memory;
use tvim::{Editor, Pos, keys};

pub fn ed(text: &str) -> Editor {
    let mut e = Editor::with_text(text);
    e.set_clipboard(Box::new(Memory::default()));
    e.set_screen(80, 24);
    e
}

pub fn after(text: &str, spec: &str) -> (String, (usize, usize)) {
    let mut e = ed(text);
    e.keys(&keys::parse(spec));
    let Pos { row, col } = e.cursor();
    (e.text(), (row, col))
}

pub fn text_after(text: &str, spec: &str) -> String {
    after(text, spec).0
}

pub fn cursor_after(text: &str, spec: &str) -> (usize, usize) {
    after(text, spec).1
}

/// The folder the tests run in, which titles name: tvim in most checkouts, but not all (CI's
/// is called repo).
pub fn folder() -> String {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    dir.file_name().unwrap().to_string_lossy().into_owned()
}

pub fn temp(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("tvim-tests-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

/// `path` rewritten as something else would, changed later than it was, however coarsely the
/// file system keeps the time.
pub fn rewrite(path: &std::path::Path, contents: &[u8]) {
    std::fs::write(path, contents).unwrap();
    let later = std::time::SystemTime::now() + std::time::Duration::from_secs(2);
    std::fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(later)
        .unwrap();
}
