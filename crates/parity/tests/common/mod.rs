//! What the tests of the comparison tool share and that start no program: the colours of the
//! synthetic videos and a folder of a test's own.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

pub const BACKGROUND: [u8; 3] = [0x10, 0x18, 0x20];
pub const CLIP: [u8; 3] = [0xff, 0x5a, 0x36];
pub const MOVER: [u8; 3] = [0x2e, 0xcc, 0x71];
pub const BOX: [u8; 3] = [0x33, 0x66, 0xff];

/// A folder of this test's own, empty at the start.
pub fn folder(name: &str) -> PathBuf {
    let dir = std::path::absolute(Path::new(env!("CARGO_TARGET_TMPDIR")).join("parity").join(name)).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Small pictures for the videos the comparison tests write with frx7 (`measure::write_video`).
pub mod pictures {
    use measure::Frame;

    pub const WIDTH: u32 = 160;
    pub const HEIGHT: u32 = 90;

    /// `count` pictures of `width` x `height`: a dark ground, a square that moves right and a still box.
    pub fn moving(count: usize, width: u32, height: u32) -> Vec<Frame> {
        measure::moving_pictures(count, width, height)
    }

    /// The same pictures, except that picture `number` has a green block that covers `percent` of its
    /// pixels, from the top left, over whole rows.
    pub fn with_block(frames: &[Frame], number: usize, percent: u32) -> Vec<Frame> {
        measure::with_green_rows(frames, number, (frames[number].height * percent).div_ceil(100).max(1))
    }
}
