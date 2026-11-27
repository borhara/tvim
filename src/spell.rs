//! Spelling, checked against the system's word list and words you add, with the endings
//! English puts on words taken off to find them.

use std::collections::HashSet;
use std::path::PathBuf;

const SYSTEM: &[&str] = &["/usr/share/dict/words", "/usr/dict/words"];

pub struct Dictionary {
    words: HashSet<String>,
    // where words added with zg are kept, one a line
    own: Option<PathBuf>,
}

impl Dictionary {
    /// The system's words and your own, or None if there's no system word list.
    pub fn load() -> Option<Dictionary> {
        let text = SYSTEM.iter().find_map(|p| std::fs::read_to_string(p).ok())?;
        // beside the config, as ~/.config/tvim/spell.txt
        let own = crate::session::config_path().and_then(|c| Some(c.parent()?.join("spell.txt")));
        let mine = own
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .unwrap_or_default();
        let mut d = Dictionary::from_words(text.lines().chain(mine.lines()));
        d.own = own;
        Some(d)
    }

    pub fn from_words<'a>(words: impl IntoIterator<Item = &'a str>) -> Dictionary {
        Dictionary {
            words: words
                .into_iter()
                .map(|w| w.trim().to_lowercase())
                .filter(|w| !w.is_empty())
                .collect(),
            own: None,
        }
    }

    /// Whether `word` is spelled right: known as it is, or once an ending is taken off.
    pub fn knows(&self, word: &str) -> bool {
        let word = word.to_lowercase();
        let word = word.strip_suffix("'s").or(word.strip_suffix("’s")).unwrap_or(&word);
        self.words.contains(word) || stems(word).iter().any(|s| self.words.contains(s))
    }

    /// Adds `word`, keeping it in your own list for next time.
    pub fn add(&mut self, word: &str) -> std::io::Result<()> {
        self.words.insert(word.to_lowercase());
        let Some(path) = &self.own else { return Ok(()) };
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new().create(true).append(true).open(path)?;
        writeln!(file, "{word}")
    }

    /// Known words a letter or two from `word`, the nearest first, at most `n`, in its case.
    pub fn suggest(&self, word: &str, n: usize) -> Vec<String> {
        let lower = word.to_lowercase();
        let near: Vec<String> = edits(&lower);
        let mut out: Vec<String> = Vec::new();
        let keep = |w: &String, out: &mut Vec<String>| {
            if self.words.contains(w) && !out.contains(w) && *w != lower {
                out.push(w.clone());
            }
        };
        for w in &near {
            keep(w, &mut out);
        }
        // two letters away only when nothing's one away
        if out.is_empty() {
            for w in &near {
                for v in edits(w) {
                    keep(&v, &mut out);
                }
                if out.len() >= n * 4 {
                    break;
                }
            }
        }
        out.truncate(n);
        let capital = word.chars().next().is_some_and(char::is_uppercase);
        out.into_iter()
            .map(|w| match capital {
                true => w[..1].to_uppercase() + &w[1..],
                false => w,
            })
            .collect()
    }
}

// what a word may have been before an ending went on
fn stems(word: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut push = |s: &str| {
        if s.len() >= 2 {
            out.push(s.to_string());
        }
    };
    for (end, back) in [
        ("ies", "y"),
        ("ied", "y"),
        ("ier", "y"),
        ("iest", "y"),
        ("ily", "y"),
        ("es", ""),
        ("s", ""),
        ("ed", ""),
        ("ed", "e"),
        ("d", ""),
        ("ing", ""),
        ("ing", "e"),
        ("er", ""),
        ("er", "e"),
        ("ers", ""),
        ("ers", "e"),
        ("est", ""),
        ("ly", ""),
        ("ness", ""),
        ("ment", ""),
        ("ments", ""),
        ("able", ""),
        ("able", "e"),
    ] {
        if let Some(base) = word.strip_suffix(end) {
            push(&format!("{base}{back}"));
            // running is run with its last letter doubled
            let b = base.as_bytes();
            if b.len() >= 3 && b[b.len() - 1] == b[b.len() - 2] && back.is_empty() {
                push(&base[..base.len() - 1]);
            }
        }
    }
    out
}

