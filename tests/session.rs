mod common;
use common::temp;
use std::path::{Path, PathBuf};
use tvim::keys::parse;
use tvim::session;
use tvim::{Editor, Pos};

fn fresh(name: &str) -> PathBuf {
    let dir = temp(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn numbered(dir: &Path, name: &str, lines: usize) -> PathBuf {
    let path = dir.join(name);
    let text: String = (1..=lines).map(|i| format!("line {i}\n")).collect();
    std::fs::write(&path, text).unwrap();
    path.canonicalize().unwrap()
}

#[test]
fn open_files_come_back_where_they_were() {
    let dir = fresh("session-back");
    let state = dir.join("state");
    let a = numbered(&dir, "a.txt", 50);
    let b = numbered(&dir, "b.txt", 50);
    let mut e = Editor::new();
    e.ex(&format!("e {}", a.display()));
    e.keys(&parse("30G3l"));
    e.ex(&format!("e {}", b.display()));
    e.keys(&parse("12G"));
    e.ex("bfirst");
    e.save_state(&state).unwrap();

    let saved = std::fs::read_to_string(state.join("session")).unwrap();
    assert_eq!(
        saved,
        format!("current 0\n30 4 {}\n12 1 {}\n", a.display(), b.display())
    );

    let mut e = Editor::new();
    assert!(e.restore_session(&state));
    assert_eq!(e.buffers().len(), 2);
    assert_eq!(e.buffer_index(), 0);
    assert_eq!(e.cursor(), Pos::new(29, 3));
    assert_eq!(e.message().unwrap().text, "reopened 2 files");
    e.ex("bnext");
    assert_eq!(e.cursor(), Pos::new(11, 0));
}
