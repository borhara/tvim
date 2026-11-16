mod common;
use tvim::Editor;
use tvim::keys::parse;
use tvim::ui::Tvim;

fn ed(text: &str) -> Editor {
    common::ed(text)
}

fn screen(e: Editor, width: usize, height: usize) -> String {
    tinsel::with_color_mode(tinsel::ColorMode::None, || {
        tinsel::app::snapshot(&Tvim::new(e), width, height).plain()
    })
}

fn rows(shown: &str) -> Vec<&str> {
    shown.lines().collect()
}

#[test]
fn the_text_sits_on_the_night_sky() {
    let mut e = ed("first\n\tsecond");
    e.keys(&parse("j"));
    let expected = [
        "    ✶ untitled                           2 lines",
        "    1   first",
        "    2 ›     second",
        "",
        "",
        "              ⋆",
        "  ·······················✶······················",
        "                        ╰ untitled ╯         2:1",
    ];
    assert_eq!(
        screen(e, 50, 8),
        expected.join("\n"),
        "a change dot, the number and › for the cursor's line; a wandering star past the end; \
         the star line; then the file's bracket, widened to name it, and the place; the file's length \
         and line ending in the corner"
    );
}

#[test]
fn line_numbers_can_start_with_zeros() {
    let mut e = ed("a\nb");
    assert_eq!(&rows(&screen(e, 30, 8))[1..3], ["    1 › a", "    2   b"]);
    e = ed("a\nb");
    e.ex("set zp");
    assert_eq!(&rows(&screen(e, 30, 8))[1..3], ["   01 › a", "   02   b"]);
}

#[test]
fn relative_numbers() {
    let mut e = ed("a\nb\nc");
    e.ex("set rnu");
    e.keys(&parse("j"));
    let shown = screen(e, 30, 8);
    assert_eq!(&rows(&shown)[1..4], ["    1   a", "  2   › b", "    1   c"]);
}

#[test]
fn the_last_row_follows_the_mode() {
    let last = |keys: &str| {
        let mut e = ed("abc");
        e.keys(&parse(keys));
        rows(&screen(e, 50, 8)).last().unwrap().to_string()
    };
    // the mode is the colour of the star, the cursor and the labels, not a word
    for keys in ["", "i", "v", "V", "R", ":"] {
        let row = last(keys);
        assert!(row.contains('╰') && !row.contains("normal"), "{keys}: {row}");
    }
    assert!(last("/").starts_with("  / "), "{}", last("/"));
    assert!(last("<Tab>").ends_with("1 of 8"), "{}", last("<Tab>"));
    let mut e = ed("abc");
    e.keys(&parse("x"));
    assert!(rows(&screen(e, 50, 8))[0].contains("untitled ●"), "modified");
    assert!(last("qa").contains("● rec @a"));
}

#[test]
fn searching_shows_how_many_matches() {
    let mut e = ed("a b a b a");
    e.keys(&parse("/a"));
    let shown = screen(e, 40, 5);
    let last = shown.lines().last().unwrap();
    assert!(last.starts_with("  / a ") && last.ends_with("2 of 3   1:5"), "{last:?}");
    let mut e = ed("a b a b a");
    e.keys(&parse("/a<CR>n"));
    assert!(screen(e, 60, 5).contains("/ 3/3"));
    let mut e = ed("abc");
    e.keys(&parse("/z"));
    assert!(screen(e, 40, 5).contains("no match"));
}

#[test]
fn long_lines_scroll_sideways() {
    let mut e = ed(&format!("{}END", "x".repeat(50)));
    e.keys(&parse("$"));
    let shown = screen(e, 30, 8);
    let first = rows(&shown)[1];
    assert!(first.starts_with("    1 › x") && first.ends_with("xxEND"), "{first:?}");
    assert_eq!(first.chars().count(), 29, "the last column stays clear");
}

#[test]
fn lines_changed_since_saving_are_marked() {
    let path = common::temp("marks.txt");
    std::fs::write(&path, "one\ntwo\nthree\n").unwrap();
    let mut e = Editor::open(&path).unwrap();
    let marks = |e: &Editor| e.change_marks().to_vec();
    assert_eq!(marks(&e), [None, None, None]);
    e.keys(&parse("jAx<Esc>Gonew<Esc>"));
    use tvim::diff::Mark::*;
    assert_eq!(marks(&e), [None, Some(Changed), None, Some(Added)]);
    e.keys(&parse("ggdd"));
    assert_eq!(
        marks(&e),
        [Some(Changed), None, Some(Added)],
        "the changed line moved up, and one went above it"
    );
    e.keys(&parse("u"));
    e.keys(&parse(":w<CR>"));
    assert!(marks(&e).iter().all(Option::is_none), "saving clears them");
}

#[test]
fn colouring_carries_over_lines() {
    let path = common::temp("carry.rs");
    std::fs::write(&path, "/* a\nb */ fn x() {}\n").unwrap();
    let e = Editor::open(&path).unwrap();
    use tvim::syntax::Kind::*;
    let second = e.highlight(1).unwrap();
    assert_eq!((second[0], second[5], second[8]), (Comment, Keyword, Function));
    let plain = common::temp("plain.txt");
    std::fs::write(&plain, "fn x").unwrap();
    assert!(Editor::open(&plain).unwrap().highlight(0).is_none());
}

#[test]
fn colouring_follows_edits_and_undo() {
    let path = common::temp("edits.rs");
    let text = "fn a() {}\nlet s = \"x\";\n// c\nfn b() {}\nlet t = 1;\nfn c() {}\n";
    std::fs::write(&path, text).unwrap();
    let mut e = Editor::open(&path).unwrap();
    let all = |e: &Editor| (0..e.text().lines().count()).map(|r| e.highlight(r).unwrap()).collect::<Vec<_>>();
    let fresh = |e: &Editor| {
        let p = common::temp("edits_fresh.rs");
        std::fs::write(&p, e.text()).unwrap();
        all(&Editor::open(&p).unwrap())
    };
    let before = all(&e);
    for keys in ["3GO/*<Esc>", "5Gox */<Esc>", "u", "2Gf\"a\\<Esc>", "ggdd", "Gp", "u", "u", "u", "u"] {
        e.keys(&parse(keys));
        assert_eq!(all(&e), fresh(&e), "after {keys}");
    }
    assert_eq!(all(&e), before);
}
