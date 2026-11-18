mod common;
use common::{ed, text_after as t};
use tvim::Mode;
use tvim::keys::parse;

#[test]
fn normal_mode_mappings() {
    assert_eq!(t("one two", ":nnoremap Q dw<CR>Q"), "two");
    assert_eq!(t("one two", ":nmap <leader>d dw<CR> d"), "two", "<leader> is space");
    assert_eq!(
        t("a\nb\nc", ":nnoremap Q dd<CR>2Q"),
        "c",
        "a count goes before the mapping"
    );
    assert_eq!(
        t("ab cd", ":nnoremap w l<CR>dw"),
        "cd",
        "a motion after an operator isn't mapped"
    );
    assert_eq!(t("ab", ":nnoremap b x<CR>rb"), "bb", "nor is r's character");
}

#[test]
fn noremap_and_recursive_mappings() {
    assert_eq!(t("abc", ":nnoremap x l<CR>:nmap Q x<CR>Q"), "abc", "Q is x is l");
    let mut e = ed("abc");
    e.keys(&parse(":nnoremap x l<CR>:nnoremap Q x<CR>Q"));
    assert_eq!(
        (e.text(), e.cursor().col),
        ("bc".into(), 0),
        "noremap's x is the real x"
    );
    assert_eq!(
        t("ab", ":nmap x xp<CR>x"),
        "ba",
        "a mapping's own keys aren't mapped again"
    );
    let mut e = ed("ab");
    e.keys(&parse(":nmap Q W<CR>:nmap W Q<CR>Q"));
    assert!(e.message().is_some_and(|m| m.error && m.text.contains("E223")));
}

#[test]
fn insert_mode_mappings_wait_for_the_rest() {
    let mut e = ed("");
    e.keys(&parse(":inoremap jk <lt>Esc><CR>ihijk"));
    assert_eq!((e.text().as_str(), e.mode()), ("hi", Mode::Normal));
    let mut e = ed("");
    e.keys(&parse(":inoremap jk <lt>Esc><CR>ij"));
    assert_eq!(e.text(), "", "j waits to see if k follows");
    assert!(e.mapping_pending());
    e.keys(&parse("j"));
    assert_eq!(e.text(), "j", "jj isn't jk, so the first j goes in");
    e.flush_mapping();
    assert_eq!(
        (e.text().as_str(), e.mode()),
        ("jj", Mode::Insert),
        "and the second when the wait is over"
    );
}

#[test]
fn the_longest_mapping_wins() {
    let mut e = ed("one two three");
    e.keys(&parse(":nnoremap g w<CR>:nnoremap gxy dd<CR>gxq"));
    assert_eq!(e.cursor().col, 4, "gx isn't gxy, so g runs as w and x as itself");
    assert_eq!(e.text(), "one wo three");
}

#[test]
fn visual_and_command_line_mappings() {
    assert_eq!(t("abc", ":xnoremap Q d<CR>vlQ"), "c");
    assert_eq!(
        t("abc", ":vmap Q d<CR>Q"),
        "abc",
        "visual mappings stay out of normal mode"
    );
    assert_eq!(t("abc", ":cnoremap XX s/a/b/<CR>:XX<CR>"), "bbc");
    assert_eq!(t("abc", ":map Q x<CR>vlQ"), "c", ":map is for normal and visual");
    assert_eq!(
        t("", ":map! QQ hey<CR>iQQ"),
        "hey",
        ":map! is for insert and the command line"
    );
}

#[test]
fn unmapping_and_listing() {
    assert_eq!(t("ab", ":nnoremap Q x<CR>:nunmap Q<CR>Q"), "ab");
    assert_eq!(t("ab", ":nnoremap Q x<CR>:mapclear<CR>Q"), "ab");
    assert_eq!(t("ab", ":nnoremap Q <Nop><CR>Qx"), "b", "<Nop> does nothing");
    let mut e = ed("");
    e.keys(&parse(":nunmap Q<CR>"));
    assert_eq!(e.message().map(|m| m.text.as_str()), Some("E31: No such mapping"));
    e.keys(&parse(
        ":nnoremap <silent> <leader>w :w<lt>CR><CR>:inoremap jk <lt>Esc><CR>:nmap<CR>",
    ));
    assert_eq!(e.message().map(|m| m.text.as_str()), Some("n  <Space>w *:w<CR>"));
    e.keys(&parse(":map!<CR>"));
    assert_eq!(e.message().map(|m| m.text.as_str()), Some("i  jk *<Esc>"));
    e.keys(&parse(":imap x<CR>"));
    assert_eq!(e.message().map(|m| m.text.as_str()), Some("No mapping found"));
}

#[test]
fn repeats_and_macros() {
    assert_eq!(
        t("a b c d", ":nnoremap Q dw<CR>Q.."),
        "d",
        ". repeats what the mapping did"
    );
    assert_eq!(
        t("a b c d", ":nnoremap Q dw<CR>qqQq@q"),
        "c d",
        "a macro keeps the typed keys"
    );
    assert_eq!(
        t("ab", ":nnoremap x l<CR>:normal! x<CR>"),
        "b",
        ":normal! leaves mappings out"
    );
    assert_eq!(t("ab", ":nnoremap Q x<CR>:normal Q<CR>"), "b", ":normal uses them");
}

#[test]
fn other_commands_keep_their_short_names() {
    let mut e = ed("a\nb");
    e.keys(&parse(":nohl<CR>:norm x<CR>"));
    assert_eq!(e.text(), "\nb");
}

#[test]
fn held_keys_go_through_once_timeoutlen_is_up() {
    use tinsel::app::{App, Event, Key};
    let mut e = ed("");
    e.keys(&parse(":inoremap jk <lt>Esc><CR>i"));
    let mut tvim = tvim::ui::Tvim::new(e);
    tvim.event(&Event::Key(Key::Char('j')));
    tvim.event(&Event::Tick);
    assert_eq!(tvim.editor.text(), "", "a second hasn't passed");
    tvim.editor.options.timeout_len = 0;
    tvim.event(&Event::Tick);
    assert_eq!(tvim.editor.text(), "j");
    assert!(!tvim.editor.mapping_pending());
}
