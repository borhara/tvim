use tinsel::app::Key;

use super::{Dot, Editor, Effect, Input, Kind, Mode, Range};
use crate::buffer::{Buffer, Pos};
use crate::command::{Act, Cmd, Motion, Op, Parse, Target, parse};
use crate::diff::Hunk;
use crate::languages::Ask;
use crate::motion;
use crate::text::{char_count, col_at, first_non_blank, vcol};

impl Editor {
    pub(super) fn normal(&mut self, input: Input) {
        let key = match input {
            Input::Paste(text) => {
                self.pending.clear();
                if !self.mode.is_visual() {
                    self.paste_normal(&text);
                }
                return;
            }
            Input::Key(k) => k,
        };
        if self.pending.is_empty() && !matches!(key, Key::Char(':' | '/' | '?')) {
            self.message = None;
        }
        if key == Key::Char('q') && self.pending.is_empty() && self.recording.is_some() {
            self.stop_recording();
            return;
        }
        if matches!(key, Key::Esc | Key::Ctrl('c')) {
            let had = !self.pending.is_empty();
            self.pending.clear();
            if self.mode.is_visual() {
                self.leave_visual();
            } else if key == Key::Ctrl('c') && !had {
                self.info("Type  :qa!  and press Enter to abandon all changes and exit tvim");
            }
            return;
        }
        self.pending.push(key);
        match parse(&self.pending, self.mode.is_visual()) {
            Parse::More => {}
            Parse::Invalid => self.pending.clear(),
            Parse::Done(cmd) => {
                self.pending.clear();
                if self.mode.is_visual() {
                    // commenting and moving lines change them too, though they repeat their own way
                    let changes = Self::is_visual_change(&cmd)
                        || matches!(cmd.act, Act::Operate(Op::Comment, _) | Act::Key(Key::Alt(_)));
                    if changes && self.refuses_change() {
                        self.mode = Mode::Normal;
                        return;
                    }
                    let change = Self::is_visual_change(&cmd) && !self.replaying;
                    if change {
                        self.building = Some(Dot {
                            count: cmd.count,
                            register: cmd.register,
                            inputs: cmd.keys.iter().map(|&k| Input::Key(k)).collect(),
                            visual: Some(self.visual_size()),
                        });
                    }
                    self.run_visual(cmd);
                    if change && !matches!(self.mode, Mode::Insert | Mode::Replace) {
                        self.dot = self.building.take();
                    }
                } else {
                    self.run(cmd);
                }
            }
        }
    }

    pub(super) fn is_change(cmd: &Cmd) -> bool {
        match cmd.act {
            // the server's tidying comes later, as a change of its own
            Act::Operate(op, _) => !matches!(op, Op::Yank | Op::Filter | Op::Tidy),
            Act::Key(Key::Char(c)) => "xXsSDCpPJ~&iaIAoOR".contains(c),
            Act::Key(Key::Ctrl('a' | 'x')) | Act::Key(Key::Delete) => true,
            Act::With('r', _) | Act::G('J' | 'p' | 'P' | 'i') | Act::Hunk('r') => true,
            Act::Surround(..) | Act::Unsurround(_) | Act::Resurround(..) => true,
            _ => false,
        }
    }

    pub(super) fn run(&mut self, cmd: Cmd) {
        if Self::is_change(&cmd) && self.refuses_change() {
            return;
        }
        let change = Self::is_change(&cmd) && !self.replaying;
        if change {
            self.building = Some(Dot {
                count: cmd.count,
                register: cmd.register,
                inputs: cmd.keys.iter().map(|&k| Input::Key(k)).collect(),
                visual: None,
            });
        }
        let n = cmd.n();
        match cmd.act {
            Act::Move(m) => self.move_cursor(m, cmd.count),
            Act::Operate(op, target) => self.operate(op, target, cmd.count, cmd.register),
            Act::Surround(target, c) => self.surround_target(target, cmd.count, c),
            Act::Unsurround(c) => self.unsurround(c),
            Act::Resurround(old, new) => self.resurround(old, Some(new)),
            Act::Key(k) => self.normal_key(k, cmd.count, cmd.register),
            Act::With(c, ch) => self.normal_with(c, ch, n),
            Act::G('t') => match cmd.count {
                Some(k) => self.switch_to(k.saturating_sub(1)),
                None => self.cycle_buffer(1),
            },
            Act::G('T') => self.cycle_buffer(-(n as isize)),
            Act::G(c) => self.normal_g(c, n, cmd.register),
            Act::Z(k) => self.normal_z(k, cmd.count),
            Act::Select { .. } => {}
            Act::Hunk(_) => self.reset_hunk(),
            Act::Code(_) => self.format(None),
            Act::Window(k) => self.window_key(k, cmd.count),
            Act::Leader(c) => self.leader(c),
        }
        if change && !matches!(self.mode, Mode::Insert | Mode::Replace) {
            self.dot = self.building.take();
        }
    }

