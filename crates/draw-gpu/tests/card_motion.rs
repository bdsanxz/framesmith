//! The card draws moved and faded marks as the processor does (`docs/spec/card-renderer.md`, section 10): a group with a
//! turned, scaled and slanted fill, a turned text, a faded group over another fill, and a group cut to a rectangle each
//! agree with the processor's picture within the tolerance of section 4; and the coverage the card keeps for a text that
//! turns in every frame stops rising at the bound of section 9.6. Each test needs a graphics card that frx7 can draw
//! with, so each is reference machine only (`docs/dev/gates.md` section 4).

use std::fs;
use std::path::Path;
use std::sync::Arc;

use draw::{Colour, Filter, Font, Frame, Glyph, GlyphRun, Group, Map, Mark, Picture, Renderer, Step, TextMark};
use draw_cpu::CpuRenderer;
use draw_gpu::{CardRenderer, MASK_BOUND};
use layout::Rect;

const LEAST_SIMILARITY: f64 = 0.995;
const ONE_IN: u64 = 2000;

fn face() -> Font {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/fonts/framesmith-test-stroke-400.ttf");
    Font {
        data: Arc::new(fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))),
        index: 0,
    }
}

fn colour(red: u8, green: u8, blue: u8) -> Colour {
    Colour { red, green, blue }
}

fn fill(left: i32, top: i32, width: u32, height: u32, colour: Colour) -> Mark {
    Mark::Step(Step::Fill {
        rect: Rect { left, top, width, height },
        colour,
    })
}

fn text(face: &Font, size: f32, x: f32, y: f32) -> Mark {
    Mark::Text(TextMark {
        runs: vec![GlyphRun {
            font: face.clone(),
            size,
            skew_degrees: 0.0,
            colour: colour(250, 240, 40),
            glyphs: [5u32, 6, 7, 8]
                .iter()
                .enumerate()
                .map(|(index, &id)| Glyph {
                    id,
                    x: x + size * 0.6 * index as f32,
                    y,
                })
                .collect(),
        }],
        pictures: Vec::new(),
        clip: None,
        shadows: Vec::new(),
        outline: None,
    })
}

/// The map of section 4.5 of `render-command.md`: moved by (x, y), turned, slanted and scaled about (ox, oy).
fn about(origin: (f64, f64), moved: (f64, f64), scale: (f64, f64), slant_degrees: f64, turn_degrees: f64) -> Map {
    let (sin, cos) = turn_degrees.to_radians().sin_cos();
    let tan = slant_degrees.to_radians().tan();
    Map::moving(moved.0, moved.1)
        .then_after(Map::moving(origin.0, origin.1))
        .then_after(Map {
            a: cos,
            b: sin,
            c: -sin,
            d: cos,
            e: 0.0,
            f: 0.0,
        })
        .then_after(Map {
            a: 1.0,
            b: tan,
            c: 0.0,
            d: 1.0,
            e: 0.0,
            f: 0.0,
        })
        .then_after(Map {
            a: scale.0,
            b: 0.0,
            c: 0.0,
            d: scale.1,
            e: 0.0,
            f: 0.0,
        })
        .then_after(Map::moving(-origin.0, -origin.1))
}

fn group(map: Map, opacity: f64, clip: Option<Rect>, marks: Vec<Mark>) -> Mark {
    Mark::Group(Group {
        map,
        opacity,
        clip,
        marks,
        turn: None,
        filter: Filter::NONE,
        shadow: None,
    })
}

