//! Finding the centre of an element in a picture, where no video is needed.

mod common;

use common::{BACKGROUND, CLIP, MOVER};
use parity::timing;

#[test]
fn the_centre_of_a_flat_element_is_found_to_a_fraction_of_a_pixel() {
    let mut frame = measure::Frame {
        width: 100,
        height: 100,
        rgb: BACKGROUND.iter().copied().cycle().take(100 * 100 * 3).collect(),
    };
    let mut paint = |x: u32, y: u32, colour: [u8; 3]| {
        let at = ((y * 100 + x) * 3) as usize;
        frame.rgb[at..at + 3].copy_from_slice(&colour);
    };
    for y in 40..60 {
        for x in 10..30 {
            paint(x, y, MOVER);
        }
    }
    let found = timing::centre(&frame, BACKGROUND, MOVER, 24, 100).unwrap();
    assert!((found.0 - 20.0).abs() < 1e-9 && (found.1 - 50.0).abs() < 1e-9, "{found:?}");
    assert!(
        timing::centre(&frame, BACKGROUND, CLIP, 24, 100).is_none(),
        "another colour is not found"
    );
}
