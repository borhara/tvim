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

#[test]
fn line_breaks_keep_the_indent() {
    assert_eq!(after("    a", "Ab<CR>c<Esc>"), ("    ab\n    c".into(), (1, 4)));
    assert_eq!(t("    a", "ob<Esc>"), "    a\n    b");
    assert_eq!(t("if x {", "A<CR>y<Esc>"), "if x {\n    y");
    assert_eq!(t("f(", "A<CR>a<Esc>"), "f(\n    a");
    assert_eq!(
        t("x {}", "f}i<CR>y<Esc>"),
        "x {\n    y\n}",
        "between brackets, the closer goes below"
    );
    assert_eq!(t("ab cd", "fci<CR><Esc>"), "ab\ncd", "the blank at the break goes");
}

#[test]
fn opening_a_line_inside_a_block_indents_it() {
    assert_eq!(t("fn x() {\n}", "oa;<Esc>"), "fn x() {\n    a;\n}");
    assert_eq!(t("fn x() {\n}", "jOa;<Esc>"), "fn x() {\n    a;\n}");
    assert_eq!(t("  x", "oy<Esc>"), "  x\n  y");
}

#[test]
fn an_indent_nothing_follows_goes_away() {
    assert_eq!(t("    a", "o<Esc>"), "    a\n");
    assert_eq!(t("    a", "A<CR><CR>b<Esc>"), "    a\n\n    b");
}

#[test]
fn closing_brackets_line_up_with_their_opener() {
    assert_eq!(t("if x {", "A<CR>y<CR>}<Esc>"), "if x {\n    y\n}");
}
