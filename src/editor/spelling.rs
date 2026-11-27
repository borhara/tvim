use super::{Choosing, Editor};
use crate::buffer::Pos;
use crate::menu::{Entry, Menu};
use crate::spell::{self, Dictionary};
use crate::syntax::{Kind, Syntax};

impl Editor {
    // the word list, read the first time it's wanted; None if the system has none
    fn with_dictionary<T>(&self, f: impl FnOnce(&mut Dictionary) -> T) -> Option<T> {
        let mut d = self.dictionary.borrow_mut();
        d.get_or_insert_with(Dictionary::load).as_mut().map(f)
    }

    /// Uses `d` to check spelling, in place of the system's word list.
    pub fn set_dictionary(&mut self, d: Dictionary) {
        *self.dictionary.borrow_mut() = Some(Some(d));
    }

    /// Where the words spelled wrong are in `line`, coloured as `kinds` for a file of
    /// `syntax`: every word in prose, and in code only those in comments.
    pub(crate) fn misspelled(&self, line: &str, kinds: Option<&[Kind]>, syntax: Option<Syntax>) -> Vec<(usize, usize)> {
        let prose = matches!(syntax, None | Some(Syntax::Markdown));
        if !self.options.spell || !prose && kinds.is_none() {
            return Vec::new();
        }
        let checked = |at: usize| match kinds.and_then(|k| k.get(at)) {
            None => prose,
            Some(Kind::Comment | Kind::Todo) => true,
            Some(Kind::Code | Kind::Url) => false,
            Some(_) => prose,
        };
        let words = spell::words(line).into_iter().filter(|&(a, _)| checked(a));
        let chars: Vec<char> = line.chars().collect();
        self.with_dictionary(|d| {
            words
                .filter(|&(a, b)| !d.knows(&chars[a..b].iter().collect::<String>()))
                .collect()
        })
        .unwrap_or_default()
    }

    // the words spelled wrong in the file's line `row`
    fn misspelled_at(&self, row: usize) -> Vec<(usize, usize)> {
        let text = self.shown_text();
        let syntax = text.syntax(true);
        let kinds = text.highlight(row, true);
        self.misspelled(self.buf.line(row), kinds.as_deref(), syntax)
    }

    // ]s and [s: the next word spelled wrong, or the one before, round past the ends
    pub(super) fn next_misspelled(&self, from: Pos, forward: bool, n: usize) -> Option<Pos> {
        let rows = self.buf.len();
        let mut at = from;
        for _ in 0..n {
            let found = (0..=rows).find_map(|k| {
                let row = if forward {
                    (at.row + k) % rows
                } else {
                    (at.row + rows * 2 - k) % rows
                };
                let spans = self.misspelled_at(row);
                let mut cols = spans.iter().map(|&(a, _)| a);
                let hit = match (forward, k) {
                    (true, 0) => cols.find(|&c| c > at.col),
                    (true, _) => cols.next(),
                    (false, 0) => cols.rfind(|&c| c < at.col),
                    (false, _) => cols.next_back(),
                };
                hit.map(|col| Pos::new(row, col))
            })?;
            at = found;
        }
        (at != from).then_some(at)
    }

    // the word under the cursor, as where it starts and ends, and itself
    fn word_for_spelling(&self) -> Option<(usize, usize, String)> {
        let line = self.buf.line(self.cursor.row);
        let col = self.cursor.col;
        let (a, b) = spell::words(line).into_iter().find(|&(a, b)| (a..b).contains(&col))?;
        Some((a, b, line.chars().skip(a).take(b - a).collect()))
    }

    // z=: what the word under the cursor may have been meant to be, to choose from
    pub(super) fn suggest_spelling(&mut self) {
        let Some((from, to, word)) = self.word_for_spelling() else {
            return self.error("no word to spell here");
        };
        let Some(found) = self.with_dictionary(|d| d.suggest(&word, 9)) else {
            return self.error("no word list to spell with");
        };
        if found.is_empty() {
            return self.info(format!("nothing like {word}"));
        }
        let entries = (found.iter())
            .map(|w| Entry {
                label: w.clone(),
                hint: String::new(),
                disabled: None,
            })
            .collect();
        let row = self.cursor.row;
        self.menu = Some((
            Menu::new(&format!("spelling of {word}"), entries),
            Choosing::Spelling(Pos::new(row, from), Pos::new(row, to), found),
        ));
    }

    // puts `word` in place of the word from `from` to `to`
    pub(super) fn respell(&mut self, from: Pos, to: Pos, word: &str) {
        self.touch();
        self.buf.delete(from, to);
        self.buf.insert(from, word);
        self.cursor = from;
        self.end_change();
    }
}
