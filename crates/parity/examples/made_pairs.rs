//! Development only: writes pairs of videos with a known shift in time or in place, for trying the
//! event check and the tracking check of `parity compare` by hand (`docs/dev/comparison.md`, section 5).
//!
//! `cargo run -p parity --example made_pairs -- <new folder>` writes, at 160 x 90 and 30 frames a
//! second, 30 frames each, on the background 101820: a still square of ff5a36 (`clip`) that appears
//! on frame 10, and a square of 2ecc71 (`mover`) that moves right 2 pixels a frame from frame 5 to
//! frame 20. The files:
//!
//! - `base.mp4` and `same.mp4`: written separately from the same pictures;
//! - `clip-late-1.mp4` and `clip-late-2.mp4`: the clip appears 1 or 2 frames later (frame 11 or 12);
//! - `mover-off-1.mp4`: the mover 1 pixel to the right on every frame;
//! - `mover-off-2-at-14.mp4`: the mover 2 pixels to the right on frame 14 alone.

use measure::Frame;
use std::path::Path;

const WIDTH: u32 = 160;
const HEIGHT: u32 = 90;
const BACKGROUND: [u8; 3] = [0x10, 0x18, 0x20];
const CLIP: [u8; 3] = [0xff, 0x5a, 0x36];
const MOVER: [u8; 3] = [0x2e, 0xcc, 0x71];

fn pictures(appears: usize, left: impl Fn(usize) -> u32) -> Vec<Frame> {
    (0..30)
        .map(|at| {
            let mut frame = Frame {
                width: WIDTH,
                height: HEIGHT,
                rgb: BACKGROUND.iter().copied().cycle().take((WIDTH * HEIGHT * 3) as usize).collect(),
            };
            let mut paint = |x0: u32, y0: u32, colour: [u8; 3]| {
                for y in y0..y0 + 20 {
                    for x in x0..x0 + 20 {
                        let i = ((y * WIDTH + x) * 3) as usize;
                        frame.rgb[i..i + 3].copy_from_slice(&colour);
                    }
                }
            };
            if at >= appears {
                paint(110, 50, CLIP);
            }
            paint(left(at), 20, MOVER);
            frame
        })
        .collect()
}

fn path(at: usize) -> u32 {
    10 + 2 * (at.clamp(5, 20) as u32 - 5)
}

fn main() {
    let Some(folder) = std::env::args().nth(1) else {
        eprintln!("usage: made_pairs <new folder>");
        std::process::exit(2);
    };
    let folder = Path::new(&folder);
    if folder.exists() {
        eprintln!("{} exists; give a new folder", folder.display());
        std::process::exit(2);
    }
    std::fs::create_dir_all(folder).unwrap();
    let pairs: [(&str, Vec<Frame>); 6] = [
        ("base.mp4", pictures(10, path)),
        ("same.mp4", pictures(10, path)),
        ("clip-late-1.mp4", pictures(11, path)),
        ("clip-late-2.mp4", pictures(12, path)),
        ("mover-off-1.mp4", pictures(10, |at| path(at) + 1)),
        ("mover-off-2-at-14.mp4", pictures(10, |at| path(at) + if at == 14 { 2 } else { 0 })),
    ];
    for (name, frames) in pairs {
        let file = folder.join(name);
        if let Err(why) = measure::write_video(&file, (30, 1), &frames) {
            eprintln!("{why}");
            std::process::exit(1);
        }
        println!("wrote {}", file.display());
    }
}
