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
fn yanks_and_deletes() {
    let mut e = ed("keep\ngone");
    e.keys(&parse("yyjdd"));
    assert_eq!(e.register('0').unwrap().text, "keep");
    assert_eq!(e.register('1').unwrap().text, "gone");
    assert_eq!(e.register('"').unwrap().text, "gone");
    e.keys(&parse("\"0p"));
    assert_eq!(e.text(), "keep\nkeep");
    let mut e = ed("abc");
    e.keys(&parse("x"));
    assert_eq!(e.register('-').unwrap().text, "a");
}

#[test]
fn empty_registers_say_so() {
    let mut e = ed("a");
    e.keys(&parse("\"zp"));
    assert_eq!(e.message().unwrap().text, "E353: Nothing in register z");
    assert_eq!(e.text(), "a");
}

#[test]
fn the_clipboard() {
    let mut e = ed("hello\nworld");
    e.keys(&parse("\"+yyj\"+p"));
    assert_eq!(e.text(), "hello\nworld\nhello");
    e.keys(&parse("gg\"*yiwG$\"*p"));
    assert_eq!(e.text(), "hello\nworld\nhellohello");
}

#[test]
fn the_search_register() {
    assert_eq!(t("ab ab", "/b<CR>\"/P"), "abb ab");
}

#[test]
fn listing() {
    let mut e = ed("a");
    e.ex("reg");
    assert_eq!(e.message().unwrap().text, "No registers set");
    e.keys(&parse("\"kyy"));
    e.ex("reg");
    assert_eq!(e.message().unwrap().text, "\"\" a  \"0 a  \"k a");
}
