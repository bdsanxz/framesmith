//! The card draws borders, borders of two lines, rounded corners and cut children as the processor does
//! (`docs/spec/box-edges.md`, section 7): each frame agrees with the processor's picture within the tolerance of
//! `docs/spec/card-renderer.md` section 4, and a square border of whole pixels is the processor's picture exactly. Each test
//! needs a graphics card that frx7 can draw with, so each is reference machine only (`docs/dev/gates.md` section 4).

use draw::{Colour, Edges, Filter, Frame, Group, Line, Map, Mark, Picture, Renderer, Rgba, Step};
use draw_cpu::CpuRenderer;
use draw_gpu::CardRenderer;
use layout::Rect;

const LEAST_SIMILARITY: f64 = 0.995;
const ONE_IN: u64 = 2000;

fn colour(red: u8, green: u8, blue: u8) -> Colour {
    Colour { red, green, blue }
}

fn rect(left: i32, top: i32, width: u32, height: u32) -> Rect {
    Rect { left, top, width, height }
}

fn fill(rect: Rect, colour: Colour) -> Mark {
    Mark::Step(Step::Fill { rect, colour })
}

const BORDER: Rgba = Rgba {
    red: 200,
    green: 20,
    blue: 30,
    alpha: 255,
};

fn frame(marks: Vec<Mark>) -> Frame {
    Frame {
        width: 200,
        height: 120,
        background: colour(16, 32, 64),
        marks,
    }
}

/// A box at (20, 20), 160 by 80, filled, with these edges and children.
fn boxed(radius: f64, border: Option<(f64, bool)>, cut: bool, children: Vec<Mark>) -> Vec<Mark> {
    let place = rect(20, 20, 160, 80);
    Edges {
        radius,
        border: border.map(|(width, double)| (Line { width, double }, BORDER)),
        cut,
    }
    .marks(place, vec![fill(place, colour(235, 230, 220))], children)
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
fn reference_machine_only_a_square_border_of_whole_pixels_is_the_processors_picture() {
    let frame = frame(boxed(0.0, Some((6.0, false)), false, Vec::new()));
    let processor = CpuRenderer.draw_frame(&frame).unwrap();
    let card = CardRenderer::new().unwrap().draw_frame(&frame).unwrap();
    assert_eq!(card, processor);
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_a_border_of_two_lines_agrees_with_the_processor() {
    the_card_agrees_with_the_processor("double", &frame(boxed(0.0, Some((9.0, true)), false, Vec::new())));
    the_card_agrees_with_the_processor("rounded double", &frame(boxed(18.0, Some((9.0, true)), false, Vec::new())));
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_rounded_corners_and_border_agree_with_the_processor() {
    the_card_agrees_with_the_processor("rounded", &frame(boxed(24.0, None, false, Vec::new())));
    the_card_agrees_with_the_processor("rounded border", &frame(boxed(24.0, Some((5.5, false)), false, Vec::new())));
    the_card_agrees_with_the_processor("capsule", &frame(boxed(500.0, Some((4.0, false)), false, Vec::new())));
    let tall = rect(80, 10, 30, 100);
    let standing = Edges {
        radius: 400.0,
        border: Some((Line { width: 4.0, double: false }, BORDER)),
        cut: false,
    }
    .marks(tall, vec![fill(tall, colour(235, 230, 220))], Vec::new());
    the_card_agrees_with_the_processor("standing capsule", &frame(standing));
    the_card_agrees_with_the_processor("wide double", &frame(boxed(0.0, Some((60.0, true)), false, Vec::new())));
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_cut_children_agree_with_the_processor() {
    let children = vec![
        fill(rect(0, 0, 120, 120), colour(30, 160, 60)),
        fill(rect(100, 50, 100, 70), colour(240, 200, 30)),
    ];
    the_card_agrees_with_the_processor("cut", &frame(boxed(24.0, Some((4.0, false)), true, children.clone())));
    the_card_agrees_with_the_processor("not cut", &frame(boxed(24.0, Some((4.0, false)), false, children.clone())));
    // A cut inside a cut, and a cut box that turns.
    let inner = Edges {
        radius: 30.0,
        border: None,
        cut: true,
    }
    .marks(rect(40, 30, 80, 60), Vec::new(), children.clone());
    the_card_agrees_with_the_processor("cut in a cut", &frame(boxed(24.0, None, true, inner)));
    let (sin, cos) = 17_f64.to_radians().sin_cos();
    let turn = Map::moving(100.0, 60.0)
        .then_after(Map {
            a: cos,
            b: sin,
            c: -sin,
            d: cos,
            e: 0.0,
            f: 0.0,
        })
        .then_after(Map::moving(-100.0, -60.0));
    the_card_agrees_with_the_processor(
        "turned",
        &frame(vec![Mark::Group(Group {
            map: turn,
            opacity: 0.8,
            clip: None,
            marks: boxed(24.0, Some((6.0, true)), true, children),
            turn: None,
            filter: Filter::NONE,
        })]),
    );
}
