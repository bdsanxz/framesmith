//! The motion block: its format, its reader, and the pure function that
//! gives every animated value at a time.
//!
//! Behaviour is specified in `docs/spec/motion-block.md`.

mod ease;
mod eval;
pub mod json;
mod parse;
mod spec;

pub use ease::Ease;
pub use eval::{motion_end, ElementMotion, Timeline, Track};
pub use parse::{parse_block, Block, Finding};
pub use spec::{Kind, Prop, Tween, Values, PROPS};
