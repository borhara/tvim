mod common;
use common::ed;
use tvim::keys::parse;

fn screen(e: &tvim::Editor) -> String {
    tinsel::with_color_mode(tinsel::ColorMode::None, || {
        let mut frame = tinsel::app::Frame::new(50, 12);
        tvim::ui::draw(e, &mut frame);
        frame.snapshot().plain()
    })
}

#[test]
fn a_substitute_being_typed_shows_its_change_without_making_it() {
    let mut e = ed("one foo\ntwo foo foo\nthree");
    e.keys(&parse(":%s/foo/bar/g"));
    let shown = screen(&e);
    assert!(shown.contains("one bar"), "{shown}");
    assert!(shown.contains("two bar bar"), "{shown}");
    assert_eq!(e.text(), "one foo\ntwo foo foo\nthree", "nothing changed yet");
    e.keys(&parse("<Esc>"));
    assert!(screen(&e).contains("two foo foo"), "gone with esc");
    e.keys(&parse(":%s/foo/bar/<CR>"));
    assert_eq!(e.text(), "one bar\ntwo bar foo\nthree", "and made when run");
}

#[test]
fn only_the_lines_given_are_previewed() {
    let mut e = ed("foo\nfoo");
    e.keys(&parse(":s/foo/x"));
    let shown = screen(&e);
    assert!(shown.contains("1 › x"), "{shown}");
    assert!(shown.contains("2   foo"), "{shown}");
}

#[test]
fn a_change_out_of_view_is_brought_into_it_and_back() {
    let text: Vec<String> = (0..60)
        .map(|i| if i == 50 { "needle".into() } else { format!("line {i}") })
        .collect();
    let mut e = ed(&text.join("\n"));
    e.keys(&parse(":%s/needle/found"));
    assert!(screen(&e).contains("found"));
    e.keys(&parse("<Esc>"));
    assert_eq!(e.top(), 0, "back where it was");
}
