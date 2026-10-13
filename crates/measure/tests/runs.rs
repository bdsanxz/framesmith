//! Comparing two runs frame for frame.

use measure::{same_frames, Frame};

/// Four small frames, each different from the others.
fn run() -> Vec<Frame> {
    (0..4u8)
        .map(|frame| Frame {
            width: 6,
            height: 4,
            rgb: (0..6 * 4 * 3).map(|at| (at as u8).wrapping_mul(7).wrapping_add(frame)).collect(),
        })
        .collect()
}

#[test]
fn two_equal_runs_compare_equal() {
    assert_eq!(same_frames(&run(), &run()), Ok(()));
}

#[test]
fn one_differing_pixel_fails_and_names_the_frame() {
    let mut second = run();
    // One level of one channel of one pixel: the pixel at column 5 of row 2 of frame 2.
    second[2].rgb[(2 * 6 + 5) * 3 + 1] ^= 1;
    assert_eq!(
        same_frames(&run(), &second),
        Err("frame 2 differs: its first differing pixel is at (5, 2)".to_string())
    );
}

#[test]
fn the_first_of_several_differing_frames_is_the_one_named() {
    let mut second = run();
    second[3].rgb[0] ^= 0x80;
    second[1].rgb[4 * 3] ^= 0x80;
    assert_eq!(
        same_frames(&run(), &second),
        Err("frame 1 differs: its first differing pixel is at (4, 0)".to_string())
    );
}

#[test]
fn a_missing_frame_and_a_frame_of_another_size_fail_too() {
    let mut short = run();
    short.pop();
    assert_eq!(
        same_frames(&run(), &short),
        Err("frame 3 is missing from one run: the runs have 4 and 3 frames".to_string())
    );
    let mut wide = run();
    wide[0] = Frame {
        width: 12,
        height: 2,
        rgb: wide[0].rgb.clone(),
    };
    assert_eq!(
        same_frames(&run(), &wide),
        Err("frame 0 differs: it is 6x4 in one run and 12x2 in the other".to_string())
    );
}
