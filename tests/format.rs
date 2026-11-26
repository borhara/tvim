mod common;
use common::{cursor_after as at, ed, temp, text_after as t};
use tvim::Editor;
use tvim::keys::parse;

fn after(name: &str, text: &str, spec: &str) -> String {
    let path = temp(name);
    std::fs::write(&path, text).unwrap();
    let mut e = Editor::open(&path).unwrap();
    e.set_clipboard(Box::new(tvim::clipboard::Memory::default()));
    e.keys(&parse(spec));
    e.text()
}

#[test]
fn gq_rewraps_lines() {
    assert_eq!(t("aa bb cc dd", ":set tw=5<CR>gqq"), "aa bb\ncc dd");
    assert_eq!(t("aa\nbb\ncc\n\ndd", "gqip"), "aa bb cc\n\ndd");
    assert_eq!(t("aa\nbb\n\ncc\ndd", "gqG"), "aa bb\n\ncc dd");
    assert_eq!(t("aa\nbb\ncc", "Vjgq"), "aa bb\ncc");
    assert_eq!(t("aa\nbb\ncc", "2gqq"), "aa bb\ncc");
    let long = "word ".repeat(20);
    let wrapped = t(long.trim(), "gqq");
    assert!(wrapped.lines().all(|l| l.len() <= 79), "79 wide without textwidth");
    assert_eq!(wrapped.lines().count(), 2);
    assert_eq!(t("aa\nbb", "gqju"), "aa\nbb", "one change");
}

#[test]
fn gq_ends_on_the_last_line_and_gw_stays() {
    assert_eq!(at("aa bb cc\nx", ":set tw=5<CR>gqq"), (1, 0));
    assert_eq!(at("aa bb cc\nx", ":set tw=5<CR>wgww"), (0, 3));
    assert_eq!(
        at("aa bb cc dd ee", ":set tw=5<CR>$gww"),
        (2, 1),
        "on the same letter of ee"
    );
    assert_eq!(at("aa\nbb\ncc", "jlgwip"), (0, 4), "joined up, still on bb");
    assert_eq!(t("aa bb cc", ":set tw=5<CR>gwip"), "aa bb\ncc");
}

#[test]
fn gq_keeps_comment_markers_and_bullets() {
    assert_eq!(
        after("a.rs", "    // aa bb cc\n    // dd\nfn x() {}", ":set tw=15<CR>gqj"),
        "    // aa bb cc\n    // dd\nfn x() {}"
    );
    assert_eq!(
        after("b.rs", "// aa bb cc dd", ":set tw=10<CR>gqq"),
        "// aa bb\n// cc dd"
    );
    assert_eq!(
        after("c.md", "# Title\n- aa bb cc\n- dd", ":set tw=8<CR>gqG"),
        "# Title\n- aa bb\n  cc\n- dd"
    );
}

#[test]
fn typing_past_textwidth_breaks_the_line() {
    assert_eq!(t("", ":set tw=10<CR>iaaa bbb ccc ddd<Esc>"), "aaa bbb\nccc ddd");
    assert_eq!(
        t("", "iaaa bbb ccc ddd<Esc>"),
        "aaa bbb ccc ddd",
        "not without textwidth"
    );
    assert_eq!(
        t("", ":set tw=6<CR>i  aaa bbb<Esc>"),
        "  aaa\n  bbb",
        "keeping the indent"
    );
    assert_eq!(
        t("", ":set tw=4<CR>iabcdefgh<Esc>"),
        "abcdefgh",
        "a word with nowhere to break"
    );
    assert_eq!(
        after("d.rs", "", ":set tw=12<CR>i// aaa bbb ccc<Esc>"),
        "// aaa bbb\n// ccc",
        "comments in code"
    );
    assert_eq!(
        after("e.rs", "", ":set tw=12<CR>ilet aaa = bbb + ccc;<Esc>"),
        "let aaa = bbb + ccc;",
        "but not the code"
    );
    assert_eq!(
        after("f.md", "", ":set tw=8<CR>i- aaa bbb ccc<Esc>"),
        "- aaa\n  bbb\n  ccc"
    );
    assert_eq!(
        after("g.md", "", ":set tw=5<CR>i- verylongword<Esc>"),
        "- verylongword",
        "not between a bullet and its text"
    );
    let mut e = ed("");
    e.keys(&parse(":set tw=10<CR>iaaa bbb ccc ddd"));
    assert_eq!(e.cursor(), tvim::Pos::new(1, 7), "the cursor goes along");
}
