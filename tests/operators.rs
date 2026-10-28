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
fn whole_lines() {
    let src = "a\nb\nc\nd";
    assert_eq!(after(src, "dd"), ("b\nc\nd".into(), (0, 0)));
    assert_eq!(t(src, "2dd"), "c\nd");
    assert_eq!(t(src, "jdj"), "a\nd");
    assert_eq!(t(src, "jjdk"), "a\nd");
    assert_eq!(t(src, "jdG"), "a");
    assert_eq!(t(src, "jjdgg"), "d");
    assert_eq!(t(src, "Gdd"), "a\nb\nc");
    assert_eq!(t(src, "9dd"), src, "more lines than there are does nothing");
    assert_eq!(t("only", "dd"), "");
    assert_eq!(t("  x\ny", "ccz<Esc>"), "  z\ny", "cc keeps the indent");
    assert_eq!(t("a\nb", "Sz<Esc>"), "z\nb");
}

#[test]
fn to_line_ends() {
    assert_eq!(after("hello world", "wD"), ("hello ".into(), (0, 5)));
    assert_eq!(t("hello world", "wd$"), "hello ");
    assert_eq!(t("hello world", "wCthere<Esc>"), "hello there");
    assert_eq!(t("hello world", "wd0"), "world");
    assert_eq!(t("   hello", "$d^"), "   o");
}

#[test]
fn single_characters() {
    assert_eq!(after("abc", "x"), ("bc".into(), (0, 0)));
    assert_eq!(after("abc", "$x"), ("ab".into(), (0, 1)));
    assert_eq!(t("abcdef", "3x"), "def");
    assert_eq!(t("abcdef", "9x"), "");
    assert_eq!(t("abc", "$X"), "ac");
    assert_eq!(t("abc", "X"), "abc");
    assert_eq!(t("abc", "sz<Esc>"), "zbc");
    assert_eq!(t("", "x"), "");
}

#[test]
fn exclusive_motions_ending_at_a_line_start() {
    assert_eq!(t("a\nb\n\nc", "d}"), "\nc", "from the line's start, whole lines");
    assert_eq!(t("xa\nb\n\nc", "ld}"), "x\n\nc", "from inside, up to the line's end");
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

#[test]
fn indenting() {
    assert_eq!(t("a\nb", ">>"), "    a\nb");
    assert_eq!(t("a\nb", ">j"), "    a\n    b");
    assert_eq!(t("a\nb", "2>>"), "    a\n    b");
    assert_eq!(t("        a", "<<"), "    a");
    assert_eq!(t("  a", "<<"), "a");
    assert_eq!(t("a\n\nb", ">2j"), "    a\n\n    b", "empty lines stay empty");
    assert_eq!(after("  a", ">>").1, (0, 6));
}

#[test]
fn case() {
    assert_eq!(t("hello world", "gUiw"), "HELLO world");
    assert_eq!(t("Hello World", "guu"), "hello world");
    assert_eq!(t("Hello World", "g~~"), "hELLO wORLD");
    assert_eq!(t("Hello World", "g~w"), "hELLO World");
    assert_eq!(after("abc", "~~"), ("ABc".into(), (0, 2)));
    assert_eq!(t("abc", "5~"), "ABC");
    assert_eq!(
        t("Straße", "gUU"),
        "STRAßE",
        "a letter with no one-letter capital stays"
    );
}

#[test]
fn yank_and_put() {
    assert_eq!(after("a\nb", "yyp"), ("a\na\nb".into(), (1, 0)));
    assert_eq!(after("a\nb", "jyyP"), ("a\nb\nb".into(), (1, 0)));
    assert_eq!(t("a\nb", "yy3p"), "a\na\na\na\nb");
    assert_eq!(after("one two", "ywP"), ("one one two".into(), (0, 3)));
    assert_eq!(after("one two", "yw$p"), ("one twoone ".into(), (0, 10)));
    assert_eq!(after("ab", "ylp"), ("aab".into(), (0, 1)));
    assert_eq!(t("ab", "yl3p"), "aaaab");
    assert_eq!(after("one\ntwo", "vjy$p"), ("oneone\nt\ntwo".into(), (0, 3)));
    assert_eq!(t("a\nb\nc", "ddp"), "b\na\nc", "swap two lines");
    assert_eq!(t("ab", "xp"), "ba", "swap two characters");
    assert_eq!(t("", "p"), "");
    assert_eq!(after("hello", "yiwgP").1, (0, 5));
    assert_eq!(after("a\nb", "yygp").1, (2, 0));
    assert_eq!(
        after("hello world", "wyy").1,
        (0, 6),
        "yanking lines leaves the cursor be"
    );
    assert_eq!(after("hello world", "wyb").1, (0, 0), "yanking back goes to the start");
}

#[test]
fn y_to_end_of_line_like_neovim() {
    assert_eq!(t("hello world", "wY$p"), "hello worldworld");
}

#[test]
fn joining() {
    assert_eq!(after("a\n   b\nc", "J"), ("a b\nc".into(), (0, 1)));
    assert_eq!(t("a\nb\nc", "3J"), "a b c");
    assert_eq!(t("a\nb\nc", "VjjJ"), "a b c");
    assert_eq!(t("a \nb", "J"), "a b");
    assert_eq!(t("a\n)", "J"), "a)");
    assert_eq!(t("a\n", "J"), "a");
    assert_eq!(t("a\n  b", "gJ"), "a  b");
    assert_eq!(t("a", "J"), "a");
}

#[test]
fn changing_every_line_leaves_one() {
    assert_eq!(t("abc", "ccx<Esc>"), "x");
    assert_eq!(t("a\nb", "cGx<Esc>"), "x");
}
