use std::path::{Path, PathBuf};

use super::{BufferInfo, Doc, Editor, FileInfo, Mode, display_name, expand_home, relative_to_cwd};
use crate::buffer::{Buffer, Pos};

impl Editor {
    pub(super) fn stash(&mut self) -> Doc {
        Doc {
            id: self.buffer_id,
            buf: std::mem::take(&mut self.buf),
            cursor: self.cursor,
            want: self.want,
            last_visual: self.last_visual.take(),
            undo: std::mem::take(&mut self.undo),
            redo: std::mem::take(&mut self.redo),
            version: self.version,
            saved: self.saved,
            marks: std::mem::take(&mut self.marks),
            file: std::mem::take(&mut self.file),
            saved_lines: std::mem::take(&mut self.saved_lines),
            git: std::mem::take(&mut self.git),
            top: self.top.get(),
            left: self.left.get(),
            states: std::mem::take(&mut self.states),
            diffs: std::mem::take(&mut self.marks_cache),
        }
    }

    pub(super) fn unstash(&mut self, d: Doc) {
        self.buffer_id = d.id;
        self.buf = d.buf;
        self.cursor = d.cursor;
        self.want = d.want;
        self.last_visual = d.last_visual;
        self.undo = d.undo;
        self.redo = d.redo;
        self.version = d.version;
        self.saved = d.saved;
        self.marks = d.marks;
        self.file = d.file;
        self.saved_lines = d.saved_lines;
        self.git = d.git;
        self.git.probe(self.file.path.as_deref());
        self.top.set(d.top);
        self.left.set(d.left);
        self.anim.reset();
        self.changing = false;
        self.states = d.states;
        self.marks_cache = d.diffs;
        self.mode = Mode::Normal;
        self.pending.clear();
        self.completion = None;
    }

    pub fn buffers(&self) -> Vec<BufferInfo> {
        self.docs
            .iter()
            .enumerate()
            .map(|(i, d)| {
                let current = i == self.current;
                let (id, path, modified) = if current {
                    (self.buffer_id, self.file.path.clone(), self.is_modified())
                } else {
                    (d.id, d.file.path.clone(), d.modified())
                };
                BufferInfo {
                    id,
                    name: display_name(path.as_deref()),
                    path,
                    modified,
                    current,
                }
            })
            .collect()
    }

    pub fn places(&self) -> Vec<(PathBuf, Pos)> {
        let mut out = Vec::new();
        for (i, d) in self.docs.iter().enumerate() {
            let (path, pos) = if i == self.current {
                (&self.file.path, self.cursor)
            } else {
                (&d.file.path, d.cursor)
            };
            if let Some(p) = path {
                out.push((p.clone(), pos));
            }
        }
        out
    }

    pub fn buffer_index(&self) -> usize {
        self.current
    }

    pub(crate) fn switch_to(&mut self, index: usize) {
        if index == self.current || index >= self.docs.len() {
            return;
        }
        self.push_jump(self.cursor);
        let here = self.stash();
        self.docs[self.current] = here;
        let there = std::mem::take(&mut self.docs[index]);
        self.unstash(there);
        self.current = index;
        self.clamp();
        self.follow_file();
    }

    pub(crate) fn cycle_buffer(&mut self, by: isize) {
        let n = self.docs.len() as isize;
        let to = (self.current as isize + by).rem_euclid(n) as usize;
        self.switch_to(to);
    }

    pub(super) fn disposable(&self) -> bool {
        let shown_elsewhere = self
            .wins
            .iter()
            .any(|w| w.id != self.focus && w.buffer == self.buffer_id);
        self.file.path.is_none()
            && self.buf.is_empty()
            && self.undo.is_empty()
            && !self.is_modified()
            && !shown_elsewhere
    }

    pub(crate) fn new_buffer(&mut self) {
        let here = self.stash();
        self.docs[self.current] = here;
        self.docs.push(Doc::default());
        self.current = self.docs.len() - 1;
        self.fresh_doc();
    }

    pub(super) fn fresh_doc(&mut self) {
        let id = self.next_id;
        self.next_id += 1;
        self.unstash(Doc { id, ..Doc::default() });
        self.file.final_newline = true;
        self.saved_lines = vec![String::new()];
    }