    pub(crate) fn leader(&mut self, c: char) {
        match c {
            'f' => self.find_files(None),
            'b' => self.pick_buffer_list(),
            'z' => self.toggle_zen(),
            'm' => self.toggle_preview(),
            'G' => self.open_git_menu(None),
            'r' => self.pick_recent(),
            'n' => self.rename(""),
            _ => self.grep(None),
        }
    }

    pub(super) fn normal_key(&mut self, key: Key, count: Option<usize>, register: Option<char>) {
        let n = count.unwrap_or(1);
        let op = |e: &mut Self, op: Op, m: Motion| e.operate(op, Target::Motion(m), count, register);
        match key {
            Key::Alt(c) => {
                let row = self.cursor.row;
                let by = self.shift_lines(row, row, c == 'j', n);
                self.cursor.row = if c == 'j' { row + by } else { row - by };
            }
            Key::Char('K') => self.ask_server(Ask::Hover),
            Key::Char('x') | Key::Delete => {
                if self.buf.line_len(self.cursor.row) > 0 {
                    op(self, Op::Delete, Motion::Right)
                }
            }
            Key::Char('X') => op(self, Op::Delete, Motion::Left),
            Key::Char('s') => {
                if self.buf.line_len(self.cursor.row) == 0 {
                    self.begin_insert(1, None);
                } else {
                    op(self, Op::Change, Motion::Right)
                }
            }
            Key::Char('S') => self.operate(Op::Change, Target::Lines, count, register),
            Key::Char('D') => op(self, Op::Delete, Motion::LineEnd),
            Key::Char('C') => op(self, Op::Change, Motion::LineEnd),
            Key::Char('Y') => op(self, Op::Yank, Motion::LineEnd),
            Key::Char('p') => self.put(register, n, false),
            Key::Char('P') => self.put(register, n, true),
            Key::Char('J') => self.join(self.cursor.row, n.max(2), true),
            Key::Char('~') => self.toggle_case_chars(n),
            Key::Char('u' | 'U') => self.undo(n),
            Key::Ctrl('r') => self.redo(n),
            Key::Char('.') => self.repeat(count),
            Key::Char('i') => self.begin_insert(n, None),
            Key::Char('a') => {
                if self.buf.line_len(self.cursor.row) > 0 {
                    self.cursor.col += 1;
                }
                self.begin_insert(n, None);
            }
            Key::Char('I') => {
                self.cursor = self.first_non_blank_pos(self.cursor.row);
                self.begin_insert(n, None);
            }
            Key::Char('A') => {
                self.cursor.col = self.buf.line_len(self.cursor.row);
                self.begin_insert(n, None);
            }
            Key::Char('o') => self.open_line(false, n),
            Key::Char('O') => self.open_line(true, n),
            Key::Char('R') => {
                self.begin_insert(n, None);
                self.mode = Mode::Replace;
            }
            Key::Char('v') => self.enter_visual(Mode::Visual),
            Key::Char('V') => self.enter_visual(Mode::VisualLine),
            Key::Char(':') => self.begin_cmdline(':', String::new()),
            Key::Char('/') => self.begin_cmdline('/', String::new()),
            Key::Char('?') => self.begin_cmdline('?', String::new()),
            Key::Char('*') => self.search_word(true),
            Key::Char('#') => self.search_word(false),
            Key::Char('&') => self.ex("s"),
            Key::Ctrl('a') => self.increment(n as i64),
            Key::Ctrl('x') => self.increment(-(n as i64)),
            Key::Ctrl('d') => self.scroll_half(true, count),
            Key::Ctrl('u') => self.scroll_half(false, count),
            Key::Ctrl('f') | Key::PageDown => self.scroll_page(true, n),
            Key::Ctrl('b') | Key::PageUp => self.scroll_page(false, n),
            Key::Ctrl('e') => self.scroll_lines(n as isize),
            Key::Ctrl('y') => self.scroll_lines(-(n as isize)),
            Key::Ctrl('g') => self.file_info(),
            Key::Ctrl('l') => self.message = None,
            Key::Ctrl('z') => self.effects.push(Effect::Suspend),
            Key::Ctrl('o') => self.jump_older(n),
            Key::Ctrl('i') => self.jump_newer(n),
            // a terminal sends Tab for Ctrl-i
            Key::Tab if self.jumps.has_newer() => self.jump_newer(n),
            Key::Tab => self.tab_focus(),
            Key::Ctrl('p') => self.find_files(None),
            // terminals often keep Ctrl-q for themselves
            Key::Ctrl('v' | 'q') => self.enter_visual(Mode::VisualBlock),
            _ => {}
        }
    }

