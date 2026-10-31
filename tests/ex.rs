mod common;
use common::{cursor_after as at, ed, text_after as t};

#[test]
fn going_to_lines() {
    let src = "a\n  b\nc\nd";
    assert_eq!(at(src, ":2<CR>"), (1, 2));
    assert_eq!(at(src, ":$<CR>"), (3, 0));
    assert_eq!(at(src, ":99<CR>"), (3, 0));
    assert_eq!(at(src, ":$-1<CR>"), (2, 0));
    assert_eq!(at(src, "j:.+2<CR>"), (3, 0));
    assert_eq!(at(src, "jjma:1<CR>:'a<CR>"), (2, 0));
}

#[test]
fn substitute() {
    assert_eq!(t("a a\na a", ":s/a/b/<CR>"), "b a\na a");
    assert_eq!(t("a a\na a", ":s/a/b/g<CR>"), "b b\na a");
    assert_eq!(t("a a\na a", ":%s/a/b/g<CR>"), "b b\nb b");
    assert_eq!(t("a\na\na", ":2,3s/a/b/<CR>"), "a\nb\nb");
    assert_eq!(t("a/b", r":s#/#\/x#<CR>"), "a/xb");
    assert_eq!(t("a/b", r":s/\//-/<CR>"), "a-b");
    assert_eq!(t("cat", ":s/cat/[&]/<CR>"), "[cat]");
    assert_eq!(t("a,b", r":s/,/\n/<CR>"), "a\nb");
    assert_eq!(t("Aa", ":s/a/x/g<CR>"), "xx", "smart case: lowercase matches both");
    assert_eq!(t("Aa", ":s/a/x/gI<CR>"), "Ax");
    assert_eq!(
        t("foo bar", "/bar<CR>:s//baz/<CR>"),
        "foo baz",
        "an empty pattern is the last search"
    );
    assert_eq!(t("a a\na a", ":s/a/b/<CR>j&"), "b a\nb a", "& does it again");
}

#[test]
fn substitute_reports() {
    let mut e = ed("a a\na");
    e.ex("%s/a/b/g");
    assert_eq!(e.message().unwrap().text, "3 substitutions on 2 lines");
    e.ex("s/zzz/y/");
    assert_eq!(e.message().unwrap().text, "E486: Pattern not found: zzz");
    e.ex("s/zzz/y/e");
    assert!(e.message().is_none_or(|m| !m.error));
    e.ex("sxaxbx");
    assert!(e.message().unwrap().text.starts_with("E146"));
}

#[test]
fn substitute_is_one_change() {
    assert_eq!(t("a\na\na", ":%s/a/b/<CR>u"), "a\na\na");
}

#[test]
fn substitute_with_regular_expressions() {
    assert_eq!(t("hello world", r":s/(\w+) (\w+)/\2 \1/<CR>"), "world hello");
    assert_eq!(t("a\nb", ":%s/^/# /<CR>"), "# a\n# b");
    assert_eq!(t("a\nb", ":%s/$/;/<CR>"), "a;\nb;");
    assert_eq!(t("aa b aaa", ":s/a+/X/g<CR>"), "X b X");
    assert_eq!(t("a1b2", r":s/\d/<&>/g<CR>"), "a<1>b<2>");
    assert_eq!(t("a1b2", r":s/\d/<\0>/<CR>"), "a<1>b2");
    assert_eq!(t("abc", ":s/x*/-/g<CR>"), "-a-b-c-");
    assert_eq!(t("k=v", r":s/=/\n/<CR>"), "k\nv");
    assert_eq!(t("Aa", r":s/a\C/x/gi<CR>"), "Ax", "\\C beats the i flag");
    assert_eq!(t("one.two", r":s/\./ /<CR>"), "one two");
    assert_eq!(t("(x)", r":s/\((\w)\)/[\1]/<CR>"), "[x]");
}

#[test]
fn substitute_with_a_bad_pattern() {
    let mut e = ed("a[b");
    e.ex("s/[b/x/");
    assert_eq!(e.text(), "a[b");
    assert!(e.message().unwrap().text.starts_with("E383"));
}
