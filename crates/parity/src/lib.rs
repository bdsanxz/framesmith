//! Development only. Runs the reference as a black box and compares the
//! video it makes with ours: container facts, pictures, timing of events
//! and the position of a moving element. The product, its build and its
//! tests never depend on this crate, nor on the reference.

pub mod blackbox;
pub mod cli;
pub mod container;
pub mod frames;
pub mod pictures;
pub mod report;
pub mod sheet;
pub mod timing;
pub mod tools;

pub use report::{Line, Report, Verdict};
