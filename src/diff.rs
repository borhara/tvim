use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    Added,
    Changed,
    Removed,
    RemovedAbove,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hunk {
    pub old: Range<usize>,
    pub new: Range<usize>,
}

impl Hunk {
    // where the hunk shows in the gutter: its first line, or the line above a removal
    pub fn row(&self) -> usize {
        if self.new.is_empty() {
            self.new.start.saturating_sub(1)
        } else {
            self.new.start
        }
    }
}
