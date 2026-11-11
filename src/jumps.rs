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

    pub fn back(&mut self, here: Spot, n: usize) -> Option<Spot> {
        if self.at >= self.spots.len() {
            self.add(here);
            self.at = self.spots.len() - 1;
        }
        self.at = self.at.checked_sub(n)?;
        Some(self.spots[self.at].clone())
    }

    pub fn forward(&mut self, n: usize) -> Option<Spot> {
        let to = self.at + n;
        if to >= self.spots.len() {
            return None;
        }
        self.at = to;
        Some(self.spots[to].clone())
    }

    pub fn has_newer(&self) -> bool {
        self.at + 1 < self.spots.len()
    }

}
