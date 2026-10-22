use std::collections::HashSet;

use crate::buffer::Pos;
use crate::text::is_word;

const MAX: usize = 100;

fn words_in(line: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut start = None;
    for (col, (i, c)) in line.char_indices().enumerate() {
        match (is_word(c), start) {
            (true, None) => start = Some((col, i)),
            (false, Some((col0, i0))) => {
                out.push((col0, &line[i0..i]));
                start = None;
            }
            _ => {}
        }
    }
    if let Some((col0, i0)) = start {
        out.push((col0, &line[i0..]));
    }
    out
}

pub fn candidates(lines: &[String], at: Pos, prefix: &str, others: &[&[String]]) -> Vec<String> {
    let mut near: Vec<((usize, usize), &str)> = Vec::new();
    for (row, line) in lines.iter().enumerate() {
        for (col, w) in words_in(line) {
            if w.starts_with(prefix) && w != prefix && Pos::new(row, col) != at {
                near.push(((row.abs_diff(at.row), col.abs_diff(at.col)), w));
            }
        }
    }
    near.sort_by_key(|(d, _)| *d);
    let far = others
        .iter()
        .flat_map(|lines| lines.iter())
        .flat_map(|line| words_in(line))
        .filter(|(_, w)| w.starts_with(prefix) && *w != prefix)
        .map(|(_, w)| w);
    let mut seen = HashSet::new();
    near.into_iter()
        .map(|(_, w)| w)
        .chain(far)
        .filter(|w| seen.insert(*w))
        .take(MAX)
        .map(str::to_string)
        .collect()
}
