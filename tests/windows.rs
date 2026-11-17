mod common;
use common::{ed, temp};
use tinsel::app::Frame;
use tvim::keys::parse;
use tvim::ui::draw;
use tvim::{Editor, Pos};

fn screen(e: &Editor, width: usize, height: usize) -> String {
    tinsel::with_color_mode(tinsel::ColorMode::None, || {
        let mut frame = Frame::new(width, height);
        draw(e, &mut frame);
        frame.snapshot().plain()
    })
}

fn lines(n: usize) -> String {
    (1..=n).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n")
}

fn file(name: &str, text: &str) -> std::path::PathBuf {
    let p = temp(name);
    std::fs::write(&p, text).unwrap();
    p
}

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

#[test]
fn ctrl_w_q_and_close() {
    let mut e = ed("hello");
    e.keys(&parse("<C-w>s<C-w>q"));
    assert_eq!(e.window_count(), 1);
    e.ex("close");
    assert!(e.message().unwrap().text.starts_with("E444"));
    assert!(!e.should_quit());
    e.keys(&parse("<C-w>q"));
    assert!(e.should_quit());
}

#[test]
fn only_keeps_the_focused_window() {
    let mut e = ed("hello");
    e.keys(&parse("<C-w>s<C-w>v<C-w>v"));
    e.keys(&parse("<C-w>o"));
    assert_eq!(e.window_count(), 1);
    e.ex("sp");
    e.ex("only");
    assert_eq!(e.window_count(), 1);
}

#[test]
fn moving_between_windows() {
    let mut e = ed("hello");
    e.ex("vsplit");
    e.ex("split");
    // top left, bottom left, right
    assert_eq!(e.window_index(), 0);
    e.keys(&parse("<C-w>j"));
    assert_eq!(e.window_index(), 1);
    e.keys(&parse("<C-w>l"));
    assert_eq!(e.window_index(), 2);
    e.keys(&parse("<C-w>h"));
    assert_eq!(e.window_index(), 0, "the one level with the cursor");
    e.keys(&parse("<C-w><Down>"));
    assert_eq!(e.window_index(), 1);
    e.keys(&parse("<C-w>k"));
    assert_eq!(e.window_index(), 0);
    e.keys(&parse("<C-w>k"));
    assert_eq!(e.window_index(), 0, "nothing above stays put");
    e.keys(&parse("<C-w>w<C-w>w<C-w>w"));
    assert_eq!(e.window_index(), 0, "w goes round");
    e.keys(&parse("<C-w>W"));
    assert_eq!(e.window_index(), 2);
    e.keys(&parse("2<C-w>w"));
    assert_eq!(e.window_index(), 1);
}

#[test]
fn each_window_keeps_its_own_cursor() {
    let mut e = ed(&lines(50));
    e.keys(&parse("<C-w>s40G"));
    e.keys(&parse("<C-w>j"));
    assert_eq!(e.cursor(), Pos::new(0, 0));
    e.keys(&parse("<C-w>k"));
    assert_eq!(e.cursor().row, 39);
}

#[test]
fn a_shared_buffer_stays_in_sync() {
    let mut e = ed(&lines(30));
    e.keys(&parse("<C-w>v20G"));
    e.keys(&parse("<C-w>l"));
    e.keys(&parse("ggOnew<Esc>"));
    e.keys(&parse("<C-w>h"));
    assert_eq!(e.cursor().row, 20, "a line added above moves the other window down");
    assert_eq!(e.buffer().line(20), "line 20");
    assert_eq!(e.buffer().line(0), "new");
    e.keys(&parse("<C-w>lggd3d<C-w>h"));
    assert_eq!(e.buffer().line(e.cursor().row), "line 20");
    e.keys(&parse("<C-w>l"));
    e.ex("%d");
    e.keys(&parse("<C-w>h"));
    assert_eq!(e.cursor(), Pos::new(0, 0), "and stays inside a buffer that shrank");
}

#[test]
fn edits_below_leave_the_other_window_alone() {
    let mut e = ed(&lines(30));
    e.keys(&parse("<C-w>s5G<C-w>j25Gdd<C-w>k"));
    assert_eq!(e.cursor().row, 4);
    e.keys(&parse("<C-w>jgg"));
    e.ex("s/line/LINE/");
    e.keys(&parse("<C-w>k"));
    assert_eq!(e.cursor().row, 4, "a changed line above doesn't move it");
}

#[test]
fn split_with_a_file() {
    let a = file("win-a.txt", "alpha\n");
    let b = file("win-b.txt", "beta\n");
    let mut e = Editor::open(&a).unwrap();
    e.set_screen(80, 24);
    e.ex(&format!("vsplit {}", b.display()));
    assert_eq!(e.text(), "beta");
    e.keys(&parse("<C-w>l"));
    assert_eq!(e.text(), "alpha");
    e.ex(&format!("sp {}", a.display()));
    assert_eq!(e.text(), "alpha", "the same file isn't reloaded");
    assert_eq!(e.buffers().len(), 2);
}

