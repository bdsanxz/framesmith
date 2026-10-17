//! The reader of the notation.
//!
//! Behaviour is specified in `docs/spec/notation.md` (the notation) and `docs/spec/check-command.md`
//! (what is read of a composition file and how a problem is worded). This version reads the root of a
//! composition file and judges the form of the whole file: [`read_root`], which gives every problem it
//! finds, each with its [`Code`], its [`Kind`] and its remedy. [`Element`] holds the elements of the
//! notation, the attributes each takes and the elements each holds. [`read`] gives the same for a valid
//! file and also the [`Tree`] of its elements, in document order, with their attributes and lines.

mod code;
mod problem;
mod root;
mod schema;
mod structure;
mod styles;
mod tree;
mod value;

pub use code::{Code, Kind};
pub use problem::Problem;
pub use root::{read, read_root, shown, Colour, Composition, Root};
pub use schema::{Element, Form};
pub use tree::{Attribute, Length, Node, Tree};
