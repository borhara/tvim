use tinsel::app::Key;

use super::{Editor, Input, Mode};
use crate::explorer::Explorer;
use crate::mapping::{Lookup, MapMode, Mapping};

impl Editor {
    pub fn keys(&mut self, keys: &[Key]) {
        for &k in keys {
            self.key(k);
        }
    }

    /// Types keys as they are, leaving mappings out.
    pub fn keys_noremap(&mut self, keys: &[Key]) {
        self.noremap += 1;
        self.keys(keys);
        self.noremap -= 1;
    }

    /// Whether alt and `key` comes to the editor as it is, rather than as esc and the key:
    /// Alt-j and Alt-k move lines in the text in normal and visual mode, with nothing typed
    /// before them, and everywhere else alt is esc as usual.
    pub fn takes_alt(&self, key: Key) -> bool {
        matches!(key, Key::Alt('j' | 'k'))
            && (self.mode == Mode::Normal || self.mode.is_visual())
            && self.pending.is_empty()
            && self.git_menu.is_none()
            && self.menu.is_none()
            && self.finder.is_none()
            && self.grep.is_none()
            && !self.explorer_focused()
            && !self.previews.contains_key(&self.focus)
    }

    pub fn key(&mut self, key: Key) {
        self.input(Input::Key(key));
    }

    pub fn paste(&mut self, text: &str) {
        self.input(Input::Paste(text.to_string()));
    }

    pub(super) fn input(&mut self, input: Input) {
        let Input::Key(key) = input else {
            self.resolve_mappings(true);
            return self.feed(input);
        };
        if self.noremap > 0 || self.replaying || self.maps.is_empty() {
            return self.feed(Input::Key(key));
        }
        self.map_typed.push(key);
        self.resolve_mappings(false);
    }

    /// Whether typed keys are being held back in case they start a mapping.
    pub fn mapping_pending(&self) -> bool {
        !self.map_typed.is_empty()
    }

    /// Stops waiting for a longer mapping and runs what the held keys come to.
    pub fn flush_mapping(&mut self) {
        self.resolve_mappings(true);
    }

    pub(super) fn map_mode(&self) -> Option<MapMode> {
        let overlay = self.git_menu.is_some() || self.menu.is_some() || self.finder.is_some() || self.grep.is_some();
        if overlay || self.explorer_focused() && self.mode != Mode::Command {
            return None;
        }
        let fresh = crate::command::awaits_command(&self.pending);
        match self.mode {
            Mode::Normal if self.previews.contains_key(&self.focus) => None,
            Mode::Normal if fresh => Some(MapMode::Normal),
            Mode::Normal if crate::command::awaits_motion(&self.pending) => Some(MapMode::Operator),
            m if m.is_visual() && fresh => Some(MapMode::Visual),
            Mode::Insert | Mode::Replace => Some(MapMode::Insert),
            Mode::Command | Mode::Search => Some(MapMode::Command),
            _ => None,
        }
    }

    // vim's rules, see :h map-typeahead: wait while a longer mapping could match, else take the
    // longest one that does, else let the first key through as it is
    pub(super) fn resolve_mappings(&mut self, timed_out: bool) {
        while !self.map_typed.is_empty() {
            let Some(mode) = self.map_mode().filter(|m| self.maps.has_mode(*m)) else {
                let k = self.map_typed.remove(0);
                self.feed(Input::Key(k));
                continue;
            };
            let found =
                match self.maps.lookup(mode, &self.map_typed) {
                    Lookup::Prefix { .. } if !timed_out => return,
                    Lookup::Exact(m) | Lookup::Prefix { exact: Some(m) } => Some((self.map_typed.len(), m)),
                    Lookup::None | Lookup::Prefix { exact: None } => (1..self.map_typed.len()).rev().find_map(|i| {
                        match self.maps.lookup(mode, &self.map_typed[..i]) {
                            Lookup::Exact(m) | Lookup::Prefix { exact: Some(m) } => Some((i, m)),
                            _ => None,
                        }
                    }),
                };
            match found {
                Some((used, m)) => {
                    let rest: Vec<Key> = self.map_typed.drain(used..).collect();
                    self.map_typed.clear();
                    self.run_mapping(m);
                    self.map_typed.extend(rest);
                }
                None => {
                    let k = self.map_typed.remove(0);
                    self.feed(Input::Key(k));
                }
            }
        }
    }

    pub(super) fn run_mapping(&mut self, m: Mapping) {
        if self.map_depth >= 100 {
            self.map_typed.clear();
            return self.error("E223: Recursive mapping");
        }
        if self.from_map == 0 && self.macro_depth == 0 && !self.insert_replay {
            if let Some((_, keys)) = &mut self.recording {
                keys.extend(m.lhs.iter().map(|&k| Input::Key(k)));
            }
        }
        self.map_depth += 1;
        self.from_map += 1;
        // nmap x xp doesn't map its own x again, see :h recursive_mapping
        let own = if m.rhs.starts_with(&m.lhs) { m.lhs.len() } else { 0 };
        for (i, &k) in m.rhs.iter().enumerate() {
            if m.noremap || i < own {
                self.resolve_mappings(true);
                self.noremap += 1;
                self.feed(Input::Key(k));
                self.noremap -= 1;
            } else {
                self.input(Input::Key(k));
            }
        }
        self.from_map -= 1;
        self.map_depth -= 1;
    }

    pub(super) fn feed(&mut self, input: Input) {
        self.hover = None;
        if self.git_menu.is_some() {
            return self.git_menu_input(input);
        }
        if self.menu.is_some() {
            return self.menu_input(input);
        }
        if self.finder.is_some() {
            return self.finder_input(input);
        }
        if self.grep.is_some() {
            return self.grep_input(input);
        }
        let filtering = self.explorer.as_ref().is_some_and(Explorer::is_filtering);
        if input == Input::Key(Key::Ctrl('p')) && self.explorer_focused() && !filtering && self.mode == Mode::Normal {
            return self.find_files(None);
        }
        if self.explorer_focused() && self.mode != Mode::Command {
            return self.explorer_input(input);
        }
        if self.mode == Mode::Normal && self.previews.contains_key(&self.focus) {
            if let Input::Key(k) = input {
                self.preview_input(k);
            }
            return;
        }
        if let Some((_, keys)) = &mut self.recording {
            if !self.replaying && self.macro_depth == 0 && !self.insert_replay && self.from_map == 0 {
                keys.push(input.clone());
            }
        }
        match self.mode {
            Mode::Normal | Mode::Visual | Mode::VisualLine | Mode::VisualBlock => self.normal(input),
            Mode::Insert | Mode::Replace => self.insert_input(input),
            Mode::Command | Mode::Search => self.cmdline_input(input),
        }
        if (self.mode == Mode::Normal || self.mode.is_visual()) && self.pending.is_empty() {
            self.end_change();
        }
        self.clamp();
        self.sync_windows();
        // a preview sent to another buffer by :e is an ordinary window now
        if self
            .previews
            .get(&self.focus)
            .is_some_and(|p| p.buffer != self.buffer_id)
        {
            self.previews.remove(&self.focus);
        }
    }
}
