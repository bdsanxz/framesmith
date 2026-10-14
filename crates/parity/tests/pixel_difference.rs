//! How many pixels of two pictures differ: the count of `pixelmatch` at its defaults
//! (`docs/dev/comparison.md`, section 4.2). The worked values below were taken from `pixelmatch`
//! 8.0.0 on the same pictures; the yardstick test `yardstick_pixelmatch_counts` holds the count to it
//! on a hundred thousand more.

use measure::Frame;
use parity::difference::{compare, distance, THRESHOLD};

fn flat(width: u32, height: u32, colour: [u8; 3]) -> Frame {
    Frame {
        width,
        height,
        rgb: colour.iter().copied().cycle().take((width * height * 3) as usize).collect(),
    }
}

fn set(frame: &mut Frame, x: u32, y: u32, colour: [u8; 3]) {
    let at = ((y * frame.width + x) * 3) as usize;
    frame.rgb[at..at + 3].copy_from_slice(&colour);
}

#[test]
fn equal_pictures_have_no_differing_pixel() {
    let a = flat(40, 30, [20, 30, 50]);
    let found = compare(&a, &a.clone()).unwrap();
    assert_eq!((found.pixels, found.differing), (1200, 0));
    assert!(found.mask.iter().all(|&marked| !marked));
}

#[test]
fn a_block_of_another_colour_counts_exactly_its_pixels_and_marks_them() {
    let a = flat(40, 30, [20, 30, 50]);
    let mut b = a.clone();
    for y in 10..20 {
        for x in 5..15 {
            set(&mut b, x, y, [200, 30, 50]);
        }
    }
    let found = compare(&a, &b).unwrap();
    assert_eq!(found.differing, 100);
    for y in 0..30u32 {
        for x in 0..40u32 {
            let inside = (5..15).contains(&x) && (10..20).contains(&y);
            assert_eq!(found.mask[(y * 40 + x) as usize], inside, "({x}, {y})");
        }
    }
}

#[test]
fn a_change_inside_the_threshold_is_not_counted_and_one_outside_is() {
    let a = flat(8, 8, [100, 100, 100]);
    let mut near = a.clone();
    set(&mut near, 3, 3, [101, 100, 100]);
    assert!(distance([100, 100, 100], [101, 100, 100]) < THRESHOLD);
    assert_eq!(compare(&a, &near).unwrap().differing, 0);
    let mut far = a.clone();
    set(&mut far, 3, 3, [200, 100, 100]);
    assert!(distance([100, 100, 100], [200, 100, 100]) > THRESHOLD);
    assert_eq!(compare(&a, &far).unwrap().differing, 1);
}

#[test]
fn the_distance_of_black_and_white_is_one_and_of_a_colour_with_itself_is_nothing() {
    assert!((distance([0, 0, 0], [255, 255, 255]) - 1.0).abs() < 1e-6);
    assert_eq!(distance([12, 200, 99], [12, 200, 99]), 0.0);
}

/// A vertical edge in three steps: two black columns, a middle column, two white columns.
fn edge(middle: u8) -> Frame {
    let mut rgb = Vec::new();
    for _ in 0..3 {
        for x in 0..5 {
            let value = match x {
                0 | 1 => 0,
                2 => middle,
                _ => 255,
            };
            rgb.extend_from_slice(&[value; 3]);
        }
    }
    Frame { width: 5, height: 3, rgb }
}

#[test]
fn anti_aliased_pixels_on_an_edge_are_not_counted() {
    // `pixelmatch` gives 0 for each of these (and 3 with anti-aliasing counted).
    for (first, second) in [(128, 64), (128, 192), (100, 160)] {
        let found = compare(&edge(first), &edge(second)).unwrap();
        assert_eq!(found.differing, 0, "middle column {first} against {second}");
    }
}

#[test]
fn a_lone_changed_pixel_in_flat_colour_is_not_anti_aliasing_and_is_counted() {
    // `pixelmatch` gives 1 here.
    let a = flat(5, 3, [0, 0, 0]);
    let mut b = a.clone();
    set(&mut b, 2, 1, [255, 255, 255]);
    assert_eq!(compare(&a, &b).unwrap().differing, 1);
}

#[test]
fn pictures_of_two_sizes_cannot_be_set_side_by_side() {
    let why = compare(&flat(4, 4, [0, 0, 0]), &flat(4, 5, [0, 0, 0])).unwrap_err();
    assert_eq!(why, "pictures of 4x4 and 4x5 cannot be set side by side");
}
