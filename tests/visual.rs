mod common;
use common::{after, ed, text_after as t};
use tvim::{Mode, keys::parse};

#[test]
fn characters() {
    assert_eq!(after("hello world", "vlld"), ("lo world".into(), (0, 0)));
    assert_eq!(t("hello world", "wvex"), "hello ");
    assert_eq!(t("one\ntwo\nthree", "lvjd"), "oo\nthree");
    assert_eq!(t("hello", "$vhd"), "hel");
    assert_eq!(t("a\n\nb", "jvd"), "a\nb", "an empty line takes its line break");
    assert_eq!(t("hello world", "vecbye<Esc>"), "bye world");
}

#[test]
fn lines() {
    assert_eq!(t("a\nb\nc", "Vjd"), "c");
    assert_eq!(t("a\nb\nc", "jVd"), "a\nc");
    assert_eq!(t("a\nb\nc", "VjyGp"), "a\nb\nc\na\nb");
    assert_eq!(t("a\nb\nc", "Vj>"), "    a\n    b\nc");
    assert_eq!(t("a\nb", "V3>"), "            a\nb");
    assert_eq!(t("a\nb\nc", "VjcX<Esc>"), "X\nc");
    assert_eq!(t("ab", "vX"), "", "X deletes the lines");
}

#[test]
fn switching_and_leaving() {
    let mut e = ed("abc");
    e.keys(&parse("v"));
    assert_eq!(e.mode(), Mode::Visual);
    e.keys(&parse("V"));
    assert_eq!(e.mode(), Mode::VisualLine);
    e.keys(&parse("V"));
    assert_eq!(e.mode(), Mode::Normal);
    e.keys(&parse("vl<Esc>"));
    assert_eq!(e.mode(), Mode::Normal);
    assert_eq!(e.cursor().col, 1);
    e.keys(&parse("gvd"));
    assert_eq!(e.text(), "c", "gv selects it again");
}

#[test]
fn the_other_end() {
    assert_eq!(after("abcdef", "lvlloh").1, (0, 0));
    assert_eq!(t("abcdef", "lllvlohd"), "abf");
}

#[test]
fn text_objects_grow_the_selection() {
    assert_eq!(t("say (hi there) now", "fhvi(d"), "say () now");
    assert_eq!(t("say (hi there) now", "fhva(d"), "say  now");
    assert_eq!(t("one two three", "wviwd"), "one  three");
    assert_eq!(t("a\nb\n\nc", "vipd"), "\nc");
}

#[test]
fn case_and_replacing() {
    assert_eq!(t("hello world", "veU"), "HELLO world");
    assert_eq!(t("HELLO world", "Vu"), "hello world");
    assert_eq!(t("Hello", "v$~"), "hELLO");
    assert_eq!(t("hello\nworld", "vjrx"), "xxxxx\nxorld");
}

#[test]
fn putting_over_a_selection() {
    assert_eq!(after("one two", "yiwwviwp"), ("one one".into(), (0, 6)));
    let mut e = ed("one two");
    e.keys(&parse("yiwwviwp"));
    assert_eq!(e.register('"').unwrap().text, "two", "p leaves what it replaced");
    let mut e = ed("one two");
    e.keys(&parse("yiwwviwP"));
    assert_eq!(e.register('"').unwrap().text, "one", "P leaves the registers be");
    assert_eq!(t("a\nb\nc", "yyjVp"), "a\na\nc");
    assert_eq!(t("a\nb", "yyjVp"), "a\na");
    assert_eq!(t("x\nab", "yyjlvp"), "x\na\nx\n");
}

#[test]
fn colon_from_a_selection_works_on_its_lines() {
    let mut e = ed("a\na\na");
    e.keys(&parse("Vj:"));
    assert_eq!(e.cmdline().text, "'<,'>");
    e.keys(&parse("s/a/b/<CR>"));
    assert_eq!(e.text(), "b\nb\na");
}

#[test]
fn insert_at_the_ends_of_a_selection() {
    assert_eq!(t("  ab\ncd", "VjIx<Esc>"), "  xab\ncd");
    assert_eq!(t("ab\ncd", "VjAx<Esc>"), "ab\ncdx");
}

#[test]
fn yanking_goes_to_the_start() {
    assert_eq!(after("hello world", "$vby").1, (0, 6));
    let mut e = ed("hello");
    e.keys(&parse("vly"));
    assert_eq!(e.register('0').unwrap().text, "he");
}

#[test]
fn a_selection_ending_on_an_empty_line_stops_there() {
    assert_eq!(t("a\n\nb\nc", "vj:d<CR>"), "b\nc");
    assert_eq!(t("a\n\nb\nc", "jvD"), "a\nb\nc");
}

#[test]
fn putting_over_a_selection_repeats_whole_lines() {
    assert_eq!(t("a\nb\nz", "yjGV2p"), "a\nb\na\nb\na\nb");
}