    pub fn open_path(&mut self, path: &Path) -> std::io::Result<()> {
        if path.is_dir() {
            self.explore(path);
            return Ok(());
        }
        // in full, so a file open but not saved yet is found too
        let full = |p: &Path| Some(crate::paths::absolute(p));
        let want = full(path);
        if self.file.path.as_deref().and_then(full) == want {
            return Ok(());
        }
        let open = (0..self.docs.len())
            .find(|&i| i != self.current && self.docs[i].file.path.as_deref().and_then(full) == want);
        if let Some(i) = open {
            self.switch_to(i);
            return Ok(());
        }
        if self.disposable() {
            self.load(path)?;
            self.restore_place();
            return Ok(());
        }
        self.push_jump(self.cursor);
        let back = self.current;
        let here = self.stash();
        self.docs[self.current] = here;
        self.docs.push(Doc::default());
        self.current = self.docs.len() - 1;
        self.fresh_doc();
        if let Err(e) = self.load(path) {
            self.docs.pop();
            self.current = back;
            let there = std::mem::take(&mut self.docs[back]);
            self.unstash(there);
            return Err(e);
        }
        self.restore_place();
        Ok(())
    }

    pub(super) fn open_or_report(&mut self, path: &Path) -> bool {
        let path = relative_to_cwd(path);
        match self.open_path(&path) {
            Ok(()) => true,
            Err(e) => {
                self.error(format!("\"{}\" {e}", path.display()));
                false
            }
        }
    }

    pub(crate) fn close_buffer(&mut self, force: bool) {
        if self.is_modified() && !force {
            let name = self.name();
            return self.error(format!(
                "E89: No write since last change for buffer \"{name}\" (add ! to override)"
            ));
        }
        self.leave_preview();
        self.remember_place();
        if self.docs.len() == 1 {
            self.fresh_doc();
            self.drop_orphan_windows();
            return;
        }
        let gone = self.current;
        let to = if gone + 1 < self.docs.len() { gone + 1 } else { gone - 1 };
        self.switch_to(to);
        self.docs.remove(gone);
        if gone < self.current {
            self.current -= 1;
        }
        self.drop_orphan_windows();
    }

    pub(crate) fn unsaved_elsewhere(&self) -> Option<String> {
        self.buffers()
            .into_iter()
            .find(|b| b.modified && !b.current)
            .map(|b| b.name)
    }

    pub(crate) fn write_all(&mut self) -> bool {
        let back = self.current;
        let mut ok = true;
        self.no_jumps = true;
        for i in 0..self.docs.len() {
            let modified = if i == self.current {
                self.is_modified()
            } else {
                self.docs[i].modified()
            };
            if modified {
                self.switch_to(i);
                ok &= self.write(None, false, None);
            }
        }
        self.switch_to(back);
        self.no_jumps = false;
        ok
    }