#[test]
fn new_splits_an_empty_buffer() {
    let mut e = ed("hello");
    e.ex("new");
    assert_eq!(e.text(), "");
    assert_eq!(e.buffers().len(), 2);
    e.keys(&parse("<C-w>j"));
    assert_eq!(e.text(), "hello");
    e.keys(&parse("<C-w>n"));
    assert_eq!(e.buffers().len(), 3);
}

#[test]
fn deleting_a_buffer_closes_its_other_windows() {
    let a = file("win-del-a.txt", "alpha\n");
    let b = file("win-del-b.txt", "beta\n");
    let mut e = Editor::open(&a).unwrap();
    e.set_screen(80, 24);
    e.ex(&format!("vs {}", b.display()));
    e.ex("sp");
    assert_eq!(e.window_count(), 3);
    e.ex("bd");
    assert_eq!(e.window_count(), 2);
    assert_eq!(e.text(), "alpha");
}

#[test]
fn substitute_still_reads_as_one() {
    let mut e = ed("aaa");
    e.ex("s/a/b/");
    assert_eq!(e.text(), "baa");
    assert_eq!(e.window_count(), 1);
}

#[test]
fn not_enough_room() {
    let mut e = ed("hello");
    e.set_screen(80, 5);
    e.ex("sp");
    assert!(e.message().unwrap().text.starts_with("E36"));
    assert_eq!(e.window_count(), 1);
}

#[test]
fn windows_are_drawn_with_titles_and_a_divider() {
    let p = file("win-draw.txt", "alpha\nbeta\n");
    let mut e = Editor::open(&p).unwrap();
    e.set_screen(80, 24);
    e.keys(&parse("<C-w>vj"));
    let shown = screen(&e, 60, 12);
    let rows: Vec<&str> = shown.lines().collect();
    let titles = rows.iter().find(|r| r.contains("── ✶ win-draw.txt")).expect(&shown);
    assert!(titles.contains("│── win-draw.txt"), "{shown}");
    let text = rows.iter().find(|r| r.contains("alpha")).unwrap();
    assert_eq!(text.matches("1   alpha").count(), 2, "{shown}");
    assert!(text.contains('│'));
}

#[test]
fn the_cursor_is_in_the_focused_window() {
    let mut e = ed("alpha\nbeta");
    e.keys(&parse("<C-w>v<C-w>l"));
    let mut frame = Frame::new(60, 12);
    draw(&e, &mut frame);
    let (x, _, _) = frame.cursor().unwrap();
    assert!(x > 30, "the right window has it: {x}");
}

#[test]
fn every_window_is_drawn() {
    let mut e = ed("alpha");
    e.keys(&parse("<C-w>s<C-w>v"));
    let shown = screen(&e, 70, 20);
    assert_eq!(shown.matches("1   alpha").count(), 2, "{shown}");
    assert_eq!(
        shown.matches("1 › alpha").count(),
        1,
        "only the focused one marks its line: {shown}"
    );
}

#[test]
fn zen_shows_only_the_focused_window() {
    let mut e = ed("alpha");
    e.keys(&parse("<C-w>v"));
    e.ex("zen");
    let shown = screen(&e, 120, 12);
    assert_eq!(shown.matches("alpha").count(), 1, "{shown}");
    assert!(!shown.contains('│'));
}

#[test]
fn resizing_and_evening_out() {
    let mut e = ed("alpha");
    e.keys(&parse("<C-w>v"));
    let width = |e: &Editor| {
        let shown = screen(e, 61, 12);
        let row = shown.lines().find(|r| r.contains("alpha")).unwrap().to_string();
        row.chars().position(|c| c == '│').unwrap()
    };
    let even = width(&e);
    e.keys(&parse("5<C-w>>"));
    assert_eq!(width(&e), even + 5);
    e.keys(&parse("<C-w>3<lt>"));
    assert_eq!(width(&e), even + 2);
    e.keys(&parse("<C-w>="));
    assert_eq!(width(&e), even);
    e.keys(&parse("<C-w>s4<C-w>+"));
    let shown = screen(&e, 61, 20);
    let rows: Vec<&str> = shown.lines().collect();
    let titles: Vec<usize> = (0..rows.len())
        .filter(|&i| rows[i].trim_start().starts_with("── "))
        .collect();
    assert_eq!(titles.len(), 2, "{shown}");
}

#[test]
fn a_window_on_another_file_draws_that_file() {
    let a = file("win-two-a.rs", "fn alpha() {}\n");
    let b = file("win-two-b.txt", "beta\n");
    let mut e = Editor::open(&a).unwrap();
    e.set_screen(80, 24);
    e.ex(&format!("vs {}", b.display()));
    e.keys(&parse("x"));
    let shown = screen(&e, 70, 12);
    assert!(shown.contains("fn alpha()") && shown.contains("eta"), "{shown}");
    assert!(
        shown.contains("win-two-b.txt ●"),
        "the focused title marks the change: {shown}"
    );
}
