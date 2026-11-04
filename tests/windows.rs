mod common;
use common::ed;
use tvim::keys::parse;

#[test]
fn splitting_by_key_and_command() {
    let mut e = ed("hello");
    e.keys(&parse("<C-w>s"));
    assert_eq!(e.window_count(), 2);
    e.keys(&parse("<C-w>v"));
    assert_eq!(e.window_count(), 3);
    e.ex("sp");
    e.ex("vs");
    assert_eq!(e.window_count(), 5);
    assert_eq!(e.window_index(), 0, "the new window comes first and has the focus");
}

#[test]
fn quit_closes_the_window_until_the_last() {
    let mut e = ed("hello");
    e.ex("vsplit");
    e.keys(&parse("x"));
    e.ex("q");
    assert!(!e.should_quit(), "a window closes even with unsaved changes");
    assert_eq!(e.window_count(), 1);
    e.ex("q");
    assert!(!e.should_quit(), "the last one still guards them");
    assert!(e.message().unwrap().text.starts_with("E37"));
    e.ex("q!");
    assert!(e.should_quit());
}
