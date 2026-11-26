//! Undo history kept on disk, so a file's changes can be undone after tvim closes. Each step
//! is kept as the lines that differ from the step after it, newest first, and the history
//! is only taken back when the file still reads as it did when it was saved.

use std::path::{Path, PathBuf};

use crate::buffer::Pos;
use crate::diff;

const HEADER: &str = "tvim undo 1";

/// How many lines all the steps kept may hold between them. Each step taken back is the
/// whole file as it was, so a big file keeps fewer, newest first, and opens as quickly.
pub const KEPT_LINES: usize = 500_000;

/// A step back: the lines as they were, and where the cursor was.
pub type Step = (Vec<String>, Pos);

/// Where `file`'s history is kept in `dir`, named for its full path.
pub fn path_for(dir: &Path, file: &Path) -> PathBuf {
    dir.join(format!("{:016x}", hash(file.to_string_lossy().as_bytes())))
}

/// The newest of `steps`, oldest first, written out for `file` as it reads now, in `lines`:
/// as many as hold no more than `budget` lines between them.
pub fn encode(file: &Path, lines: &[String], steps: &[(&[String], Pos)], budget: usize) -> String {
    let mut out = format!("{HEADER}\npath {}\ntext {:016x}\n", file.display(), hash_lines(lines));
    let mut newer = lines;
    let mut kept = 0;
    for (older, cursor) in steps.iter().rev() {
        kept += older.len();
        if kept > budget {
            break;
        }
        let hunks = diff::hunks(newer, older);
        out.push_str(&format!("step {} {} {}\n", cursor.row, cursor.col, hunks.len()));
        for h in &hunks {
            out.push_str(&format!("hunk {} {} {}\n", h.old.start, h.old.len(), h.new.len()));
            for line in &older[h.new.clone()] {
                out.push_str(line);
                out.push('\n');
            }
        }
        newer = older;
    }
    // so a file cut short reads as nothing
    out.push_str("end\n");
    out
}

/// The steps written for `file`, oldest first, if they were written when it read as `lines`,
/// up to `budget` lines of them, newest first.
pub fn decode(text: &str, file: &Path, lines: &[String], budget: usize) -> Option<Vec<Step>> {
    let mut rows = text.split('\n');
    if rows.next()? != HEADER || rows.next()?.strip_prefix("path ")? != file.to_string_lossy() {
        return None;
    }
    let want = u64::from_str_radix(rows.next()?.strip_prefix("text ")?, 16).ok()?;
    if want != hash_lines(lines) {
        return None;
    }
    let mut steps = Vec::new();
    let mut newer = lines.to_vec();
    let mut kept = 0;
    loop {
        let row = rows.next()?;
        if row == "end" {
            break;
        }
        let [r, c, n] = numbers(row.strip_prefix("step ")?)?;
        let mut hunks = Vec::new();
        for _ in 0..n {
            let [start, gone, added] = numbers(rows.next()?.strip_prefix("hunk ")?)?;
            let put: Vec<String> = (0..added)
                .map(|_| rows.next().map(str::to_string))
                .collect::<Option<_>>()?;
            hunks.push((start, gone, put));
        }
        // from the last, so the rows of those before it still hold
        let mut older = newer.clone();
        for (start, gone, put) in hunks.into_iter().rev() {
            if start + gone > older.len() {
                return None;
            }
            older.splice(start..start + gone, put);
        }
        kept += older.len();
        if kept > budget {
            break;
        }
        steps.push((older.clone(), Pos::new(r, c)));
        newer = older;
    }
    steps.reverse();
    Some(steps)
}

fn numbers(s: &str) -> Option<[usize; 3]> {
    let mut parts = s.split(' ').map(|p| p.parse().ok());
    let found = [parts.next()??, parts.next()??, parts.next()??];
    parts.next().is_none().then_some(found)
}

fn hash_lines(lines: &[String]) -> u64 {
    hash(lines.join("\n").as_bytes())
}

// FNV-1a, which unlike std's hasher reads the same from one build of tvim to the next
fn hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, &b| {
        (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(s: &str) -> Vec<String> {
        s.split('\n').map(str::to_string).collect()
    }

    #[test]
    fn steps_come_back_as_they_went() {
        let file = Path::new("/tmp/a.txt");
        let steps = vec![
            (lines("one"), Pos::new(0, 0)),
            (lines("one\ntwo"), Pos::new(1, 2)),
            (lines("zero\none\ntwo\n"), Pos::new(0, 3)),
        ];
        let now = lines("zero\none and a bit\ntwo\nthree");
        let steps_ref: Vec<(&[String], Pos)> = steps.iter().map(|(l, p)| (l.as_slice(), *p)).collect();
        let text = encode(file, &now, &steps_ref, KEPT_LINES);
        assert_eq!(decode(&text, file, &now, KEPT_LINES), Some(steps));
    }

    #[test]
    fn a_changed_file_or_another_name_gets_nothing() {
        let file = Path::new("/tmp/a.txt");
        let then = lines("then");
        let text = encode(file, &lines("now"), &[(then.as_slice(), Pos::default())], KEPT_LINES);
        let read = |text: &str, file: &Path, now: &str| decode(text, file, &lines(now), KEPT_LINES);
        assert_eq!(read(&text, file, "changed since"), None);
        assert_eq!(read(&text, Path::new("/tmp/b.txt"), "now"), None);
        assert_eq!(read("garbage", file, "now"), None);
        let cut = &text[..text.len() - 3];
        assert_eq!(read(cut, file, "now"), None, "a file cut short");
    }

    #[test]
    fn a_big_history_keeps_its_newest_steps() {
        let file = Path::new("/tmp/a.txt");
        let steps: Vec<Step> = (1..=5).map(|n| (lines(&"x\n".repeat(n)), Pos::default())).collect();
        let now = lines(&"x\n".repeat(6));
        let steps_ref: Vec<(&[String], Pos)> = steps.iter().map(|(l, p)| (l.as_slice(), *p)).collect();
        // the newest three hold 6, 5 and 4 lines, each with the empty one after its last x
        let text = encode(file, &now, &steps_ref, 15);
        assert_eq!(decode(&text, file, &now, KEPT_LINES), Some(steps[2..].to_vec()));
        let all = encode(file, &now, &steps_ref, KEPT_LINES);
        assert_eq!(
            decode(&all, file, &now, 15),
            Some(steps[2..].to_vec()),
            "and so does reading one"
        );
    }

    #[test]
    fn names_are_kept_apart() {
        let dir = Path::new("/state");
        assert_ne!(path_for(dir, Path::new("/a")), path_for(dir, Path::new("/b")));
        assert_eq!(path_for(dir, Path::new("/a")), path_for(dir, Path::new("/a")));
    }
}
