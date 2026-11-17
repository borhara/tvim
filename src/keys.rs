use tinsel::app::{Key, Mods, NamedKey};

pub fn parse(spec: &str) -> Vec<Key> {
    let mut keys = Vec::new();
    let mut rest = spec;
    while let Some(c) = rest.chars().next() {
        if c == '<' {
            if let Some((key, used)) = rest.find('>').and_then(|end| Some((named(&rest[1..end])?, end + 1))) {
                keys.push(key);
                rest = &rest[used..];
                continue;
            }
        }
        keys.push(Key::Char(c));
        rest = &rest[c.len_utf8()..];
    }
    keys
}

fn named(name: &str) -> Option<Key> {
    let lower = name.to_ascii_lowercase();
    Some(match lower.as_str() {
        "esc" => Key::Esc,
        "cr" | "enter" | "return" => Key::Enter,
        "bs" | "backspace" => Key::Backspace,
        "del" | "delete" => Key::Delete,
        "tab" => Key::Tab,
        "s-tab" => Key::BackTab,
        "up" => Key::Up,
        "down" => Key::Down,
        "left" => Key::Left,
        "right" => Key::Right,
        "home" => Key::Home,
        "end" => Key::End,
        "pageup" => Key::PageUp,
        "pagedown" => Key::PageDown,
        "lt" => Key::Char('<'),
        "space" => Key::Char(' '),
        _ => {
            let letter = lower.strip_prefix("c-")?;
            let mut chars = letter.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) if c.is_ascii_lowercase() => Key::Ctrl(c),
                _ => return None,
            }
        }
    })
}

pub(crate) fn notation(k: Key) -> String {
    match k {
        Key::Char('<') => "<lt>".into(),
        Key::Char(c) => c.to_string(),
        Key::Ctrl(c) => format!("<C-{c}>"),
        Key::Enter => "<CR>".into(),
        Key::Esc => "<Esc>".into(),
        Key::Tab => "<Tab>".into(),
        Key::BackTab => "<S-Tab>".into(),
        Key::Backspace => "<BS>".into(),
        Key::Delete => "<Del>".into(),
        Key::Up => "<Up>".into(),
        Key::Down => "<Down>".into(),
        Key::Left => "<Left>".into(),
        Key::Right => "<Right>".into(),
        Key::Home => "<Home>".into(),
        Key::End => "<End>".into(),
        Key::PageUp => "<PageUp>".into(),
        Key::PageDown => "<PageDown>".into(),
        Key::F(n) => format!("<F{n}>"),
        other => format!("<{other:?}>"),
    }
}

/// A key as vim takes it: alt and a key is esc then the key, as terminals have always sent
/// it (so alt-j leaves insert mode and moves down), and any other modifier is let go.
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
        assert_eq!(as_vim(Key::Mod(Mods::ALT, NamedKey::Backspace)), (Some(Key::Esc), Key::Backspace));
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
