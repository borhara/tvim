use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::{Editor, cwd, listed, relative_to_cwd};
use crate::buffer::Pos;
use crate::grep::Grep;
use crate::languages::{Answer, Ask, Edit, FileEdits, OpenDoc, Place, Problem, Said};

impl Editor {
    // tells the language servers what changed in the open files, and takes in what they found
    pub fn sync_languages(&mut self) {
        let mut docs = Vec::new();
        if self.options.lsp {
            docs.push(OpenDoc {
                id: self.buffer_id,
                path: self.file.path.as_deref(),
                version: self.version,
                saved: self.saved,
                lines: self.buf.lines(),
                final_newline: self.file.final_newline,
            });
            let others = self.docs.iter().enumerate().filter(|&(i, _)| i != self.current);
            docs.extend(others.map(|(_, d)| OpenDoc {
                id: d.id,
                path: d.file.path.as_deref(),
                version: d.version,
                saved: d.saved,
                lines: d.buf.lines(),
                final_newline: d.file.final_newline,
            }));
        }
        let update = self.languages.update(&docs, &self.servers);
        if !update.errors.is_empty() {
            self.error(update.errors.join("; "));
        }
        for answer in update.answers {
            self.answered(answer);
        }
    }

    // asks the file's language server about the cursor's place
    pub(super) fn ask_server(&mut self, ask: Ask) {
        if !self.options.lsp {
            return self.error("language servers are off (:set lsp)");
        }
        // so the server has the text as it is now
        self.sync_languages();
        if let Err(why) = self.languages.ask(self.buffer_id, self.cursor, ask) {
            self.error(why);
        }
    }

    pub(crate) fn answered(&mut self, answer: Answer) {
        // asked about somewhere the cursor has since left, or text that has since changed
        if answer.id != self.buffer_id || answer.version != self.version || answer.at != self.cursor {
            return;
        }
        match (answer.ask, answer.said) {
            (_, Said::Refused(why)) => self.error(why),
            (Ask::Definition, Said::Places(places)) => match places.as_slice() {
                [] => self.error("no definition found"),
                [place] => _ = self.go_to(&place.path, place.start),
                _ => self.list_places("definitions", "of", &places, &answer.read),
            },
            (Ask::References, Said::Places(places)) if places.is_empty() => self.error("no references found"),
            (Ask::References, Said::Places(places)) => self.list_places("references", "to", &places, &answer.read),
            (Ask::Hover, Said::Text(text)) if text.is_empty() => self.error("nothing to say about this"),
            (Ask::Hover, Said::Text(text)) => self.hover = Some(text),
            (Ask::Rename(_), Said::Edits(files)) => self.apply_edits(files, &answer.versions),
            _ => {}
        }
    }

    // renames the word under the cursor everywhere the language server says it's used
    pub(crate) fn rename(&mut self, to: &str) {
        let to = to.trim();
        if to.is_empty() {
            let word = self.word_under_cursor().map_or(String::new(), |(_, w)| w);
            return self.begin_cmdline(':', format!("rename {word}"));
        }
        self.ask_server(Ask::Rename(to.to_string()));
    }

    // makes each file's edits, opening those that aren't open yet, and leaves them unsaved
    pub(super) fn apply_edits(&mut self, files: Vec<FileEdits>, asked: &[(usize, u64)]) {
        // an open file changed since asking would take the edits in the wrong places
        let open = std::iter::once((self.buffer_id, self.version, self.file.path.as_deref())).chain(
            (self.docs.iter().enumerate())
                .filter(|&(i, _)| i != self.current)
                .map(|(_, d)| (d.id, d.version, d.file.path.as_deref())),
        );
        let changed: Vec<PathBuf> = open
            .filter(|&(id, version, _)| asked.iter().any(|&(i, v)| i == id && v != version))
            .filter_map(|(_, _, path)| path.map(crate::paths::absolute))
            .collect();
        if let Some(f) = files
            .iter()
            .find(|f| changed.contains(&crate::paths::absolute(&f.path)))
        {
            return self.error(format!("{} changed while renaming, so nothing was", listed(&f.path)));
        }
        // in full rather than canonical, as a file not saved yet has no canonical path
        let back = self.current;
        let here = self.file.path.as_deref().map(crate::paths::absolute);
        let (mut changed, mut touched, mut elsewhere) = (0, 0, 0);
        // files that couldn't be opened, said at the end so the count doesn't hide them
        let mut skipped = Vec::new();
        self.no_jumps = true;
        for f in files.iter().filter(|f| !f.edits.is_empty()) {
            if here.is_some() && Some(crate::paths::absolute(&f.path)) == here {
                self.switch_to(back);
            } else {
                let path = relative_to_cwd(&f.path);
                match self.open_path(&path) {
                    Ok(()) => elsewhere += 1,
                    Err(e) => {
                        skipped.push(format!("\"{}\" {e}", path.display()));
                        continue;
                    }
                }
            }
            self.edit_text(&f.edits);
            changed += f.edits.len();
            touched += 1;
        }
        self.switch_to(back);
        self.no_jumps = false;
        let mut said = match (changed, touched) {
            (0, _) if skipped.is_empty() => return self.error("nothing to rename"),
            (0, _) => "renamed nothing".to_string(),
            (1, _) => "renamed 1 place".to_string(),
            (n, 1) => format!("renamed {n} places"),
            (n, f) => format!("renamed {n} places in {f} files"),
        };
        if !skipped.is_empty() {
            return self.error(format!("{said}, but couldn't open {}", skipped.join("; ")));
        }
        if elsewhere > 0 {
            said.push_str(", :wa saves them all");
        }
        self.info(said);
    }

