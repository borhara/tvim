use crate::buffer::Pos;
use crate::command::Op;
use crate::editor::{Editor, Effect, Range, expand_home};
use crate::mapping::{self, MapMode, Mapping};
use crate::regex::{Regex, Span};

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
                self.remember_jump();
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
            self.push_effect(Effect::Shell(cmd.trim().to_string()));
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
            self.sync_windows();
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

        if let Some((modes, kind)) = map_command(name, force) {
            return self.map_ex(&modes, kind, arg);
        }
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
            if (!needed || self.write(non_empty(arg), force, range)) && !self.close_window() && self.others_saved(force)
            {
                self.set_quit();
            }
        } else if is("quit", 1) || is("qall", 2) || is("quitall", 5) {
            let closed = is("quit", 1) && self.close_window();
            if !closed && self.dirty_guard(force) && self.others_saved(force) {
                self.set_quit();
            }
        } else if is("close", 3) {
            if !self.close_window() {
                self.error("E444: Cannot close last window");
            }
        } else if is("only", 2) {
            self.only_window();
        } else if is("split", 2) || is("vsplit", 2) || is("new", 3) || is("vnew", 3) {
            let fresh = is("new", 3) || is("vnew", 3);
            if self.split_window(name.starts_with('v'), fresh && arg.is_empty()) {
                if let Some(p) = non_empty(arg) {
                    let target = std::path::PathBuf::from(expand_home(p));
                    if let Err(e) = self.open_path(&target) {
                        self.error(format!("\"{}\" {e}", target.display()));
                    }
                }
            }
        } else if is("bnext", 2) {
            self.cycle_buffer(1);
        } else if is("bprevious", 2) || is("bNext", 2) {
            self.cycle_buffer(-1);
        } else if is("bfirst", 2) || is("brewind", 2) {
            self.switch_to(0);
        } else if is("blast", 2) {
            self.switch_to(self.buffers().len() - 1);
        } else if is("bdelete", 2) || is("bwipeout", 2) || is("bunload", 3) {
            self.close_buffer(force);
        } else if is("buffers", 7) || is("ls", 2) {
            self.list_buffers();
        } else if is("buffer", 1) {
            self.pick_buffer(arg);
        } else if is("grep", 2) || is("Grep", 2) || is("vimgrep", 3) {
            let query = arg.trim_matches('/');
            self.grep(non_empty(query));
        } else if is("find", 3) || is("Files", 2) || is("files", 3) {
            self.find_files(non_empty(arg));
        } else if is("Lexplore", 2) || is("lexplore", 3) {
            self.toggle_explorer();
        } else if is("preview", 4) {
            self.toggle_preview();
        } else if is("git", 3) || is("Git", 3) {
            self.open_git_menu(non_empty(arg));
        } else if is("recent", 3) {
            self.pick_recent();
        } else if is("zen", 3) {
            self.toggle_zen();
        } else if is("Explore", 2) || is("explore", 3) {
            self.leave_zen();
            let dir = match non_empty(arg) {
                Some(d) => std::path::PathBuf::from(expand_home(d)),
                None => self.file_dir(),
            };
            self.explore(&dir);
        } else if is("edit", 1) {
            self.edit(non_empty(arg), force);
        } else if is("read", 1) {
            let at = if zero {
                0
            } else {
                range.map_or(self.cursor.row, |r| r.1) + 1
            };
            self.read_file(arg, at);
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
            self.keys_noremap(&crate::keys::parse(&format!("{n}J")));
        } else if is("normal", 4) {
            let (a, b) = range.unwrap_or(current);
            let keys = crate::keys::parse(arg);
            // :normal! leaves mappings out
            self.noremap += usize::from(force);
            for row in a..=b.min(self.buf.last_row()) {
                self.set_cursor(Pos::new(row, 0));
                self.keys(&keys);
                self.flush_mapping();
                if matches!(self.mode(), crate::editor::Mode::Insert | crate::editor::Mode::Replace) {
                    self.key(tinsel::app::Key::Esc);
                }
                self.leave_visual_if_any();
            }
            self.noremap -= usize::from(force);
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
            self.push_effect(Effect::Shell(shell));
        } else if is("stop", 2) || is("suspend", 3) {
            self.push_effect(Effect::Suspend);
        } else if is("undo", 1) {
            self.keys_noremap(&[tinsel::app::Key::Char('u')]);
        } else if is("redo", 3) {
            self.keys_noremap(&[tinsel::app::Key::Ctrl('r')]);
        } else if is("version", 2) {
            self.info(format!("tvim {}", env!("CARGO_PKG_VERSION")));
        } else {
            self.error(format!("E492: Not an editor command: {line}"));
        }
        self.end_change();
        self.sync_windows();
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

    fn edit(&mut self, path: Option<&str>, force: bool) {
        if let Some(p) = path {
            let target = std::path::PathBuf::from(expand_home(p));
            let here = self.file().path.as_ref().and_then(|f| crate::paths::canonical(f).ok());
            let same = here.is_some() && crate::paths::canonical(&target).ok() == here;
            if !same {
                if let Err(e) = self.open_path(&target) {
                    self.error(format!("\"{}\" {e}", target.display()));
                }
                return self.reset_after_reload();
            }
        }
        let Some(target) = self.file().path.clone() else {
            return self.error("E32: No file name");
        };
        if !self.dirty_guard(force) {
            return;
        }
        if let Err(e) = self.load(&target) {
            self.error(format!("\"{}\" {e}", target.display()));
        }
        self.reset_after_reload();
    }

    fn others_saved(&mut self, force: bool) -> bool {
        match self.unsaved_elsewhere() {
            Some(name) if !force => {
                self.error(format!("E162: No write since last change for buffer \"{name}\""));
                false
            }
            _ => true,
        }
    }

    fn list_buffers(&mut self) {
        let shown: Vec<String> = self
            .buffers()
            .iter()
            .map(|b| {
                let mark = if b.current { "%" } else { " " };
                let changed = if b.modified { " +" } else { "" };
                format!("{}{mark} {}{changed}", b.id, b.name)
            })
            .collect();
        self.info(shown.join("   "));
    }

    fn pick_buffer(&mut self, arg: &str) {
        let all = self.buffers();
        if arg.is_empty() {
            return;
        }
        let found = match arg.parse::<usize>() {
            Ok(id) => all.iter().position(|b| b.id == id),
            Err(_) => {
                let hits: Vec<usize> = (0..all.len()).filter(|&i| all[i].name.contains(arg)).collect();
                if hits.len() > 1 {
                    return self.error(format!("E93: More than one match for {arg}"));
                }
                hits.first().copied()
            }
        };
        match found {
            Some(i) => self.switch_to(i),
            None => self.error(format!("E94: No matching buffer for {arg}")),
        }
    }

    fn read_file(&mut self, path: &str, at: usize) {
        if path.is_empty() {
            return self.error("E32: No file name");
        }
        let path = expand_home(path);
        match std::fs::read_to_string(&path) {
            Ok(text) => {
                let text = text.replace("\r\n", "\n");
                let lines: Vec<String> = text
                    .strip_suffix('\n')
                    .unwrap_or(&text)
                    .split('\n')
                    .map(str::to_string)
                    .collect();
                let n = lines.len();
                self.touch();
                let at = at.min(self.buf.len());
                self.buf.insert_lines(at, lines);
                self.set_cursor(self.first_non_blank_pos(at));
                self.info(format!("\"{path}\" {n}L, {}B", text.len()));
            }
            Err(e) => self.error(format!("E484: Can't open file {path}: {e}")),
        }
    }

    fn substitute(&mut self, (first, last): (usize, usize), arg: &str) {
        let mut chars = arg.chars();
        let (pat, rep, flags) = match chars.next() {
            None => match self.last_substitute.clone() {
                Some((p, r)) => (p, r, String::new()),
                None => return self.error("E35: No previous regular expression"),
            },
            Some(d) if d.is_alphanumeric() || "\\\"| ".contains(d) => {
                return self.error("E146: Regular expressions can't be delimited by letters");
            }
            Some(d) => {
                let parts = split_unescaped(chars.as_str(), d);
                let pat = parts.first().cloned().unwrap_or_default();
                let rep = parts.get(1).cloned().unwrap_or_default();
                let flags = parts.get(2).cloned().unwrap_or_default();
                (pat, rep, flags)
            }
        };
        let pat = if pat.is_empty() {
            match self.last_search.clone() {
                Some(p) => p,
                None => return self.error("E35: No previous regular expression"),
            }
        } else {
            pat
        };
        let mut re = match Regex::new(&pat) {
            Ok(re) => re,
            Err(e) => return self.bad_pattern(&pat, &e),
        };
        self.last_substitute = Some((pat.clone(), rep.clone()));
        self.last_search = Some(pat.clone());
        let all = flags.contains('g');
        if flags.contains('I') {
            re.set_case(false, false);
        } else if flags.contains('i') {
            re.set_case(true, false);
        } else {
            re.set_case(self.options.ignore_case, self.options.smart_case);
        }
        let quiet = flags.contains('e');
        let mut count = 0;
        let mut changed_lines = 0;
        let mut last_row = None;
        let mut last = last.min(self.buf.last_row());
        let mut row = first;
        while row <= last {
            let chars: Vec<char> = self.buf.line(row).chars().collect();
            let hits = if all {
                re.captures_all(&chars)
            } else {
                re.find_at(&chars, 0).into_iter().collect()
            };
            if hits.is_empty() {
                row += 1;
                continue;
            }
            let mut out = String::new();
            let mut at = 0;
            for groups in &hits {
                let Some((from, to)) = groups[0] else { continue };
                out.extend(&chars[at..from]);
                out.push_str(&expand_replacement(&rep, &chars, groups));
                at = to;
            }
            out.extend(&chars[at..]);
            if count == 0 {
                self.touch();
            }
            count += hits.len();
            changed_lines += 1;
            // lines the replacement splits off push the rest of the range down
            let parts: Vec<String> = out.split('\n').map(str::to_string).collect();
            let added = parts.len() - 1;
            self.buf.replace_lines(row, row, parts);
            last += added;
            row += added;
            last_row = Some(row);
            row += 1;
        }
        match last_row {
            Some(row) => {
                self.search_shown = true;
                self.set_cursor(self.first_non_blank_pos(row));
                if count > 1 || changed_lines > 1 {
                    let s = |n: usize| if n == 1 { "" } else { "s" };
                    self.info(format!(
                        "{count} substitution{} on {changed_lines} line{}",
                        s(count),
                        s(changed_lines)
                    ));
                }
            }
            None if quiet => {}
            None => self.error(format!("E486: Pattern not found: {pat}")),
        }
    }

    fn set(&mut self, arg: &str) {
        if arg.is_empty() {
            let o = &self.options;
            let flag = |on: bool, name: &str| if on { name.to_string() } else { format!("no{name}") };
            let all = [
                flag(o.number, "number"),
                flag(o.relative_number, "relativenumber"),
                flag(o.expand_tab, "expandtab"),
                format!("tabstop={}", o.tab_stop),
                format!("shiftwidth={}", o.shift_width),
                flag(o.ignore_case, "ignorecase"),
                flag(o.smart_case, "smartcase"),
                flag(o.highlight_search, "hlsearch"),
                flag(o.incremental_search, "incsearch"),
                flag(o.auto_indent, "autoindent"),
                format!("scrolloff={}", o.scroll_off),
                format!("timeoutlen={}", o.timeout_len),
                flag(o.syntax, "syntax"),
                flag(o.cursor_line, "cursorline"),
                flag(o.indent_guides, "indentguides"),
                flag(o.auto_pairs, "autopairs"),
                flag(o.smooth, "smooth"),
                flag(o.tab_preview, "tabpreview"),
                flag(o.dim, "dim"),
                flag(o.gitlens, "gitlens"),
                flag(o.roomy, "roomy"),
                flag(o.zero_pad, "zeropad"),
                flag(o.wrap, "wrap"),
            ];
            return self.info(all.join("  "));
        }
        for word in arg.split_whitespace() {
            if let Err(e) = self.set_one(word) {
                return self.error(e);
            }
        }
    }

    fn set_one(&mut self, word: &str) -> Result<(), String> {
        let (name, value) = match word.split_once('=') {
            Some((n, v)) => (n, Some(v)),
            None => (word, None),
        };
        let unknown = || format!("E518: Unknown option: {word}");
        if let Some(v) = value {
            let n: usize = v
                .parse()
                .map_err(|_| format!("E521: Number required after =: {word}"))?;
            let o = &mut self.options;
            match name {
                "tabstop" | "ts" if n > 0 => o.tab_stop = n.min(32),
                "shiftwidth" | "sw" if n > 0 => o.shift_width = n.min(32),
                "scrolloff" | "so" => o.scroll_off = n,
                "timeoutlen" | "tm" => o.timeout_len = n,
                "tabstop" | "ts" | "shiftwidth" | "sw" => {
                    return Err(format!("E487: Argument must be positive: {word}"));
                }
                _ => return Err(unknown()),
            }
            return Ok(());
        }
        if let Some(name) = name.strip_suffix('?') {
            let shown = match name {
                "tabstop" | "ts" => format!("tabstop={}", self.options.tab_stop),
                "shiftwidth" | "sw" => format!("shiftwidth={}", self.options.shift_width),
                "scrolloff" | "so" => format!("scrolloff={}", self.options.scroll_off),
                "timeoutlen" | "tm" => format!("timeoutlen={}", self.options.timeout_len),
                other => {
                    let (name, on) = self.options.flag_mut(other).ok_or_else(unknown)?;
                    if *on { name.to_string() } else { format!("no{name}") }
                }
            };
            self.info(shown);
            return Ok(());
        }
        let (name, how) = if let Some(n) = name.strip_prefix("no") {
            if self.options.flag_mut(n).is_some() {
                (n, Some(false))
            } else {
                (name, Some(true))
            }
        } else if let Some(n) = name.strip_prefix("inv") {
            (n, None)
        } else if let Some(n) = name.strip_suffix('!') {
            (n, None)
        } else {
            (name, Some(true))
        };
        let (_, flag) = self.options.flag_mut(name).ok_or_else(unknown)?;
        *flag = how.unwrap_or(!*flag);
        Ok(())
    }

    fn map_ex(&mut self, modes: &[MapMode], kind: MapKind, arg: &str) {
        let mut arg = arg;
        // these change nothing here, but a vimrc line may well have them
        while let Some(rest) = ["<silent>", "<nowait>", "<unique>", "<buffer>"].iter().find_map(|m| {
            arg.get(..m.len())
                .filter(|a| a.eq_ignore_ascii_case(m))
                .map(|_| &arg[m.len()..])
        }) {
            arg = rest.trim_start();
        }
        let (lhs, rhs) = arg.split_once(char::is_whitespace).unwrap_or((arg, ""));
        let lhs = mapping::parse_keys(lhs);
        match kind {
            MapKind::Clear => modes.iter().for_each(|&m| self.maps.clear(m)),
            MapKind::Unmap if lhs.is_empty() => self.error("E474: Invalid argument"),
            MapKind::Unmap => {
                let mut gone = false;
                for &m in modes {
                    gone |= self.maps.remove(m, &lhs);
                }
                if !gone {
                    self.error("E31: No such mapping");
                }
            }
            MapKind::Map { .. } if rhs.trim().is_empty() => {
                let found: Vec<String> = self.maps.list(modes, &lhs).into_iter().map(mapping::describe).collect();
                if found.is_empty() {
                    self.info("No mapping found");
                } else {
                    self.info(found.join("  ·  "));
                }
            }
            MapKind::Map { noremap } => {
                let rhs = mapping::parse_keys(rhs.trim_start());
                for &mode in modes {
                    self.maps.add(Mapping {
                        mode,
                        lhs: lhs.clone(),
                        rhs: rhs.clone(),
                        noremap,
                    });
                }
            }
        }
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MapKind {
    Map { noremap: bool },
    Unmap,
    Clear,
}

// :map, :nnoremap, :iunmap, :mapclear and the rest, with vim's shortest names
pub(crate) fn map_command(name: &str, bang: bool) -> Option<(Vec<MapMode>, MapKind)> {
    use MapMode::*;
    let (modes, rest) = match name.chars().next()? {
        'n' if !name.starts_with("no") => (vec![Normal], &name[1..]),
        'v' | 'x' => (vec![Visual], &name[1..]),
        'i' => (vec![Insert], &name[1..]),
        'c' => (vec![Command], &name[1..]),
        _ if bang => (vec![Insert, Command], name),
        _ => (vec![Normal, Visual], name),
    };
    // map! and friends are only for the plain names
    if bang && modes.len() == 1 {
        return None;
    }
    let is = |full: &str, min: usize| rest.len() >= min && full.starts_with(rest);
    // nn is :nnoremap but cn is :cnext, and nu is :number
    let first = name.chars().next()?;
    let (map_min, noremap_min, unmap_min) = match first {
        _ if modes.len() == 2 => (3, 2, 3),
        'n' => (1, 1, 2),
        'v' | 'x' => (1, 1, 1),
        _ => (1, 2, 1),
    };
    let kind = if is("mapclear", 4) {
        MapKind::Clear
    } else if is("map", map_min) {
        MapKind::Map { noremap: false }
    } else if is("noremap", noremap_min) {
        MapKind::Map { noremap: true }
    } else if is("unmap", unmap_min) {
        MapKind::Unmap
    } else {
        return None;
    };
    Some((modes, kind))
}

fn is_known(word: &str) -> bool {
    [
        "set", "se", "shell", "sh", "stop", "st", "suspend", "sus", "sp", "spl", "spli", "split",
    ]
    .contains(&word)
}

const HELP: &str =
    "i insert · Esc normal · :w save · :q quit · :wq both · u undo · ^R redo · /text search · :s/a/b/g replace";

fn non_empty(s: &str) -> Option<&str> {
    (!s.is_empty()).then_some(s)
}

fn split_unescaped(s: &str, delim: char) -> Vec<String> {
    let mut parts = vec![String::new()];
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.peek() {
                Some(&n) if n == delim => {
                    parts.last_mut().unwrap().push(n);
                    chars.next();
                }
                Some(&n) => {
                    parts.last_mut().unwrap().push('\\');
                    parts.last_mut().unwrap().push(n);
                    chars.next();
                }
                None => parts.last_mut().unwrap().push('\\'),
            }
        } else if c == delim {
            parts.push(String::new());
        } else {
            parts.last_mut().unwrap().push(c);
        }
    }
    parts
}

