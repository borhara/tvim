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
fn the_language_comes_from_the_name() {
    let (e, _) = open("code.rs", b"fn main() {}\n");
    assert_eq!(e.syntax(), Some(tvim::syntax::Syntax::Rust));
    let (e, _) = open("notes.txt", b"");
    assert_eq!(e.syntax(), None);
}
