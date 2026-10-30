mod common;
use common::temp;
use tvim::Editor;

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
fn files_that_arent_text_dont_open() {
    let path = temp("binary.bin");
    std::fs::write(&path, [0xff, 0xfe, 0x00]).unwrap();
    let err = Editor::open(&path).err().unwrap();
    assert!(err.to_string().contains("isn't UTF-8"), "{err}");
}

#[test]
fn the_language_comes_from_the_name() {
    let (e, _) = open("code.rs", b"fn main() {}\n");
    assert_eq!(e.syntax(), Some(tvim::syntax::Syntax::Rust));
    let (e, _) = open("notes.txt", b"");
    assert_eq!(e.syntax(), None);
}
