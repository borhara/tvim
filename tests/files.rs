mod common;

#[test]
fn the_language_comes_from_the_name() {
    let (e, _) = open("code.rs", b"fn main() {}\n");
    assert_eq!(e.syntax(), Some(tvim::syntax::Syntax::Rust));
    let (e, _) = open("notes.txt", b"");
    assert_eq!(e.syntax(), None);
}
