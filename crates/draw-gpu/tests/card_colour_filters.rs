//! The card draws colour-filtered still pictures as the processor does (`docs/spec/notation.md`, section 7.9;
//! `docs/spec/card-renderer.md`, section 4): the card's frame agrees with the processor's at a similarity of 0.995 or
//! better, with at most 0.05 % of values further apart than 4 levels. Each test needs a graphics card that frx7 can
//! draw with, so each is reference machine only (`docs/dev/gates.md` section 4).
//!
//! Proposal for T060 (spec-first; the implementation follows when the build constraint lifts): the steps ride on the
//! mark as `StillMark::filter`, a `draw::ColourFilter` of `draw::ColourStep`s.

use std::sync::Arc;

use draw::{Colour, ColourFilter, ColourStep, Fit, Frame, Mark, Picture, Renderer, Still, StillMark};
use draw_cpu::CpuRenderer;
use draw_gpu::CardRenderer;
use layout::Rect;

/// The least similarity of two pictures, and the most values further apart than 4 levels, as one in this many
/// values (`card-renderer.md` section 4; `measure`).
const LEAST_SIMILARITY: f64 = 0.995;
const ONE_IN: u64 = 2000;

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

fn frame() -> Frame {
    use ColourStep::{Brightness, Contrast, Saturate};
    let still = strip();
    Frame {
        width: 6,
        height: 1,
        background: Colour { red: 0, green: 0, blue: 0 },
        marks: vec![
            filtered(0, 3, still.clone(), vec![Brightness(0.5), Contrast(2.0)]),
            filtered(3, 3, still, vec![Saturate(0.0)]),
        ],
    }
}

fn measured(picture: &Picture) -> measure::Frame {
    measure::Frame {
        width: picture.width(),
        height: picture.height(),
        rgb: picture
            .pixels()
            .chunks(4)
            .flat_map(|pixel| [pixel[0], pixel[1], pixel[2]])
            .collect(),
    }
}

/// Asserts that the card's picture agrees with the processor's within the tolerance of section 4.
fn agrees(processor: &Picture, card: &Picture) {
    let similarity = measure::similarity(&measured(processor), &measured(card));
    let further_apart: u64 = processor
        .pixels()
        .chunks(4)
        .zip(card.pixels().chunks(4))
        .map(|(a, b)| a[..3].iter().zip(&b[..3]).filter(|&(x, y)| x.abs_diff(*y) > 4).count() as u64)
        .sum();
    let values = u64::from(processor.width()) * u64::from(processor.height()) * 3;
    assert!(
        matches!(similarity, Some(value) if value >= LEAST_SIMILARITY),
        "similarity {similarity:?} is below {LEAST_SIMILARITY}"
    );
    assert!(
        further_apart * ONE_IN <= values,
        "{further_apart} of {values} values are further apart than 4 levels, above 0.05 %"
    );
}

#[cfg(target_os = "macos")]
#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_colour_filtered_stills_agree_with_the_processor() {
    let frame = frame();
    let processor = CpuRenderer.draw_frame(&frame).unwrap();
    let card = CardRenderer::new()
        .unwrap_or_else(|error| panic!("the card could not be started: {error}"))
        .draw_frame(&frame)
        .unwrap_or_else(|error| panic!("the card could not draw it: {error}"));
    agrees(&processor, &card);
}
