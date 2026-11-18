mod common;
use common::ed;
use tinsel::app::Frame;
use tvim::Editor;
use tvim::keys::parse;
use tvim::ui::{Tvim, draw};

fn screen(e: Editor, width: usize, height: usize) -> String {
    tinsel::with_color_mode(tinsel::ColorMode::None, || {
        tinsel::app::snapshot(&Tvim::new(e), width, height).plain()
    })
}

fn cursor(e: &Editor, width: usize, height: usize) -> Option<(usize, usize)> {
    let mut frame = Frame::new(width, height);
    draw(e, &mut frame);
    frame.cursor().map(|c| (c.x, c.y))
}

fn wrapping(text: &str) -> Editor {
    let mut e = ed(text);
    e.keys(&parse(":set wrap<CR>"));
    e
}

#[test]
fn long_lines_carry_on_below() {
    let e = wrapping("abcdefghijklmnopqrstuvwxyz0123456789\nnext");
    let shown = screen(e, 30, 8);
    let rows: Vec<&str> = shown.lines().collect();
    assert_eq!(rows[1], "    1 › abcdefghijklmnopqrstu", "{shown}");
    assert_eq!(rows[2], "        vwxyz0123456789", "the rest has no number");
    assert_eq!(rows[3], "    2   next");
}

#[test]
fn without_wrap_long_lines_run_off_the_side() {
    let e = ed("abcdefghijklmnopqrstuvwxyz0123456789\nnext");
    let shown = screen(e, 30, 8);
    let rows: Vec<&str> = shown.lines().collect();
    assert_eq!(rows[2], "    2   next");
}

#[test]
fn the_cursor_sits_on_its_screen_row() {
    let mut e = wrapping("abcdefghijklmnopqrstuvwxyz0123456789\nnext");
    e.keys(&parse("25|"));
    assert_eq!(
        cursor(&e, 30, 8),
        Some((11, 2)),
        "column 25 is the fourth along the second row"
    );
    e.keys(&parse("j"));
    assert_eq!(cursor(&e, 30, 8).map(|c| c.1), Some(3));
}

#[test]
fn a_full_row_puts_the_insert_cursor_below() {
    let mut e = wrapping("abcdefghijklmnopqrstu");
    e.keys(&parse("A"));
    assert_eq!(cursor(&e, 30, 8), Some((8, 2)));
}
