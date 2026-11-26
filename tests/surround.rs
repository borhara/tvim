mod common;
use common::{cursor_after as at, text_after as t};

#[test]
fn ys_puts_a_pair_around() {
    assert_eq!(t("foo bar", "ysiw)"), "(foo) bar");
    assert_eq!(t("foo bar", "ysiw("), "( foo ) bar", "an opening bracket leaves spaces");
    assert_eq!(t("foo bar", "ysiw]"), "[foo] bar");
    assert_eq!(t("foo bar", "ysiwB"), "{foo} bar");
    assert_eq!(t("foo bar", "ysiw\""), "\"foo\" bar");
    assert_eq!(t("foo bar", "ysiw*"), "*foo* bar", "any mark goes on both sides");
    assert_eq!(t("foo bar baz", "ysw'"), "'foo' bar baz", "without the blank after");
    assert_eq!(t("foo bar baz", "ys2w)"), "(foo bar) baz");
    assert_eq!(t("foo bar baz", "2ysw)"), "(foo bar) baz");
    assert_eq!(t("foo bar", "ys$>"), "<foo bar>");
    assert_eq!(t("  foo bar", "yss)"), "  (foo bar)", "yss from the text on");
    assert_eq!(t("a\nb", "ysj]"), "[a\nb]");
    assert_eq!(t("f(x)", "f(ysa(\""), "f\"(x)\"");
    assert_eq!(t("foo bar", "ysiw)u"), "foo bar", "one change");
    assert_eq!(t("foo bar", "ysiw)W."), "(foo) (bar)", "and . does it again");
    assert_eq!(
        t("a0b0 c0d0", "ys2f0)W."),
        "(a0b0) (c0d0)",
        "with a count and a 0 in the motion"
    );
}

#[test]
fn ds_takes_a_pair_away() {
    assert_eq!(t("(foo)", "lds)"), "foo");
    assert_eq!(t("(foo)", "ldsb"), "foo");
    assert_eq!(t("( foo )", "fods)"), " foo ");
    assert_eq!(t("( foo )", "fods("), "foo", "an opening bracket takes the blanks too");
    assert_eq!(t("x [a] y", "fads["), "x a y");
    assert_eq!(t("say \"hi there\" ok", "fhds\""), "say hi there ok");
    assert_eq!(t("f('a')", "fads'"), "f(a)");
    assert_eq!(t("fn() {\n    x\n}", "jds{"), "fn() \n    x\n");
    assert_eq!(t("((a))", "fads("), "(a)", "the nearest pair");
    assert_eq!(t("foo", "ds("), "foo", "no pair, no change");
}

#[test]
fn cs_changes_a_pair() {
    assert_eq!(t("\"foo\"", "lcs\"'"), "'foo'");
    assert_eq!(t("(foo)", "lcs)]"), "[foo]");
    assert_eq!(t("(foo)", "lcs)["), "[ foo ]");
    assert_eq!(t("( foo )", "focs()"), "(foo)");
    assert_eq!(
        t("[a] [b]", "lcs])w."),
        "(a) [b]",
        "with . on a line without the pair left"
    );
    assert_eq!(t("'a' 'b'", "lcs'\"fb."), "\"a\" \"b\"");
    assert_eq!(t("(foo)", "lcs)]u"), "(foo)", "one change");
}

#[test]
fn lazyvims_names_do_the_same() {
    assert_eq!(t("foo bar", "gsaiw)"), "(foo) bar");
    assert_eq!(t("foo bar baz", "gsa2w\""), "\"foo bar\" baz");
    assert_eq!(t("(foo)", "lgsd)"), "foo");
    assert_eq!(t("\"foo\"", "lgsr\"'"), "'foo'");
    assert_eq!(t("foo bar", "vegsa]"), "[foo] bar");
    assert_eq!(t("a b", "gsaiw)W."), "(a) (b)", "and . does it again");
}

#[test]
fn visual_s_puts_a_pair_around_the_selection() {
    assert_eq!(t("foo bar", "veS)"), "(foo) bar");
    assert_eq!(t("foo bar", "veS\""), "\"foo\" bar");
    assert_eq!(t("  foo bar", "VS]"), "  [foo bar]");
    assert_eq!(t("ab x\ncd y", "<C-v>jlS)"), "(ab) x\n(cd) y", "each row of a block");
    assert_eq!(
        t("ab x\nc\nef y", "<C-v>jjlS'"),
        "'ab' x\n'c'\n'ef' y",
        "a short row too"
    );
    assert_eq!(t("ab\ncd", "<C-v>jlS)u"), "ab\ncd", "as one change");
    assert_eq!(at("foo bar", "wysiw)"), (0, 4));
}

#[test]
fn surround_odd_cases() {
    assert_eq!(t("foo", "ysiw{"), "{ foo }");
    assert_eq!(t("", "yss)"), "", "an empty line has nothing to surround");
    assert_eq!(
        t("a b\n\nc", "ys})"),
        "(a b)\n\nc",
        "up to the paragraph's end, not the line after"
    );
    assert_eq!(t("ab\ncd", "lvj0S)"), "a(b\nc)d");
    assert_eq!(
        t("a b\nc", "wv$S)"),
        "a (b)\nc",
        "a selection to the end leaves the line break out"
    );
    assert_eq!(t("foo bar", "veSx"), "foo bar", "S with a letter does nothing");
    assert_eq!(t("foo", "vgsax"), "foo");
    assert_eq!(t("foo bar", "ysizb"), "foo bar", "nor with an object that isn't one");
}
