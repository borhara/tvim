use tinsel::app::Key;

use super::{Editor, Input, Mode, Register};

impl Editor {
    pub(super) fn repeat(&mut self, count: Option<usize>) {
        let Some(dot) = self.dot.clone() else { return };
        let mut inputs = Vec::new();
        if let Some(r) = dot.register {
            inputs.push(Input::Key(Key::Char('"')));
            inputs.push(Input::Key(Key::Char(r)));
        }
        if let Some(n) = count.or(dot.count) {
            inputs.extend(n.to_string().chars().map(|c| Input::Key(Key::Char(c))));
        }
        inputs.extend(dot.inputs.iter().cloned());
        self.replaying = true;
        self.end_change();
        if let Some(v) = dot.visual {
            self.select_again(v);
        }
        for i in inputs {
            self.input(i);
        }
        if matches!(self.mode, Mode::Insert | Mode::Replace) {
            self.input(Input::Key(Key::Esc));
        }
        self.replaying = false;
        if let Some(d) = self.dot.as_mut() {
            if count.is_some() {
                d.count = count;
            }
        }
    }

    pub(super) fn play_macro(&mut self, name: char, n: usize) {
        let name = if name == '@' {
            match self.last_macro {
                Some(m) => m,
                None => {
                    self.error("E748: No previously used register");
                    return;
                }
            }
        } else {
            name
        };
        if name == ':' {
            self.last_macro = Some(name);
            if let Some(last) = self.cmd_history.last().cloned() {
                for _ in 0..n {
                    self.ex(&last);
                }
            }
            return;
        }
        let Some(inputs) = self.macros.get(&name.to_ascii_lowercase()).cloned().or_else(|| {
            self.registers
                .get(&name.to_ascii_lowercase())
                .map(|r| crate::keys::parse(&r.text).into_iter().map(Input::Key).collect())
        }) else {
            return;
        };
        if self.macro_depth > 20 {
            self.error("E169: Command too recursive");
            return;
        }
        self.last_macro = Some(name);
        self.macro_depth += 1;
        self.end_change();
        for _ in 0..n {
            for i in &inputs {
                self.input(i.clone());
            }
        }
        self.macro_depth -= 1;
    }

    pub(super) fn stop_recording(&mut self) {
        if let Some((name, mut keys)) = self.recording.take() {
            // drop the q that ended the recording
            keys.pop();
            let text: String = keys
                .iter()
                .map(|i| match i {
                    Input::Key(k) => k.to_string(),
                    Input::Paste(t) => t.clone(),
                })
                .collect();
            // qA adds to what a already holds
            let (mut text, mut keys) = (text, keys);
            if name.is_ascii_uppercase() {
                let lower = name.to_ascii_lowercase();
                let old = self.registers.get(&lower).map_or(String::new(), |r| r.text.clone());
                let mut old_keys = self
                    .macros
                    .get(&lower)
                    .cloned()
                    .unwrap_or_else(|| crate::keys::parse(&old).into_iter().map(Input::Key).collect());
                old_keys.append(&mut keys);
                keys = old_keys;
                text = old + &text;
            }
            let name = name.to_ascii_lowercase();
            if name.is_ascii_alphabetic() {
                self.registers.insert(
                    name,
                    Register {
                        text,
                        ..Register::default()
                    },
                );
            }
            self.macros.insert(name, keys);
        }
    }
}
