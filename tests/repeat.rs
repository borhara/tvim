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

#[test]
fn macros() {
    assert_eq!(t("1\n2\n3", "qaA!<Esc>jq@a"), "1!\n2!\n3");
    assert_eq!(t("1\n2\n3\n4", "qaA!<Esc>jq2@a"), "1!\n2!\n3!\n4");
    assert_eq!(t("1\n2\n3", "qaA!<Esc>jq@a@@"), "1!\n2!\n3!");
    assert_eq!(t("a b c", "qqdwq@q"), "c");
    let mut e = ed("x");
    e.keys(&parse("qaiy<Esc>q"));
    assert_eq!(e.register('a').unwrap().text, "iy<Esc>");
    assert_eq!(e.recording(), None);
    e.keys(&parse("qb"));
    assert_eq!(e.recording(), Some('b'));
}

#[test]
fn a_macro_that_calls_itself_stops() {
    let mut e = ed("a");
    e.keys(&parse("qaq"));
    e.keys(&parse("qa@aq"));
    e.keys(&parse("@a"));
    assert!(!e.should_quit());
}

#[test]
fn colon_commands_repeat_with_at_colon() {
    assert_eq!(t("a\nb\nc", ":s/$/!<CR>j@:"), "a!\nb!\nc");
    assert_eq!(t("xa\nxb\nxc", ":s/x/-/<CR>j@:j@:"), "-a\n-b\n-c");
}

#[test]
fn a_register_written_after_recording_is_what_plays() {
    assert_eq!(t("x\nhello", "qajqk\"ayyj@a"), "x\nello");
}

#[test]
fn a_capital_register_adds_to_a_macro() {
    assert_eq!(t("abcdef", "qaxqqAxq@a"), "ef");
}

#[test]
fn at_at_repeats_at_colon() {
    let mut e = ed("a\na\na\na");
    e.keys(&parse("qaxq"));
    e.keys(&parse(":s/a/b/<CR>j@:j@@"));
    assert_eq!(e.text(), "\nb\nb\na");
}

#[test]
fn visual_changes_repeat_on_as_much_text() {
    assert_eq!(t("abcdef", "vld."), "ef", "two characters, then two more");
    assert_eq!(t("a\nb\nc\nd\ne", "Vjdj."), "c", "two lines, then two more");
    assert_eq!(t("a\nb\nc\nd", "Vj>jj."), "    a\n    b\n    c\n    d");
    assert_eq!(t("abcd efgh", "vlUw."), "ABcd EFgh");
    assert_eq!(t("one two", "vecX<Esc>w."), "X X", "a change repeats what was typed");
    assert_eq!(t("abc abc", "vlrxw."), "xxc xxc");
    assert_eq!(
        t("abcd\nefgh\nijkl\nmnop", "<C-v>jldjj."),
        "cd\ngh\nkl\nop",
        "a block of the same size"
    );
    assert_eq!(t("ab\ncd\nef", "<C-v>jI-<Esc>jj0."), "-ab\n-cd\n-ef");
    assert_eq!(
        t("abc\ndef\nghi", "vjd."),
        "hi",
        "down to the same column on the last line"
    );
}

#[test]
fn yanking_in_visual_mode_leaves_dot_alone() {
    assert_eq!(t("a b c", "dwvy."), "c");
}
