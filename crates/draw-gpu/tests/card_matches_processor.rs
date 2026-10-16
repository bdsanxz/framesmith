//! The card's pictures equal the processor's to the byte. Each test here needs a graphics card that
//! frx7 can draw with, so each is reference machine only (`docs/dev/gates.md` section 4).

use draw::{Colour, DisplayList, Picture, Renderer, Step};
use draw_cpu::CpuRenderer;
use draw_gpu::CardRenderer;
use layout::Rect;

fn colour(red: u8, green: u8, blue: u8) -> Colour {
    Colour { red, green, blue }
}

fn fill(left: i32, top: i32, width: u32, height: u32, colour: Colour) -> Step {
    Step::Fill {
        rect: Rect { left, top, width, height },
        colour,
    }
}

fn card() -> CardRenderer {
    CardRenderer::new().unwrap_or_else(|error| panic!("the card could not be started: {error}"))
}

/// A frame of 101 by 37 pixels, an odd size, with fills that overlap, are clipped on each side,
/// have no width or no height, or lie outside the frame.
fn odd_frame() -> DisplayList {
    DisplayList {
        width: 101,
        height: 37,
        background: colour(16, 32, 64),
        steps: vec![
            fill(0, 0, 101, 37, colour(9, 9, 9)),
            fill(3, 2, 40, 20, colour(200, 30, 30)),
            fill(30, 10, 50, 25, colour(30, 200, 30)),
            fill(-10, -5, 30, 15, colour(5, 5, 250)),
            fill(90, 30, 40, 20, colour(250, 250, 0)),
            fill(20, 20, 0, 9, colour(1, 2, 3)),
            fill(50, 10, 7, 0, colour(1, 2, 3)),
            fill(500, 500, 10, 10, colour(1, 2, 3)),
            fill(1, 1, 1, 1, colour(255, 255, 255)),
            fill(60, 5, 9, 9, colour(128, 0, 255)),
        ],
    }
}

/// A frame of 256 by 2 pixels: in each column, one pixel of each colour value in each channel.
fn every_colour_value() -> DisplayList {
    DisplayList {
        width: 256,
        height: 2,
        background: colour(0, 0, 0),
        steps: (0..=255u8)
            .flat_map(|value| {
                [
                    fill(i32::from(value), 0, 1, 1, colour(value, 255 - value, value)),
                    fill(i32::from(value), 1, 1, 1, colour(value, value, value)),
                ]
            })
            .collect(),
    }
}

/// The first pixel of two pictures that differ, with both values, or nothing when they are equal.
fn first_difference(drawn: &Picture, expected: &Picture) -> Option<String> {
    let at = drawn
        .pixels()
        .chunks(4)
        .zip(expected.pixels().chunks(4))
        .position(|(a, b)| a != b)?;
    let (x, y) = (at as u32 % drawn.width(), at as u32 / drawn.width());
    Some(format!(
        "pixel ({x}, {y}) is {:?} on the card and {:?} on the processor",
        drawn.pixel(x, y).unwrap(),
        expected.pixel(x, y).unwrap()
    ))
}

/// Draws the list on the card and on the processor, and asserts that the two pictures are equal.
fn assert_equal(card: &mut CardRenderer, list: &DisplayList) {
    let drawn = card
        .draw(list)
        .unwrap_or_else(|error| panic!("the card could not draw it: {error}"));
    let expected = CpuRenderer
        .draw(list)
        .unwrap_or_else(|error| panic!("the processor could not draw it: {error}"));
    assert_eq!((drawn.width(), drawn.height()), (list.width, list.height));
    if let Some(difference) = first_difference(&drawn, &expected) {
        panic!("{difference}");
    }
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_the_card_is_named_card() {
    assert_eq!(card().name(), "card");
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_an_odd_sized_frame_with_overlapping_clipped_and_empty_fills_equals_the_processor() {
    assert_equal(&mut card(), &odd_frame());
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_every_colour_value_in_every_channel_equals_the_processor() {
    assert_equal(&mut card(), &every_colour_value());
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_repeated_draws_do_not_depend_on_what_was_drawn_before() {
    let mut card = card();
    let other = DisplayList {
        width: 64,
        height: 64,
        background: colour(0, 0, 0),
        steps: vec![fill(10, 10, 20, 20, colour(250, 0, 0))],
    };
    assert_equal(&mut card, &odd_frame());
    assert_equal(&mut card, &other);
    assert_equal(&mut card, &odd_frame());
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_a_frame_with_no_pixels_is_refused_and_the_card_still_draws() {
    let mut card = card();
    let empty = DisplayList {
        width: 0,
        height: 10,
        background: colour(0, 0, 0),
        steps: vec![],
    };
    let error = card.draw(&empty).unwrap_err();
    assert!(error.to_string().contains("cannot be drawn on the card"), "{error}");
    assert_equal(&mut card, &odd_frame());
}
