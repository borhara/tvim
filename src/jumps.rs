use std::path::PathBuf;

use crate::buffer::Pos;

const MAX_JUMPS: usize = 100;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Spot {
    pub buffer: usize,
    pub path: Option<PathBuf>,
    pub pos: Pos,
}

#[derive(Clone, Debug, Default)]
pub struct JumpList {
    spots: Vec<Spot>,
    // spots.len() when not stepping through the list
    at: usize,
}

impl JumpList {
    pub fn push(&mut self, spot: Spot) {
        self.spots.truncate(self.at);
    }

    fn add(&mut self, spot: Spot) {
        self.spots
        if self.spots.len() > MAX_JUMPS {
            self.spots.remove(0);
        }
        self.at = self.spots.len();
    }

}
