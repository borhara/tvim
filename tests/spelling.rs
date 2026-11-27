mod common;
use common::{ed, temp};
use tvim::keys::parse;
use tvim::spell::Dictionary;

fn words() -> Dictionary {
    Dictionary::from_words([
        "the", "quick", "brown", "fox", "jumps", "over", "lazy", "dog", "fix", "this", "here", "let",
    ])
}

fn opened(name: &str, text: &str) -> tvim::Editor {
    let path = temp(name);
    std::fs::write(&path, text).unwrap();
    let mut e = ed("");
    e.set_dictionary(words());
    e.ex(&format!("e {}", path.display()));
    e
}

// whether `word` is drawn underlined
fn underlined(e: &tvim::Editor, word: &str) -> bool {
    let screen = screen(e);
    let seg = screen
        .split("\u{1b}[")
        .find(|s| s.split_once('m').is_some_and(|(_, t)| t.starts_with(word)));
    seg.is_some_and(|s| {
        s.split('m')
            .next()
            .unwrap()
            .split(';')
            .take_while(|p| *p != "38")
            .any(|p| p == "4")
    })
}

fn screen(e: &tvim::Editor) -> String {
    tinsel::with_color_mode(tinsel::ColorMode::Ansi256, || {
        let mut frame = tinsel::app::Frame::new(60, 10);
        tvim::ui::draw(e, &mut frame);
        frame.snapshot().to_string()
    })
}

#[test]
fn brackets_s_go_from_one_word_spelled_wrong_to_the_next() {
    let mut e = opened("spell.md", "the quikc fox\njumps ovr the `lazzy` dog\n");
    e.keys(&parse("]s"));
    assert_eq!((e.cursor().row, e.cursor().col), (0, 4));
    e.keys(&parse("]s"));
    assert_eq!((e.cursor().row, e.cursor().col), (1, 6), "not inside code");
    e.keys(&parse("]s"));
    assert_eq!((e.cursor().row, e.cursor().col), (0, 4), "round past the end");
    e.keys(&parse("[s"));
    assert_eq!((e.cursor().row, e.cursor().col), (1, 6));
}

#[test]
fn in_code_only_comments_are_checked() {
    let mut e = opened("spell.rs", "let quikc = 1; // fix thsi here\n");
    e.keys(&parse("]s"));
    assert_eq!(e.cursor().col, 22);
}

#[test]
fn words_spelled_wrong_are_underlined_unless_spell_is_off() {
    let mut e = opened("under.md", "the quikc fox\n");
    assert!(underlined(&e, "quikc"), "{:?}", screen(&e));
    assert!(!underlined(&e, "fox"));
    e.ex("set nospell");
    assert!(!underlined(&e, "quikc"));
}
