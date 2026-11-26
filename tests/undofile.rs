mod common;
use common::temp;
use tvim::Editor;
use tvim::keys::parse;

fn open(path: &std::path::Path, undo: &std::path::Path) -> Editor {
    let mut e = Editor::open(path).unwrap();
    e.set_clipboard(Box::new(tvim::clipboard::Memory::default()));
    e.set_undo_dir(Some(undo.to_path_buf()));
    e
}

#[test]
fn undo_outlasts_closing_the_file() {
    let (file, undo) = (temp("kept.txt"), temp("undo-kept"));
    std::fs::write(&file, "one\n").unwrap();
    let mut e = open(&file, &undo);
    e.keys(&parse("otwo<Esc>othree<Esc>:w<CR>"));
    drop(e);

    let mut e = open(&file, &undo);
    assert_eq!(e.text(), "one\ntwo\nthree");
    assert!(!e.is_modified());
    e.keys(&parse("u"));
    assert_eq!(e.text(), "one\ntwo");
    assert!(e.is_modified());
    e.keys(&parse("u"));
    assert_eq!(e.text(), "one");
    e.keys(&parse("<C-r><C-r>"));
    assert_eq!(e.text(), "one\ntwo\nthree");
    assert!(!e.is_modified(), "redone back to as it was saved");
}

#[test]
fn a_file_changed_elsewhere_starts_afresh() {
    let (file, undo) = (temp("changed.txt"), temp("undo-changed"));
    std::fs::write(&file, "one\n").unwrap();
    let mut e = open(&file, &undo);
    e.keys(&parse("otwo<Esc>:w<CR>"));
    std::fs::write(&file, "something else\n").unwrap();
    let mut e = open(&file, &undo);
    e.keys(&parse("u"));
    assert_eq!(e.text(), "something else");
}

#[test]
fn files_opened_later_get_theirs_too() {
    let (a, b, undo) = (temp("later-a.txt"), temp("later-b.txt"), temp("undo-later"));
    std::fs::write(&a, "a\n").unwrap();
    std::fs::write(&b, "b\n").unwrap();
    let mut e = open(&b, &undo);
    e.keys(&parse("ddib2<Esc>:w<CR>"));
    let mut e = open(&a, &undo);
    e.ex(&format!("e {}", b.display()));
    e.keys(&parse("u"));
    assert_eq!(e.text(), "", "the first change undone leaves what dd did");
    e.keys(&parse("u"));
    assert_eq!(e.text(), "b");
}

#[test]
fn noundofile_keeps_nothing() {
    let (file, undo) = (temp("none.txt"), temp("undo-none"));
    std::fs::write(&file, "one\n").unwrap();
    let mut e = open(&file, &undo);
    e.keys(&parse(":set noudf<CR>otwo<Esc>:w<CR>"));
    assert!(!undo.exists() || std::fs::read_dir(&undo).unwrap().next().is_none());
}

#[cfg(unix)]
#[test]
fn only_you_can_read_the_history() {
    use std::os::unix::fs::PermissionsExt;
    let (file, undo) = (temp("private.txt"), temp("undo-private"));
    std::fs::write(&file, "one\n").unwrap();
    let mut e = open(&file, &undo);
    e.keys(&parse("otwo<Esc>:w<CR>"));
    let mode = |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(&undo), 0o700);
    let kept = std::fs::read_dir(&undo).unwrap().next().unwrap().unwrap().path();
    assert_eq!(mode(&kept), 0o600);
}
