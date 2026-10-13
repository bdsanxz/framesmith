//! The reader of the notation.
//!
//! Behaviour is specified in `docs/spec/notation.md` (the notation) and `docs/spec/check-command.md`
//! (what is read of a composition's root and how a problem is worded). This version reads the root
//! of a composition file: [`read_root`], which gives every problem it finds, each with its [`Code`].

mod code;
mod problem;
mod root;
mod value;

pub use code::Code;
pub use problem::Problem;
pub use root::{read_root, Colour, Root};
