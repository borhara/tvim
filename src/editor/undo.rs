use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::{Editor, MAX_UNDO, Mode, Snapshot};
use crate::buffer::Pos;
use crate::diff;

impl Editor {
    /// Whether the open file can't be changed, as a picture can't, saying so if not. Asked
    /// before a change starts, so nothing it would do on the way happens, to registers or
    /// marks; what gets past it is undone in [`end_change`](Self::end_change).
    pub(crate) fn refuses_change(&mut self) -> bool {
        if self.file.image.is_none() {
            return false;
        }
        self.info("images can't be changed");
        true
    }

    pub(crate) fn touch(&mut self) {
        if !self.changing {
            self.undo.push(self.snapshot());
            if self.undo.len() > MAX_UNDO {
                self.undo.remove(0);
            }
            self.redo.clear();
            self.changing = true;
        }
        self.versions += 1;
        self.version = self.versions;
    }

    pub(crate) fn end_change(&mut self) {
        // a picture's empty buffer stays empty, whatever tried to change it, once :g is done
        if self.changing && self.grouped == 0 && self.file.image.is_some() {
            self.changing = false;
            if let Some(s) = self.undo.pop() {
                self.buf.replace_all(s.lines);
                self.version = s.version;
                self.cursor = Pos::default();
            }
            self.redo.clear();
            return self.info("images can't be changed");
        }
        if self.changing && self.grouped == 0 {
            self.changing = false;
            if self.undo.last().is_some_and(|s| s.lines == self.buf.lines()) {
                let s = self.undo.pop().expect("just checked");
                self.version = s.version;
            } else if let Some(s) = self.undo.last() {
                let before = Some(&s.marks);
                follow_marks(
                    &mut self.marks,
                    &mut self.last_visual,
                    &s.lines,
                    self.buf.lines(),
                    before,
                );
            }
        }
    }

    pub(super) fn undo(&mut self, count: usize) {
        for n in 0..count {
            let Some(s) = self.undo.pop() else {
                if n == 0 {
                    self.info("Already at oldest change");
                }
                return;
            };
            let now = self.snapshot();
            self.redo.push(now);
            self.restore(s);
        }
    }

    pub(super) fn redo(&mut self, count: usize) {
        for n in 0..count {
            let Some(s) = self.redo.pop() else {
                if n == 0 {
                    self.info("Already at newest change");
                }
                return;
            };
            let now = self.snapshot();
            self.undo.push(now);
            self.restore(s);
        }
    }

    /// Keeps undo history in `dir` between runs, and takes back what's there for the files
    /// open now.
    pub fn set_undo_dir(&mut self, dir: Option<PathBuf>) {
        self.undo_dir = dir;
        if self.undo.is_empty() && !self.is_modified() {
            self.read_undo();
        }
        let back = self.current;
        let message = self.message.take();
        self.no_jumps = true;
        for i in (0..self.docs.len()).filter(|&i| i != back) {
            if self.docs[i].undo.is_empty() && !self.docs[i].modified() {
                self.switch_to(i);
                self.read_undo();
                self.switch_to(back);
            }
        }
        self.no_jumps = false;
        self.message = message;
    }

    fn undo_path(&self) -> Option<(PathBuf, PathBuf)> {
        let dir = self.undo_dir.as_deref().filter(|_| self.options.undo_file)?;
        let file = crate::paths::absolute(self.file.path.as_deref()?);
        Some((crate::undofile::path_for(dir, &file), file))
    }

    /// Takes back the open file's history, if it was kept when the file read as it does now.
    pub(crate) fn read_undo(&mut self) {
        let Some((at, file)) = self.undo_path() else { return };
        let Ok(text) = std::fs::read_to_string(&at) else { return };
        let Some(steps) = crate::undofile::decode(&text, &file, self.buf.lines(), crate::undofile::KEPT_LINES) else {
            return;
        };
        self.undo = steps
            .into_iter()
            .map(|(lines, cursor)| {
                self.versions += 1;
                Snapshot {
                    lines,
                    cursor,
                    version: self.versions,
                    marks: HashMap::new(),
                }
            })
            .collect();
    }

    /// Keeps the open file's history, as it's written.
    pub(crate) fn write_undo(&mut self) {
        let Some((at, file)) = self.undo_path() else { return };
        if self.undo.is_empty() {
            let _ = std::fs::remove_file(&at);
            return;
        }
        let steps: Vec<_> = self.undo.iter().map(|s| (s.lines.as_slice(), s.cursor)).collect();
        let text = crate::undofile::encode(&file, self.buf.lines(), &steps, crate::undofile::KEPT_LINES);
        if let Err(e) = write_private(&at, &text) {
            self.error(format!("E828: Cannot open undo file for writing: {e}"));
        }
    }

    pub(super) fn snapshot(&self) -> Snapshot {
        Snapshot {
            lines: self.buf.lines().to_vec(),
            cursor: self.cursor,
            version: self.version,
            marks: self.marks.clone(),
        }
    }

    pub(super) fn restore(&mut self, s: Snapshot) {
        let changed = (0..self.buf.len().max(s.lines.len()))
            .find(|&r| self.buf.lines().get(r) != s.lines.get(r))
            .unwrap_or(0);
        follow_marks(&mut self.marks, &mut self.last_visual, self.buf.lines(), &s.lines, None);
        // a mark the change took away with its line comes back with it
        for (k, p) in s.marks {
            self.marks.entry(k).or_insert(p);
        }
        self.buf.replace_all(s.lines);
        self.version = s.version;
        let row = changed.min(self.buf.last_row());
        self.cursor = if s.cursor.row == row {
            s.cursor
        } else {
            self.first_non_blank_pos(row)
        };
        self.mode = Mode::Normal;
    }
}

// marks keep to their lines as lines come and go above them; a letter's mark on a line
// removed goes with it, and the others, like '< and '>, move to the line in its place. Only
// those as they were `before` move, as one set since, like '^ at the end of an insert, is
// already where it belongs.
fn follow_marks<M>(
    marks: &mut HashMap<char, Pos>,
    visual: &mut Option<(Pos, Pos, M)>,
    old: &[String],
    new: &[String],
    before: Option<&HashMap<char, Pos>>,
) {
    if marks.is_empty() && visual.is_none() {
        return;
    }
    let hunks = diff::hunks(old, new);
    let last = new.len().saturating_sub(1);
    let follow = |p: Pos| match diff::follow(&hunks, p.row) {
        Ok(row) => Ok(Pos::new(row, p.col)),
        Err(row) => Err(Pos::new(row.min(last), 0)),
    };
    marks.retain(|&k, p| match follow(*p) {
        _ if before.is_some_and(|b| b.get(&k) != Some(p)) => true,
        Err(_) if k.is_ascii_lowercase() => false,
        Ok(to) | Err(to) => {
            *p = to;
            true
        }
    });
    if let Some((anchor, cursor, _)) = visual {
        *anchor = follow(*anchor).unwrap_or_else(|to| to);
        *cursor = follow(*cursor).unwrap_or_else(|to| to);
    }
}

// the history holds the file's text, so only its owner can read it; written whole, so two
// tvims saving at once never leave half of one
fn write_private(path: &Path, text: &str) -> std::io::Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(dir)?;
    let tmp = path.with_extension(format!("{}.tmp", std::process::id()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    std::io::Write::write_all(&mut options.open(&tmp)?, text.as_bytes())?;
    std::fs::rename(&tmp, path)
}