fn expand_replacement(rep: &str, line: &[char], groups: &[Option<Span>]) -> String {
    let group = |n: usize| -> String {
        groups
            .get(n)
            .copied()
            .flatten()
            .map_or_else(String::new, |(a, b)| line[a..b].iter().collect())
    };
    let mut out = String::new();
    let mut chars = rep.chars();
    while let Some(c) = chars.next() {
        match c {
            '&' => out.push_str(&group(0)),
            '\\' => match chars.next() {
                Some(d @ '0'..='9') => out.push_str(&group(d as usize - '0' as usize)),
                Some('n' | 'r') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some(other) => out.push(other),
                None => out.push('\\'),
            },
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_on_unescaped_delimiters() {
        assert_eq!(split_unescaped("a\\/b/c/g", '/'), ["a/b", "c", "g"]);
        assert_eq!(split_unescaped("a\\nb#c", '#'), ["a\\nb", "c"]);
        assert_eq!(split_unescaped("x", '/'), ["x"]);
    }

    #[test]
    fn replacements() {
        let line: Vec<char> = "hi there".chars().collect();
        let whole = [Some((0, 2))];
        assert_eq!(expand_replacement("<&>", &line, &whole), "<hi>");
        assert_eq!(expand_replacement("\\&\\n\\\\", &line, &whole), "&\n\\");
        let groups = [Some((0, 8)), Some((3, 8)), None];
        assert_eq!(
            expand_replacement("\\1 \\0|\\2|\\9", &line, &groups),
            "there hi there||"
        );
    }
}
