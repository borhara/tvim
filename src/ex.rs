use crate::buffer::Pos;
use crate::command::Op;
use crate::editor::{Editor, Range, expand_home};

type Lines = (usize, usize);

impl Editor {
    pub fn ex(&mut self, line: &str) {
        let line = line.trim_start_matches([' ', ':']).trim_end();
        self.message = None;
        if line.is_empty() {
            return;
        }
        let (range, rest) = match self.parse_range(line) {
            Ok(r) => r,
            Err(e) => return self.error(e),
        };
        // :0r reads in above the first line
        let zero = line[..line.len() - rest.len()].trim().parse() == Ok(0);
        let rest = rest.trim_start();
        if rest.is_empty() {
            if let Some((_, last)) = range {
                let row = last.min(self.buf.last_row());
                self.set_cursor(self.first_non_blank_pos(row));
                self.land(false);
            }
            return;
        }
        if let Some(cmd) = rest.strip_prefix('!') {
            if cmd.trim().is_empty() {
                return self.error("E471: Argument required");
            }
            return;
        }
        let mut name_len = rest.find(|c: char| !c.is_ascii_alphabetic()).unwrap_or(rest.len());
        // :s/a/b/ has no space before the delimiter
        if rest.starts_with('s') && !"substitute".starts_with(&rest[..name_len]) && !is_known(&rest[..name_len]) {
            name_len = 1;
        }
        if name_len == 0 && rest.starts_with(['>', '<']) {
            let c = rest.chars().next().unwrap_or('>');
            let times = rest.chars().take_while(|&x| x == c).count();
            let (a, b) = range.unwrap_or((self.cursor.row, self.cursor.row));
            let op = if c == '>' { Op::Indent } else { Op::Dedent };
            for _ in 0..times {
                self.apply(op, Range::lines(a, b), None, 1);
            }
            self.end_change();
            return;
        }
        let (name, mut arg) = rest.split_at(name_len);
        let mut force = false;
        if let Some(a) = arg.strip_prefix('!') {
            force = true;
            arg = a;
        }
        let arg = arg.trim();
        let current = (self.cursor.row, self.cursor.row);
        let is = |full: &str, min: usize| name.len() >= min && full.starts_with(name);

        if is("write", 1) {
            self.write(non_empty(arg), force, range);
        } else if is("update", 2) {
            if self.is_modified() {
                self.write(non_empty(arg), force, range);
            }
        } else if is("wqall", 3) || is("xall", 2) {
            if self.write_all() {
                self.set_quit();
            }
        } else if is("wall", 2) {
            self.write_all();
        } else if is("wq", 2) || is("xit", 1) || is("exit", 3) {
            let needed = is("wq", 2) || self.is_modified() || non_empty(arg).is_some();
            {
                self.set_quit();
            }
        } else if is("quit", 1) || is("qall", 2) || is("quitall", 5) {
        } else if is("close", 3) {
        } else if is("only", 2) {
        } else if is("split", 2) || is("vsplit", 2) || is("new", 3) || is("vnew", 3) {
            let fresh = is("new", 3) || is("vnew", 3);
        } else if is("bnext", 2) {
        } else if is("bprevious", 2) || is("bNext", 2) {
        } else if is("bfirst", 2) || is("brewind", 2) {
        } else if is("blast", 2) {
        } else if is("bdelete", 2) || is("bwipeout", 2) || is("bunload", 3) {
        } else if is("buffers", 7) || is("ls", 2) {
        } else if is("buffer", 1) {
        } else if is("grep", 2) || is("Grep", 2) || is("vimgrep", 3) {
            let query = arg.trim_matches('/');
            self.grep(non_empty(query));
        } else if is("find", 3) || is("Files", 2) || is("files", 3) {
        } else if is("Lexplore", 2) || is("lexplore", 3) {
        } else if is("preview", 4) {
        } else if is("git", 3) || is("Git", 3) {
        } else if is("recent", 3) {
        } else if is("zen", 3) {
        } else if is("Explore", 2) || is("explore", 3) {
            let dir = match non_empty(arg) {
                Some(d) => std::path::PathBuf::from(expand_home(d)),
            };
        } else if is("edit", 1) {
            self.edit(non_empty(arg), force);
        } else if is("read", 1) {
            let at = if zero { 0 } else { range.map_or(self.cursor.row, |r| r.1) + 1 };
        } else if name == "s" || is("substitute", 2) {
            self.substitute(
                range.unwrap_or(current),
                &format!("{}{}", if force { "!" } else { "" }, arg),
            );
        } else if is("delete", 1) {
            let (a, b) = range.unwrap_or(current);
            let reg = arg.chars().next();
            self.apply(Op::Delete, Range::lines(a, b), reg, 1);
        } else if is("yank", 1) {
            let (a, b) = range.unwrap_or(current);
            let reg = arg.chars().next();
            let at = self.cursor;
            self.apply(Op::Yank, Range::lines(a, b), reg, 1);
            self.set_cursor(at);
        } else if is("join", 1) {
            let (a, b) = range.unwrap_or(current);
            let n = if b > a { b - a + 1 } else { 2 };
            self.set_cursor(Pos::new(a, 0));
            self.keys(&crate::keys::parse(&format!("{n}J")));
        } else if is("normal", 4) {
            let (a, b) = range.unwrap_or(current);
            let keys = crate::keys::parse(arg);
            for row in a..=b.min(self.buf.last_row()) {
                self.set_cursor(Pos::new(row, 0));
                self.keys(&keys);
                if matches!(self.mode(), crate::editor::Mode::Insert | crate::editor::Mode::Replace) {
                    self.key(tinsel::app::Key::Esc);
                }
            }
        } else if is("set", 2) {
            self.set(arg);
        } else if is("nohlsearch", 3) {
            self.search_shown = false;
        } else if is("registers", 3) || is("display", 2) {
            self.show_registers();
        } else if is("marks", 4) {
            self.show_marks();
        } else if is("help", 1) {
            self.info(HELP);
        } else if is("shell", 2) {
            let shell = std::env::var("SHELL").unwrap_or_else(|_| "sh".into());
        } else if is("stop", 2) || is("suspend", 3) {
        } else if is("undo", 1) {
            self.key(tinsel::app::Key::Char('u'));
        } else if is("redo", 3) {
            self.key(tinsel::app::Key::Ctrl('r'));
        } else if is("version", 2) {
            self.info(format!("tvim {}", env!("CARGO_PKG_VERSION")));
        } else {
            self.error(format!("E492: Not an editor command: {line}"));
        }
        self.end_change();
    }

