//! The edges of a shape on the processor hold the coverage worked out from its geometry (`fixtures/shape-worked-values.md`):
//! a pixel inside or outside the outline is exact, and a pixel the outline crosses is within one level of the coverage
//! worked from the exact circle, or from the dash pattern and the round caps. The values are worked independently of
//! this renderer.

use draw::{Colour, Frame, Mark, Path, Renderer, Rgba, ShapeMark, ViewBox};
use draw_cpu::CpuRenderer;
use layout::Rect;

const BACKGROUND: Colour = Colour {
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
const INK_BYTES: [u8; 3] = [224, 160, 48];
const BACKGROUND_BYTES: [u8; 3] = [16, 32, 64];

/// The most a partly covered pixel may lie from its worked value, in levels.
const TOLERANCE: i32 = 1;

fn shape(width: f64, view_box: f64, data: &str, stroke_width: Option<f64>, dashes: Vec<f64>) -> Mark {
    Mark::Shape(ShapeMark {
        left: 0.0,
        top: 0.0,
        width,
        height: width,
        view_box: ViewBox {
            x: 0.0,
            y: 0.0,
            width: view_box,
            height: view_box,
        },
        path: Path::parse(data).unwrap(),
        colour: INK,
        stroke_width,
        dashes,
    })
}

fn picture_of(size: u32, mark: Mark) -> draw::Picture {
    CpuRenderer
        .draw_frame(&Frame {
            width: size,
            height: size,
            background: BACKGROUND,
            marks: vec![mark],
        })
        .unwrap()
}

/// Checks each probe `(x, y, worked, partly)`: an exact probe must match its worked bytes, and a partly covered one must
/// be within `TOLERANCE` levels of them in every channel.
fn hold_probes(name: &str, picture: &draw::Picture, probes: &[(u32, u32, [u8; 3], bool)]) {
    for &(x, y, worked, partly) in probes {
        let [red, green, blue, alpha] = picture.pixel(x, y).unwrap();
        assert_eq!(alpha, 255, "{name}: ({x}, {y}) is not opaque");
        let got = [red, green, blue];
        let worst = (0..3).map(|i| (i32::from(got[i]) - i32::from(worked[i])).abs()).max().unwrap();
        if partly {
            assert!(
                worst <= TOLERANCE,
                "{name}: ({x}, {y}) is {got:?}, worked {worked:?}, {worst} levels apart"
            );
        } else {
            assert_eq!(got, worked, "{name}: ({x}, {y})");
        }
    }
}

#[test]
fn a_curve_edge_holds_the_coverage_of_the_exact_circle() {
    // A circle of radius 40 at (50, 50) in a 100 by 100 box: the pixels the circle's edge crosses at the middle of each
    // side are 0.9959 covered, so 223, 159, 48 (`fixtures/shape-worked-values.md`, section 5).
    let picture = picture_of(
        100,
        shape(100.0, 100.0, "M 10 50 A 40 40 0 0 1 90 50 A 40 40 0 0 1 10 50 Z", None, Vec::new()),
    );
    let edge = [223, 159, 48];
    hold_probes(
        "curve",
        &picture,
        &[
            (11, 50, INK_BYTES, false),
            (9, 50, BACKGROUND_BYTES, false),
            (90, 50, BACKGROUND_BYTES, false),
            (89, 50, edge, true),
            (50, 11, INK_BYTES, false),
            (50, 9, BACKGROUND_BYTES, false),
            (50, 90, BACKGROUND_BYTES, false),
            (50, 89, edge, true),
            (10, 50, edge, true),
            (50, 10, edge, true),
            (10, 40, BACKGROUND_BYTES, false),
            (10, 60, BACKGROUND_BYTES, false),
            (40, 10, BACKGROUND_BYTES, false),
            (60, 10, BACKGROUND_BYTES, false),
            (15, 15, BACKGROUND_BYTES, false),
            (50, 50, INK_BYTES, false),
        ],
    );
}

#[test]
fn a_round_cap_holds_the_coverage_of_its_half_disc() {
    // A line of width 4 about y = 16 from x = 8 to x = 56, dashed 10 and 5: the start cap is a half disc of radius 2 on
    // the left of x = 8, covered 0.9132 at column 6 on row 15, so 206, 149, 49 (`fixtures/shape-worked-values.md`,
    // section 4). The end cap mirrors it at x = 56.
    let picture = picture_of(64, shape(64.0, 64.0, "M 8 16 H 56", Some(4.0), vec![10.0, 5.0]));
    let cap = [206, 149, 49];
    hold_probes(
        "dashes",
        &picture,
        &[
            (6, 15, cap, true),
            (57, 15, cap, true),
            (7, 15, INK_BYTES, false),
            (56, 15, INK_BYTES, false),
            (5, 16, BACKGROUND_BYTES, false),
            (58, 16, BACKGROUND_BYTES, false),
            (8, 14, INK_BYTES, false),
            (8, 17, INK_BYTES, false),
            (12, 14, INK_BYTES, false),
            (12, 17, INK_BYTES, false),
            (17, 15, INK_BYTES, false),
            (18, 15, BACKGROUND_BYTES, false),
            (22, 15, BACKGROUND_BYTES, false),
            (23, 15, INK_BYTES, false),
            (32, 15, INK_BYTES, false),
            (33, 15, BACKGROUND_BYTES, false),
            (37, 15, BACKGROUND_BYTES, false),
            (38, 15, INK_BYTES, false),
            (47, 15, INK_BYTES, false),
            (48, 15, BACKGROUND_BYTES, false),
            (52, 15, BACKGROUND_BYTES, false),
            (53, 15, INK_BYTES, false),
            (55, 15, INK_BYTES, false),
            (12, 13, BACKGROUND_BYTES, false),
            (12, 18, BACKGROUND_BYTES, false),
            (30, 30, BACKGROUND_BYTES, false),
        ],
    );
}

#[test]
fn a_square_edge_on_a_whole_pixel_is_exact() {
    // The square of the corners fixture: its edges lie on whole pixels, so every pixel is exactly ink or background.
    let rect = Rect {
        left: 8,
        top: 8,
        width: 48,
        height: 48,
    };
    let picture = picture_of(
        64,
        Mark::Shape(ShapeMark {
            left: f64::from(rect.left),
            top: f64::from(rect.top),
            width: f64::from(rect.width),
            height: f64::from(rect.height),
            view_box: ViewBox {
                x: 0.0,
                y: 0.0,
                width: 24.0,
                height: 24.0,
            },
            path: Path::parse("M 0 0 H 24 V 24 H 0 Z").unwrap(),
            colour: INK,
            stroke_width: None,
            dashes: Vec::new(),
        }),
    );
    hold_probes(
        "square",
        &picture,
        &[
            (8, 8, INK_BYTES, false),
            (55, 55, INK_BYTES, false),
            (7, 7, BACKGROUND_BYTES, false),
            (56, 56, BACKGROUND_BYTES, false),
            (7, 20, BACKGROUND_BYTES, false),
            (56, 40, BACKGROUND_BYTES, false),
        ],
    );
}
