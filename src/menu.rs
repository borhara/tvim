//! A short list to choose one thing from, shown by the cursor: what a language server can do
//! with the code, say.

use tinsel::app::Key;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub label: String,
    // said after it, quieter
    pub hint: String,
    // why it can't be chosen, if it can't
    pub disabled: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Menu {
    pub title: String,
    pub entries: Vec<Entry>,
    pub selected: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Picked {
    Stay,
    Close,
    Chose(usize),
}

impl Menu {
    pub fn new(title: &str, entries: Vec<Entry>) -> Menu {
        Menu {
            title: title.to_string(),
            entries,
            selected: 0,
        }
    }

    pub fn key(&mut self, key: Key) -> Picked {
        let n = self.entries.len();
        match key {
            Key::Esc | Key::Ctrl('c') | Key::Char('q') => return Picked::Close,
            Key::Down | Key::Char('j') | Key::Ctrl('n') | Key::Tab => self.selected = (self.selected + 1) % n.max(1),
            Key::Up | Key::Char('k') | Key::Ctrl('p') | Key::BackTab => {
                self.selected = (self.selected + n.max(1) - 1) % n.max(1)
            }
            Key::Home | Key::Char('g') => self.selected = 0,
            Key::End | Key::Char('G') => self.selected = n.saturating_sub(1),
            Key::Enter => return self.choose(self.selected),
            // the first nine are numbered, to choose straight away
            Key::Char(c @ '1'..='9') => {
                let i = c as usize - '1' as usize;
                if i < n {
                    self.selected = i;
                    return self.choose(i);
                }
            }
            _ => {}
        }
        Picked::Stay
    }

    fn choose(&self, i: usize) -> Picked {
        match self.entries.get(i) {
            Some(e) if e.disabled.is_none() => Picked::Chose(i),
            _ => Picked::Stay,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(label: &str, disabled: bool) -> Entry {
        Entry {
            label: label.into(),
            hint: String::new(),
            disabled: disabled.then(|| "no".to_string()),
        }
    }

    fn menu() -> Menu {
        Menu::new("t", vec![entry("a", false), entry("b", true), entry("c", false)])
    }

    #[test]
    fn moving_round_and_choosing() {
        let mut m = menu();
        assert_eq!(m.key(Key::Char('k')), Picked::Stay);
        assert_eq!(m.selected, 2, "round to the end");
        assert_eq!(m.key(Key::Down), Picked::Stay);
        assert_eq!(m.selected, 0, "and back");
        m.key(Key::Ctrl('n'));
        assert_eq!(m.key(Key::Enter), Picked::Stay, "one that's disabled can't be chosen");
        m.key(Key::Char('j'));
        assert_eq!(m.key(Key::Enter), Picked::Chose(2));
        assert_eq!(m.key(Key::Esc), Picked::Close);
    }

    #[test]
    fn a_number_chooses_straight_away() {
        let mut m = menu();
        assert_eq!(m.key(Key::Char('3')), Picked::Chose(2));
        assert_eq!(m.key(Key::Char('2')), Picked::Stay);
        assert_eq!(m.key(Key::Char('9')), Picked::Stay, "past the end");
    }
}
