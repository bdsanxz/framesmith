//! Makes the clips of the footage fixture with frx7's writer, from flat colours of our own, and writes them beside its
//! composition: `fixtures/footage-on-screen/numbered.mp4` and `split.mp4`.
//!
//! `cargo run -p render --example write_footage_clips`
//!
//! The clips are checked in, so that the checked-in pictures are drawn from the same encoded pictures on every machine
//! (`docs/spec/render-command.md` section 9).

use measure::Frame;
use std::path::Path;

/// The colour of picture `k` of the numbered clip (`crates/render/tests/footage_checked_in.rs` works its values from it).
fn numbered(k: u32) -> [u8; 3] {
    [(k % 5 * 50 + 20) as u8, 128, (k / 5 * 100 + 20) as u8]
}

fn picture(width: u32, height: u32, colour: impl Fn(u32) -> [u8; 3]) -> Frame {
    let mut rgb = Vec::with_capacity((width * height * 3) as usize);
    for _ in 0..height {
        for x in 0..width {
            rgb.extend_from_slice(&colour(x));
        }
    }
    Frame { width, height, rgb }
}

fn main() {
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/footage-on-screen");
    let pictures: Vec<Frame> = (0..10).map(|k| picture(40, 24, |_| numbered(k))).collect();
    measure::write_video(&folder.join("numbered.mp4"), (10, 1), &pictures).unwrap_or_else(|error| panic!("{error}"));
    let halves = picture(8, 8, |x| if x < 4 { [220, 20, 20] } else { [20, 20, 220] });
    measure::write_video(&folder.join("split.mp4"), (10, 1), &vec![halves; 30]).unwrap_or_else(|error| panic!("{error}"));
    println!("{}", folder.display());
}
