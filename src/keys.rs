use tinsel::app::Key;

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
