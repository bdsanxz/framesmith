//! The motion engine: the tweens of a composition and the pure function that
//! gives every animated value at a time.
//!
//! Behaviour is specified in `docs/spec/motion.md`. How motion is written in a
//! composition is the notation's business and not this crate's.

mod ease;
mod eval;
mod spec;

pub use ease::Ease;
pub use eval::{motion_end, ElementMotion, Timeline, Track};
pub use spec::{Kind, Prop, Tween, Values, PROPS};
