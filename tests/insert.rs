mod common;
use common::{after, text_after as t};

#[test]
fn ways_in() {
    assert_eq!(after("bc", "ia<Esc>"), ("abc".into(), (0, 0)));
    assert_eq!(after("ac", "ab<Esc>"), ("abc".into(), (0, 1)));
    assert_eq!(t("  b", "$Ia<Esc>"), "  ab");
    assert_eq!(t("a", "Ab<Esc>"), "ab");
    assert_eq!(after("a\nc", "ob<Esc>"), ("a\nb\nc".into(), (1, 0)));
    assert_eq!(t("b", "Oa<Esc>"), "a\nb");
    assert_eq!(t("", "ahi<Esc>"), "hi");
}

#[test]
fn counts_repeat_what_was_typed() {
    assert_eq!(t("", "3ia<Esc>"), "aaa");
    assert_eq!(t("x", "2Aab<Esc>"), "xabab");
    assert_eq!(t("x", "3oy<Esc>"), "x\ny\ny\ny");
    assert_eq!(t("x", "2Oy<Esc>"), "y\ny\nx");
}