fn frame(marks: Vec<Mark>) -> Frame {
    Frame {
        width: 200,
        height: 120,
        background: colour(16, 32, 64),
        marks,
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

fn the_card_agrees_with_the_processor(name: &str, frame: &Frame) {
    let processor = CpuRenderer.draw_frame(frame).unwrap();
    let card = CardRenderer::new().unwrap().draw_frame(frame).unwrap();
    let background = processor.pixel(0, 0).unwrap();
    let ink = processor.pixels().chunks(4).filter(|pixel| pixel[..3] != background[..3]).count();
    assert!(ink > 300, "{name}: the frame draws too little to test anything: {ink} pixels");
    let similarity = measure::similarity(&measured(&processor), &measured(&card));
    let further_apart: u64 = processor
        .pixels()
        .chunks(4)
        .zip(card.pixels().chunks(4))
        .map(|(a, b)| a[..3].iter().zip(&b[..3]).filter(|&(x, y)| x.abs_diff(*y) > 4).count() as u64)
        .sum();
    let values = u64::from(processor.width()) * u64::from(processor.height()) * 3;
    println!("{name}: similarity {similarity:?}, {further_apart} of {values} values further apart than 4 levels");
    assert!(
        matches!(similarity, Some(value) if value >= LEAST_SIMILARITY),
        "{name}: similarity {similarity:?}"
    );
    assert!(
        further_apart * ONE_IN <= values,
        "{name}: {further_apart} of {values} values further apart than 4 levels"
    );
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_a_turned_scaled_and_slanted_fill_agrees_with_the_processor() {
    let map = about((80.0, 60.0), (12.5, -4.25), (1.6, 0.8), 12.0, 33.0);
    the_card_agrees_with_the_processor(
        "fill",
        &frame(vec![group(map, 1.0, None, vec![fill(50, 40, 60, 40, colour(220, 60, 40))])]),
    );
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_a_turned_text_agrees_with_the_processor() {
    let map = about((40.0, 60.0), (3.3, 7.7), (1.3, 1.3), 0.0, -21.0);
    the_card_agrees_with_the_processor("text", &frame(vec![group(map, 1.0, None, vec![text(&face(), 36.0, 30.0, 70.0)])]));
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_a_faded_group_over_another_fill_agrees_with_the_processor() {
    let map = about((100.0, 60.0), (0.0, 0.0), (1.0, 1.0), 0.0, 15.0);
    the_card_agrees_with_the_processor(
        "faded",
        &frame(vec![
            fill(20, 30, 160, 40, colour(40, 200, 90)),
            group(
                map,
                0.45,
                None,
                vec![
                    fill(60, 20, 50, 60, colour(220, 60, 40)),
                    fill(90, 40, 50, 60, colour(40, 80, 220)),
                    text(&face(), 24.0, 70.0, 60.0),
                ],
            ),
        ]),
    );
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_a_group_cut_to_a_rectangle_agrees_with_the_processor() {
    let map = about((100.0, 60.0), (5.0, 0.0), (2.0, 2.0), 0.0, 40.0);
    let cut = Rect {
        left: 40,
        top: 20,
        width: 120,
        height: 70,
    };
    the_card_agrees_with_the_processor(
        "cut",
        &frame(vec![group(
            map,
            0.8,
            Some(cut),
            vec![fill(70, 40, 60, 40, colour(220, 60, 40)), text(&face(), 30.0, 70.0, 75.0)],
        )]),
    );
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_the_kept_coverage_of_a_text_that_turns_in_every_frame_stops_rising_at_the_bound() {
    let face = face();
    let mut card = CardRenderer::new().unwrap();
    let mut kept = Vec::new();
    for number in 0..900u32 {
        let map = about((100.0, 60.0), (0.0, 0.0), (1.0, 1.0), 0.0, f64::from(number) * 0.37);
        let big = Frame {
            width: 640,
            height: 360,
            background: colour(0, 0, 0),
            marks: vec![group(map, 1.0, None, vec![text(&face, 140.0, 40.0, 220.0)])],
        };
        card.draw_frame(&big).unwrap();
        kept.push(card.mask_bytes());
    }
    let most = *kept.iter().max().unwrap();
    assert!(most <= MASK_BOUND, "the card kept {most} bytes, above the bound of {MASK_BOUND}");
    let first_full = kept
        .iter()
        .position(|&bytes| bytes * 10 >= MASK_BOUND * 9)
        .expect("the kept coverage reaches the bound");
    assert!(first_full < 600, "the kept coverage reached the bound only at frame {first_full}");
    let after = &kept[first_full..];
    assert!(
        after.iter().all(|&bytes| bytes * 10 >= MASK_BOUND * 9),
        "the kept coverage fell after it reached the bound"
    );
}
