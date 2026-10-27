//! A curve of a shape is drawn smooth: its edge lies within a quarter pixel of the true curve (`docs/spec/shape.md`, section
//! 4, decision A29). The edge of a filled circle is read from the coverage of each row, and compared with the circle's own
//! edge, worked from its radius and centre.

use draw::{Colour, Frame, Mark, Path, Picture, Renderer, Rgba, ShapeMark, ViewBox};
use draw_cpu::CpuRenderer;

const UNDER: Colour = Colour {
    red: 16,
    green: 32,
    blue: 64,
};
const INK: Rgba = Rgba {
    red: 224,
    green: 160,
    blue: 48,
    alpha: 255,
};

const SIZE: u32 = 100;
const CENTRE: f64 = 50.0;
const RADIUS: f64 = 40.0;

/// The fraction of a pixel that is ink, read from its red channel, which runs from the background's to the ink's.
fn fraction(picture: &Picture, x: u32, y: u32) -> f64 {
    let [red, _, _, _] = picture.pixel(x, y).unwrap();
    (f64::from(red) - f64::from(UNDER.red)) / (f64::from(INK.red) - f64::from(UNDER.red))
}

#[test]
fn a_filled_circle_s_edge_lies_within_a_quarter_pixel_of_the_true_circle() {
    // A circle of radius 40 about (50, 50), drawn as two half circles. On each row the ink lies to the right of the left
    // edge, so the edge is 50 minus the ink of the row's pixels from 0 to 49.
    let mark = Mark::Shape(ShapeMark {
        shadow: None,
        left: 0.0,
        top: 0.0,
        width: f64::from(SIZE),
        height: f64::from(SIZE),
        view_box: ViewBox {
            x: 0.0,
            y: 0.0,
            width: f64::from(SIZE),
            height: f64::from(SIZE),
        },
        path: Path::parse("M 10 50 A 40 40 0 0 1 90 50 A 40 40 0 0 1 10 50 Z").unwrap(),
        colour: INK,
        stroke_width: None,
        dashes: Vec::new(),
    });
    let picture = CpuRenderer
        .draw_frame(&Frame {
            width: SIZE,
            height: SIZE,
            background: UNDER,
            marks: vec![mark],
        })
        .unwrap();
    let mut worst: f64 = 0.0;
    for y in 15..85 {
        let centre_y = f64::from(y) + 0.5;
        let true_edge = CENTRE - (RADIUS * RADIUS - (centre_y - CENTRE).powi(2)).sqrt();
        let ink: f64 = (0..CENTRE as u32).map(|x| fraction(&picture, x, y)).sum();
        let measured_edge = CENTRE - ink;
        worst = worst.max((measured_edge - true_edge).abs());
    }
    assert!(worst <= 0.25, "the worst row is {worst} pixels from the true circle");
}
