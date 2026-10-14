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
