mod common;
use common::{after, text_after as t};

#[test]
fn delete_words() {
    assert_eq!(after("one two three", "dw"), ("two three".into(), (0, 0)));
    assert_eq!(t("one two three", "d2w"), "three");
    assert_eq!(t("one two three", "2dw"), "three");
    assert_eq!(
        t("one two\nthree", "wdw"),
        "one \nthree",
        "the last word stops at the line's end"
    );
    assert_eq!(t("one\n  two", "dw"), "\n  two");
    assert_eq!(t("one two", "wdb"), "two");
    assert_eq!(t("one two", "de"), " two");
    assert_eq!(t("a.b c", "dW"), "c");
}

#[test]
fn change_words_keep_the_space_after() {
    assert_eq!(after("one two", "cwxx<Esc>"), ("xx two".into(), (0, 1)));
    assert_eq!(t("one two three", "c2wx<Esc>"), "x three");
    assert_eq!(
        t("a b", "cwz<Esc>"),
        "z b",
        "on the last letter of a word, just that letter"
    );
    assert_eq!(t("one  two", "llcwx<Esc>"), "onx  two");
    assert_eq!(t("one  two", "3lcwx<Esc>"), "onextwo", "on blanks, like dw");
}

#[test]
fn text_objects() {
    assert_eq!(t("foo bar baz", "wdiw"), "foo  baz");
    assert_eq!(t("foo bar baz", "wdaw"), "foo baz");
    assert_eq!(t(r#"say("hi there")"#, "fhci\"bye<Esc>"), r#"say("bye")"#);
    assert_eq!(
        t(r#"x = "a" + 1"#, "da\""),
        "x = + 1",
        "before any quotes: the next quoted text"
    );
    assert_eq!(t(r#"x = "a" + 1"#, "fada\""), "x = + 1");
    assert_eq!(t("f(a, (b), c)", "fbdi("), "f(a, (), c)");
    assert_eq!(t("f(a, (b), c)", "fbd2i("), "f()");
    assert_eq!(t("f(a, (b), c)", "fbda)"), "f(a, , c)");
    assert_eq!(t("x[1, 2]", "f1ci]9<Esc>"), "x[9]");
    assert_eq!(t("fn x() {\n    a;\n    b;\n}", "jdi{"), "fn x() {\n}");
    assert_eq!(t("fn x() {\n    a;\n}", "jci{z<Esc>"), "fn x() {\n    z\n}");
    assert_eq!(t("a\nb\n\nc", "dap"), "c");
    assert_eq!(t("a\nb\n\nc", "dip"), "\nc");
    assert_eq!(t("<a href>", "ldi<"), "<>");
}
