mod common;
use common::temp;
use std::path::{Path, PathBuf};
use tvim::keys::parse;
use tvim::session::{self, Place};
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

fn place(path: &Path, row: usize, col: usize) -> Place {
    Place {
        path: path.to_path_buf(),
        pos: Pos::new(row, col),
    }
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

#[test]
fn files_that_are_gone_are_skipped() {
    let dir = fresh("session-gone");
    let state = dir.join("state");
    let a = numbered(&dir, "a.txt", 3);
    let b = numbered(&dir, "b.txt", 3);
    let mut e = Editor::new();
    e.ex(&format!("e {}", a.display()));
    e.ex(&format!("e {}", b.display()));
    e.save_state(&state).unwrap();
    std::fs::remove_file(&b).unwrap();
    let mut e = Editor::new();
    assert!(e.restore_session(&state));
    assert_eq!(e.buffers().len(), 1);
    assert_eq!(e.file().path.as_ref().unwrap().file_name().unwrap(), "a.txt");

    assert!(!Editor::new().restore_session(&dir.join("nothing-here")));
}

#[test]
fn recent_files_are_most_recent_first_and_remember_the_line() {
    let dir = fresh("session-recent");
    let state = dir.join("state");
    let a = numbered(&dir, "a.txt", 20);
    let b = numbered(&dir, "b.txt", 20);
    let c = numbered(&dir, "c.txt", 20);
    let mut e = Editor::new();
    e.ex(&format!("e {}", a.display()));
    e.keys(&parse("5G"));
    e.ex(&format!("e {}", b.display()));
    e.ex("bd");
    e.ex(&format!("e {}", c.display()));
    e.keys(&parse("9G"));
    e.save_state(&state).unwrap();
    let recent = session::read_recent(&state);
    assert_eq!(recent, [place(&c, 8, 0), place(&a, 4, 0), place(&b, 0, 0)]);
    assert_eq!(
        std::fs::read_to_string(state.join("recent"))
            .unwrap()
            .lines()
            .next()
            .unwrap(),
        format!("9 1 {}", c.display())
    );

    let mut e = Editor::open(&a).unwrap();
    e.set_recent(recent.clone());
    e.restore_place();
    assert_eq!(e.cursor(), Pos::new(4, 0), "an argument opens at its last line too");

    let mut e = Editor::new();
    e.set_recent(recent);
    e.ex(&format!("e {}", c.display()));
    assert_eq!(e.cursor().row, 8, "and so does :e");
}