    pub(super) fn normal_with(&mut self, c: char, ch: char, n: usize) {
        match c {
            'r' => self.replace_chars(ch, n),
            'm' => {
                if ch.is_ascii_lowercase() || ch == '\'' || ch == '`' {
                    self.marks.insert(if ch == '`' { '\'' } else { ch }, self.cursor);
                }
            }
            'q' => {
                if ch.is_ascii_alphanumeric() || ch == '"' {
                    self.recording = Some((ch, Vec::new()));
                }
            }
            '@' => self.play_macro(ch, n),
            'Z' if ch == 'Z' => self.ex("x"),
            'Z' => self.ex("q!"),
            _ => {}
        }
    }

    pub(super) fn normal_g(&mut self, c: char, n: usize, register: Option<char>) {
        match c {
            'v' => {
                if let Some((anchor, cursor, mode)) = self.last_visual {
                    self.anchor = self.clamp_pos(anchor);
                    self.cursor = self.clamp_pos(cursor);
                    self.mode = mode;
                }
            }
            'J' => self.join(self.cursor.row, n.max(2), false),
            'p' | 'P' => {
                let Some(r) = self.fetch(register) else { return };
                let end = self.put_register(&r, n, c == 'P');
                self.cursor = if r.linewise {
                    Pos::new((end.row + 1).min(self.buf.last_row()), 0)
                } else {
                    end
                };
                self.set_want();
            }
            'i' => {
                if let Some(&p) = self.marks.get(&'^') {
                    let row = p.row.min(self.buf.last_row());
                    self.cursor = Pos::new(row, p.col.min(self.buf.line_len(row)));
                }
                self.begin_insert(n, None);
            }
            'd' => self.ask_server(Ask::Definition),
            'r' => self.ask_server(Ask::References),
            _ => {}
        }
    }

    pub(super) fn normal_z(&mut self, key: Key, count: Option<usize>) {
        if let Some(n) = count {
            self.cursor.row = (n - 1).min(self.buf.last_row());
        }
        let rows = self.text_rows();
        let so = self.scroll_off();
        let row = self.cursor.row;
        let top = if self.options.wrap {
            let below: usize = (row + 1..=(row + so).min(self.buf.last_row()))
                .map(|r| self.line_height(r))
                .sum();
            let room = rows.saturating_sub(self.line_height(row));
            match key {
                Key::Char('z' | '.') => self.top_with_above(row, room / 2),
                Key::Char('t') | Key::Enter => row.saturating_sub(so),
                _ => self.top_with_above(row, room.saturating_sub(below)),
            }
        } else {
            match key {
                Key::Char('z' | '.') => row.saturating_sub(rows / 2),
                Key::Char('t') | Key::Enter => row.saturating_sub(so),
                _ => (row + so + 1).saturating_sub(rows),
            }
        };
        self.top.set(top);
        if matches!(key, Key::Enter | Key::Char('.' | '-')) {
            self.cursor = self.first_non_blank_pos(row);
        }
    }

