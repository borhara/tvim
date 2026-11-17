mod common;
use common::{after, ed, text_after as t};
use tvim::{Mode, keys::parse};

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

#[test]
fn tabs_and_backspace() {
    assert_eq!(t("", "i<Tab>a<Esc>"), "    a");
    assert_eq!(t("ab", "a<Tab><Esc>"), "a   b");
    assert_eq!(
        t("", "i<Tab><Tab><BS>a<Esc>"),
        "    a",
        "backspace takes a whole indent step"
    );
    assert_eq!(t("abc", "A<BS><BS><Esc>"), "a");
    assert_eq!(
        t("a\nb", "jI<BS><Esc>"),
        "ab",
        "at a line's start it joins the line above"
    );
    assert_eq!(t("ab", "i<BS><Esc>"), "ab");
    assert_eq!(t("ab", "i<Del><Esc>"), "b");
    assert_eq!(t("a\nb", "A<Del><Esc>"), "ab");
    let mut e = ed("");
    e.ex("set noet");
    e.keys(&parse("i<Tab>a<Esc>"));
    assert_eq!(e.text(), "\ta");
}

#[test]
fn deleting_words_and_lines_while_typing() {
    assert_eq!(t("foo bar", "A<C-w><Esc>"), "foo ");
    assert_eq!(t("foo bar  ", "A<C-w><Esc>"), "foo ");
    assert_eq!(t("foo.bar", "A<C-w><C-w><Esc>"), "foo");
    assert_eq!(t("  foo bar", "A<C-u><Esc>"), "  ");
    assert_eq!(t("a\nb", "jI<C-w><Esc>"), "ab");
}

#[test]
fn moving_while_typing() {
    assert_eq!(t("ac", "a<Right>b<Esc>"), "acb");
    assert_eq!(t("abc", "A<Left><Left>x<Esc>"), "axbc");
    assert_eq!(t("ab\ncd", "A<Down>x<Esc>"), "ab\ncdx");
    assert_eq!(t("abc", "A<Home>x<Esc>"), "xabc");
}

#[test]
fn indenting_while_typing() {
    assert_eq!(t("a", "A<C-t><Esc>"), "    a");
    assert_eq!(t("        a", "A<C-d><Esc>"), "    a");
    assert_eq!(t("", "i<C-t>x<Esc>"), "    x");
}

#[test]
fn pasting() {
    let mut e = ed("ab");
    e.keys(&parse("a"));
    e.paste("x\n  y");
    e.key(tinsel::app::Key::Esc);
    assert_eq!(e.text(), "ax\n  yb", "pasted text keeps its own indent");
    let mut e = ed("ab");
    e.paste("z");
    assert_eq!(e.text(), "azb", "a paste in normal mode goes after the cursor");
    assert_eq!(e.mode(), Mode::Normal);
}

#[test]
fn registers_while_typing() {
    assert_eq!(t("word", "yiwA <C-r>\"<Esc>"), "word word");
    assert_eq!(t("a", "yyA<C-r>\"<Esc>"), "aa\n");
    assert_eq!(t("x", "\"qyiwo<C-r>q!<Esc>"), "x\nx!");
}

#[test]
fn ctrl_c_leaves_insert_mode() {
    let (text, at) = after("", "ihi<C-c>");
    assert_eq!((text.as_str(), at), ("hi", (0, 1)));
    let mut e = ed("");
    e.keys(&parse("<C-c>"));
    assert!(e.message().unwrap().text.contains(":qa!"));
    assert!(!e.should_quit());
}
