use tinsel::app::{Key, TextInput};

use super::{CmdLine, Editor, Input, Mode, remember};
use crate::text::{char_count, is_word};

impl Editor {
    pub(crate) fn begin_cmdline(&mut self, prompt: char, text: String) {
        self.message = None;
        self.cmd = CmdLine {
            cursor: char_count(&text),
            text,
            prompt,
            browse: None,
            origin: self.cursor,
            origin_top: self.top.get(),
            completing: None,
        };
        self.mode = if prompt == ':' { Mode::Command } else { Mode::Search };
    }

    pub(super) fn cmdline_input(&mut self, input: Input) {
        let key = match input {
            Input::Paste(text) => {
                let first = text.lines().next().unwrap_or("");
                let at = crate::text::byte_at(&self.cmd.text, self.cmd.cursor);
                self.cmd.text.insert_str(at, first);
                self.cmd.cursor += char_count(first);
                self.preview_search();
                self.preview_substitute();
                return;
            }
            Input::Key(k) => k,
        };
        if !matches!(key, Key::Tab | Key::BackTab) {
            self.cmd.completing = None;
        }
        let edit = match key {
            Key::Esc | Key::Ctrl('c') => {
                self.cancel_cmdline();
                return;
            }
            Key::Enter | Key::Ctrl('j') => return self.finish_cmdline(),
            Key::Tab if self.mode == Mode::Command => return self.complete(false),
            Key::BackTab if self.mode == Mode::Command => return self.complete(true),
            Key::Backspace | Key::Ctrl('h') if self.cmd.text.is_empty() => {
                self.cancel_cmdline();
                return;
            }
            Key::Up | Key::Down => {
                self.browse_history(key == Key::Up);
                None
            }
            Key::Ctrl('h') => Some(Key::Backspace),
            Key::Ctrl('b') => Some(Key::Home),
            // vim's keys for the line; tinsel's others (Ctrl-A, Ctrl-K) mean something else here
            Key::Char(_)
            | Key::Backspace
            | Key::Delete
            | Key::Ctrl('w' | 'u' | 'e')
            | Key::Left
            | Key::Right
            | Key::Home
            | Key::End => Some(key),
            _ => None,
        };
        if let Some(key) = edit {
            let mut line = TextInput::new().word(is_word);
            line.focus(true);
            line.set_value(&self.cmd.text);
            line.set_cursor(self.cmd.cursor);
            line.edit(&tinsel::app::Event::Key(key));
            self.cmd.text = line.value();
            self.cmd.cursor = line.cursor();
        }
        self.preview_search();
        self.preview_substitute();
    }

    pub(super) fn browse_history(&mut self, back: bool) {
        let history = if self.mode == Mode::Command {
            &self.cmd_history
        } else {
            &self.search_history
        };
        if history.is_empty() {
            return;
        }
        let at = match (self.cmd.browse, back) {
            (None, true) => Some(history.len() - 1),
            (None, false) => return,
            (Some(i), true) => Some(i.saturating_sub(1)),
            (Some(i), false) if i + 1 < history.len() => Some(i + 1),
            (Some(_), false) => None,
        };
        self.cmd.browse = at;
        self.cmd.text = at.map(|i| history[i].clone()).unwrap_or_default();
        self.cmd.cursor = char_count(&self.cmd.text);
    }

    pub(super) fn preview_search(&mut self) {
        if self.mode != Mode::Search || !self.options.incremental_search {
            return;
        }
        self.cursor = self.cmd.origin;
        self.top.set(self.cmd.origin_top);
        if self.cmd.text.is_empty() {
            return;
        }
        let pat = self.cmd.text.clone();
        let message = self.message.take();
        if let Some(p) = self.find(&pat, self.cmd.origin, self.cmd.prompt == '/', true) {
            self.cursor = p;
        }
        self.message = message;
    }

    pub(super) fn cancel_cmdline(&mut self) {
        if self.mode == Mode::Search || self.substitute_preview.take().is_some() {
            self.cursor = self.cmd.origin;
            self.top.set(self.cmd.origin_top);
        }
        self.mode = Mode::Normal;
        self.cmd.text.clear();
        self.message = None;
    }

    pub(super) fn finish_cmdline(&mut self) {
        self.substitute_preview = None;
        let text = std::mem::take(&mut self.cmd.text);
        let prompt = self.cmd.prompt;
        self.mode = Mode::Normal;
        if prompt == ':' {
            remember(&mut self.cmd_history, &text);
            self.ex(&text);
        } else {
            let pat = if text.is_empty() {
                self.last_search.clone().unwrap_or_default()
            } else {
                text.clone()
            };
            if !text.is_empty() {
                remember(&mut self.search_history, &text);
            }
            if pat.is_empty() {
                self.error("E35: No previous regular expression");
                return;
            }
            self.cursor = self.cmd.origin;
            self.last_search = Some(pat.clone());
            self.search_backward = prompt == '?';
            self.search_shown = true;
            match self.find(&pat, self.cmd.origin, prompt == '/', true) {
                Some(p) => {
                    self.jump_from(self.cmd.origin);
                    self.cursor = p;
                    self.set_want();
                    self.land(true);
                    if self.message.is_none() {
                        self.info(format!("{prompt}{pat}"));
                    }
                }
                None => self.top.set(self.cmd.origin_top),
            }
        }
        self.clamp();
    }

    pub fn cmd_suggestions(&self) -> (Vec<crate::suggest::Suggestion>, Option<usize>) {
        if let Some((list, at)) = &self.cmd.completing {
            return (list.clone(), Some(*at));
        }
        let names: Vec<String> = self.buffers().into_iter().map(|b| b.name).collect();
        (crate::suggest::suggestions(&self.cmd.text, &self.options, &names), None)
    }

    pub(super) fn complete(&mut self, back: bool) {
        let (list, at) = match self.cmd.completing.take() {
            Some((list, at)) => {
                let n = list.len();
                let at = if back { (at + n - 1) % n } else { (at + 1) % n };
                (list, at)
            }
            None => {
                let list = self.cmd_suggestions().0;
                if list.is_empty() {
                    return;
                }
                let at = if back { list.len() - 1 } else { 0 };
                (list, at)
            }
        };
        self.cmd.text = list[at].replace.clone();
        self.cmd.cursor = char_count(&self.cmd.text);
        // a folder: next Tab lists inside it
        if list.len() > 1 && !list[at].replace.ends_with('/') {
            self.cmd.completing = Some((list, at));
        }
    }
}
