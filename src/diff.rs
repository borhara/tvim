use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    Added,
    Changed,
    Removed,
    RemovedAbove,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hunk {
    pub old: Range<usize>,
    pub new: Range<usize>,
}

impl Hunk {
    // where the hunk shows in the gutter: its first line, or the line above a removal
    pub fn row(&self) -> usize {
        if self.new.is_empty() {
            self.new.start.saturating_sub(1)
        } else {
            self.new.start
        }
    }
}

pub fn marks(old: &[String], new: &[String]) -> Vec<Option<Mark>> {
    marks_from(&hunks(old, new), new.len())
}

pub fn marks_from(hunks: &[Hunk], len: usize) -> Vec<Option<Mark>> {
    let mut out = vec![None; len];
    for h in hunks {
        if !h.new.is_empty() {
            let mark = if h.old.is_empty() { Mark::Added } else { Mark::Changed };
            for m in &mut out[h.new.clone()] {
                *m = Some(mark);
            }
        } else if h.new.start == 0 {
            if let Some(m) = out.first_mut().filter(|m| m.is_none()) {
                *m = Some(Mark::RemovedAbove);
            }
        } else if let Some(m) = out.get_mut(h.new.start - 1).filter(|m| m.is_none()) {
            *m = Some(Mark::Removed);
        }
    }
    out
}

pub fn common_ends(old: &[String], new: &[String]) -> (usize, usize) {
    let prefix = old.iter().zip(new).take_while(|(a, b)| a == b).count();
    let room = old.len().min(new.len()) - prefix;
    let suffix = old
        .iter()
        .rev()
        .zip(new.iter().rev())
        .take(room)
        .take_while(|(a, b)| a == b)
        .count();
    (prefix, suffix)
}

pub fn hunks(old: &[String], new: &[String]) -> Vec<Hunk> {
    let mut out = Vec::new();
    let (prefix, suffix) = common_ends(old, new);
    let a = &old[prefix..old.len() - suffix];
    let b = &new[prefix..new.len() - suffix];
    if a.is_empty() && b.is_empty() {
        return out;
    }
    // LCS is O(n*m), so give up on big hunks
    let pairs = if a.len().saturating_mul(b.len()) <= 4_000_000 {
        common(a, b)
    } else {
        Vec::new()
    };
    let (mut i, mut j) = (0, 0);
    for (pi, pj) in pairs.into_iter().chain(std::iter::once((a.len(), b.len()))) {
        if pi > i || pj > j {
            out.push(Hunk {
                old: prefix + i..prefix + pi,
                new: prefix + j..prefix + pj,
            });
        }
        i = pi + 1;
        j = pj + 1;
    }
    out
}

/// Where line `row` of the old text is in the new, from their hunks: Ok for a line kept or
/// changed (one of fewer changed lines than before goes to the last of them), and Err for a
/// line removed, with the line now in its place.
pub fn follow(hunks: &[Hunk], row: usize) -> Result<usize, usize> {
    follow_from(hunks, row, false)
}

/// As [`follow`], from the new text back to the old.
pub fn follow_back(hunks: &[Hunk], row: usize) -> Result<usize, usize> {
    follow_from(hunks, row, true)
}

fn follow_from(hunks: &[Hunk], row: usize, back: bool) -> Result<usize, usize> {
    let mut to = row;
    for h in hunks {
        let (from, into) = if back { (&h.new, &h.old) } else { (&h.old, &h.new) };
        if row < from.start {
            break;
        }
        if row < from.end {
            if into.is_empty() {
                return Err(into.start);
            }
            return Ok((into.start + row - from.start).min(into.end - 1));
        }
        to = to + into.len() - from.len();
    }
    Ok(to)
}

fn common(a: &[String], b: &[String]) -> Vec<(usize, usize)> {
    let (n, m) = (a.len(), b.len());
    let mut len = vec![0u32; (n + 1) * (m + 1)];
    let at = |i: usize, j: usize| i * (m + 1) + j;
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            len[at(i, j)] = if a[i] == b[j] {
                len[at(i + 1, j + 1)] + 1
            } else {
                len[at(i + 1, j)].max(len[at(i, j + 1)])
            };
        }
    }
    let (mut i, mut j) = (0, 0);
    let mut pairs = Vec::new();
    while i < n && j < m {
        if a[i] == b[j] {
            pairs.push((i, j));
            i += 1;
            j += 1;
        } else if len[at(i + 1, j)] >= len[at(i, j + 1)] {
            i += 1;
        } else {
            j += 1;
        }
    }
    pairs
}

#[cfg(test)]
mod tests {
    use super::*;
    use Mark::*;

    fn lines(s: &str) -> Vec<String> {
        s.split('\n').map(str::to_string).collect()
    }

    fn m(old: &str, new: &str) -> Vec<Option<Mark>> {
        marks(&lines(old), &lines(new))
    }

    #[test]
    fn nothing_changed() {
        assert_eq!(m("a\nb", "a\nb"), [None, None]);
    }

    #[test]
    fn added_changed_removed() {
        assert_eq!(m("a\nb", "a\nx\nb"), [None, Some(Added), None]);
        assert_eq!(m("a\nb\nc", "a\nB\nc"), [None, Some(Changed), None]);
        assert_eq!(m("a\nb\nc", "a\nc"), [Some(Removed), None]);
        assert_eq!(m("a\nb", "b"), [Some(RemovedAbove)]);
        assert_eq!(
            m("a\nb\nc\nd", "A\nb\nc\nd\ne"),
            [Some(Changed), None, None, None, Some(Added)]
        );
    }

    #[test]
    fn scattered_changes_are_marked_apart() {
        assert_eq!(
            m("1\n2\n3\n4\n5", "1\nX\n3\n4\nY"),
            [None, Some(Changed), None, None, Some(Changed)]
        );
    }

    #[test]
    fn hunks_know_both_sides() {
        let h = hunks(&lines("1\n2\n3\n4"), &lines("1\nX\nY\n3"));
        assert_eq!(h, [Hunk { old: 1..2, new: 1..3 }, Hunk { old: 3..4, new: 4..4 }]);
        assert_eq!(follow_back(&h, 4), Ok(4));
        assert_eq!(
            follow_back(&h, 2),
            Ok(1),
            "a line put in goes back to the one it replaced"
        );
        assert_eq!((h[0].row(), h[1].row()), (1, 3));
    }

    #[test]
    fn lines_followed_through_hunks() {
        let h = hunks(&lines("a\nb\nc\nd\ne"), &lines("x\na\nc\nD\nE\nF"));
        let f = |row| follow(&h, row);
        assert_eq!(f(0), Ok(1), "pushed down by a line put above");
        assert_eq!(f(1), Err(2), "removed, with c in its place");
        assert_eq!(f(2), Ok(2));
        assert_eq!((f(3), f(4)), (Ok(3), Ok(4)), "changed lines stay put");
        let joined = hunks(&lines("a\nb\nc\nz"), &lines("a b c\nz"));
        assert_eq!(follow(&joined, 2), Ok(0), "into what's left of a change");
        assert_eq!(follow(&joined, 3), Ok(1));
        assert_eq!(
            follow(&hunks(&lines("a\nb"), &lines("a")), 1),
            Err(1),
            "removed at the end"
        );
    }
}
