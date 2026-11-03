mod common;
use tvim::keys::parse;

#[test]
fn line_numbers_can_start_with_zeros() {
    let mut e = ed("a\nb");
    assert_eq!(&rows(&screen(e, 30, 8))[1..3], ["    1 › a", "    2   b"]);
    e = ed("a\nb");
    e.ex("set zp");
    assert_eq!(&rows(&screen(e, 30, 8))[1..3], ["   01 › a", "   02   b"]);
}

#[test]
fn relative_numbers() {
    let mut e = ed("a\nb\nc");
    e.ex("set rnu");
    e.keys(&parse("j"));
    let shown = screen(e, 30, 8);
    assert_eq!(&rows(&shown)[1..4], ["    1   a", "  2   › b", "    1   c"]);
}

#[test]
fn searching_shows_how_many_matches() {
    let mut e = ed("a b a b a");
    e.keys(&parse("/a"));
    let shown = screen(e, 40, 5);
    let last = shown.lines().last().unwrap();
    assert!(last.starts_with("  / a ") && last.ends_with("2 of 3   1:5"), "{last:?}");
    let mut e = ed("a b a b a");
    e.keys(&parse("/a<CR>n"));
    assert!(screen(e, 60, 5).contains("/ 3/3"));
    let mut e = ed("abc");
    e.keys(&parse("/z"));
    assert!(screen(e, 40, 5).contains("no match"));
}

#[test]
fn long_lines_scroll_sideways() {
    let mut e = ed(&format!("{}END", "x".repeat(50)));
    e.keys(&parse("$"));
    let shown = screen(e, 30, 8);
    let first = rows(&shown)[1];
    assert!(first.starts_with("    1 › x") && first.ends_with("xxEND"), "{first:?}");
    assert_eq!(first.chars().count(), 29, "the last column stays clear");
}
