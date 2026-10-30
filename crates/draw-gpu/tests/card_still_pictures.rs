//! The card draws still pictures as the processor does (`docs/spec/card-renderer.md`, section 12): a frame of pictures
//! filled, covered and contained, enlarged and made smaller, transparent, with an edge between pixels, moved, and turned
//! and scaled by a map, agrees with the processor's frame within the tolerance of section 4. Each test needs a graphics
//! card that frx7 can draw with, so each is reference machine only (`docs/dev/gates.md` section 4).

use std::sync::Arc;

use draw::{Blend, Colour, Filter, Fit, Frame, Group, Map, Mark, Picture, Renderer, Still, StillMark};
use draw_cpu::CpuRenderer;
use draw_gpu::CardRenderer;
use layout::Rect;

/// The least similarity of two pictures, and the most values further apart than 4 levels, as one in this many
/// values (`card-renderer.md` section 4; `measure`).
const LEAST_SIMILARITY: f64 = 0.995;
const ONE_IN: u64 = 2000;

/// A picture of our own: a gradient across, another down, and an alpha that falls along the diagonal.
fn picture(width: u32, height: u32, alpha_falls: bool) -> Arc<Still> {
    let mut rgba = Vec::new();
    for y in 0..height {
        for x in 0..width {
            let alpha = if alpha_falls {
                255 - ((x + y) * 255 / (width + height)) as u8
            } else {
                255
            };
            rgba.extend([(x * 255 / width) as u8, (y * 255 / height) as u8, 90, alpha]);
        }
    }
    Arc::new(Still::from_straight(width, height, &rgba).unwrap())
}

fn placed(left: i32, top: i32, width: u32, height: u32, still: Arc<Still>, fit: Fit, position: (f64, f64)) -> Mark {
    StillMark {
        rect: Rect { left, top, width, height },
        still,
        fit,
        position,
    }
    .mark()
}

fn group(map: Map, marks: Vec<Mark>) -> Mark {
    Mark::Group(Group {
        map,
        opacity: 1.0,
        clip: None,
        marks,
        turn: None,
        filter: Filter::NONE,
        shadow: None,
        blend: Blend::Normal,
    })
}

fn frame() -> Frame {
    let (big, small, glass) = (picture(64, 48, false), picture(7, 5, false), picture(24, 24, true));
    // A turn of 20 degrees with a scale of 0.8, and a move.
    let (sin, cos) = (20.0_f64.to_radians().sin() * 0.8, 20.0_f64.to_radians().cos() * 0.8);
    Frame {
        width: 200,
        height: 140,
        background: Colour {
            red: 20,
            green: 40,
            blue: 60,
        },
        marks: vec![
            placed(4, 4, 40, 30, small.clone(), Fit::Fill, (0.5, 0.5)),
            placed(50, 4, 40, 30, big.clone(), Fit::Cover, (0.0, 1.0)),
            placed(96, 4, 40, 30, big.clone(), Fit::Contain, (0.5, 0.5)),
            placed(140, 4, 50, 50, big.clone(), Fit::Contain, (0.3, 0.7)),
            placed(4, 40, 17, 13, big.clone(), Fit::Fill, (0.5, 0.5)),
            placed(30, 40, 60, 60, glass.clone(), Fit::Fill, (0.5, 0.5)),
            group(Map::moving(2.5, 0.25), vec![placed(96, 40, 30, 30, small, Fit::Cover, (1.0, 0.0))]),
            group(
                Map {
                    a: cos,
                    b: sin,
                    c: -sin,
                    d: cos,
                    e: 150.0,
                    f: 60.0,
                },
                vec![
                    placed(0, 0, 40, 30, big, Fit::Fill, (0.5, 0.5)),
                    placed(10, 10, 24, 24, glass, Fit::Contain, (0.5, 0.5)),
                ],
            ),
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
fn reference_machine_only_still_pictures_agree_with_the_processor() {
    let frame = frame();
    let processor = CpuRenderer.draw_frame(&frame).unwrap();
    let card = CardRenderer::new().unwrap().draw_frame(&frame).unwrap();
    let background = processor.pixel(199, 139).unwrap();
    let drawn = processor.pixels().chunks(4).filter(|pixel| pixel[..3] != background[..3]).count();
    assert!(drawn > 8000, "the frame draws too few pictures to test anything: {drawn} pixels");
    agrees(&processor, &card);
}
