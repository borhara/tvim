mod common;
use common::{cursor_after as at, ed};
use tvim::keys::parse;
use tvim::ui::Tvim;

const CODE: &str = "fn a() {\n    let x = 1;\n    if x {\n        go();\n    }\n}\nfn b() {\n    two();\n}";

#[test]
fn zc_closes_the_fold_around_the_cursor_and_j_steps_over_it() {
    assert_eq!(at(CODE, "jzc"), (0, 0), "to its first line");
    assert_eq!(at(CODE, "jzcj"), (6, 0), "past it in one");
    assert_eq!(at(CODE, "jzcjk"), (0, 0), "and back onto it");
    assert_eq!(at(CODE, "3jzcj"), (5, 0), "only the inner one, with its closer");
    assert_eq!(at(CODE, "3jzczcj"), (6, 0), "and again, the one around it");
    let mut e = ed("x\ny");
    e.keys(&parse("zc"));
    assert_eq!(e.message().unwrap().text, "E490: No fold found");
}

#[test]
fn zo_and_za_open_what_zc_closed() {
    assert_eq!(at(CODE, "jzczoj"), (1, 0));
    assert_eq!(at(CODE, "jzazaj"), (1, 0), "za both ways");
    assert_eq!(at(CODE, "zajk"), (0, 0));
}

#[test]
fn zm_closes_every_fold_and_zr_opens_them() {
    let mut e = ed(CODE);
    e.keys(&parse("zMj"));
    assert_eq!(e.cursor().row, 6);
    assert_eq!(e.fold_count(), 2, "the inner one inside the first");
    e.keys(&parse("zRgg"));
    assert_eq!(e.fold_count(), 0);
}

#[test]
fn going_into_a_closed_fold_opens_it() {
    let mut e = ed(CODE);
    e.keys(&parse("zM/go<CR>"));
    assert_eq!(e.cursor().row, 3);
    assert_eq!(e.fold_count(), 1, "only the one around b is left");
}

#[test]
fn a_closed_fold_moves_with_the_lines_above_it() {
    let mut e = ed(CODE);
    e.keys(&parse("7Gzcggonew<Esc>"));
    assert_eq!(e.fold_count(), 1);
    e.keys(&parse("G"));
    assert_eq!(e.cursor().row, 7, "fn b, a line further down, still closed");
    e.keys(&parse("u"));
    assert_eq!(e.fold_count(), 1, "and back with undo");
    e.keys(&parse("G"));
    assert_eq!(e.cursor().row, 6);
}

#[test]
fn a_closed_fold_shows_as_its_first_line_and_what_it_hides() {
    let mut e = ed(CODE);
    e.keys(&parse("jzc"));
    let shown = tinsel::with_color_mode(tinsel::ColorMode::None, || {
        tinsel::app::snapshot(&Tvim::new(e), 50, 10).plain()
    });
    let rows: Vec<&str> = shown.lines().collect();
    assert!(rows[2].ends_with("1 › fn a() { ··· }  5 lines"), "{shown}");
    assert!(rows[3].ends_with("7   fn b() {"), "{shown}");
}