    pub(crate) fn first_non_blank_pos(&self, row: usize) -> Pos {
        Pos::new(row, first_non_blank(self.buf.line(row)))
    }

    pub(super) fn clamp_pos(&self, p: Pos) -> Pos {
        let row = p.row.min(self.buf.last_row());
        Pos::new(row, p.col.min(self.buf.line_len(row).saturating_sub(1)))
    }

    pub(super) fn set_want(&mut self) {
        self.want = vcol(self.buf.line(self.cursor.row), self.cursor.col, self.options.tab_stop);
    }

    pub(super) fn col_for_want(&self, row: usize) -> usize {
        let line = self.buf.line(row);
        let max = self.max_col(char_count(line));
        if self.want == usize::MAX {
            return max;
        }
        col_at(line, self.want, self.options.tab_stop).min(max)
    }

    pub(super) fn is_jump(m: Motion) -> bool {
        matches!(
            m,
            Motion::FirstLine
                | Motion::LastLine
                | Motion::Bracket
                | Motion::SearchNext(_)
                | Motion::Mark(..)
                | Motion::ScreenTop
                | Motion::ScreenMiddle
                | Motion::ScreenBottom
                | Motion::ParagraphForward
                | Motion::ParagraphBack
                | Motion::Hunk(_)
                | Motion::Problem(_)
        )
    }

    pub(super) fn move_cursor(&mut self, m: Motion, count: Option<usize>) {
        let Some((to, _)) = self.target(m, count, false) else {
            return;
        };
        let jump = Self::is_jump(m);
        if jump {
            self.jump_from(self.cursor);
        }
        self.cursor = to;
        match m {
            Motion::Up | Motion::Down => {}
            Motion::ScreenUp | Motion::ScreenDown if !self.options.wrap => {}
            Motion::LineEnd => self.want = usize::MAX,
            _ => self.set_want(),
        }
        if !self.mode.is_visual() {
            self.clamp();
        }
        if jump {
            self.land(matches!(m, Motion::SearchNext(_)));
        }
    }

    // searches always glow; other jumps only when they leave the screen
    pub(crate) fn land(&self, search: bool) {
        let top = self.top.get();
        if search || !(top..=self.last_shown(top)).contains(&self.cursor.row) {
            self.anim.land();
        }
    }

