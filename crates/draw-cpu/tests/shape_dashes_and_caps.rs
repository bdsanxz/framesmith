//! A dashed line has runs of its dash and gaps of its gap along the centre line, every dash end butt; the two ends of an
//! open outline are round, and a dash that holds an end takes that end's round cap (`docs/spec/shape.md`, sections 4 and
//! 5). The expected pixels are worked by hand: a line of width 2 on the row 5 covers the row's pixels 0 to 40 in the
//! middle, and a round cap of radius 1 reaches one pixel past an end.

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
const UNDER_BYTES: [u8; 3] = [16, 32, 64];

/// A 40 by 10 frame and a box of the same size, with a view box in the same units, so that one unit is one pixel.
fn draw_line(data: &str, stroke_width: f64, dashes: Vec<f64>) -> Picture {
    let mark = Mark::Shape(ShapeMark {
        left: 0.0,
        top: 0.0,
        width: 40.0,
        height: 10.0,
        view_box: ViewBox {
            x: 0.0,
            y: 0.0,
            width: 40.0,
            height: 10.0,
        },
        path: Path::parse(data).unwrap(),
        colour: INK,
        stroke_width: Some(stroke_width),
        dashes,
    });
    CpuRenderer
        .draw_frame(&Frame {
            width: 40,
            height: 10,
            background: UNDER,
            marks: vec![mark],
        })
        .unwrap()
}

/// Whether any of the pixel is inked: a round cap that reaches only part of a pixel blends it with the background.
fn inked(picture: &Picture, x: u32, y: u32) -> bool {
    let [red, green, blue, alpha] = picture.pixel(x, y).unwrap();
    assert_eq!(alpha, 255);
    [red, green, blue] != UNDER_BYTES
}

#[test]
fn dashes_of_ten_and_gaps_of_five_run_along_the_centre_line_with_butt_ends() {
    // The line from 10 to 30 is 20 long: a dash of 10 (distance 0 to 10), a gap of 5 (10 to 15), then a dash from 15 to 25
    // and a gap 25 to 30. Pixels 10 to 19 are the first dash, 20 to 24 the gap, 25 to 29 the second dash.
    let picture = draw_line("M 10 5 L 30 5", 2.0, vec![10.0, 5.0]);
    for x in 10..20 {
        assert!(inked(&picture, x, 5), "pixel {x} is in a dash");
    }
    for x in 20..25 {
        assert!(!inked(&picture, x, 5), "pixel {x} is in a gap of 5");
    }
    for x in 25..30 {
        assert!(inked(&picture, x, 5), "pixel {x} is in the second dash");
    }
}

#[test]
fn the_end_of_a_dash_inside_the_line_is_butt_not_round() {
    // The first dash ends at 20; a round cap would ink pixel 20, so it stays out.
    let picture = draw_line("M 10 5 L 30 5", 2.0, vec![10.0, 5.0]);
    assert!(inked(&picture, 19, 5));
    assert!(!inked(&picture, 20, 5), "a dash end inside the line has no round cap");
}

#[test]
fn an_open_outline_has_a_round_end_at_its_start_and_at_its_end() {
    // The round cap of radius 1 at (10, 5) reaches pixel 9 at its centre (9.5, 5.5), which is 0.71 from the end.
    let picture = draw_line("M 10 5 L 30 5", 2.0, vec![10.0, 5.0]);
    assert!(inked(&picture, 9, 5), "the round start cap reaches the pixel before the start");
    assert!(!inked(&picture, 8, 5));
    // The second dash holds the end at 30 (distance 20 is in the dash from 15 to 25), so the round cap reaches pixel 30.
    assert!(inked(&picture, 30, 5), "the round end cap reaches the pixel past the end");
    assert!(!inked(&picture, 31, 5));
}

#[test]
fn an_end_in_a_gap_takes_no_round_cap() {
    // The line from 10 to 25 is 15 long, which ends a gap's start: distance 15 begins a dash that holds no part of it, so
    // pixel 25 is not inked, while a dash that holds the start is round as before.
    let picture = draw_line("M 10 5 L 25 5", 2.0, vec![10.0, 5.0]);
    assert!(inked(&picture, 19, 5));
    assert!(!inked(&picture, 20, 5));
    assert!(!inked(&picture, 25, 5), "the end in the gap has no round cap");
    assert!(inked(&picture, 9, 5), "the start is round");
}

#[test]
fn a_solid_open_outline_has_round_ends_of_its_line_s_width() {
    let picture = draw_line("M 10 5 L 30 5", 2.0, Vec::new());
    assert!(inked(&picture, 9, 5));
    assert!(inked(&picture, 30, 5));
    assert!(!inked(&picture, 31, 5));
    assert!(!inked(&picture, 8, 5));
}

#[test]
fn a_lone_move_stroked_is_a_round_dot_of_the_line_s_width() {
    // A subpath with no length is drawn as a dot of radius 1 at its point (section 4).
    let picture = draw_line("M 20 5", 2.0, Vec::new());
    assert!(inked(&picture, 19, 5), "the dot reaches one pixel to the left");
    assert!(inked(&picture, 20, 5));
    assert!(!inked(&picture, 17, 5));
}
