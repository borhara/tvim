mod common;
use common::{ed, text_after as t};
use tvim::keys::parse;

#[test]
fn named_registers() {
    assert_eq!(t("a\nb", "\"ayyj\"byy\"ap\"bp"), "a\nb\na\nb");
    assert_eq!(t("a\nb", "\"ayyj\"Ayy\"ap"), "a\nb\na\nb");
    assert_eq!(t("one two", "\"xyiww\"Xyiw$\"xp"), "one twoonetwo");
}

#[test]
fn the_black_hole_keeps_what_was_there() {
    assert_eq!(t("a\nb", "yyj\"_ddp"), "a\na");
}

#[test]
fn the_clipboard() {
    let mut e = ed("hello\nworld");
    e.keys(&parse("\"+yyj\"+p"));
    assert_eq!(e.text(), "hello\nworld\nhello");
    e.keys(&parse("gg\"*yiwG$\"*p"));
    assert_eq!(e.text(), "hello\nworld\nhellohello");
}
