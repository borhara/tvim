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

fn long(n: usize) -> String {
    // lines two rows long in a 30-column window
    (0..n)
        .map(|i| format!("{i:02}{}", "x".repeat(30)))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn the_view_scrolls_by_screen_rows() {
    let mut e = wrapping(&long(20));
    e.keys(&parse(":set so=0<CR>"));
    e.set_screen(30, 12);
    e.keys(&parse("4j"));
    e.scroll();
    assert_eq!(e.top(), 0, "lines 0 to 4 take ten rows, which fit");
    e.keys(&parse("j"));
    e.scroll();
    assert_eq!(e.top(), 1, "line 5 needs two more");
    e.keys(&parse("$"));
    e.scroll();
    assert_eq!(e.top(), 1);
}

#[test]
fn scrolloff_counts_the_rows_below_too() {
    let mut e = wrapping(&long(20));
    e.keys(&parse(":set so=1<CR>"));
    e.set_screen(30, 12);
    e.keys(&parse("3j"));
    e.scroll();
    assert_eq!(e.top(), 0);
    e.keys(&parse("j"));
    e.scroll();
    assert_eq!(e.top(), 1);
}

#[test]
fn h_m_and_l_go_by_whats_shown() {
    let mut e = wrapping(&long(20));
    e.keys(&parse(":set so=0<CR>"));
    e.set_screen(30, 12);
    e.keys(&parse("L"));
    assert_eq!(e.cursor().row, 4, "five lines of two rows fill the ten");
    e.keys(&parse("M"));
    assert_eq!(e.cursor().row, 2);
}

#[test]
fn zz_and_zb_with_long_lines() {
    let mut e = wrapping(&long(20));
    e.keys(&parse(":set so=0<CR>"));
    e.set_screen(30, 12);
    e.keys(&parse("10Gzb"));
    assert_eq!(e.top(), 5, "lines 5 to 9 fill the screen down to the cursor's");
    e.keys(&parse("zz"));
    assert_eq!(e.top(), 7);
}
