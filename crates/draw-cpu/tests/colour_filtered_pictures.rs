//! Still pictures drawn with colour filters by the processor (`docs/spec/notation.md`, section 7.9): a filtered
//! still at its own size on whole pixels gives the colours the arithmetic of its steps states, within one level.
//!
//! Proposal for T060 (spec-first; the implementation follows when the build constraint lifts): the steps ride on the
//! mark as `StillMark::filter`, a `draw::ColourFilter` of `draw::ColourStep`s, applied in order to the picture's colours.

use std::sync::Arc;

use draw::{Colour, ColourFilter, ColourStep, Fit, Frame, Mark, Picture, Renderer, Still, StillMark};
use draw_cpu::CpuRenderer;
use layout::Rect;

/// A still of three worked colours in a row: warm, cool and red, all opaque.
fn strip() -> Arc<Still> {
    Arc::new(
        Still::from_straight(
            3,
            1,
            &[
                200, 100, 50, 255, //
                16, 32, 64, 255, //
                255, 0, 0, 255,
            ],
        )
        .unwrap(),
    )
}

fn filtered(left: i32, width: u32, still: Arc<Still>, steps: Vec<ColourStep>) -> Mark {
    StillMark {
        rect: Rect { left, top: 0, width, height: 1 },
        still,
        fit: Fit::Fill,
        position: (0.5, 0.5),
        filter: ColourFilter { steps },
    }
    .mark()
}

fn drawn() -> Picture {
    use ColourStep::{Brightness, Contrast, Saturate};
    let still = strip();
    CpuRenderer
        .draw_frame(&Frame {
            width: 6,
            height: 1,
            background: Colour { red: 0, green: 0, blue: 0 },
            marks: vec![
                filtered(0, 3, still.clone(), vec![Brightness(0.5), Contrast(2.0)]),
                filtered(3, 3, still, vec![Saturate(0.0)]),
            ],
        })
        .unwrap()
}

/// Asserts that the pixel at `(x, 0)` is within one level of `want`, and opaque.
fn assert_pixel(picture: &Picture, x: u32, want: [u8; 3]) {
    let pixel = picture.pixel(x, 0).unwrap();
    assert_eq!(pixel[3], 255, "pixel ({x}, 0) lost its transparency");
    for (channel, (got, want)) in pixel[..3].iter().zip(want.iter()).enumerate() {
        assert!(
            got.abs_diff(*want) <= 1,
            "pixel ({x}, 0) channel {channel}: got {got}, want {want} (within one level)"
        );
    }
}

#[test]
fn two_ordered_steps_on_a_still_give_the_first_order_result() {
    let picture = drawn();
    assert_pixel(&picture, 0, [73, 0, 0]);
    assert_pixel(&picture, 1, [0, 0, 0]);
    assert_pixel(&picture, 2, [129, 0, 0]);
}

#[test]
fn draining_a_still_gives_every_worked_colour_its_grey() {
    let picture = drawn();
    assert_pixel(&picture, 3, [118, 118, 118]);
    assert_pixel(&picture, 4, [31, 31, 31]);
    assert_pixel(&picture, 5, [54, 54, 54]);
}
