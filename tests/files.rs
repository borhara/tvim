mod common;
use common::temp;
use tvim::{Editor, keys::parse};

fn open(name: &str, contents: &[u8]) -> (Editor, std::path::PathBuf) {
    let path = temp(name);
    std::fs::write(&path, contents).unwrap();
    let mut e = Editor::open(&path).unwrap();
    e.set_clipboard(Box::new(tvim::clipboard::Memory::default()));
    (e, path)
}

#[test]
fn opening_says_what_was_read() {
    let (e, path) = open("read.txt", b"one\ntwo\n");
    assert_eq!(e.text(), "one\ntwo");
    assert_eq!(e.message().unwrap().text, format!("\"{}\" 2L, 8B", path.display()));
    assert!(!e.is_modified());
}

#[test]
fn line_endings_and_the_final_newline_are_kept() {
    let (mut e, path) = open("crlf.txt", b"a\r\nb\r\n");
    assert!(e.file().crlf);
    assert_eq!(e.text(), "a\nb");
    e.keys(&parse("Ax<Esc>:w<CR>"));
    assert_eq!(std::fs::read(&path).unwrap(), b"ax\r\nb\r\n");

    let (mut e, path) = open("noeol.txt", b"a\nb");
    assert!(!e.file().final_newline);
    assert!(e.message().unwrap().text.contains("[noeol]"));
    e.keys(&parse("x:w<CR>"));
    assert_eq!(std::fs::read(&path).unwrap(), b"\nb");

    let (mut e, path) = open("empty.txt", b"");
    e.keys(&parse(":w<CR>"));
    assert_eq!(std::fs::read(&path).unwrap(), b"");
    assert_eq!(e.text(), "");

    let (mut e, path) = open("blank.txt", b"\n");
    assert!(e.message().unwrap().text.contains(" 1L, 1B"));
    e.keys(&parse(":w<CR>"));
    assert_eq!(
        std::fs::read(&path).unwrap(),
        b"\n",
        "one empty line isn't an empty file"
    );
    assert!(!e.is_modified());

    let (mut e, path) = open("blank-dos.txt", b"\r\n");
    e.keys(&parse(":w<CR>"));
    assert_eq!(std::fs::read(&path).unwrap(), b"\r\n");
}

#[test]
fn a_new_file() {
    let path = temp("brand-new.txt");
    let _ = std::fs::remove_file(&path);
    let mut e = Editor::open(&path).unwrap();
    assert!(e.file().new);
    assert!(e.message().unwrap().text.ends_with("[New]"));
    e.keys(&parse("ihello<Esc>:w<CR>"));
    assert!(e.message().unwrap().text.contains("[New] 1L, 6B written"));
    assert!(!e.is_modified());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "hello\n");
}

#[test]
fn files_that_arent_text_dont_open() {
    let path = temp("binary.bin");
    std::fs::write(&path, [0xff, 0xfe, 0x00]).unwrap();
    let err = Editor::open(&path).err().unwrap();
    assert!(err.to_string().contains("isn't UTF-8"), "{err}");
}

#[test]
fn writing_somewhere_else() {
    let (mut e, path) = open("mine.txt", b"x\n");
    let other = temp("other.txt");
    std::fs::write(&other, "taken").unwrap();
    e.ex(&format!("w {}", other.display()));
    assert_eq!(e.message().unwrap().text, "E13: File exists (add ! to override)");
    e.ex(&format!("w! {}", other.display()));
    assert_eq!(std::fs::read_to_string(&other).unwrap(), "x\n");
    assert_eq!(
        e.file().path.as_deref(),
        Some(path.as_path()),
        "the buffer keeps its own name"
    );
    e.ex(&format!("2,1w {}", temp("part.txt").display()));
    e.keys(&parse("ix<Esc>"));
    e.ex(&format!("1w! {}", temp("part.txt").display()));
    assert_eq!(std::fs::read_to_string(temp("part.txt")).unwrap(), "xx\n");
    assert!(e.is_modified(), "writing part of it elsewhere doesn't save it");
}

#[test]
fn reloading_and_switching_files() {
    let (mut e, path) = open("reload.txt", b"first\n");
    e.keys(&parse("dd"));
    e.ex("e");
    assert!(e.message().unwrap().text.starts_with("E37"));
    e.ex("e!");
    assert_eq!(e.text(), "first");
    assert!(!e.is_modified());
    e.keys(&parse("u"));
    assert_eq!(e.text(), "first", "reloading starts the undo history again");
    let other = temp("switch.txt");
    std::fs::write(&other, "second\n").unwrap();
    e.ex(&format!("e {}", other.display()));
    assert_eq!(e.text(), "second");
    assert_eq!(e.file().path.as_deref(), Some(other.as_path()));
    let _ = path;
}

#[test]
fn reading_a_file_in() {
    let (mut e, _) = open("host.txt", b"a\nb\n");
    let part = temp("part-in.txt");
    std::fs::write(&part, "x\ny\n").unwrap();
    e.ex(&format!("r {}", part.display()));
    assert_eq!(e.text(), "a\nx\ny\nb");
    assert_eq!(e.cursor().row, 1);
    e.ex("r /nonexistent/nope");
    assert!(e.message().unwrap().text.starts_with("E484"));
}

#[test]
fn write_errors_are_shown() {
    let mut e = Editor::with_text("x");
    e.ex("w");
    assert_eq!(e.message().unwrap().text, "E32: No file name");
    e.ex("w /nonexistent-dir/x.txt");
    assert!(e.message().unwrap().text.starts_with("E212"));
}

#[test]
fn wq_writes_even_when_unchanged_and_x_only_when_changed() {
    let (mut e, path) = open("x.txt", b"same\n");
    std::fs::write(&path, "changed behind our back\n").unwrap();
    e.ex("x");
    assert!(e.should_quit());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "changed behind our back\n");
    let (mut e, path) = open("wq.txt", b"same\n");
    std::fs::write(&path, "changed behind our back\n").unwrap();
    e.ex("wq");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "same\n");
}

#[test]
fn the_language_comes_from_the_name() {
    let (e, _) = open("code.rs", b"fn main() {}\n");
    assert_eq!(e.syntax(), Some(tvim::syntax::Syntax::Rust));
    let (e, _) = open("notes.txt", b"");
    assert_eq!(e.syntax(), None);
}
