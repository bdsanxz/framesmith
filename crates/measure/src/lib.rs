//! Development only. Reads video facts and frames with frx7 (no program is started), reads them also
//! with the user's own `ffmpeg` and `ffprobe` where a yardstick test compares with them, writes the
//! small videos tests need, and measures what is painted in a frame. The product never depends on
//! this crate.

mod clip;
mod command;
mod facts;
pub mod font;
mod frame;
mod pictures;
mod shape;
mod similarity;
mod sound;
mod video;
pub mod yardstick;

pub use clip::{clip_facts, write_clip, ClipFacts, ClipSpec};
pub use command::{run, tool, Ran};
pub use facts::{facts, Facts};
pub use frame::{first_difference, frames_from_pngs, frames_from_video, png_size, same_frames, Bounds, Frame, Rgb};
pub use pictures::{moving_pictures, with_green_rows};
pub use shape::{corner_distance, expected_corners, shape, Shape};
pub use similarity::similarity;
pub use sound::{has_sound, made_sound, read_sound, write_video_with_sound, Sound};
pub use video::{open_pictures, video_facts, write_video, write_video_with_times, PictureReader, VideoFacts};
