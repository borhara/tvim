mod common;
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
