//! The place of a still picture in its box (`docs/spec/still-pictures.md`, section 5) and its premultiplied pixels
//! (section 2, point 4), against values worked by hand from the spec.

use std::sync::Arc;

use draw::{Area, Fit, Still, StillMark};
use layout::Rect;

fn opaque(width: u32, height: u32) -> Arc<Still> {
    Arc::new(Still {
        width,
        height,
        pixels: vec![255; width as usize * height as usize * 4],
    })
}

/// A picture of `width` by `height` in the box (10, 20) 100 by 50.
fn mark(width: u32, height: u32, fit: Fit, position: (f64, f64)) -> StillMark {
    StillMark {
        rect: Rect {
            left: 10,
            top: 20,
            width: 100,
            height: 50,
        },
        still: opaque(width, height),
        fit,
        position,
    }
}

fn area(left: f64, top: f64, width: f64, height: f64) -> Area {
    Area { left, top, width, height }
}

#[test]
fn fill_stretches_the_picture_to_the_box_whatever_its_shape() {
    for (width, height) in [(200, 200), (40, 80), (7, 3)] {
        let mark = mark(width, height, Fit::Fill, (0.0, 1.0));
        assert_eq!(mark.place(), Some(area(10.0, 20.0, 100.0, 50.0)), "{width}x{height}");
        assert_eq!(mark.shown(), Some(area(10.0, 20.0, 100.0, 50.0)));
    }
}

#[test]
fn cover_scales_evenly_until_the_box_is_covered_and_cuts_what_reaches_beyond() {
    // A square of 200: the scale is max(100/200, 50/200) = 0.5, so 100 by 100, and 50 reach beyond down.
    assert_eq!(mark(200, 200, Fit::Cover, (0.5, 0.5)).place(), Some(area(10.0, -5.0, 100.0, 100.0)));
    assert_eq!(mark(200, 200, Fit::Cover, (0.0, 0.0)).place(), Some(area(10.0, 20.0, 100.0, 100.0)));
    assert_eq!(
        mark(200, 200, Fit::Cover, (1.0, 1.0)).place(),
        Some(area(10.0, -30.0, 100.0, 100.0))
    );
    // A portrait of 40 by 80: the scale is max(2.5, 0.625) = 2.5, so 100 by 200.
    assert_eq!(mark(40, 80, Fit::Cover, (0.5, 0.5)).place(), Some(area(10.0, -55.0, 100.0, 200.0)));
    // What is drawn is the box: the picture covers it.
    for position in [(0.0, 0.0), (0.5, 0.5), (1.0, 1.0)] {
        assert_eq!(mark(40, 80, Fit::Cover, position).shown(), Some(area(10.0, 20.0, 100.0, 50.0)));
    }
    // A landscape of 400 by 100: the scale is max(0.25, 0.5) = 0.5, so 200 by 50, and 100 reach beyond across.
    assert_eq!(mark(400, 100, Fit::Cover, (1.0, 0.0)).place(), Some(area(-90.0, 20.0, 200.0, 50.0)));
}

#[test]
fn contain_scales_evenly_until_all_of_the_picture_is_in_the_box() {
    // A square of 200: the scale is min(0.5, 0.25) = 0.25, so 50 by 50, with 50 of room across.
    assert_eq!(mark(200, 200, Fit::Contain, (0.5, 0.5)).place(), Some(area(35.0, 20.0, 50.0, 50.0)));
    assert_eq!(mark(200, 200, Fit::Contain, (0.0, 0.0)).place(), Some(area(10.0, 20.0, 50.0, 50.0)));
    assert_eq!(mark(200, 200, Fit::Contain, (1.0, 1.0)).place(), Some(area(60.0, 20.0, 50.0, 50.0)));
    // A portrait of 40 by 80: the scale is min(2.5, 0.625) = 0.625, so 25 by 50, and its left edge between pixels.
    assert_eq!(mark(40, 80, Fit::Contain, (0.5, 0.5)).place(), Some(area(47.5, 20.0, 25.0, 50.0)));
    // What is drawn is the whole picture.
    assert_eq!(mark(40, 80, Fit::Contain, (0.5, 0.5)).shown(), Some(area(47.5, 20.0, 25.0, 50.0)));
}

#[test]
fn a_picture_or_a_box_of_no_size_has_no_place() {
    let mut empty = mark(200, 200, Fit::Cover, (0.5, 0.5));
    empty.rect.width = 0;
    assert_eq!(empty.place(), None);
    assert_eq!(mark(0, 10, Fit::Fill, (0.5, 0.5)).place(), None);
}

#[test]
fn the_pixels_are_multiplied_by_their_alpha_rounded_halves_up() {
    // 200 * 128 / 255 = 100.39, 100 * 128 / 255 = 50.20, 50 * 128 / 255 = 25.10; at alpha 0 the colour goes; at 255 it stays.
    let still = Still::from_straight(3, 1, &[200, 100, 50, 128, 9, 9, 9, 0, 1, 2, 3, 255]).unwrap();
    assert_eq!(still.pixels, vec![100, 50, 25, 128, 0, 0, 0, 0, 1, 2, 3, 255]);
    // 1 * 128 / 255 = 0.502 rounds up to 1; 1 * 127 / 255 = 0.498 rounds down to 0.
    let halves = Still::from_straight(2, 1, &[1, 1, 1, 128, 1, 1, 1, 127]).unwrap();
    assert_eq!(halves.pixels, vec![1, 1, 1, 128, 0, 0, 0, 127]);
    assert!(Still::from_straight(2, 2, &[0; 12]).is_none());
}
