use crate::motion::Find;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motion {
    Left,
    Right,
    Up,
    Down,
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
}

pub enum MotionParse {
    More,
    Invalid,
    Done(Motion, usize),
}
