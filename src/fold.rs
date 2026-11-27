//! Folds, found from indentation: a line folds the lines under it that are indented further,
//! and with them a closing bracket or `end` at its own indent, so `fn a() {` folds down to
//! its `}`.

use crate::text::{first_non_blank, vcol};

fn indent_width(line: &str, tab_stop: usize) -> usize {
    vcol(line, first_non_blank(line), tab_stop)
}

// a line that closes what the line above its block opened
fn closes(line: &str) -> bool {
    let t = line.trim();
    t.starts_with(['}', ']', ')']) || t == "end" || t.starts_with("end ") || t.starts_with("end)")
}

/// The last line of the fold line `row` starts, if it starts one.
pub fn fold_at(lines: &[String], row: usize, tab_stop: usize) -> Option<usize> {
    let line = lines.get(row)?;
    if line.trim().is_empty() {
        return None;
    }
    let base = indent_width(line, tab_stop);
    let mut end = None;
    for (r, l) in lines.iter().enumerate().skip(row + 1) {
        if l.trim().is_empty() {
            continue;
        }
        let w = indent_width(l, tab_stop);
        if w > base {
            end = Some(r);
            continue;
        }
        if end.is_some() && w == base && closes(l) {
            end = Some(r);
        }
        break;
    }
    end
}

/// The folds holding line `row`, from the innermost out, each as its first and last line.
pub fn holding(lines: &[String], row: usize, tab_stop: usize) -> Vec<(usize, usize)> {
    let mut out: Vec<(usize, usize)> = Vec::new();
    for start in (0..=row.min(lines.len().saturating_sub(1))).rev() {
        // an outer fold starts less indented than the last one found
        if let Some(&(inner, _)) = out.last() {
            let line = &lines[start];
            if line.trim().is_empty() || indent_width(line, tab_stop) >= indent_width(&lines[inner], tab_stop) {
                continue;
            }
        }
        if let Some(end) = fold_at(lines, start, tab_stop).filter(|&end| end >= row) {
            out.push((start, end));
        }
    }
    out
}

/// Every fold, each as its first and last line, in order of where they start.
pub fn all(lines: &[String], tab_stop: usize) -> Vec<(usize, usize)> {
    (0..lines.len())
        .filter_map(|r| fold_at(lines, r, tab_stop).map(|end| (r, end)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(text: &str) -> Vec<String> {
        text.split('\n').map(str::to_string).collect()
    }

    const CODE: &str = "fn a() {\n    let x = 1;\n\n    if x {\n        go();\n    }\n}\n\nfn b() {}";

    #[test]
    fn a_fold_takes_the_lines_indented_under_it_and_its_closer() {
        let l = lines(CODE);
        assert_eq!(fold_at(&l, 0, 4), Some(6), "down to the closing bracket");
        assert_eq!(fold_at(&l, 3, 4), Some(5));
        assert_eq!(fold_at(&l, 1, 4), None, "nothing under it");
        assert_eq!(fold_at(&l, 2, 4), None, "a blank line");
        assert_eq!(fold_at(&l, 8, 4), None);
        let py = lines("def a():\n    pass\n\n\nb = 1");
        assert_eq!(fold_at(&py, 0, 4), Some(1), "not the blank lines after");
        let rb = lines("def a\n\tx\nend");
        assert_eq!(fold_at(&rb, 0, 4), Some(2), "tabs, and end");
    }

    #[test]
    fn the_folds_holding_a_line_from_the_innermost_out() {
        let l = lines(CODE);
        assert_eq!(holding(&l, 4, 4), [(3, 5), (0, 6)]);
        assert_eq!(holding(&l, 3, 4), [(3, 5), (0, 6)], "its own first line too");
        assert_eq!(holding(&l, 2, 4), [(0, 6)]);
        assert_eq!(holding(&l, 8, 4), []);
        assert_eq!(all(&l, 4), [(0, 6), (3, 5)]);
    }
}
