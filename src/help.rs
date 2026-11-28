//! The docs, as the README has them, to read and search inside tvim with :help.

pub const DOCS: &str = include_str!("../README.md");

/// One thing the docs cover: a heading, or a paragraph that starts with a name in bold.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Topic {
    pub name: String,
    pub row: usize,
    /// the start of what it says, to tell topics apart in a list
    pub gist: String,
}

pub fn lines() -> Vec<String> {
    DOCS.lines().map(str::to_string).collect()
}

/// Every topic in `docs`, in order.
pub fn topics(docs: &str) -> Vec<Topic> {
    let mut out = Vec::new();
    let mut lines = docs.lines().enumerate().peekable();
    while let Some((row, line)) = lines.next() {
        let (name, rest) = if let Some(h) = line.strip_prefix('#') {
            let rest = lines.peek().map_or("", |&(_, l)| l);
            (h.trim_start_matches('#').trim().to_string(), rest)
        } else if let Some(bold) = line.strip_prefix("**")
            && let Some((name, rest)) = bold.split_once("**")
        {
            (name.trim_end_matches('.').to_string(), rest)
        } else {
            continue;
        };
        let gist: String = rest.trim().replace('`', "").chars().take(60).collect();
        out.push(Topic { name, row, gist });
    }
    out
}

/// Where in `docs` to read about `query`: a topic by that name, or starting with it; then a
/// line naming it in code, as `zg` or `:Gdiff`; then a topic with it in its name; then any
/// line with it. A single character is only found as a name or in code, a letter or digit
/// that isn't going to what's said about coming from vim. Case doesn't matter.
pub fn find(docs: &str, query: &str) -> Option<usize> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Some(0);
    }
    let topics = topics(docs);
    let named = |test: &dyn Fn(&str) -> bool| topics.iter().find(|t| test(&t.name.to_lowercase())).map(|t| t.row);
    let lines: Vec<String> = docs.lines().map(str::to_lowercase).collect();
    let code = format!("`{q}`");
    // a single letter is a key, found as one rather than inside some other word, and one not
    // written about does what it does in vim
    if q.chars().count() == 1 {
        let vim = q
            .chars()
            .all(char::is_alphanumeric)
            .then(|| named(&|n| n == "coming from vim"));
        return named(&|n| n == q)
            .or_else(|| lines.iter().position(|l| l.contains(&code)))
            .or(vim.flatten());
    }
    named(&|n| n == q)
        .or_else(|| named(&|n| n.starts_with(&q)))
        .or_else(|| lines.iter().position(|l| l.contains(&code)))
        .or_else(|| named(&|n| n.contains(&q)))
        .or_else(|| {
            lines
                .iter()
                .position(|l| l.contains(&format!("`{q}")) || l.contains(&format!("{q}`")))
        })
        .or_else(|| lines.iter().position(|l| l.contains(&q)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOCS: &str = "# tvim\n\nAn editor.\n\n## Extras\n\n**Folds.** `zc` closes one.\n\n**Spelling.** `z=` and `zg`, and `]s`.\n\n## Settings\n\nSet with `:set`.\n\n`x` deletes a character.\n\n## Coming from vim\n\nKeys work as in vim.\n";

    #[test]
    fn headings_and_bold_names_are_topics() {
        let names: Vec<String> = topics(DOCS).into_iter().map(|t| t.name).collect();
        assert_eq!(
            names,
            ["tvim", "Extras", "Folds", "Spelling", "Settings", "Coming from vim"]
        );
        assert_eq!(topics(DOCS)[2].gist, "zc closes one.");
    }

    #[test]
    fn names_come_first_then_keys_in_code_then_any_word() {
        assert_eq!(find(DOCS, "spelling"), Some(8));
        assert_eq!(find(DOCS, "fold"), Some(6), "the start of a name");
        assert_eq!(find(DOCS, "set"), Some(10), "a name before a key");
        assert_eq!(find(DOCS, "zg"), Some(8));
        assert_eq!(find(DOCS, ":set"), Some(12));
        assert_eq!(find(DOCS, "closes"), Some(6));
        assert_eq!(find(DOCS, "nothing like it"), None);
        assert_eq!(find(DOCS, ""), Some(0));
        assert_eq!(find(DOCS, "x"), Some(14), "a key before a letter inside Extras");
        assert_eq!(find(DOCS, "e"), Some(16), "not the e in Extras, but as vim has it");
        assert_eq!(find(DOCS, "?"), None, "not a key vim has");
    }

    #[test]
    fn the_readme_has_topics() {
        let names: Vec<String> = topics(super::DOCS).into_iter().map(|t| t.name).collect();
        assert!(names.contains(&"Spelling".to_string()), "{names:?}");
    }
}
