//! Development only. Runs the reference as a black box and compares the
//! video it makes with ours: frame count, frame rate, size, length and the pictures
//! (`docs/dev/comparison.md`), the timing of events, the position of a tracked element and the sound.
//! The product, its build and its tests never depend on this crate, nor on the reference.

pub mod against;
pub mod blackbox;
pub mod cli;
pub mod compare;
pub mod difference;
pub mod frames;
pub mod picture;
pub mod place;
pub mod png;
pub mod report;
pub mod sheet;
pub mod sound;
pub mod structure;
pub mod timing;
pub mod tools;
pub mod tracking;

pub use report::{Line, Report, Verdict};
