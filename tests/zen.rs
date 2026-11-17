mod common;
use tinsel::app::Frame;
use tvim::Editor;
use tvim::keys::parse;
use tvim::ui::{Tvim, draw};

fn screen(e: &Editor, width: usize, height: usize) -> String {
    tinsel::with_color_mode(tinsel::ColorMode::None, || {
        let mut frame = Frame::new(width, height);
        draw(e, &mut frame);
        frame.snapshot().plain()
    })
}

fn rows(shown: &str) -> Vec<&str> {
    shown.lines().collect()
}

#[test]
fn only_the_text_in_a_centred_column() {
    let mut e = common::ed("hello\nworld");
    e.keys(&parse("<space>z"));
    assert!(e.zen());
    let shown = screen(&e, 121, 16);
    let lines = rows(&shown);
    let indent = " ".repeat((121 - 91) / 2);
    assert_eq!(lines[0], "");
    assert_eq!(lines[1], format!("{indent}hello"));
    assert_eq!(lines[2], format!("{indent}world"));
    assert!(
        lines[3..].iter().all(|l| l.is_empty()),
        "no status, tabs or numbers: {shown}"
    );
}

#[test]
fn leaving_puts_everything_back() {
    let mut e = common::ed("hello\nworld");
    let before = screen(&e, 100, 12);
    e.ex("zen");
    assert_ne!(screen(&e, 100, 12), before);
    e.ex("zen");
    assert!(!e.zen());
    assert_eq!(screen(&e, 100, 12), before);
}

#[test]
fn a_narrow_screen_uses_all_of_it() {
    let mut e = common::ed("hello");
    e.ex("zen");
    assert_eq!(rows(&screen(&e, 40, 8))[0], "hello");
}

#[test]
fn the_sidebar_hides_and_comes_back_with_its_focus() {
    let dir = common::temp("zen-side");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("notes.txt"), "hi\n").unwrap();
    let mut e = common::ed("hello");
    e.explore(&dir);
    assert!(e.explorer_focused());
    e.toggle_zen();
    assert!(!e.explorer_focused(), "keys go to the text");
    assert!(!screen(&e, 100, 12).contains("notes.txt"));
    e.keys(&parse("<space>z"));
    assert!(e.explorer_focused());
    assert!(screen(&e, 100, 12).contains("notes.txt"));

    e.keys(&parse("<Tab>"));
    e.toggle_zen();
    e.keys(&parse("<Tab>"));
    assert!(!e.zen(), "going to the sidebar leaves zen");
    assert!(e.explorer_focused());
}

#[test]
fn messages_stay_quiet() {
    let mut e = common::ed("hello");
    e.ex("zen");
    e.ex("nope");
    let shown = screen(&e, 100, 12);
    let last = rows(&shown)[11];
    assert_eq!(last.trim(), "E492: Not an editor command: nope");
    e.keys(&parse("/hel"));
    let shown = screen(&e, 100, 12);
    assert_eq!(rows(&shown)[11].trim(), "/ hel", "the search is typed at the bottom");
}

#[test]
fn the_column_slides_in() {
    let mut e = common::ed("hello");
    let column = |e: &Editor, t: f64| {
        let mut frame = Frame::new(121, 16).at(t);
        tinsel::with_color_mode(tinsel::ColorMode::None, || draw(e, &mut frame));
        let shown = frame.snapshot().plain();
        shown
            .lines()
            .find_map(|l| l.find("hello").map(|b| l[..b].chars().count()))
            .unwrap()
    };
    let start = column(&e, 1.0);
    e.ex("zen");
    let mid = column(&e, 1.05);
    assert!(mid > start && mid < 15, "{start} {mid}");
    assert_eq!(column(&e, 2.0), 15);
}

#[test]
fn it_is_in_the_palette_and_the_space_hints() {
    let mut e = common::ed("hello");
    e.keys(&parse(":ze"));
    let shown = tinsel::with_color_mode(tinsel::ColorMode::None, || {
        tinsel::app::snapshot(&Tvim::new(e), 80, 20).plain()
    });
    assert!(shown.contains("zen"), "{shown}");
    let mut e = common::ed("hello");
    e.keys(&parse("<space>"));
    assert!(screen(&e, 80, 20).contains("zen"));
}