    fn parse_range<'a>(&self, line: &'a str) -> Result<(Option<Lines>, &'a str), String> {
        if let Some(rest) = line.strip_prefix('%') {
            return Ok((Some((0, self.buf.last_row())), rest));
        }
        let (first, rest) = self.address(line)?;
        let Some(first) = first else { return Ok((None, line)) };
        let rest_trim = rest.trim_start();
        if let Some(after) = rest_trim.strip_prefix([',', ';']) {
            let (second, rest) = self.address(after.trim_start())?;
            let second = second.unwrap_or(first);
            let (a, b) = if second < first {
                (second, first)
            } else {
                (first, second)
            };
            if b > self.buf.last_row() {
                return Err("E16: Invalid range".into());
            }
            return Ok((Some((a, b)), rest));
        }
        if first > self.buf.last_row() {
            return Ok((Some((self.buf.last_row(), self.buf.last_row())), rest));
        }
        Ok((Some((first, first)), rest))
    }

    fn address<'a>(&self, s: &'a str) -> Result<(Option<usize>, &'a str), String> {
        let digits = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
        let (mut row, mut rest): (Option<isize>, &str) = if digits > 0 {
            let n: isize = s[..digits].parse().map_err(|_| "E16: Invalid range".to_string())?;
            (Some(n - 1), &s[digits..])
        } else if let Some(r) = s.strip_prefix('.') {
            (Some(self.cursor.row as isize), r)
        } else if let Some(r) = s.strip_prefix('$') {
            (Some(self.buf.last_row() as isize), r)
        } else if let Some(r) = s.strip_prefix('\'') {
            let mut chars = r.chars();
            let m = chars.next().ok_or("E20: Mark not set")?;
            let pos = self.marks().get(&m).ok_or("E20: Mark not set")?;
            (Some(pos.row as isize), chars.as_str())
        } else {
            (None, s)
        };
        loop {
            let sign = match rest.chars().next() {
                Some('+') => 1,
                Some('-') => -1,
                _ => break,
            };
            let after = &rest[1..];
            let d = after.find(|c: char| !c.is_ascii_digit()).unwrap_or(after.len());
            let n: isize = if d == 0 { 1 } else { after[..d].parse().unwrap_or(1) };
            row = Some(row.unwrap_or(self.cursor.row as isize) + sign * n);
            rest = &after[d..];
        }
        Ok((row.map(|r| r.max(0) as usize), rest))
    }

    fn show_registers(&mut self) {
        let all = self.registers_list();
        if all.is_empty() {
            return self.info("No registers set");
        }
        let shown: Vec<String> = all
            .iter()
            .map(|(k, r)| {
                let text: String = r.text.replace('\n', "⏎").chars().take(24).collect();
                format!("\"{k} {text}")
            })
            .collect();
        self.info(shown.join("  "));
    }

    fn show_marks(&mut self) {
        let mut marks: Vec<(char, Pos)> = self.marks().iter().map(|(k, v)| (*k, *v)).collect();
        if marks.is_empty() {
            return self.info("No marks set");
        }
        marks.sort();
        let shown: Vec<String> = marks
            .iter()
            .map(|(k, p)| format!("{k} {}:{}", p.row + 1, p.col + 1))
            .collect();
        self.info(shown.join("  "));
    }
}

fn is_known(word: &str) -> bool {
    [
        "set",
        "se",
        "shell",
        "sh",
        "stop",
        "st",
        "suspend",
        "sus",
        "sp",
        "spl",
        "spli",
        "split",
    ]
    .contains(&word)
}

const HELP: &str =
    "i insert · Esc normal · :w save · :q quit · :wq both · u undo · ^R redo · /text search · :s/a/b/g replace";

fn non_empty(s: &str) -> Option<&str> {
    (!s.is_empty()).then_some(s)
}
