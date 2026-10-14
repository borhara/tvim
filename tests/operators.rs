use common::text_after as t;

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
