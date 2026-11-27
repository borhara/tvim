mod common;
use std::path::Path;

use common::temp;
use tvim::Editor;
use tvim::clipboard::Memory;

fn open(path: &Path) -> Editor {
    let mut e = Editor::open(path).unwrap();
    e.set_clipboard(Box::new(Memory::default()));
    e.set_screen(80, 24);
    e.wait_for_git();
    e
}

// the background colour the text `word` is drawn on
fn background(e: &Editor, word: &str) -> String {
    let screen = tinsel::with_color_mode(tinsel::ColorMode::TrueColor, || {
        let mut frame = tinsel::app::Frame::new(80, 20);
        tvim::ui::draw(e, &mut frame);
        frame.snapshot().to_string()
    });
    let seg = screen
        .split("\u{1b}[")
        .find(|s| s.split_once('m').is_some_and(|(_, t)| t.trim_start().starts_with(word)))
        .unwrap_or_else(|| panic!("{word} in {screen:?}"));
    let code = seg.split('m').next().unwrap();
    code.split(';')
        .skip_while(|p| *p != "48")
        .take(5)
        .collect::<Vec<_>>()
        .join(";")
}

#[test]
fn diffthis_compares_two_windows_of_any_files() {
    let a = temp("diffthis_a.txt");
    let b = temp("diffthis_b.txt");
    std::fs::write(&a, "same\nleft\n").unwrap();
    std::fs::write(&b, "same\nright\n").unwrap();
    let mut e = open(&a);
    e.ex("diffthis");
    assert_eq!(
        e.message().unwrap().text,
        "comparing this window: :diffthis in another to compare it with"
    );
    e.ex(&format!("vsplit {}", b.display()));
    let (right, left) = (background(&e, "right"), background(&e, "left"));
    e.ex("diffthis");
    assert_ne!(background(&e, "right"), right);
    assert_ne!(background(&e, "left"), left);
    e.ex("diffoff");
    assert_eq!(background(&e, "right"), right);
}
