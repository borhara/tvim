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

#[test]
fn dot_repeats_the_last_change() {
    assert_eq!(t("a b c d", "dw."), "c d");
    assert_eq!(t("a b c d e f", "d2w."), "e f");
    assert_eq!(t("a b c d e f", "dw3."), "e f", "a count replaces the old one");
    assert_eq!(t("one two", "ciwX<Esc>w."), "X X");
    assert_eq!(t("a\nb", "A;<Esc>j."), "a;\nb;");
    assert_eq!(t("a\nb\nc", "dd."), "c");
    assert_eq!(t("abc", "x.."), "");
    assert_eq!(t("a\nb", ">>j."), "    a\n    b");
    assert_eq!(t("x", "oy<Esc>."), "x\ny\ny");
    assert_eq!(t("abc", "rx$."), "xbx");
    assert_eq!(t("a b", "yw$."), "a b", "yanking isn't a change");
    assert_eq!(t("1", "<C-a>.."), "4");
    assert_eq!(t("x", "3ia<Esc>."), "aaaaaax");
    assert_eq!(t("a b c", "dwu."), "b c");
}

#[test]
fn dot_keeps_the_register() {
    let mut e = ed("a\nb\nc");
    e.keys(&parse("\"Add."));
    assert_eq!(e.register('a').unwrap().text, "a\nb");
}
