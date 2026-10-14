//! The reader of the notation.
//!
//! Behaviour is specified in `docs/spec/notation.md` (the notation) and `docs/spec/check-command.md`
//! (what is read of a composition file and how a problem is worded). This version reads the root of a
//! composition file and judges the form of the whole file: [`read_root`], which gives every problem it
//! finds, each with its [`Code`], its [`Kind`] and its remedy. [`Element`] holds the elements of the
//! notation, the attributes each takes and the elements each holds.

mod code;
mod problem;
mod root;
mod schema;
mod structure;
mod value;

pub use code::{Code, Kind};
pub use problem::Problem;
pub use root::{read_root, shown, Colour, Root};
pub use schema::{Element, Form};
