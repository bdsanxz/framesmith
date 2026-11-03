//! The motion engine: the tweens of a composition and the pure function that
//! gives every animated value at a time.
//!
//! Behaviour is specified in `docs/spec/motion.md`. How motion is written in a
//! composition is the notation's business and not this crate's.

mod ease;
mod eval;
pub mod expr;
pub mod path;
mod spec;
mod stagger;

pub use ease::{Ease, SPRING_DEFAULT_DAMPING, SPRING_DEFAULT_STIFFNESS, WIGGLE_DEFAULT_COUNT};
pub use eval::{motion_end, ElementMotion, Timeline, Track};
pub use spec::{Kind, Prop, Tween, Values, PROPS, PROP_COUNT};
pub use stagger::{
    delay as stagger_delay, grid_delay as stagger_grid_delay,
    grid_delay_rows as stagger_grid_delay_rows, grid_rank as stagger_grid_rank,
    grid_rows as stagger_grid_rows, max_delay as stagger_max_delay,
    max_delay_grid as stagger_max_delay_grid, passes as repeat_passes, rank as stagger_rank,
    repeat_end as repeat_motion_end, Origin as StaggerOrigin, Pass as RepeatPass,
};
