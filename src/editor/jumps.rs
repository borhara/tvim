use std::collections::HashMap;

use super::{Editor, Mode};
use crate::buffer::Pos;
use crate::jumps::Spot;

impl Editor {
    pub(crate) fn remember_jump(&mut self) {
        self.jump_from(self.cursor);
    }

    pub(super) fn jump_from(&mut self, from: Pos) {
        self.marks.insert('\'', from);
        self.push_jump(from);
    }

    pub(super) fn push_jump(&mut self, pos: Pos) {
        if self.no_jumps {
            return;
        }
        self.jumps.push(Spot { pos, ..self.here() });
    }

    pub(super) fn here(&self) -> Spot {
        Spot {
            buffer: self.buffer_id,
            path: self.file.path.clone(),
            pos: self.cursor,
        }
    }

    pub(super) fn jump_older(&mut self, mut n: usize) {
        let mut here = self.here();
        while let Some(spot) = self.jumps.back(here.clone(), n) {
            if self.go_to_spot(spot) {
                return;
            }
            here = self.here();
            n = 1;
        }
    }

    pub(super) fn jump_newer(&mut self, mut n: usize) {
        while let Some(spot) = self.jumps.forward(n) {
            if self.go_to_spot(spot) {
                return;
            }
            n = 1;
        }
    }

    pub(super) fn go_to_spot(&mut self, spot: Spot) -> bool {
        self.no_jumps = true;
        let found = if spot.buffer == self.buffer_id {
            true
        } else if let Some(i) = (0..self.docs.len()).find(|&i| i != self.current && self.docs[i].id == spot.buffer) {
            self.switch_to(i);
            true
        } else if let Some(path) = &spot.path {
            match self.open_path(path) {
                Ok(()) => true,
                Err(e) => {
                    self.error(format!("\"{}\" {e}", path.display()));
                    false
                }
            }
        } else {
            false
        };
        self.no_jumps = false;
        if found {
            self.leave_visual_if_any();
            self.mode = Mode::Normal;
            self.cursor = self.clamp_pos(spot.pos);
            self.set_want();
        }
        found
    }

    pub(crate) fn marks(&self) -> &HashMap<char, Pos> {
        &self.marks
    }
}