    pub(crate) fn load(&mut self, path: &Path) -> std::io::Result<()> {
        if path.is_dir() {
            self.explore(path);
            return Ok(());
        }
        let display = path.display().to_string();
        match std::fs::read(path) {
            Ok(bytes) if tinsel::image::Image::looks_like(&bytes) => {
                let image = tinsel::image::Image::new(bytes)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
                self.info(format!(
                    "\"{display}\" {}×{} {}",
                    image.width(),
                    image.height(),
                    image.format().name()
                ));
                self.buf = Buffer::new();
                self.file = FileInfo {
                    path: Some(path.to_path_buf()),
                    final_newline: true,
                    image: Some(image),
                    ..FileInfo::default()
                };
            }
            Ok(bytes) => {
                let size = bytes.len();
                // the path goes before it, as the caller says which file
                let text = String::from_utf8(bytes)
                    .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "isn't UTF-8 text"))?;
                let lf = text.matches('\n').count();
                let crlf = lf > 0 && text.matches("\r\n").count() == lf;
                let text = if crlf { text.replace("\r\n", "\n") } else { text };
                let final_newline = text.is_empty() || text.ends_with('\n');
                let body = text.strip_suffix('\n').unwrap_or(&text);
                self.buf = Buffer::from_text(body);
                self.file = FileInfo {
                    path: Some(path.to_path_buf()),
                    crlf,
                    final_newline,
                    new: false,
                    blank_line: text == "\n",
                    image: None,
                };
                let lines = if text.is_empty() { 0 } else { self.buf.len() };
                let mut notes = String::new();
                if crlf {
                    notes.push_str(" [dos]");
                }
                if !final_newline {
                    notes.push_str(" [noeol]");
                }
                self.info(format!("\"{display}\"{notes} {lines}L, {size}B"));
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                self.buf = Buffer::new();
                self.file = FileInfo {
                    path: Some(path.to_path_buf()),
                    crlf: false,
                    final_newline: true,
                    new: true,
                    blank_line: false,
                    image: None,
                };
                self.info(format!("\"{display}\" [New]"));
            }
            Err(e) => return Err(e),
        }
        self.cursor = Pos::default();
        self.saved_lines = self.buf.lines().to_vec();
        self.git.probe(self.file.path.as_deref());
        self.undo.clear();
        self.redo.clear();
        self.versions += 1;
        self.version = self.versions;
        self.saved = self.version;
        self.top.set(0);
        self.left.set(0);
        self.marks.clear();
        self.explorer_focused = false;
        self.mode = Mode::Normal;
        self.follow_file();
        Ok(())
    }

    pub fn contents(&self, rows: Option<(usize, usize)>) -> String {
        let nl = if self.file.crlf { "\r\n" } else { "\n" };
        let lines = match rows {
            Some((a, b)) => &self.buf.lines()[a..=b],
            None => self.buf.lines(),
        };
        let mut out = lines.join(nl);
        let whole = rows.is_none();
        // keep the file's original trailing newline, or lack of one
        if !whole || self.file.final_newline && (!self.buf.is_empty() || self.file.blank_line) {
            out.push_str(nl);
        }
        out
    }

    pub(crate) fn write(&mut self, path: Option<&str>, force: bool, rows: Option<(usize, usize)>) -> bool {
        let target = match path {
            Some(p) => PathBuf::from(expand_home(p)),
            None => match &self.file.path {
                Some(p) => p.clone(),
                None => {
                    self.error("E32: No file name");
                    return false;
                }
            },
        };
        if self.file.image.is_some() {
            self.error(format!(
                "\"{}\" is an image, which tvim shows but doesn't write",
                self.name()
            ));
            return false;
        }
        // in full, so ./a.txt is the a.txt that's open
        let full = crate::paths::absolute(&target);
        let ours = self.file.path.as_deref().map(crate::paths::absolute) == Some(full);
        if !ours && !force && target.exists() {
            self.error("E13: File exists (add ! to override)");
            return false;
        }
        if ours && rows.is_some() && !force {
            self.error("E140: Use ! to write partial buffer");
            return false;
        }
        let contents = self.contents(rows);
        let existed = target.exists();
        if let Err(e) = std::fs::write(&target, &contents) {
            self.error(format!("E212: Can't open file for writing: {e}"));
            return false;
        }
        let new = !existed;
        let named = self.file.path.is_none();
        if named {
            self.file.path = Some(target.clone());
        }
        if (ours || named) && rows.is_none() {
            self.saved = self.version;
            self.saved_lines = self.buf.lines().to_vec();
            *self.marks_cache.borrow_mut() = None;
            self.file.new = false;
            self.git.probe(self.file.path.as_deref());
        }
        self.refresh_tree();
        let lines = if contents.is_empty() {
            0
        } else {
            rows.map_or(self.buf.len(), |(a, b)| b - a + 1)
        };
        let new = if new { " [New]" } else { "" };
        self.info(format!(
            "\"{}\"{new} {lines}L, {}B written",
            target.display(),
            contents.len()
        ));
        true
    }

    pub(super) fn file_info(&mut self) {
        let lines = self.buf.len();
        let pct = (self.cursor.row + 1) * 100 / lines;
        let modified = if self.is_modified() { " [Modified]" } else { "" };
        self.info(format!(
            "\"{}\"{modified} {lines} line{} --{pct}%--",
            self.name(),
            if lines == 1 { "" } else { "s" }
        ));
    }

    pub(crate) fn reset_after_reload(&mut self) {
        self.mode = Mode::Normal;
    }

    pub(crate) fn dirty_guard(&mut self, force: bool) -> bool {
        if self.is_modified() && !force {
            self.error("E37: No write since last change (add ! to override)");
            return false;
        }
        true
    }
}