    // as one change, keeping the cursor on the same text
    pub(super) fn edit_text(&mut self, edits: &[Edit]) {
        let lines = self.buf.lines();
        let mut starts = Vec::with_capacity(lines.len());
        let mut at = 0;
        for l in lines {
            starts.push(at);
            at += l.len() + 1;
        }
        let mut text = lines.join("\n");
        let offset = |p: Pos| match lines.get(p.row) {
            Some(line) => starts[p.row] + crate::text::byte_at(line, p.col),
            None => text.len(),
        };
        let mut spans: Vec<(usize, usize, usize, &str)> = edits
            .iter()
            .enumerate()
            .map(|(i, e)| (offset(e.start), offset(e.end), i, e.text.as_str()))
            .filter(|(a, b, _, _)| a <= b)
            .collect();
        // the last given goes in first, so text put at one place keeps the order it came in
        spans.sort_by_key(|&(a, b, i, _)| std::cmp::Reverse((a, b, i)));
        let mut cursor = offset(self.cursor);
        // from the end, so places yet to come stay put; any overlapping one already made is skipped
        let mut done = usize::MAX;
        for (from, to, _, new) in spans {
            if to > done {
                continue;
            }
            text.replace_range(from..to, new);
            if cursor >= to {
                cursor = cursor + new.len() - (to - from);
            } else if cursor > from {
                cursor = from;
            }
            done = from;
        }
        let row = text[..cursor].matches('\n').count();
        let line_start = text[..cursor].rfind('\n').map_or(0, |i| i + 1);
        let col = text[line_start..cursor].chars().count();
        self.touch();
        self.buf.replace_all(text.split('\n').map(str::to_string).collect());
        self.end_change();
        self.cursor = Pos::new(row, col);
        self.clamp();
        self.set_want();
    }

    // in the grep picker, with the text of open files as it is in the editor
    pub(super) fn list_places(
        &mut self,
        what: &str,
        joiner: &str,
        places: &[Place],
        read: &HashMap<PathBuf, Vec<String>>,
    ) {
        let word = self.word_under_cursor().map_or(String::new(), |(_, w)| w);
        // each path in full once, rather than for every comparison while sorting
        let mut spots: Vec<(PathBuf, usize, usize)> = places
            .iter()
            .map(|p| (crate::paths::absolute(&p.path), p.start.row, p.start.col))
            .collect();
        // this file's first, then the others by name
        let here = self.file.path.as_deref().map(crate::paths::absolute);
        spots.sort_by(|(p, row, col), (q, r, c)| {
            (Some(p) != here.as_ref(), p, row, col).cmp(&(Some(q) != here.as_ref(), q, r, c))
        });
        spots.dedup();
        let title = if word.is_empty() {
            what.to_string()
        } else {
            format!("{what} {joiner} {word}")
        };
        self.reset_for_picker();
        // an open file's text as it is in the editor, joined only for files that are listed, and
        // others as the server's answer read them
        let open = |path: &Path| {
            let full = |p: &Path| crate::paths::absolute(p) == path;
            let (lines, final_newline) = if self.file.path.as_deref().is_some_and(full) {
                (self.buf.lines(), self.file.final_newline)
            } else {
                let doc = (self.docs.iter().enumerate())
                    .find(|&(i, d)| i != self.current && d.file.path.as_deref().is_some_and(full));
                match doc {
                    Some((_, d)) => (d.buf.lines(), d.file.final_newline),
                    None => return read.get(path).map(|lines| lines.join("\n")),
                }
            };
            let mut text = lines.join("\n");
            if final_newline {
                text.push('\n');
            }
            Some(text)
        };
        self.grep = Some(Grep::listing(&cwd(), &title, &word, &spots, open));
    }

    // opens the file at `to`, remembering where it came from for Ctrl-o, and shows it
    pub(super) fn go_to(&mut self, path: &Path, to: Pos) -> bool {
        self.remember_jump();
        if !self.open_or_report(path) {
            return false;
        }
        let row = to.row.min(self.buf.last_row());
        self.cursor = Pos::new(row, to.col);
        self.clamp();
        self.set_want();
        if !(self.top.get()..=self.last_shown(self.top.get())).contains(&row) {
            self.top.set(row.saturating_sub(self.text_rows() / 3));
        }
        self.land(true);
        true
    }

    pub fn hover(&self) -> Option<&str> {
        self.hover.as_deref()
    }

    pub(crate) fn problems(&self, buffer: usize) -> &[Problem] {
        self.languages.problems(buffer)
    }

    // the worst problem on the cursor's line
    pub fn problem_here(&self) -> Option<&Problem> {
        let row = self.cursor.row;
        self.problems(self.buffer_id)
            .iter()
            .filter(|p| (p.start.row..=p.end.row).contains(&row))
            .min_by_key(|p| p.severity)
    }
}
