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
