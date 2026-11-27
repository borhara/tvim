use super::Editor;
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
}
