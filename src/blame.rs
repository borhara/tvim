use std::collections::HashMap;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Commit {
    pub author: String,
    pub email: String,
    pub time: i64,
    pub summary: String,
}

#[derive(Debug, Default)]
pub struct Blame {
    commits: Vec<Commit>,
    lines: Vec<Option<usize>>,
    pub me: Option<String>,
}

impl Blame {
    pub fn parse(porcelain: &str, me: Option<String>) -> Blame {
        let mut b = Blame { me, ..Blame::default() };
        let mut seen: HashMap<&str, usize> = HashMap::new();
        let mut at = None;
        for line in porcelain.lines() {
            if line.starts_with('\t') {
                continue;
            }
            let mut words = line.split(' ');
            let first = words.next().unwrap_or("");
            if first.len() == 40 && first.bytes().all(|c| c.is_ascii_hexdigit()) {
                let index = *seen.entry(first).or_insert_with(|| {
                    b.commits.push(Commit::default());
                    b.commits.len() - 1
                });
                at = Some(index);
                if let Some(row) = words.nth(1).and_then(|n| n.parse::<usize>().ok()).filter(|n| *n > 0) {
                    if b.lines.len() < row {
                        b.lines.resize(row, None);
                    }
                    b.lines[row - 1] = Some(index);
                }
                continue;
            }
            let Some(c) = at.map(|i| &mut b.commits[i]) else {
                continue;
            };
            let rest = line.get(first.len() + 1..).unwrap_or("");
            match first {
                "author" => c.author = rest.to_string(),
                "author-mail" => c.email = rest.trim_start_matches('<').trim_end_matches('>').to_string(),
                "author-time" => c.time = rest.parse().unwrap_or(0),
                "summary" => c.summary = rest.to_string(),
                _ => {}
            }
        }
        b
    }

    pub fn line(&self, row: usize) -> Option<&Commit> {
        self.commits.get((*self.lines.get(row)?)?)
    }

    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
}
