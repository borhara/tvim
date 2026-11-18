mod common;
use common::ed;
use tvim::Mode;
use tvim::editor::Block;
use tvim::keys::parse;
use tvim::ui::Tvim;

fn screen(e: tvim::Editor) -> String {
    tinsel::with_color_mode(tinsel::ColorMode::None, || {
        tinsel::app::snapshot(&Tvim::new(e), 50, 8).plain()
    })
}

#[test]
fn ctrl_v_selects_a_block() {
    let mut e = ed("abcd\nefgh\nijkl");
    e.keys(&parse("l<C-v>jl"));
    assert_eq!(e.mode(), Mode::VisualBlock);
    assert_eq!(e.mode().name(), "V-BLOCK");
    assert_eq!(
        e.block_selection(),
        Some(Block {
            top: 0,
            bottom: 1,
            left: 1,
            right: 3
        })
    );
    e.keys(&parse("<Esc>"));
    assert_eq!(e.mode(), Mode::Normal);
    assert_eq!(e.block_selection(), None);
}

#[test]
fn switching_between_visual_modes() {
    let mut e = ed("abc\ndef");
    e.keys(&parse("<C-v>"));
    assert_eq!(e.mode(), Mode::VisualBlock);
    e.keys(&parse("v"));
    assert_eq!(e.mode(), Mode::Visual);
    e.keys(&parse("<C-v>"));
    assert_eq!(e.mode(), Mode::VisualBlock);
    e.keys(&parse("V"));
    assert_eq!(e.mode(), Mode::VisualLine);
    e.keys(&parse("<C-q><C-v>"));
    assert_eq!(e.mode(), Mode::Normal, "Ctrl-q is Ctrl-v too, and Ctrl-v again leaves");
    e.keys(&parse("<C-v>jl<Esc>gv"));
    assert_eq!(e.mode(), Mode::VisualBlock, "gv brings the block back");
}

#[test]
fn o_and_capital_o_go_to_the_other_corners() {
    let mut e = ed("abcd\nefgh");
    e.keys(&parse("<C-v>jll"));
    assert_eq!((e.cursor().row, e.cursor().col), (1, 2));
    e.keys(&parse("O"));
    assert_eq!(
        (e.cursor().row, e.cursor().col),
        (1, 0),
        "O swaps sides on the same line"
    );
    e.keys(&parse("o"));
    assert_eq!((e.cursor().row, e.cursor().col), (0, 2), "o goes to the far corner");
    assert_eq!(e.block_selection().map(|b| (b.left, b.right)), Some((0, 3)));
}

#[test]
fn dollar_takes_the_block_to_every_line_end() {
    let mut e = ed("ab\nabcdef");
    e.keys(&parse("<C-v>j$"));
    assert!(e.block_selection().unwrap().to_end());
    e.keys(&parse("h"));
    assert!(!e.block_selection().unwrap().to_end());
}

#[test]
fn tabs_count_by_the_columns_they_fill() {
    let mut e = ed("\tx\nabcdefgh");
    e.keys(&parse("j<C-v>kl"));
    let b = e.block_selection().unwrap();
    assert_eq!((b.left, b.right), (0, 5), "the tab fills columns 0 to 3, then x");
}

#[test]
fn the_status_line_gives_the_blocks_size() {
    let mut e = ed("abcd\nefgh\nijkl");
    e.keys(&parse("<C-v>jjl"));
    let shown = screen(e);
    assert!(shown.contains("3×2"), "{shown}");
}

#[test]
fn leaving_sets_the_corner_marks() {
    let mut e = ed("abcd\nefgh\nijkl");
    e.keys(&parse("l<C-v>jjl<Esc>gg0`>"));
    assert_eq!((e.cursor().row, e.cursor().col), (2, 2));
    e.keys(&parse("`<"));
    assert_eq!((e.cursor().row, e.cursor().col), (0, 1));
}

#[test]
fn deleting_a_block() {
    use common::{after, text_after as t};
    assert_eq!(after("abcd\nefgh\nijkl", "l<C-v>jld"), ("ad\neh\nijkl".into(), (0, 1)));
    assert_eq!(t("abcd\nefgh", "l<C-v>jx"), "acd\negh");
    assert_eq!(
        t("abcd\ne\nijkl", "ll<C-v>jjd"),
        "abd\ne\nijl",
        "short lines lose nothing"
    );
    assert_eq!(t("abcd\nefghij", "l<C-v>j$d"), "a\ne");
    assert_eq!(t("abcd\nefghij", "l<C-v>jD"), "a\ne", "D goes to the ends of the lines");
    assert_eq!(t("abcd\nefgh", "l<C-v>jdu"), "abcd\nefgh", "one undo puts it back");
}

#[test]
fn yanking_and_putting_a_block() {
    use common::{after, text_after as t};
    assert_eq!(t("ab\ncd", "<C-v>jy$p"), "aba\ncdc");
    assert_eq!(t("ab\ncd", "<C-v>jyP"), "aab\nccd");
    assert_eq!(after("xy\nzw", "l<C-v>jy0p"), ("xyy\nzww".into(), (0, 1)));
    assert_eq!(
        t("abc\nd\nefg", "<C-v>jjyG$p"),
        "abc\nd\nefga\n   d\n   e",
        "lines past the end are added and padded"
    );
    assert_eq!(t("a\nbcd", "<C-v>jlyj$p"), "a\nbcda\n   bc");
    assert_eq!(t("ab\ncd", "<C-v>jy2P"), "aaab\ncccd", "a count puts it side by side");
}

