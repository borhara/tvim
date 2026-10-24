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
