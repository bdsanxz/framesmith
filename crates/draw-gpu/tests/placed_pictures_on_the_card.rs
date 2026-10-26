//! The card lays a placed picture as the processor does (`docs/spec/card-renderer.md`, section 12): moved by whole pixels it
//! gives the processor's values exactly, and turned, scaled or cut it agrees within the tolerance of section 4. Each test needs
//! a graphics card that frx7 can draw with, so each is reference machine only (`docs/dev/gates.md` section 4).

use std::sync::Arc;

use draw::{Colour, Frame, Group, Map, Mark, Picture, PlacedPicture, Renderer};
use draw_cpu::CpuRenderer;
use draw_gpu::CardRenderer;
use layout::Rect;

const LEAST_SIMILARITY: f64 = 0.995;
const ONE_IN: u64 = 2000;

/// A picture of `width` by `height` in bands of colour, with a soft edge of half-covered pixels on its right side.
fn banded(width: u32, height: u32, x: f32, y: f32) -> PlacedPicture {
    let mut pixels = Vec::new();
    for row in 0..height {
        for column in 0..width {
            let band = (column / 8 + row / 6) % 3;
            let mut pixel = [[200u8, 40, 30, 255], [30, 160, 220, 255], [240, 220, 60, 255]][band as usize];
            if column + 4 >= width {
                pixel = [pixel[0] / 2, pixel[1] / 2, pixel[2] / 2, 128];
            }
            pixels.extend_from_slice(&pixel);
        }
    }
    PlacedPicture {
        width,
        height,
        pixels: Arc::new(pixels),
        x,
        y,
    }
}

fn frame(marks: Vec<Mark>) -> Frame {
    Frame {
        width: 200,
        height: 120,
        background: Colour {
            red: 16,
            green: 32,
            blue: 64,
        },
        marks,
    }
}

fn group(map: Map, clip: Option<Rect>, marks: Vec<Mark>) -> Mark {
    Mark::Group(Group {
        map,
        opacity: 1.0,
        clip,
        marks,
        turn: None,
    })
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
fn reference_machine_only_a_picture_moved_by_whole_pixels_is_the_processors_to_the_byte() {
    let frame = frame(vec![
        Mark::Picture(banded(64, 36, 10.4, 7.5)),
        group(Map::moving(80.0, 40.0), None, vec![Mark::Picture(banded(48, 30, 3.0, 2.0))]),
    ]);
    let processor = CpuRenderer.draw_frame(&frame).unwrap();
    let card = CardRenderer::new().unwrap().draw_frame(&frame).unwrap();
    assert_eq!(processor.pixels(), card.pixels());
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_a_turned_and_scaled_picture_agrees_with_the_processor() {
    let (sin, cos) = 0.35f64.sin_cos();
    let map = Map {
        a: 1.4 * cos,
        b: 1.4 * sin,
        c: -1.1 * sin,
        d: 1.1 * cos,
        e: 90.0,
        f: 20.0,
    };
    the_card_agrees_with_the_processor(
        "turned",
        &frame(vec![group(map, None, vec![Mark::Picture(banded(64, 40, 0.0, 0.0))])]),
    );
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_a_cut_picture_agrees_with_the_processor() {
    let clip = Some(Rect {
        left: 30,
        top: 20,
        width: 70,
        height: 50,
    });
    the_card_agrees_with_the_processor(
        "cut",
        &frame(vec![group(Map::IDENTITY, clip, vec![Mark::Picture(banded(120, 80, 12.0, 8.0))])]),
    );
}
