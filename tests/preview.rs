mod common;
use common::{ed, temp};
use tinsel::app::Frame;
use tvim::Editor;
use tvim::keys::parse;
use tvim::ui::draw;

fn screen(e: &Editor, width: usize, height: usize) -> String {
    tinsel::with_color_mode(tinsel::ColorMode::None, || {
        let mut frame = Frame::new(width, height);
        draw(e, &mut frame);
        frame.snapshot().plain()
    })
}

fn open(name: &str, text: &str) -> Editor {
    let p = temp(name);
    std::fs::write(&p, text).unwrap();
    let e = Editor::open(&p).unwrap();
    e.set_screen(100, 30);
    e
}

// the rows the preview takes up: everything right of the divider
fn right(shown: &str) -> String {
    shown
        .lines()
        .filter_map(|r| r.split_once('│').map(|(_, rest)| rest.to_string()))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn space_m_opens_a_rendered_preview_beside() {
    let mut e = open("pv-open.md", "# Hello\n\nSome **bold** text.\n");
    e.keys(&parse("<Space>m"));
    assert_eq!(e.window_count(), 2);
    assert_eq!(e.window_index(), 0, "the text keeps the focus");
    let shown = screen(&e, 100, 20);
    assert!(shown.contains("✶ pv-open.md"), "{shown}");
    assert!(shown.contains("preview · pv-open.md"), "{shown}");
    let side = right(&shown);
    assert!(side.contains("Some bold text."), "rendered, not raw: {shown}");
    assert!(!side.contains("**") && !side.contains("# Hello"), "{shown}");
    assert!(side.contains("─────"), "the heading is underlined: {shown}");
}

#[test]
fn toggling_closes_it_from_either_window() {
    let mut e = open("pv-toggle.md", "# Hi\n");
    e.ex("preview");
    assert_eq!(e.window_count(), 2);
    e.ex("preview");
    assert_eq!(e.window_count(), 1);
    e.keys(&parse("<Space>m<C-w>l"));
    assert!(e.preview_focused());
    e.keys(&parse("<Space>m"));
    assert_eq!(e.window_count(), 1);
    assert!(!e.preview_focused());
}

#[test]
fn not_markdown_says_so() {
    let mut e = ed("hello");
    e.keys(&parse("<Space>m"));
    assert_eq!(e.window_count(), 1);
    assert!(e.message().unwrap().text.contains("markdown"));
    let mut e = open("pv-plain.txt", "# Hi\n");
    e.ex("preview");
    assert_eq!(e.window_count(), 1);
}

#[test]
fn a_narrow_screen_puts_it_below() {
    let mut e = open("pv-narrow.md", "# Hi\n\nbody\n");
    e.set_screen(60, 30);
    e.ex("preview");
    let shown = screen(&e, 60, 30);
    let rows: Vec<&str> = shown.lines().collect();
    let text = rows.iter().position(|r| r.contains("✶ pv-narrow.md")).expect(&shown);
    let preview = rows
        .iter()
        .position(|r| r.contains("preview · pv-narrow.md"))
        .expect(&shown);
    assert!(preview > text, "{shown}");
    assert!(!shown.contains('│'), "{shown}");
}
