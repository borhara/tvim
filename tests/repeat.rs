mod common;
use common::{after, ed, text_after as t};
use tvim::keys::parse;

#[test]
fn undo_and_redo() {
    assert_eq!(t("a b c", "dwu"), "a b c");
    assert_eq!(t("a b c", "dwdwuu"), "a b c");
    assert_eq!(t("a b c", "dwdwuu<C-r>"), "b c");
    assert_eq!(t("a", "ix<Esc>ay<Esc>u"), "xa", "each insert is one change");
    assert_eq!(t("a", "ixyz<CR>w<Esc>u"), "a", "however much was typed");
    assert_eq!(t("a b c", "dw3x2u"), "a b c");
    assert_eq!(after("hello world", "wxu").1, (0, 6), "back where the change was");
    assert_eq!(t("a\nb\nc", "jddGu"), "a\nb\nc");
}

#[test]
fn undo_says_when_theres_nothing_left() {
    let mut e = ed("a");
    e.keys(&parse("u"));
    assert_eq!(e.message().unwrap().text, "Already at oldest change");
    e.keys(&parse("<C-r>"));
    assert_eq!(e.message().unwrap().text, "Already at newest change");
}

#[test]
fn undoing_back_to_the_saved_text_isnt_a_change() {
    let mut e = ed("a");
    assert!(!e.is_modified());
    e.keys(&parse("x"));
    assert!(e.is_modified());
    e.keys(&parse("u"));
    assert!(!e.is_modified());
    e.keys(&parse("<C-r>"));
    assert!(e.is_modified());
}

#[test]
fn moving_isnt_a_change() {
    let mut e = ed("a b");
    e.keys(&parse("wbiu<Esc>u"));
    assert_eq!(e.text(), "a b");
    e.keys(&parse("u"));
    assert_eq!(e.message().unwrap().text, "Already at oldest change");
}
