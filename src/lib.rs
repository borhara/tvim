pub mod buffer;
pub mod clipboard;
pub mod command;
pub mod comment;
pub mod complete;
pub mod diff;
pub mod editor;
pub mod motion;
pub mod object;
pub mod regex;
pub mod syntax;
pub mod text;
mod time;
pub mod ui;

pub use buffer::{Buffer, Pos};
pub use editor::{Editor, Mode};
