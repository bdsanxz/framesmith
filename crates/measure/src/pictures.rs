//! Small pictures for the videos the tests and the evidence scripts write with [`write_video`](crate::write_video):
//! made by us, from nothing, so that no third-party picture enters the repository.

use crate::Frame;

/// `count` pictures of `width` x `height` (at least 31 wide): a dark ground, a square that moves
/// right by 4 pixels a picture and a still box, so that every picture differs from the next.
pub fn moving_pictures(count: usize, width: u32, height: u32) -> Vec<Frame> {
    (0..count)
        .map(|at| {
            let mut rgb = Vec::with_capacity((width * height * 3) as usize);
            let left = (4 * at as u32) % (width - 30);
            for y in 0..height {
                for x in 0..width {
                    let colour = if (left..left + 30).contains(&x) && (20..50).contains(&y) {
                        [0xff, 0x5a, 0x36]
                    } else if (10..40).contains(&x) && (60..80).contains(&y) {
                        [0x33, 0x66, 0xff]
                    } else {
                        [0x10, 0x18, 0x20]
                    };
                    rgb.extend_from_slice(&colour);
                }
            }
            Frame { width, height, rgb }
        })
        .collect()
}

/// The same pictures, except that picture `number` has its top `rows` rows painted green.
pub fn with_green_rows(frames: &[Frame], number: usize, rows: u32) -> Vec<Frame> {
    let mut out = frames.to_vec();
    let frame = &mut out[number];
    let rows = rows.min(frame.height);
    for pixel in frame.rgb[..(rows * frame.width * 3) as usize].as_chunks_mut::<3>().0 {
        pixel.copy_from_slice(&[0x1e, 0xc8, 0x3c]);
    }
    out
}