// every string a letter away: one taken out, swapped with the next, changed or put in
fn edits(word: &str) -> Vec<String> {
    let chars: Vec<char> = word.chars().collect();
    let mut out = Vec::new();
    for i in 0..=chars.len() {
        let (a, b) = (&chars[..i], &chars[i..]);
        let s = |parts: &[&[char]]| parts.concat().into_iter().collect::<String>();
        if !b.is_empty() {
            out.push(s(&[a, &b[1..]]));
        }
        if b.len() > 1 {
            out.push(s(&[a, &[b[1], b[0]], &b[2..]]));
        }
        for c in 'a'..='z' {
            if !b.is_empty() && b[0] != c {
                out.push(s(&[a, &[c], &b[1..]]));
            }
            out.push(s(&[a, &[c], b]));
        }
    }
    out
}

/// The words in `line` worth checking, as where each starts and ends, in characters: letters
/// with apostrophes inside, leaving out short ones, those all in capitals, and any with
/// capitals inside, digits or underscores next to them, as names in code are.
pub fn words(line: &str) -> Vec<(usize, usize)> {
    let chars: Vec<char> = line.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    let part_of = |c: char| c.is_alphanumeric() || c == '_';
    while i < chars.len() {
        if !part_of(chars[i]) {
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len()
            && (part_of(chars[i]) || is_apostrophe(chars[i]) && chars.get(i + 1).is_some_and(|c| c.is_alphabetic()))
        {
            i += 1;
        }
        let word = &chars[start..i];
        let letters = word.iter().all(|&c| c.is_alphabetic() || is_apostrophe(c));
        let inner_capital = word.iter().skip(1).any(|c| c.is_uppercase());
        // a path, address or name like a.b is left alone
        let joined = start > 0 && matches!(chars[start - 1], '.' | '/' | '\\' | '@' | '-' | '#')
            || chars.get(i).is_some_and(|&c| {
                matches!(c, '/' | '@' | '(') || c == '.' && chars.get(i + 1).is_some_and(|c| c.is_alphanumeric())
            });
        if letters && !inner_capital && !joined && word.len() >= 3 {
            out.push((start, i));
        }
    }
    out
}

fn is_apostrophe(c: char) -> bool {
    c == '\'' || c == '’'
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dict() -> Dictionary {
        Dictionary::from_words([
            "file", "run", "test", "happy", "make", "colour", "the", "quick", "Paris",
        ])
    }

    #[test]
    fn words_are_known_with_their_endings() {
        let d = dict();
        for w in [
            "file", "Files", "running", "tested", "happiest", "making", "colour's", "PARIS",
        ] {
            assert!(d.knows(w), "{w}");
        }
        // a wrong ending passes, as runing for run, which is what taking endings off costs
        for w in ["fiel", "colr", "tset"] {
            assert!(!d.knows(w), "{w}");
        }
    }

    #[test]
    fn suggestions_are_known_words_nearby_in_the_same_case() {
        let d = dict();
        assert_eq!(d.suggest("teh", 3), ["the"]);
        assert_eq!(d.suggest("Qiuck", 3), ["Quick"]);
        assert_eq!(d.suggest("fiel", 3)[0], "file");
        assert!(d.suggest("zzzzzzzz", 3).is_empty());
    }

    #[test]
    fn only_words_like_prose_are_checked() {
        let line = "The quick_brown fox's camelCase NASA x2 ok a.b see/here word.";
        let found: Vec<String> = words(line)
            .into_iter()
            .map(|(a, b)| line.chars().skip(a).take(b - a).collect())
            .collect();
        assert_eq!(found, ["The", "fox's", "word"]);
    }
}
