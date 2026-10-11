//! Development only. Reads frames and video facts with the user's own
//! `ffmpeg` and `ffprobe`, and measures what is painted in a frame. The
//! product never depends on this crate.

mod command;
mod facts;
mod font;
mod frame;
mod shape;
mod similarity;

pub use command::{run, tool, Ran};
pub use facts::{facts, Facts};
pub use font::block_font;
pub use frame::{first_difference, frames_from_pngs, frames_from_video, png_size, same_frames, Bounds, Frame, Rgb};
pub use shape::{corner_distance, expected_corners, shape, Shape};
pub use similarity::similarity;
