//! The card draws shapes of path data as the processor does (`docs/spec/shape.md`, sections 3 to 5): each frame agrees with the
//! processor's picture within the tolerance of `docs/spec/card-renderer.md` section 4. The card places the processor's
//! shape layer (`docs/spec/card-renderer.md`, section 9.9), so these frames check that placement. Each test needs a
//! graphics card that frx7 can draw with, so each is reference machine only (`docs/dev/gates.md` section 4).

use draw::{Colour, Frame, Mark, Path, Picture, Renderer, Rgba, ShapeMark, ViewBox};
use draw_cpu::CpuRenderer;
use draw_gpu::CardRenderer;
use layout::Rect;

const LEAST_SIMILARITY: f64 = 0.995;
const ONE_IN: u64 = 2000;

const INK: Rgba = Rgba {
    red: 224,
    green: 160,
    blue: 48,
    alpha: 255,
};

fn colour(red: u8, green: u8, blue: u8) -> Colour {
    Colour { red, green, blue }
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

/// A shape in the box of `rect`, of a view box the same size as the box, so that one view-box unit is one pixel.
fn shape(rect: Rect, data: &str, stroke_width: Option<f64>, dashes: Vec<f64>) -> Mark {
    Mark::Shape(ShapeMark {
        left: f64::from(rect.left),
        top: f64::from(rect.top),
        width: f64::from(rect.width),
        height: f64::from(rect.height),
        view_box: ViewBox {
            x: 0.0,
            y: 0.0,
            width: f64::from(rect.width),
            height: f64::from(rect.height),
        },
        path: Path::parse(data).unwrap(),
        colour: INK,
        stroke_width,
        dashes,
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
fn reference_machine_only_a_filled_square_path_agrees_with_the_processor() {
    let rect = Rect {
        left: 20,
        top: 20,
        width: 60,
        height: 40,
    };
    the_card_agrees_with_the_processor(
        "filled square",
        &frame(vec![shape(rect, "M 0 0 H 60 V 40 H 0 Z", None, Vec::new())]),
    );
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_a_dashed_stroke_with_round_ends_agrees_with_the_processor() {
    let rect = Rect {
        left: 10,
        top: 40,
        width: 180,
        height: 20,
    };
    the_card_agrees_with_the_processor(
        "dashed line",
        &frame(vec![shape(rect, "M 10 10 L 170 10", Some(4.0), vec![10.0, 5.0])]),
    );
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_a_curve_agrees_with_the_processor() {
    let rect = Rect {
        left: 40,
        top: 10,
        width: 100,
        height: 100,
    };
    the_card_agrees_with_the_processor(
        "circle",
        &frame(vec![shape(
            rect,
            "M 10 50 A 40 40 0 0 1 90 50 A 40 40 0 0 1 10 50 Z",
            None,
            Vec::new(),
        )]),
    );
    the_card_agrees_with_the_processor(
        "curved line",
        &frame(vec![shape(
            Rect {
                left: 10,
                top: 10,
                width: 180,
                height: 100,
            },
            "M 0 90 C 60 0 120 100 180 10",
            Some(3.0),
            Vec::new(),
        )]),
    );
}
