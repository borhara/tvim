use tinsel::app::{Key, Mods, NamedKey};

// keys in vim's notation, as tinsel reads them: dw<Esc> is d, w and esc
pub fn parse(spec: &str) -> Vec<Key> {
    Key::parse_sequence(spec)
}

/// A key as vim takes it: alt and a key is esc then the key, as terminals have always sent
/// it (so alt-j leaves insert mode and moves down), and any other modifier is let go. Alt-j
/// and Alt-k in normal and visual mode are the exception, as `Editor::takes_alt` says.
pub fn as_vim(key: Key) -> (Option<Key>, Key) {
    match key {
        Key::Alt(c) => (Some(Key::Esc), Key::Char(c)),
        Key::Mod(Mods::ALT, k @ (NamedKey::Backspace | NamedKey::Enter | NamedKey::Tab)) => (Some(Key::Esc), k.into()),
        Key::Mod(_, k) => (None, k.into()),
        other => (None, other),
    }
}

pub fn show(key: Key) -> String {
    match key {
        Key::Char(' ') => "␣".into(),
        Key::Char(c) => c.to_string(),
        Key::Ctrl(c) => format!("^{}", c.to_ascii_uppercase()),
        Key::Enter => "⏎".into(),
        Key::Esc => "^[".into(),
        Key::Tab => "⇥".into(),
        other => format!("<{other:?}>"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_and_named_keys() {
        assert_eq!(
            parse("dw<Esc><C-r><cr>a<b<lt>"),
            [
                Key::Char('d'),
                Key::Char('w'),
                Key::Esc,
                Key::Ctrl('r'),
                Key::Enter,
                Key::Char('a'),
                Key::Char('<'),
                Key::Char('b'),
                Key::Char('<'),
            ]
        );
        assert_eq!(parse("<C-ab>").len(), 6);
        assert_eq!(parse("é"), [Key::Char('é')]);
    }

    #[test]
    fn alt_keys_reach_vim_as_esc_then_the_key() {
        assert_eq!(as_vim(Key::Alt('j')), (Some(Key::Esc), Key::Char('j')));
        assert_eq!(
            as_vim(Key::Mod(Mods::ALT, NamedKey::Backspace)),
            (Some(Key::Esc), Key::Backspace)
        );
        assert_eq!(as_vim(Key::Mod(Mods::CTRL, NamedKey::Left)), (None, Key::Left));
        assert_eq!(as_vim(Key::Ctrl('r')), (None, Key::Ctrl('r')));
        assert_eq!(as_vim(Key::BackTab), (None, Key::BackTab));
    }

    #[test]
    fn shows_keys() {
        assert_eq!(show(Key::Char('d')), "d");
        assert_eq!(show(Key::Ctrl('w')), "^W");
        assert_eq!(show(Key::Up), "<Up>");
    }
}
