mod common;
use common::{after, ed};
use tinsel::app::Key;
use tvim::keys::parse;

#[test]
fn alt_j_and_alt_k_move_the_line() {
    assert_eq!(after("a\nb\nc", "<A-j>"), ("b\na\nc".into(), (1, 0)));
    assert_eq!(after("a\nb\nc", "G<A-k>"), ("a\nc\nb".into(), (1, 0)));
    assert_eq!(
        after("ab\nc\nd", "l2<A-j>"),
        ("c\nd\nab".into(), (2, 1)),
        "a count, keeping the column"
    );
    assert_eq!(after("a\nb", "j<A-j>"), ("a\nb".into(), (1, 0)), "at the end, nothing");
    assert_eq!(
        after("a\nb\nc", "5<A-j>"),
        ("b\nc\na".into(), (2, 0)),
        "or as far as it goes"
    );
    assert_eq!(
        after("a\nb\nc", "<A-j><A-j>u"),
        ("b\na\nc".into(), (1, 0)),
        "each press undoes alone"
    );
    let mut e = ed("a\nb");
    e.keys(&parse("j<A-j>"));
    assert!(e.message().is_none(), "and says nothing");
}

#[test]
fn in_visual_mode_the_selection_moves() {
    assert_eq!(after("a\nb\nc\nd", "Vj<A-j>d"), ("c".to_string() + "\nd", (1, 0)));
    assert_eq!(after("a\nb\nc\nd", "Vj<A-j><Esc>"), ("c\na\nb\nd".into(), (2, 0)));
    assert_eq!(after("a\nb\nc\nd", "GVk<A-k><A-k><Esc>"), ("c\nd\na\nb".into(), (0, 0)));
}

#[test]
fn alt_is_still_esc_elsewhere() {
    let mut e = ed("a\nb");
    assert!(e.takes_alt(Key::Alt('j')));
    assert!(!e.takes_alt(Key::Alt('h')), "only j and k");
    e.keys(&parse("i"));
    assert!(!e.takes_alt(Key::Alt('j')), "insert mode keeps alt-j as esc and j");
    e.keys(&parse("<Esc>d"));
    assert!(!e.takes_alt(Key::Alt('j')), "as does an operator waiting");
}

#[test]
fn the_terminal_sends_alt_j_on_as_it_should() {
    use tinsel::app::{App, Event};
    use tvim::ui::Tvim;
    let mut app = Tvim::new(ed("a\nb\nc"));
    app.event(&Event::Key(Key::Alt('j')));
    assert_eq!(app.editor.text(), "b\na\nc", "in normal mode it moves the line");
    for k in parse("ggix") {
        app.event(&Event::Key(k));
    }
    app.event(&Event::Key(Key::Alt('j')));
    assert_eq!(app.editor.text(), "xb\na\nc", "in insert mode it's esc and j");
    assert_eq!(app.editor.mode(), tvim::Mode::Normal);
    assert_eq!(app.editor.cursor().row, 1);
}
