use tinsel::app::Key;

use crate::motion::Find;
use crate::object::is_object;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motion {
    Left,
    Right,
    Up,
    Down,
    ScreenUp,
    ScreenDown,
    LineStart,
    FirstNonBlank,
    LineEnd,
    LastNonBlank,
    WordForward(bool),
    WordBack(bool),
    WordEnd(bool),
    WordEndBack(bool),
    FirstLine,
    LastLine,
    Find(Find),
    RepeatFind(bool),
    Bracket,
    ParagraphForward,
    ParagraphBack,
    ScreenTop,
    ScreenMiddle,
    ScreenBottom,
    NextLine,
    PrevLine,
    CountLine,
    Column,
    SearchNext(bool),
    Mark(char, bool),
    Hunk(bool),
    Problem(bool),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Delete,
    Change,
    Yank,
    Indent,
    Dedent,
    Lower,
    Upper,
    ToggleCase,
    Comment,
    Filter,
    Format,
    // gw, which leaves the cursor where it was
    FormatKeep,
    // =, which asks the language server to tidy the lines
    Tidy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Motion(Motion),
    Lines,
    Object { kind: char, inner: bool },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Act {
    Move(Motion),
    Operate(Op, Target),
    Key(Key),
    With(char, char),
    G(char),
    Z(Key),
    Select { kind: char, inner: bool },
    Leader(char),
    Hunk(char),
    Window(Key),
    // ys, yss and visual S: put a pair around the target
    Surround(Target, char),
    // ds and cs: take away or change the pair around the cursor
    Unsurround(char),
    Resurround(char, char),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cmd {
    pub register: Option<char>,
    pub count: Option<usize>,
    pub act: Act,
    pub keys: Vec<Key>,
}

impl Cmd {
    pub fn n(&self) -> usize {
        self.count.unwrap_or(1)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Parse {
    More,
    Invalid,
    Done(Cmd),
}

pub fn parse(keys: &[Key], visual: bool) -> Parse {
    let mut i = 0;
    let mut register = None;
    if keys.first() == Some(&Key::Char('"')) {
        match keys.get(1) {
            None => return Parse::More,
            Some(Key::Char(c)) if is_register(*c) => register = Some(*c),
            _ => return Parse::Invalid,
        }
        i = 2;
    }
    let (count1, used) = count(&keys[i..]);
    i += used;
    let start = i;
    let Some(&key) = keys.get(i) else { return Parse::More };
    let done = |act: Act, end: usize, count: Option<usize>| {
        let typed: Vec<Key> = keys[start..end].to_vec();
        Parse::Done(Cmd {
            register,
            count,
            act,
            keys: typed,
        })
    };

    let op = match operator(&keys[i..], visual) {
        Some(Ok(found)) => Some(found),
        Some(Err(())) => return Parse::More,
        None => None,
    };
    if let Some((op, op_len)) = op {
        if visual {
            return done(Act::Operate(op, Target::Lines), i + op_len, count1);
        }
        let op_keys = &keys[i..i + op_len];
        i += op_len;
        let (count2, used) = count(&keys[i..]);
        let count2_at = i;
        i += used;
        let total = match (count1, count2) {
            (None, None) => None,
            (a, b) => Some(a.unwrap_or(1) * b.unwrap_or(1)),
        };
        let finish = |act: Act, end: usize| {
            let mut typed: Vec<Key> = keys[start..count2_at].to_vec();
            typed.extend_from_slice(&keys[count2_at + used..end]);
            Parse::Done(Cmd {
                register,
                count: total,
                act,
                keys: typed,
            })
        };
        let Some(&next) = keys.get(i) else { return Parse::More };
        if op_len == 1 && next == Key::Char('s') && matches!(op, Op::Yank | Op::Delete | Op::Change) {
            return surround(keys, i + 1, op, |act, end, more, digits| {
                // a count after ys, as in ys2w, counts with the others, and so isn't typed
                let count = match (total, more) {
                    (None, None) => None,
                    (a, b) => Some(a.unwrap_or(1) * b.unwrap_or(1)),
                };
                let mut typed: Vec<Key> = keys[start..count2_at].to_vec();
                typed.extend_from_slice(&keys[count2_at + used..digits.start]);
                typed.extend_from_slice(&keys[digits.end..end]);
                Parse::Done(Cmd {
                    register,
                    count,
                    act,
                    keys: typed,
                })
            });
        }
        let last = *op_keys.last().unwrap_or(&next);
        if next == last {
            return finish(Act::Operate(op, Target::Lines), i + 1);
        }
        if op_len == 2 && next == Key::Char('g') {
            match keys.get(i + 1) {
                None => return Parse::More,
                Some(&k) if k == last => return finish(Act::Operate(op, Target::Lines), i + 2),
                _ => {}
            }
        }
        if let Some(p) = text_object(&keys[i..], |kind, inner| {
            finish(Act::Operate(op, Target::Object { kind, inner }), i + 2)
        }) {
            return p;
        }
        return match motion(&keys[i..]) {
            MotionParse::More => Parse::More,
            MotionParse::Invalid => Parse::Invalid,
            MotionParse::Done(m, len) => finish(Act::Operate(op, Target::Motion(m)), i + len),
        };
    }

    if visual && key == Key::Char('S') {
        return match keys.get(i + 1) {
            None => Parse::More,
            Some(Key::Char(c)) if crate::editor::can_surround(*c) => {
                done(Act::Surround(Target::Lines, *c), i + 2, count1)
            }
            _ => Parse::Invalid,
        };
    }

    // gsa, gsd and gsr, the names LazyVim gives ys, ds and cs
    if key == Key::Char('g') && keys.get(i + 1) == Some(&Key::Char('s')) {
        let op = match keys.get(i + 2) {
            None => return Parse::More,
            Some(Key::Char('a')) => Op::Yank,
            Some(Key::Char('d')) if !visual => Op::Delete,
            Some(Key::Char('r')) if !visual => Op::Change,
            _ => return Parse::Invalid,
        };
        if visual {
            return match keys.get(i + 3) {
                None => Parse::More,
                Some(Key::Char(c)) if crate::editor::can_surround(*c) => {
                    done(Act::Surround(Target::Lines, *c), i + 4, count1)
                }
                _ => Parse::Invalid,
            };
        }
        return surround(keys, i + 3, op, |act, end, more, digits| {
            let count = match (count1, more) {
                (None, None) => None,
                (a, b) => Some(a.unwrap_or(1) * b.unwrap_or(1)),
            };
            let mut typed: Vec<Key> = keys[start..digits.start].to_vec();
            typed.extend_from_slice(&keys[digits.end..end]);
            Parse::Done(Cmd {
                register,
                count,
                act,
                keys: typed,
            })
        });
    }

    if visual {
        if let Some(p) = text_object(&keys[i..], |kind, inner| {
            done(Act::Select { kind, inner }, i + 2, count1)
        }) {
            return p;
        }
    }

    if key == Key::Char(' ') && !visual {
        return match keys.get(i + 1) {
            None => Parse::More,
            Some(Key::Char(c)) if "fgb/zmGrn".contains(*c) => done(Act::Leader(*c), i + 2, count1),
            Some(Key::Char('h')) => match keys.get(i + 2) {
                None => Parse::More,
                Some(Key::Char('r')) => done(Act::Hunk('r'), i + 3, count1),
                _ => Parse::Invalid,
            },
            _ => Parse::Invalid,
        };
    }

    if key == Key::Ctrl('w') && !visual {
        let (count2, used) = count(&keys[i + 1..]);
        return match keys.get(i + 1 + used) {
            None => Parse::More,
            Some(&k) => done(Act::Window(k), i + 2 + used, count2.or(count1)),
        };
    }

    match key {
        Key::Char('g') => match keys.get(i + 1) {
            None => return Parse::More,
            Some(Key::Char(c @ ('v' | 'J' | 'i' | 'p' | 'P' | 't' | 'T' | 'd' | 'r'))) => {
                return done(Act::G(*c), i + 2, count1);
            }
            _ => {}
        },
        Key::Char('z') => {
            return match keys.get(i + 1) {
                None => Parse::More,
                Some(&k @ (Key::Char('z' | 't' | 'b' | '.' | '-') | Key::Enter)) => done(Act::Z(k), i + 2, count1),
                _ => Parse::Invalid,
            };
        }
        Key::Char('Z') => {
            return match keys.get(i + 1) {
                None => Parse::More,
                Some(Key::Char(c @ ('Z' | 'Q'))) => done(Act::With('Z', *c), i + 2, count1),
                _ => Parse::Invalid,
            };
        }
        Key::Char(c) if "rmq@".contains(c) => {
            return match keys.get(i + 1) {
                None => Parse::More,
                Some(Key::Char(k)) => done(Act::With(c, *k), i + 2, count1),
                Some(Key::Enter) if c == 'r' => done(Act::With(c, '\n'), i + 2, count1),
                Some(Key::Tab) if c == 'r' => done(Act::With(c, '\t'), i + 2, count1),
                _ => Parse::Invalid,
            };
        }
        _ => {}
    }

    match motion(&keys[i..]) {
        MotionParse::More => return Parse::More,
        MotionParse::Done(m, len) => return done(Act::Move(m), i + len, count1),
        MotionParse::Invalid => {}
    }

    let simple = if visual { VISUAL_KEYS } else { NORMAL_KEYS };
    let ctrl = if visual { "acxvq" } else { "rglaxzcpoivq" };
    match key {
        Key::Char(c) if simple.contains(c) => done(Act::Key(key), i + 1, count1),
        Key::Ctrl(c) if "dufbey".contains(c) || ctrl.contains(c) => done(Act::Key(key), i + 1, count1),
        Key::Delete | Key::PageUp | Key::PageDown => done(Act::Key(key), i + 1, count1),
        Key::Tab if !visual => done(Act::Key(key), i + 1, count1),
        Key::Alt('j' | 'k') => done(Act::Key(key), i + 1, count1),
        _ => Parse::Invalid,
    }
}

const NORMAL_KEYS: &str = "xXsSDCYpPJ~uU.iaIAoORvV:/?*#&K";
const VISUAL_KEYS: &str = "xXsSDCYpPJ~uUoORvV:IA";

/// Whether `keys` are no more than a register and a count, so a new command starts next.
pub fn awaits_command(keys: &[Key]) -> bool {
    let rest = match keys {
        [Key::Char('"')] => return false,
        [Key::Char('"'), Key::Char(c), rest @ ..] if is_register(*c) => rest,
        rest => rest,
    };
    count(rest).1 == rest.len()
}

/// Whether `keys` are an operator waiting for its motion, as `d` or `"a2c3` are.
pub fn awaits_motion(keys: &[Key]) -> bool {
    let rest = match keys {
        [Key::Char('"'), Key::Char(c), rest @ ..] if is_register(*c) => rest,
        rest => rest,
    };
    let rest = &rest[count(rest).1..];
    match operator(rest, false) {
        Some(Ok((_, len))) => count(&rest[len..]).1 == rest.len() - len,
        _ => false,
    }
}

pub fn is_register(c: char) -> bool {
    c.is_ascii_alphanumeric() || "\"-_+*/.".contains(c)
}

/// What follows the s of ys, ds or cs, from `at`: for ys, a count, a target and the pair to
/// put around it; for ds, the pair to take away; and for cs, that and the one to put instead.
/// `make` gets the act, where its keys end, and the count after ys with where its digits are.
fn surround(
    keys: &[Key],
    at: usize,
    op: Op,
    make: impl FnOnce(Act, usize, Option<usize>, std::ops::Range<usize>) -> Parse,
) -> Parse {
    use crate::editor::{can_surround, can_unsurround};
    let char_at = |i: usize, ok: fn(char) -> bool| match keys.get(i) {
        None => Err(Parse::More),
        Some(Key::Char(c)) if ok(*c) => Ok(*c),
        _ => Err(Parse::Invalid),
    };
    match op {
        Op::Delete => match char_at(at, can_unsurround) {
            Ok(c) => make(Act::Unsurround(c), at + 1, None, at..at),
            Err(p) => p,
        },
        Op::Change => match (char_at(at, can_unsurround), char_at(at + 1, can_surround)) {
            (Ok(old), Ok(new)) => make(Act::Resurround(old, new), at + 2, None, at..at),
            (Err(p), _) | (_, Err(p)) => p,
        },
        _ => {
            let (count, used) = count(&keys[at..]);
            let i = at + used;
            let (target, len) = match keys.get(i) {
                None => return Parse::More,
                Some(Key::Char('s')) => (Target::Lines, 1),
                Some(Key::Char(c @ ('i' | 'a'))) => match keys.get(i + 1) {
                    None => return Parse::More,
                    Some(Key::Char(k)) if is_object(*k) => (
                        Target::Object {
                            kind: *k,
                            inner: *c == 'i',
                        },
                        2,
                    ),
                    _ => return Parse::Invalid,
                },
                _ => match motion(&keys[i..]) {
                    MotionParse::More => return Parse::More,
                    MotionParse::Invalid => return Parse::Invalid,
                    MotionParse::Done(m, len) => (Target::Motion(m), len),
                },
            };
            match char_at(i + len, can_surround) {
                Ok(c) => make(Act::Surround(target, c), i + len + 1, count, at..i),
                Err(p) => p,
            }
        }
    }
}

fn text_object(keys: &[Key], make: impl FnOnce(char, bool) -> Parse) -> Option<Parse> {
    let Some(Key::Char(c @ ('i' | 'a'))) = keys.first() else {
        return None;
    };
    Some(match keys.get(1) {
        None => Parse::More,
        Some(Key::Char(k)) if is_object(*k) => make(*k, *c == 'i'),
        _ => Parse::Invalid,
    })
}

fn count(keys: &[Key]) -> (Option<usize>, usize) {
    let mut n: Option<usize> = None;
    let mut used = 0;
    for k in keys {
        match k {
            Key::Char(c @ '0'..='9') if n.is_some() || *c != '0' => {
                let d = *c as usize - '0' as usize;
                n = Some(n.unwrap_or(0).saturating_mul(10).saturating_add(d).min(999_999));
                used += 1;
            }
            _ => break,
        }
    }
    (n, used)
}

fn operator(keys: &[Key], visual: bool) -> Option<Result<(Op, usize), ()>> {
    let op = |o| Some(Ok((o, 1)));
    match keys.first()? {
        Key::Char('d') => op(Op::Delete),
        Key::Char('c') => op(Op::Change),
        Key::Char('y') => op(Op::Yank),
        Key::Char('>') => op(Op::Indent),
        Key::Char('<') => op(Op::Dedent),
        Key::Char('!') => op(Op::Filter),
        Key::Char('=') => op(Op::Tidy),
        Key::Char('u') if visual => op(Op::Lower),
        Key::Char('U') if visual => op(Op::Upper),
        Key::Char('~') if visual => op(Op::ToggleCase),
        Key::Char('g') => match keys.get(1) {
            None => Some(Err(())),
            Some(Key::Char('u')) => Some(Ok((Op::Lower, 2))),
            Some(Key::Char('U')) => Some(Ok((Op::Upper, 2))),
            Some(Key::Char('~')) => Some(Ok((Op::ToggleCase, 2))),
            Some(Key::Char('c')) => Some(Ok((Op::Comment, 2))),
            Some(Key::Char('q')) => Some(Ok((Op::Format, 2))),
            Some(Key::Char('w')) => Some(Ok((Op::FormatKeep, 2))),
            _ => None,
        },
        _ => None,
    }
}

pub enum MotionParse {
    More,
    Invalid,
    Done(Motion, usize),
}

pub fn motion(keys: &[Key]) -> MotionParse {
    use Motion::*;
    let Some(&key) = keys.first() else {
        return MotionParse::More;
    };
    let one = |m| MotionParse::Done(m, 1);
    let with_char = |make: &dyn Fn(char) -> Motion| match keys.get(1) {
        None => MotionParse::More,
        Some(Key::Char(c)) => MotionParse::Done(make(*c), 2),
        Some(Key::Tab) => MotionParse::Done(make('\t'), 2),
        _ => MotionParse::Invalid,
    };
    match key {
        Key::Char('h') | Key::Left | Key::Backspace | Key::Ctrl('h') => one(Left),
        Key::Char('l') | Key::Right | Key::Char(' ') => one(Right),
        Key::Char('j') | Key::Down | Key::Ctrl('j') | Key::Ctrl('n') => one(Down),
        Key::Char('k') | Key::Up => one(Up),
        Key::Char('0') | Key::Home => one(LineStart),
        Key::Char('^') => one(FirstNonBlank),
        Key::Char('$') | Key::End => one(LineEnd),
        Key::Char('w') => one(WordForward(false)),
        Key::Char('W') => one(WordForward(true)),
        Key::Char('b') => one(WordBack(false)),
        Key::Char('B') => one(WordBack(true)),
        Key::Char('e') => one(WordEnd(false)),
        Key::Char('E') => one(WordEnd(true)),
        Key::Char('G') => one(LastLine),
        Key::Char('%') => one(Bracket),
        Key::Char('}') => one(ParagraphForward),
        Key::Char('{') => one(ParagraphBack),
        Key::Char('H') => one(ScreenTop),
        Key::Char('M') => one(ScreenMiddle),
        Key::Char('L') => one(ScreenBottom),
        Key::Enter | Key::Char('+') | Key::Ctrl('m') => one(NextLine),
        Key::Char('-') => one(PrevLine),
        Key::Char('_') => one(CountLine),
        Key::Char('|') => one(Column),
        Key::Char('n') => one(SearchNext(false)),
        Key::Char('N') => one(SearchNext(true)),
        Key::Char(';') => one(RepeatFind(false)),
        Key::Char(',') => one(RepeatFind(true)),
        Key::Char(c @ ('f' | 'F' | 't' | 'T')) => with_char(&|ch| {
            Find(crate::motion::Find {
                forward: c.is_ascii_lowercase(),
                till: c.eq_ignore_ascii_case(&'t'),
                ch,
            })
        }),
        Key::Char(c @ (']' | '[')) => match keys.get(1) {
            None => MotionParse::More,
            Some(Key::Char('c')) => MotionParse::Done(Hunk(c == ']'), 2),
            Some(Key::Char('d')) => MotionParse::Done(Problem(c == ']'), 2),
            _ => MotionParse::Invalid,
        },
        Key::Char('\'') => with_char(&|c| Mark(c, false)),
        Key::Char('`') => with_char(&|c| Mark(c, true)),
        Key::Char('g') => match keys.get(1) {
            None => MotionParse::More,
            Some(Key::Char('g')) => MotionParse::Done(FirstLine, 2),
            Some(Key::Char('e')) => MotionParse::Done(WordEndBack(false), 2),
            Some(Key::Char('E')) => MotionParse::Done(WordEndBack(true), 2),
            Some(Key::Char('_')) => MotionParse::Done(LastNonBlank, 2),
            Some(Key::Char('j') | Key::Down) => MotionParse::Done(ScreenDown, 2),
            Some(Key::Char('k') | Key::Up) => MotionParse::Done(ScreenUp, 2),
            Some(Key::Char('0') | Key::Home) => MotionParse::Done(LineStart, 2),
            Some(Key::Char('$') | Key::End) => MotionParse::Done(LineEnd, 2),
            _ => MotionParse::Invalid,
        },
        _ => MotionParse::Invalid,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::parse as k;

    fn cmd(spec: &str) -> Cmd {
        match parse(&k(spec), false) {
            Parse::Done(c) => c,
            other => panic!("{spec}: {other:?}"),
        }
    }

    fn state(spec: &str) -> Parse {
        parse(&k(spec), false)
    }

    #[test]
    fn motions_with_counts() {
        let c = cmd("12w");
        assert_eq!((c.count, c.act), (Some(12), Act::Move(Motion::WordForward(false))));
        assert_eq!(cmd("0").act, Act::Move(Motion::LineStart));
        assert_eq!(cmd("10j").count, Some(10));
        assert_eq!(cmd("gg").act, Act::Move(Motion::FirstLine));
        assert_eq!(state("g"), Parse::More);
        assert_eq!(state("f"), Parse::More);
        assert!(matches!(cmd("fx").act, Act::Move(Motion::Find(f)) if f.ch == 'x' && f.forward && !f.till));
    }

    #[test]
    fn operators_and_their_targets() {
        let c = cmd("2d3w");
        assert_eq!(c.count, Some(6));
        assert_eq!(
            c.act,
            Act::Operate(Op::Delete, Target::Motion(Motion::WordForward(false)))
        );
        assert_eq!(c.keys, k("dw"));
        assert_eq!(cmd("dd").act, Act::Operate(Op::Delete, Target::Lines));
        assert_eq!(cmd("gUU").act, Act::Operate(Op::Upper, Target::Lines));
        assert_eq!(cmd("gUgU").act, Act::Operate(Op::Upper, Target::Lines));
        assert_eq!(
            cmd("gUgg").act,
            Act::Operate(Op::Upper, Target::Motion(Motion::FirstLine))
        );
        assert_eq!(
            cmd("ci\"").act,
            Act::Operate(Op::Change, Target::Object { kind: '"', inner: true })
        );
        assert_eq!(cmd(">>").act, Act::Operate(Op::Indent, Target::Lines));
        assert_eq!(cmd("!!").act, Act::Operate(Op::Filter, Target::Lines));
        assert_eq!(cmd("gqq").act, Act::Operate(Op::Format, Target::Lines));
        assert_eq!(cmd("==").act, Act::Operate(Op::Tidy, Target::Lines));
        assert_eq!(
            cmd("ysiw)").act,
            Act::Surround(Target::Object { kind: 'w', inner: true }, ')')
        );
        assert_eq!(cmd("yss\"").act, Act::Surround(Target::Lines, '"'));
        let c = cmd("2ys3w]");
        assert_eq!(
            (c.count, c.act),
            (Some(6), Act::Surround(Target::Motion(Motion::WordForward(false)), ']'))
        );
        assert_eq!(c.keys, k("ysw]"));
        let c = cmd("ys2f0)");
        assert_eq!(c.keys, k("ysf0)"), "only the count's digits are left out");
        assert_eq!(cmd("ds(").act, Act::Unsurround('('));
        assert_eq!(cmd("cs\"'").act, Act::Resurround('"', '\''));
        assert_eq!(state("ys"), Parse::More);
        assert_eq!(state("ysiw"), Parse::More);
        assert_eq!(state("ysiwx"), Parse::Invalid);
        assert_eq!(state("cs("), Parse::More);
        assert_eq!(state("dsx"), Parse::Invalid);
        assert_eq!(
            cmd("gsaiw)").act,
            Act::Surround(Target::Object { kind: 'w', inner: true }, ')')
        );
        let c = cmd("gsa2w]");
        assert_eq!((c.count, c.keys), (Some(2), k("gsaw]")));
        assert_eq!(cmd("gsd(").act, Act::Unsurround('('));
        assert_eq!(cmd("gsr\"'").act, Act::Resurround('"', '\''));
        assert_eq!(state("gs"), Parse::More);
        assert_eq!(state("gsx"), Parse::Invalid);
        assert_eq!(cmd("gqgq").act, Act::Operate(Op::Format, Target::Lines));
        assert_eq!(
            cmd("gwip").act,
            Act::Operate(Op::FormatKeep, Target::Object { kind: 'p', inner: true })
        );
        assert_eq!(
            cmd("!ip").act,
            Act::Operate(Op::Filter, Target::Object { kind: 'p', inner: true })
        );
        assert_eq!(state("d"), Parse::More);
        assert_eq!(state("di"), Parse::More);
        assert_eq!(state("diz"), Parse::Invalid);
        assert_eq!(state("dz"), Parse::Invalid);
        assert_eq!(state("dg"), Parse::More);
    }

    #[test]
    fn registers_and_plain_commands() {
        let c = cmd("\"a3yy");
        assert_eq!((c.register, c.count), (Some('a'), Some(3)));
        assert_eq!(c.keys, k("yy"));
        assert_eq!(state("\""), Parse::More);
        assert_eq!(state("\"!"), Parse::Invalid);
        assert_eq!(cmd("x").act, Act::Key(Key::Char('x')));
        assert_eq!(cmd("rx").act, Act::With('r', 'x'));
        assert_eq!(cmd("r<CR>").act, Act::With('r', '\n'));
        assert_eq!(cmd("ZZ").act, Act::With('Z', 'Z'));
        assert_eq!(cmd("zz").act, Act::Z(Key::Char('z')));
        assert_eq!(cmd("gv").act, Act::G('v'));
        assert_eq!(cmd("<C-r>").act, Act::Key(Key::Ctrl('r')));
        assert_eq!(state("Q"), Parse::Invalid);
    }

    #[test]
    fn a_register_and_count_still_await_a_command() {
        assert!(awaits_command(&[]));
        assert!(awaits_command(&k("\"a12")));
        assert!(awaits_command(&k("3")));
        assert!(!awaits_command(&k("d")));
        assert!(!awaits_command(&k("2g")));
    }

    #[test]
    fn an_operator_awaits_its_motion() {
        assert!(awaits_motion(&k("d")));
        assert!(awaits_motion(&k("\"a2c3")));
        assert!(awaits_motion(&k("gU")));
        assert!(!awaits_motion(&k("g")));
        assert!(!awaits_motion(&k("di")));
        assert!(!awaits_motion(&k("")));
    }

    #[test]
    fn visual_mode_operators_need_no_target() {
        let v = |s: &str| match parse(&k(s), true) {
            Parse::Done(c) => c.act,
            other => panic!("{s}: {other:?}"),
        };
        assert_eq!(v("d"), Act::Operate(Op::Delete, Target::Lines));
        assert_eq!(v("U"), Act::Operate(Op::Upper, Target::Lines));
        assert_eq!(v("iw"), Act::Select { kind: 'w', inner: true });
        assert_eq!(v("j"), Act::Move(Motion::Down));
        assert_eq!(v("o"), Act::Key(Key::Char('o')));
        assert_eq!(v("rx"), Act::With('r', 'x'));
    }
}