    pub(crate) fn target(&mut self, m: Motion, count: Option<usize>, for_op: bool) -> Option<(Pos, Kind)> {
        use Kind::*;
        let n = count.unwrap_or(1);
        let p = self.cursor;
        let b = &self.buf;
        let len = b.line_len(p.row);
        let fnb = |row: usize| Pos::new(row, first_non_blank(b.line(row)));
        let repeat = |step: fn(&Buffer, Pos, bool) -> Option<Pos>, big: bool| {
            let mut at = p;
            for _ in 0..n {
                match step(b, at, big) {
                    Some(q) => at = q,
                    None => break,
                }
            }
            (at != p).then_some(at)
        };
        Some(match m {
            Motion::Left => {
                if p.col == 0 {
                    return None;
                }
                (Pos::new(p.row, p.col.saturating_sub(n)), Exclusive)
            }
            Motion::Right => {
                let max = if for_op || self.mode.is_visual() {
                    len
                } else {
                    len.saturating_sub(1)
                };
                if p.col >= max {
                    return None;
                }
                (Pos::new(p.row, (p.col + n).min(max)), Exclusive)
            }
            Motion::Up => {
                if p.row == 0 {
                    return None;
                }
                let row = p.row.saturating_sub(n);
                (Pos::new(row, self.col_for_want(row)), Linewise)
            }
            Motion::ScreenUp | Motion::ScreenDown if !self.options.wrap => {
                let line_wise = if m == Motion::ScreenUp {
                    Motion::Up
                } else {
                    Motion::Down
                };
                return self.target(line_wise, count, for_op);
            }
            Motion::ScreenUp | Motion::ScreenDown => (self.screen_row_move(p, n, m == Motion::ScreenDown)?, Exclusive),
            Motion::Down => {
                if p.row >= b.last_row() {
                    return None;
                }
                let row = (p.row + n).min(b.last_row());
                (Pos::new(row, self.col_for_want(row)), Linewise)
            }
            Motion::LineStart => (Pos::new(p.row, 0), Exclusive),
            Motion::FirstNonBlank => (fnb(p.row), Exclusive),
            Motion::LineEnd => {
                let row = (p.row + n - 1).min(b.last_row());
                (Pos::new(row, b.line_len(row).saturating_sub(1)), Inclusive)
            }
            Motion::LastNonBlank => {
                let row = (p.row + n - 1).min(b.last_row());
                let line = b.line(row);
                let end = char_count(line.trim_end_matches([' ', '\t']));
                (Pos::new(row, end.saturating_sub(1)), Inclusive)
            }
            Motion::WordForward(big) => {
                let mut at = p;
                let mut prev = p;
                for _ in 0..n {
                    prev = at;
                    let next = motion::word_forward(b, at, big);
                    if next == at {
                        break;
                    }
                    at = next;
                }
                if for_op {
                    // vim: dw on the last word stops at the end of the line
                    if at.row > prev.row && (at.col == 0 || at.col <= first_non_blank(b.line(at.row))) {
                        let mut row = at.row - 1;
                        while row > prev.row && b.line(row).trim().is_empty() {
                            row -= 1;
                        }
                        at = Pos::new(row, b.line_len(row));
                    }
                    if at == p {
                        return None;
                    }
                } else if at == p {
                    return None;
                } else if at.col >= b.line_len(at.row) && at.col > 0 {
                    at.col = b.line_len(at.row).saturating_sub(1);
                }
                (at, Exclusive)
            }
            Motion::WordBack(big) => (repeat(motion::word_back, big)?, Exclusive),
            Motion::WordEnd(big) => (repeat(motion::word_end, big)?, Inclusive),
            Motion::WordEndBack(big) => {
                let mut at = p;
                for _ in 0..n {
                    match motion::word_end_back(b, at, big) {
                        Some(q) => at = q,
                        None => {
                            at = Pos::new(0, 0);
                            break;
                        }
                    }
                }
                (at, Inclusive)
            }
            Motion::FirstLine => (fnb(count.map_or(0, |c| c - 1).min(b.last_row())), Linewise),
            Motion::LastLine => (fnb(count.map_or(b.last_row(), |c| c - 1).min(b.last_row())), Linewise),
            Motion::CountLine => (fnb((p.row + n - 1).min(b.last_row())), Linewise),
            Motion::NextLine => {
                if p.row >= b.last_row() {
                    return None;
                }
                (fnb((p.row + n).min(b.last_row())), Linewise)
            }
            Motion::PrevLine => {
                if p.row == 0 {
                    return None;
                }
                (fnb(p.row.saturating_sub(n)), Linewise)
            }
            Motion::Column => {
                let col = col_at(b.line(p.row), n - 1, self.options.tab_stop).min(len.saturating_sub(1));
                (Pos::new(p.row, col), Exclusive)
            }
            Motion::Find(f) => {
                self.last_find = Some(f);
                let col = motion::find_in_line(b.line(p.row), p.col, f, n, false)?;
                (Pos::new(p.row, col), if f.forward { Inclusive } else { Exclusive })
            }
            Motion::RepeatFind(reverse) => {
                let f = self.last_find?;
                let f = if reverse { f.reversed() } else { f };
                let col = motion::find_in_line(b.line(p.row), p.col, f, n, true)?;
                (Pos::new(p.row, col), if f.forward { Inclusive } else { Exclusive })
            }
            Motion::Bracket => {
                if let Some(c) = count {
                    let row = ((c.min(100) * b.len()).div_ceil(100)).saturating_sub(1);
                    return Some((fnb(row), Linewise));
                }
                (motion::match_bracket(b, p)?, Inclusive)
            }
            Motion::ParagraphForward => {
                let mut at = p;
                for _ in 0..n {
                    at = motion::paragraph_forward(b, at);
                }
                if !for_op && at.col > 0 {
                    at.col -= 1;
                }
                (at, Exclusive)
            }
            Motion::ParagraphBack => {
                let mut at = p;
                for _ in 0..n {
                    at = motion::paragraph_back(b, at);
                }
                (at, Exclusive)
            }
            Motion::ScreenTop | Motion::ScreenMiddle | Motion::ScreenBottom => {
                let top = self.top.get();
                let bottom = self.last_shown(top);
                let so = self.scroll_off();
                let row = match m {
                    Motion::ScreenTop => {
                        let off = if top == 0 { 0 } else { so };
                        (top + (n - 1).max(off)).min(bottom)
                    }
                    Motion::ScreenBottom => {
                        let off = if bottom == b.last_row() { 0 } else { so };
                        bottom.saturating_sub((n - 1).max(off)).max(top)
                    }
                    _ => top + (bottom - top) / 2,
                };
                (fnb(row), Linewise)
            }
            Motion::SearchNext(reverse) => {
                let Some(pat) = self.last_search.clone() else {
                    self.error("E35: No previous regular expression");
                    return None;
                };
                let forward = self.search_backward == reverse;
                let mut at = p;
                for _ in 0..n {
                    at = self.find(&pat, at, forward, true)?;
                }
                self.search_shown = true;
                (at, Exclusive)
            }
            Motion::Hunk(forward) => {
                let rows = self.change_hunks().iter().map(Hunk::row).collect::<Vec<_>>();
                // like vim: no wrapping, and a big count stops at the last one
                let row = if forward {
                    rows.iter().filter(|&&r| r > p.row).take(n).last()
                } else {
                    rows.iter().rev().filter(|&&r| r < p.row).take(n).last()
                };
                (fnb((*row?).min(b.last_row())), Linewise)
            }
            Motion::Problem(forward) => {
                let starts = self.problems(self.buffer_id).iter().map(|p| p.start);
                // a server may still be on older text, so its places are kept inside this one
                let mut starts: Vec<Pos> = starts
                    .map(|s| {
                        let row = s.row.min(b.last_row());
                        Pos::new(row, s.col.min(b.line_len(row).saturating_sub(1)))
                    })
                    .collect();
                // and kept in order, since those pulled in can land before the rest
                starts.sort();
                starts.dedup();
                let at = if forward {
                    starts.iter().filter(|&&s| s > p).take(n).last()
                } else {
                    starts.iter().rev().filter(|&&s| s < p).take(n).last()
                };
                (*at?, Exclusive)
            }
            Motion::Mark(c, exact) => {
                let c = if c == '`' { '\'' } else { c };
                let mut mark = self.marks.get(&c).copied();
                // '' where we already are goes to the last jump from somewhere else
                if c == '\'' && mark.is_none_or(|m| m.row == p.row && (!exact || m.col == p.col)) {
                    mark = self.jumps.latest_elsewhere(self.buffer_id, p.row).or(mark);
                }
                let Some(at) = mark else {
                    self.error("E20: Mark not set");
                    return None;
                };
                let at = self.clamp_pos(at);
                if exact {
                    (at, Exclusive)
                } else {
                    (fnb(at.row), Linewise)
                }
            }
        })
    }

    pub(super) fn range_to(&self, to: Pos, kind: Kind) -> Range {
        let from = self.cursor;
        let (a, b) = if to < from { (to, from) } else { (from, to) };
        match kind {
            Kind::Linewise => Range::lines(a.row, b.row),
            Kind::Inclusive => Range {
                start: a,
                end: Pos::new(b.row, (b.col + 1).min(self.buf.line_len(b.row))),
                linewise: false,
            },
            Kind::Exclusive => {
                // vim's exclusive motion rules, see :h exclusive
                if b.col == 0 && b.row > a.row {
                    if a.col <= first_non_blank(self.buf.line(a.row)) {
                        return Range::lines(a.row, b.row - 1);
                    }
                    let row = b.row - 1;
                    return Range {
                        start: a,
                        end: Pos::new(row, self.buf.line_len(row)),
                        linewise: false,
                    };
                }
                Range {
                    start: a,
                    end: b,
                    linewise: false,
                }
            }
        }
    }
}
