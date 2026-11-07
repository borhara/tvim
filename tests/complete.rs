mod common;
use common::{ed, text_after as t};
use tvim::keys::parse;

#[test]
fn ctrl_n_completes_from_the_nearest_word() {
    assert_eq!(t("foobar\nfoo", "jA<C-n><C-y><Esc>"), "foobar\nfoobar");
    assert_eq!(t("format\nfoobar\n", "Gifo<C-n><C-y><Esc>"), "format\nfoobar\nfoobar");
    assert_eq!(
        t("format\nfoobar\n", "Gifo<C-n><C-n><C-y><Esc>"),
        "format\nfoobar\nformat",
        "again goes further"
    );
}

#[test]
fn ctrl_p_goes_backwards() {
    assert_eq!(t("format\nfoobar\n", "Gifo<C-p><C-y><Esc>"), "format\nfoobar\nformat");
    assert_eq!(
        t("format\nfoobar\n", "Gifo<C-n><C-p><C-p><C-y><Esc>"),
        "format\nfoobar\nformat"
    );
}

#[test]
fn cycling_past_the_end_comes_back_to_what_was_typed() {
    assert_eq!(
        t("format\nfoobar\n", "Gifo<C-n><C-n><C-n><C-y><Esc><Esc>"),
        "format\nfoobar\nfo"
    );
}

#[test]
fn esc_and_ctrl_e_put_back_what_was_typed() {
    let mut e = ed("foobar\n");
    e.keys(&parse("Gifo<C-n>"));
    assert_eq!(e.text(), "foobar\nfoobar");
    e.keys(&parse("<C-e>"));
    assert_eq!(e.text(), "foobar\nfo");
    assert!(e.completion().is_none());
    e.keys(&parse("o<C-n><Esc>"));
    assert_eq!(e.text(), "foobar\nfoo");
    assert_eq!(e.mode(), tvim::Mode::Insert, "esc only closes the list");
}

#[test]
fn any_other_key_takes_the_word_and_carries_on() {
    assert_eq!(t("foobar\n", "Gifo<C-n><C-c>"), "foobar\nfoobar");
    assert_eq!(t("foobar\n", "Gifo<C-n>(<Esc>"), "foobar\nfoobar()");
    assert_eq!(t("foobar\n", "Gifo<C-n> x<Esc>"), "foobar\nfoobar x");
    assert_eq!(t("foobar\n", "Gifo<C-n><CR>x<Esc>"), "foobar\nfoobar\nx");
}

#[test]
fn the_word_being_typed_isnt_offered() {
    let mut e = ed("abc");
    e.keys(&parse("A<C-n>"));
    assert!(e.completion().is_none());
    assert_eq!(e.message().unwrap().text, "Pattern not found");
}

#[test]
fn undo_and_repeat_use_the_word_chosen() {
    assert_eq!(t("foobar\n", "Gifo<C-n><C-y><Esc>u"), "foobar\n");
    assert_eq!(
        t("foobar\n\nfork", "jifo<C-n><C-y><Esc>j0."),
        "foobar\nfoobar\nfoobarfork",
        "dot types the same word, even where another would be nearer"
    );
    assert_eq!(t("foobar\n", "G2ifo<C-n><C-y><Esc>"), "foobar\nfoobarfoobar");
}
