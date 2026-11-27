//! Merge conflicts git leaves in a file, between `<<<<<<<` and `>>>>>>>`.

/// One conflict, as the rows of its markers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Conflict {
    /// `<<<<<<<`, before our side
    pub start: usize,
    /// `|||||||`, before what both sides came from, in diff3 style
    pub base: Option<usize>,
    /// `=======`, before their side
    pub split: usize,
    /// `>>>>>>>`, after it
    pub end: usize,
}

/// Which part of a conflict a row is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    Marker,
    Ours,
    Base,
    Theirs,
}

/// What to keep of a conflict.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Keep {
    Ours,
    Theirs,
    Both,
    Neither,
}

impl Conflict {
    pub fn contains(&self, row: usize) -> bool {
        (self.start..=self.end).contains(&row)
    }

    pub fn part(&self, row: usize) -> Part {
        let ours_end = self.base.unwrap_or(self.split);
        if row == self.start || row == self.split || row == self.end || Some(row) == self.base {
            Part::Marker
        } else if row < ours_end {
            Part::Ours
        } else if row < self.split {
            Part::Base
        } else {
            Part::Theirs
        }
    }

    /// The lines to put in place of the whole conflict, from `lines`.
    pub fn resolve(&self, lines: &[String], keep: Keep) -> Vec<String> {
        let ours = &lines[self.start + 1..self.base.unwrap_or(self.split)];
        let theirs = &lines[self.split + 1..self.end];
        match keep {
            Keep::Ours => ours.to_vec(),
            Keep::Theirs => theirs.to_vec(),
            Keep::Both => [ours, theirs].concat(),
            Keep::Neither => Vec::new(),
        }
    }
}

fn marker(line: &str, c: char) -> bool {
    let rest = line.trim_start_matches(c);
    line.len() - rest.len() == 7 && (rest.is_empty() || rest.starts_with(' '))
}

/// The conflicts in `lines`, in order. A marker out of place starts looking again.
pub fn find(lines: &[String]) -> Vec<Conflict> {
    let mut out = Vec::new();
    let (mut start, mut base, mut split) = (None, None, None);
    for (row, line) in lines.iter().enumerate() {
        if marker(line, '<') {
            (start, base, split) = (Some(row), None, None);
        } else if start.is_some() && split.is_none() && base.is_none() && marker(line, '|') {
            base = Some(row);
        } else if start.is_some() && split.is_none() && line == "=======" {
            split = Some(row);
        } else if let (Some(s), Some(m)) = (start, split)
            && marker(line, '>')
        {
            out.push(Conflict {
                start: s,
                base,
                split: m,
                end: row,
            });
            (start, base, split) = (None, None, None);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(s: &str) -> Vec<String> {
        s.lines().map(str::to_string).collect()
    }

    #[test]
    fn finds_conflicts_with_and_without_a_base() {
        let text = lines(
            "a\n<<<<<<< HEAD\nours\n=======\ntheirs\n>>>>>>> topic\nb\n<<<<<<< HEAD\no\n||||||| base\nbase\n=======\nt\n>>>>>>> x\n",
        );
        let found = find(&text);
        assert_eq!(
            found,
            [
                Conflict {
                    start: 1,
                    base: None,
                    split: 3,
                    end: 5
                },
                Conflict {
                    start: 7,
                    base: Some(9),
                    split: 11,
                    end: 13
                },
            ]
        );
        assert_eq!(found[1].part(8), Part::Ours);
        assert_eq!(found[1].part(10), Part::Base);
        assert_eq!(found[1].part(12), Part::Theirs);
        assert_eq!(found[1].part(11), Part::Marker);
    }

    #[test]
    fn markers_must_be_seven_and_in_order() {
        assert!(find(&lines("<<<<<<<< long\nx\n=======\ny\n>>>>>>>\n")).is_empty());
        assert!(find(&lines("=======\n>>>>>>> x\n")).is_empty());
        assert_eq!(find(&lines("<<<<<<< a\n<<<<<<< b\n=======\n>>>>>>> c\n")).len(), 1);
    }

    #[test]
    fn resolving_keeps_a_side_both_or_neither() {
        let text = lines("<<<<<<< HEAD\no\n||||||| base\nb\n=======\nt\n>>>>>>> x");
        let c = find(&text)[0];
        assert_eq!(c.resolve(&text, Keep::Ours), ["o"]);
        assert_eq!(c.resolve(&text, Keep::Theirs), ["t"]);
        assert_eq!(c.resolve(&text, Keep::Both), ["o", "t"]);
        assert!(c.resolve(&text, Keep::Neither).is_empty());
    }
}
