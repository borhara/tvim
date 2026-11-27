use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::{Choosing, Editor, Input, cwd, listed, relative_to_cwd};
use crate::buffer::Pos;
use crate::grep::Grep;
use crate::languages::{Action, Answer, Ask, Edit, FileEdits, OpenDoc, Place, Problem, Said};
use crate::menu::{Entry, Menu, Picked};

// what a set of edits is doing, to say so: as a verb, done, and being done
pub(super) struct Doing(&'static str, &'static str, &'static str);

pub(super) const RENAMING: Doing = Doing("rename", "renamed", "renaming");
pub(super) const CHANGING: Doing = Doing("change", "changed", "changing");

// how long a write waits for the language server to tidy the file
const FORMAT_WAIT: Duration = Duration::from_secs(2);

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
        // asked for by the server itself, so made whatever has changed since
        for files in update.edits {
            self.apply_edits(files, &[], &CHANGING);
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
            // a server that couldn't complete things still leaves words to offer
            (Ask::Complete, Said::Refused(_)) => self.offer_items(Vec::new()),
            // asked while typing, so not worth stopping for
            (Ask::Signature, Said::Refused(_)) => {}
            (_, Said::Refused(why)) => self.error(why),
            (Ask::Definition, Said::Places(places)) => match places.as_slice() {
                [] => self.error("no definition found"),
                [place] => _ = self.go_to(&place.path, place.start),
                _ => self.list_about("definitions", "of", &places, &answer.read),
            },
            (Ask::References, Said::Places(places)) if places.is_empty() => self.error("no references found"),
            (Ask::References, Said::Places(places)) => self.list_about("references", "to", &places, &answer.read),
            (Ask::Hover, Said::Text(text)) if text.is_empty() => self.error("nothing to say about this"),
            (Ask::Hover, Said::Text(text)) => self.hover = Some(text),
            (Ask::Rename(_), Said::Edits(files)) => self.apply_edits(files, &answer.versions, &RENAMING),
            (Ask::Complete, Said::Items(items)) => self.offer_items(items),
            (Ask::Format { .. }, Said::Edits(files)) => self.formatted(files),
            (Ask::Actions { .. }, Said::Actions(actions)) => self.offer_actions(actions, answer.versions),
            (Ask::Signature, Said::Signature(s)) if self.mode == super::Mode::Insert => self.signature = s,
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

    // asks the language server what it can do with the code at the cursor, or that selected
    pub(crate) fn code_actions(&mut self) {
        let (from, to) = match self.mode.is_visual() {
            true => self.char_span(self.visual_range()),
            false => (self.cursor, self.cursor),
        };
        self.ask_server(Ask::Actions { from, to });
    }

    fn offer_actions(&mut self, actions: Vec<Action>, versions: Vec<(usize, u64)>) {
        if actions.is_empty() {
            return self.info("nothing to do here");
        }
        let entries = actions
            .iter()
            .map(|a| Entry {
                label: a.title.clone(),
                hint: a.kind.clone(),
                disabled: a.disabled.clone(),
            })
            .collect();
        self.reset_for_picker();
        self.menu = Some((Menu::new("code actions", entries), Choosing::Actions(actions, versions)));
    }

    pub(super) fn menu_input(&mut self, input: Input) {
        let (Input::Key(key), Some((menu, _))) = (input, self.menu.as_mut()) else {
            return;
        };
        match menu.key(key) {
            Picked::Stay => {}
            Picked::Close => self.menu = None,
            Picked::Chose(i) => {
                let Some((_, choosing)) = self.menu.take() else { return };
                match choosing {
                    Choosing::Actions(mut actions, versions) => self.act(actions.swap_remove(i), &versions),
                }
            }
        }
    }

    // makes the action's edits, then has the server run its command, if it has one
    fn act(&mut self, action: Action, versions: &[(usize, u64)]) {
        if !action.edits.is_empty() {
            self.apply_edits(action.edits, versions, &CHANGING);
        }
        if let Some(command) = action.command {
            self.sync_languages();
            if let Err(why) = self.languages.run(self.buffer_id, &command) {
                self.error(why);
            }
        }
    }

    pub fn signature(&self) -> Option<&crate::languages::Signature> {
        self.signature.as_ref()
    }

    pub fn menu(&self) -> Option<&Menu> {
        self.menu.as_ref().map(|(m, _)| m)
    }

    // asks the language server to tidy the file, or only these lines, as one change
    pub(crate) fn format(&mut self, lines: Option<(usize, usize)>) {
        let ask = Ask::Format {
            lines,
            tab_size: self.options.shift_width,
            spaces: self.options.expand_tab,
        };
        self.ask_server(ask);
    }

    // with formatonsave, asks the language server to tidy the file and waits a moment for it,
    // so what's written is tidy; a server that's slow or can't is left out of it
    pub(super) fn format_before_writing(&mut self) {
        if !self.options.lsp || !self.options.format_on_save {
            return;
        }
        self.sync_languages();
        let ask = Ask::Format {
            lines: None,
            tab_size: self.options.shift_width,
            spaces: self.options.expand_tab,
        };
        if self.languages.ask(self.buffer_id, self.cursor, ask.clone()).is_err() {
            return;
        }
        let deadline = Instant::now() + FORMAT_WAIT;
        while self.languages.waiting(self.buffer_id, &ask) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(2));
            self.sync_languages();
        }
        // so it isn't tidied after it's written, when it would look changed again
        self.languages.forget(self.buffer_id, &ask);
    }

    fn formatted(&mut self, files: Vec<FileEdits>) {
        let edits: Vec<Edit> = files.into_iter().flat_map(|f| f.edits).collect();
        let before = self.version;
        // a change that changes nothing isn't kept, and leaves the version as it was
        self.edit_text(&edits);
        self.info(if self.version == before {
            "already formatted"
        } else {
            "formatted"
        });
    }

    // makes each file's edits, opening those that aren't open yet, and leaves them unsaved
    pub(super) fn apply_edits(&mut self, files: Vec<FileEdits>, asked: &[(usize, u64)], doing: &Doing) {
        let Doing(verb, done, being) = *doing;
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
            return self.error(format!("{} changed while {being}, so nothing was", listed(&f.path)));
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
            (0, _) if skipped.is_empty() => return self.error(format!("nothing to {verb}")),
            (0, _) => format!("{done} nothing"),
            (1, _) => format!("{done} 1 place"),
            (n, 1) => format!("{done} {n} places"),
            (n, f) => format!("{done} {n} places in {f} files"),
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

    // as definitions of the word under the cursor, say
    fn list_about(&mut self, what: &str, joiner: &str, places: &[Place], read: &HashMap<PathBuf, Vec<String>>) {
        let word = self.word_under_cursor().map_or(String::new(), |(_, w)| w);
        let title = if word.is_empty() {
            what.to_string()
        } else {
            format!("{what} {joiner} {word}")
        };
        self.list_places(&title, &word, places, read);
    }

    // in the grep picker, with the text of open files as it is in the editor and `word` picked
    // out in it
    pub(super) fn list_places(
        &mut self,
        title: &str,
        word: &str,
        places: &[Place],
        read: &HashMap<PathBuf, Vec<String>>,
    ) {
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
        self.grep = Some(Grep::listing(&cwd(), title, word, &spots, open));
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

    // as if Ctrl-n had asked the server, which answered with `items`, for testing
    #[cfg(test)]
    pub(crate) fn suggested(&mut self, items: Vec<crate::languages::Item>) {
        self.suggested_by(items, super::Asking::Typed { back: false });
    }

    // as if typing had asked the server, which answered with `items`, for testing
    #[cfg(test)]
    pub(crate) fn suggested_as_typed(&mut self, items: Vec<crate::languages::Item>) {
        self.suggested_by(items, super::Asking::Auto);
    }

    #[cfg(test)]
    fn suggested_by(&mut self, items: Vec<crate::languages::Item>, asking: super::Asking) {
        self.completion_asked = Some(asking);
        let versions = vec![(self.buffer_id, self.version)];
        self.answered(Answer {
            id: self.buffer_id,
            version: self.version,
            at: self.cursor,
            versions,
            read: HashMap::new(),
            ask: Ask::Complete,
            said: Said::Items(items),
        });
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