#[test]
fn a_block_is_padded_to_line_up_with_text_after_it() {
    use common::text_after as t;
    assert_eq!(t("a\nbcd\nxyz\nxyz", "<C-v>jlyjjP"), "a\nbcd\na xyz\nbcxyz");
}

#[test]
fn putting_over_a_block() {
    use common::text_after as t;
    assert_eq!(
        t("Xbc\ndef", "yll<C-v>jlp"),
        "XX\ndX",
        "a piece of a line goes on every line"
    );
    assert_eq!(
        t("ab\ncd\nef\ngh", "<C-v>jyjjl<C-v>jp"),
        "ab\ncd\nea\ngc",
        "a block replaces a block"
    );
    assert_eq!(t("Xbc\ndef", "yll<C-v>jlPu"), "Xbc\ndef");
}

#[test]
fn block_registers_say_so() {
    let mut e = ed("ab\ncd");
    e.keys(&parse("<C-v>jy"));
    let r = e.register('"').unwrap();
    assert_eq!((r.text.as_str(), r.block, r.linewise), ("a\nc", true, false));
    assert!(e.register('0').unwrap().block);
    e.keys(&parse("yl"));
    assert!(!e.register('"').unwrap().block);
}

#[test]
fn inserting_on_every_line() {
    use common::{after, text_after as t};
    assert_eq!(
        after("abc\ndef\nghi", "l<C-v>jjI--<Esc>"),
        ("a--bc\nd--ef\ng--hi".into(), (0, 2))
    );
    assert_eq!(
        t("abc\n\ndef", "l<C-v>jjIx<Esc>"),
        "axbc\n\ndxef",
        "lines that don't reach the block are left"
    );
    assert_eq!(
        t("ab\ncd", "<C-v>jIx<CR>y<Esc>"),
        "x\nyab\ncd",
        "a new line stops the copying"
    );
    assert_eq!(
        t("abc\ndef", "<C-v>jIxy<BS><Esc>u"),
        "abc\ndef",
        "one undo takes it all back"
    );
}

#[test]
fn appending_on_every_line() {
    use common::text_after as t;
    assert_eq!(t("abc\ndef", "<C-v>jA!<Esc>"), "a!bc\nd!ef");
    assert_eq!(
        t("abcd\na\nabcd", "ll<C-v>jjA!<Esc>"),
        "abc!d\na  !\nabc!d",
        "short lines are padded"
    );
    assert_eq!(
        t("ab\nabcd", "<C-v>j$A;<Esc>"),
        "ab;\nabcd;",
        "after $ it's each line's end"
    );
    assert_eq!(t("abcd\na\nabcd", "ll<C-v>jjA!<Esc>u"), "abcd\na\nabcd");
}

#[test]
fn changing_a_block() {
    use common::text_after as t;
    assert_eq!(t("abcd\nefgh", "l<C-v>jlcX<Esc>"), "aXd\neXh");
    assert_eq!(t("abcd\nefgh", "l<C-v>jlsXY<Esc>"), "aXYd\neXYh");
    assert_eq!(
        t("abcd\nefgh", "l<C-v>jCX<Esc>"),
        "aX\neX",
        "C changes to the ends of the lines"
    );
    assert_eq!(t("abcd\nefgh", "l<C-v>jlcX<Esc>u"), "abcd\nefgh");
}

#[test]
fn replacing_and_changing_case_in_a_block() {
    use common::text_after as t;
    assert_eq!(t("abcd\nefgh", "l<C-v>jlrx"), "axxd\nexxh");
    assert_eq!(t("abcd\nefgh", "l<C-v>jlU"), "aBCd\neFGh");
    assert_eq!(t("ABCD\nEFGH", "l<C-v>jlu"), "AbcD\nEfgH");
    assert_eq!(t("aBcd\neFgh", "l<C-v>jl~"), "abCd\nefGh");
    assert_eq!(t("abcd\nefgh", "l<C-v>jlrxu"), "abcd\nefgh");
}

#[test]
fn shifting_from_the_blocks_edge() {
    use common::text_after as t;
    assert_eq!(t("ab\ncd", "l<C-v>j>"), "a    b\nc    d");
    assert_eq!(t("a      b\nc  d", "l<C-v>j<lt>"), "a  b\ncd");
    assert_eq!(
        t("ab\nc\nde", "l<C-v>jj2>"),
        "a        b\nc\nd        e",
        "short lines stay"
    );
}

#[test]
fn numbers_in_a_block_go_up_and_down() {
    use common::text_after as t;
    assert_eq!(t("1 1\n1 1", "ll<C-v>j<C-a>"), "1 2\n1 2");
    assert_eq!(t("5 5\n5 5", "ll<C-v>j3<C-x>"), "5 2\n5 2");
}

#[test]
fn joining_and_commenting_a_blocks_lines() {
    use common::text_after as t;
    assert_eq!(t("a\nb\nc", "<C-v>jJ"), "a b\nc");
    let mut e = ed("a\nb\nc");
    e.keys(&parse("l<C-v>jgc"));
    assert_eq!(e.text(), "# a\n# b\nc");
}
