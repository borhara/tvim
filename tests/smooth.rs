mod common;
use tinsel::ColorMode;
use tinsel::app::Frame;
use tvim::Editor;
use tvim::keys::parse;
use tvim::ui::draw;

fn long() -> Editor {
    let text: Vec<String> = (1..=200).map(|n| format!("line {n}")).collect();
    common::ed(&text.join("\n"))
}

fn frame_at(e: &Editor, t: f32, mode: ColorMode) -> Frame {
    tinsel::with_color_mode(mode, || {
        let mut frame = Frame::new(80, 24).at(t);
        draw(e, &mut frame);
        frame
    })
}

fn first_line(e: &Editor, t: f32) -> usize {
    let shown = frame_at(e, t, ColorMode::None).snapshot().plain();
    shown
        .lines()
        .find_map(|l| l.split("line ").nth(1)?.split_whitespace().next()?.parse().ok())
        .unwrap()
}

#[test]
fn half_pages_glide_into_place() {
    let mut e = long();
    assert_eq!(first_line(&e, 1.0), 1);
    e.keys(&parse("<C-d>"));
    let mid = first_line(&e, 1.04);
    let to = e.top() + 1;
    assert!(to > 5);
    assert!(mid > 1 && mid < to, "on its way: {mid} of {to}");
    assert!(first_line(&e, 1.08) >= mid);
    assert_eq!(first_line(&e, 1.3), to);
}

#[test]
fn every_kind_of_jump_glides() {
    for keys in ["G", "<C-f>", ":120<CR>", "/line 150<CR>", "zt"] {
        let mut e = long();
        e.keys(&parse("15G"));
        first_line(&e, 1.0);
        e.keys(&parse(keys));
        let mid = first_line(&e, 1.03);
        let to = e.top() + 1;
        assert!(mid != to, "{keys} glides");
        assert_eq!(first_line(&e, 2.0), to, "{keys} lands");
    }
}

#[test]
fn without_smooth_it_snaps() {
    let mut e = long();
    e.ex("set nosmooth");
    first_line(&e, 1.0);
    e.keys(&parse("G"));
    assert_eq!(first_line(&e, 1.01), e.top() + 1);
    e.ex("set smooth?");
    assert_eq!(e.message().unwrap().text, "nosmooth");
}

#[test]
fn snapshots_show_where_it_ends_up() {
    let mut e = long();
    first_line(&e, 0.0);
    e.keys(&parse("G"));
    assert_eq!(first_line(&e, 0.0), e.top() + 1);
}
