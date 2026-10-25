//! The processor draws borders, borders of two lines, rounded corners and cut children exactly as `docs/spec/box-edges.md`
//! gives them, as worked values.

use draw::{Colour, Edges, Frame, Line, Mark, Picture, Renderer, Rgba, Step};
use draw_cpu::CpuRenderer;
use layout::Rect;

const UNDER: Colour = Colour {
    red: 16,
    green: 32,
    blue: 64,
};
const FILL: Colour = Colour {
    red: 240,
    green: 240,
    blue: 240,
};
const CHILD: Colour = Colour {
    red: 0,
    green: 160,
    blue: 0,
};
const BORDER: Rgba = Rgba {
    red: 200,
    green: 0,
    blue: 0,
    alpha: 255,
};

fn rect(left: i32, top: i32, width: u32, height: u32) -> Rect {
    Rect { left, top, width, height }
}

fn fill(rect: Rect, colour: Colour) -> Mark {
    Mark::Step(Step::Fill { rect, colour })
}

fn draw(marks: Vec<Mark>) -> Picture {
    CpuRenderer
        .draw_frame(&Frame {
            width: 96,
            height: 96,
            background: UNDER,
            marks,
        })
        .unwrap()
}

fn rgb(picture: &Picture, x: u32, y: u32) -> [u8; 3] {
    let [red, green, blue, alpha] = picture.pixel(x, y).unwrap();
    assert_eq!(alpha, 255);
    [red, green, blue]
}

fn of(colour: Colour) -> [u8; 3] {
    [colour.red, colour.green, colour.blue]
}

const RED: [u8; 3] = [200, 0, 0];

/// A box at (10, 10), 60 by 40, filled, with these edges.
fn boxed(edges: Edges, children: Vec<Mark>) -> Vec<Mark> {
    let place = rect(10, 10, 60, 40);
    edges.marks(place, vec![fill(place, FILL)], children)
}

fn border(width: f64, double: bool) -> Option<(Line, Rgba)> {
    Some((Line { width, double }, BORDER))
}

#[test]
fn a_border_of_six_pixels_is_six_pixels_inside_the_edge_and_the_box_keeps_its_size() {
    let picture = draw(boxed(
        Edges {
            radius: 0.0,
            border: border(6.0, false),
            cut: false,
        },
        Vec::new(),
    ));
    // Across row 30: outside, six columns of border, the inside, six columns of border, outside.
    assert_eq!(rgb(&picture, 9, 30), of(UNDER));
    for x in 10..16 {
        assert_eq!(rgb(&picture, x, 30), RED, "column {x}");
    }
    assert_eq!(rgb(&picture, 16, 30), of(FILL));
    assert_eq!(rgb(&picture, 63, 30), of(FILL));
    for x in 64..70 {
        assert_eq!(rgb(&picture, x, 30), RED, "column {x}");
    }
    assert_eq!(rgb(&picture, 70, 30), of(UNDER));
    // Down column 40: the same at the top and the bottom edge.
    assert_eq!(rgb(&picture, 40, 9), of(UNDER));
    for y in (10..16).chain(44..50) {
        assert_eq!(rgb(&picture, 40, y), RED, "row {y}");
    }
    assert_eq!(rgb(&picture, 40, 16), of(FILL));
    assert_eq!(rgb(&picture, 40, 43), of(FILL));
    assert_eq!(rgb(&picture, 40, 50), of(UNDER));
}

#[test]
fn a_square_border_of_whole_pixels_is_the_same_as_four_fills() {
    let picture = draw(boxed(
        Edges {
            radius: 0.0,
            border: border(6.0, false),
            cut: false,
        },
        Vec::new(),
    ));
    let red = Colour {
        red: 200,
        green: 0,
        blue: 0,
    };
    let fills = draw(vec![
        fill(rect(10, 10, 60, 40), FILL),
        fill(rect(10, 10, 60, 6), red),
        fill(rect(10, 44, 60, 6), red),
        fill(rect(10, 10, 6, 40), red),
        fill(rect(64, 10, 6, 40), red),
    ]);
    assert_eq!(picture, fills);
}

#[test]
fn a_border_of_two_lines_nine_wide_is_three_of_line_three_of_gap_three_of_line() {
    let picture = draw(boxed(
        Edges {
            radius: 0.0,
            border: border(9.0, true),
            cut: false,
        },
        Vec::new(),
    ));
    let across: Vec<[u8; 3]> = (9..21).map(|x| rgb(&picture, x, 30)).collect();
    let wanted = [
        of(UNDER),
        RED,
        RED,
        RED,
        of(FILL),
        of(FILL),
        of(FILL),
        RED,
        RED,
        RED,
        of(FILL),
        of(FILL),
    ];
    assert_eq!(across, wanted);
    let down: Vec<[u8; 3]> = (41..51).rev().map(|y| rgb(&picture, 40, y)).collect();
    assert_eq!(down[..10], wanted[..10]);
}

#[test]
fn a_rounded_corner_shows_what_is_under_the_box_outside_it() {
    let picture = draw(boxed(
        Edges {
            radius: 12.0,
            border: None,
            cut: false,
        },
        Vec::new(),
    ));
    // The corner pixel and its neighbours along the diagonal, outside the quarter circle, are what lies under the box.
    for step in 0..3 {
        assert_eq!(rgb(&picture, 10 + step, 10 + step), of(UNDER), "corner step {step}");
        assert_eq!(rgb(&picture, 69 - step, 49 - step), of(UNDER), "corner step {step}");
    }
    // Inside the circle, and along the straight sides, the box's fill.
    assert_eq!(rgb(&picture, 10 + 6, 10 + 6), of(FILL));
    assert_eq!(rgb(&picture, 10, 30), of(FILL));
    assert_eq!(rgb(&picture, 40, 10), of(FILL));
    // Where the circle crosses a pixel, the pixel lies between the two colours.
    let edge = rgb(&picture, 13, 13)[0];
    assert!(edge > UNDER.red && edge < FILL.red, "edge pixel {edge}");
}

#[test]
fn a_rounded_border_follows_the_corner() {
    let picture = draw(boxed(
        Edges {
            radius: 16.0,
            border: border(4.0, false),
            cut: false,
        },
        Vec::new(),
    ));
    // The diagonal from the corner: under the box, then the border on the arc, then the fill.
    let centre = 10.0 + 16.0;
    let on_arc = (centre - 14.0 / 2_f64.sqrt()).floor() as u32;
    assert_eq!(rgb(&picture, 11, 11), of(UNDER));
    assert_eq!(rgb(&picture, on_arc, on_arc), RED);
    assert_eq!(rgb(&picture, 22, 22), of(FILL));
    // Along a straight side the border is four pixels.
    for x in 10..14 {
        assert_eq!(rgb(&picture, x, 30), RED);
    }
    assert_eq!(rgb(&picture, 14, 30), of(FILL));
}

#[test]
fn a_box_that_cuts_shows_no_part_of_a_child_outside_its_edge() {
    let child = fill(rect(0, 0, 96, 96), CHILD);
    let cut = draw(boxed(
        Edges {
            radius: 12.0,
            border: None,
            cut: true,
        },
        vec![child.clone()],
    ));
    assert_eq!(rgb(&cut, 5, 5), of(UNDER));
    assert_eq!(rgb(&cut, 80, 30), of(UNDER));
    assert_eq!(rgb(&cut, 10, 10), of(UNDER), "outside the rounded corner");
    assert_eq!(rgb(&cut, 40, 30), of(CHILD));
    assert_eq!(rgb(&cut, 10, 30), of(CHILD));
    // Every pixel outside the box is untouched.
    for y in 0..96 {
        for x in 0..96 {
            if !(10..70).contains(&x) || !(10..50).contains(&y) {
                assert_eq!(rgb(&cut, x, y), of(UNDER), "pixel {x}, {y}");
            }
        }
    }
    let whole = draw(boxed(
        Edges {
            radius: 12.0,
            border: None,
            cut: false,
        },
        vec![child],
    ));
    assert_eq!(rgb(&whole, 5, 5), of(CHILD));
    assert_eq!(rgb(&whole, 80, 30), of(CHILD));
    assert_eq!(rgb(&whole, 10, 10), of(CHILD));
}

#[test]
fn a_cut_lies_inside_the_border() {
    let child = fill(rect(0, 0, 96, 96), CHILD);
    let picture = draw(boxed(
        Edges {
            radius: 0.0,
            border: border(6.0, false),
            cut: true,
        },
        vec![child],
    ));
    assert_eq!(rgb(&picture, 15, 30), RED);
    assert_eq!(rgb(&picture, 16, 30), of(CHILD));
    assert_eq!(rgb(&picture, 63, 30), of(CHILD));
    assert_eq!(rgb(&picture, 64, 30), RED);
}

#[test]
fn a_radius_larger_than_half_the_shorter_side_gives_round_ends() {
    let place = rect(10, 10, 60, 40);
    let draw_with = |radius: f64| {
        draw(
            Edges {
                radius,
                border: None,
                cut: false,
            }
            .marks(place, vec![fill(place, FILL)], Vec::new()),
        )
    };
    let capsule = draw_with(20.0);
    assert_eq!(draw_with(21.0), capsule);
    assert_eq!(draw_with(1000.0), capsule);
    // The ends are half circles of radius 20: the middle of each end touches the edge, the corners are what is under the box.
    assert_eq!(rgb(&capsule, 11, 30), of(FILL));
    assert!(rgb(&capsule, 10, 30)[0] > UNDER.red, "the end touches column 10");
    assert_eq!(rgb(&capsule, 68, 29), of(FILL));
    assert_eq!(rgb(&capsule, 11, 12), of(UNDER));
    assert_eq!(rgb(&capsule, 40, 10), of(FILL));
    // A square with a radius past its half is a circle.
    let square = rect(20, 20, 40, 40);
    let circle = draw(
        Edges {
            radius: 300.0,
            border: None,
            cut: false,
        }
        .marks(square, vec![fill(square, FILL)], Vec::new()),
    );
    assert_eq!(rgb(&circle, 40, 22), of(FILL));
    assert_eq!(rgb(&circle, 22, 40), of(FILL));
    assert!(rgb(&circle, 40, 20)[0] > UNDER.red, "the top of the circle touches row 20");
    assert_eq!(rgb(&circle, 40, 19), of(UNDER));
    assert_eq!(rgb(&circle, 24, 24), of(UNDER));
    assert_eq!(rgb(&circle, 26, 26), of(FILL));
}

#[test]
fn a_border_with_alpha_lays_its_colour_over_the_fill() {
    let half = Rgba { alpha: 128, ..BORDER };
    let place = rect(10, 10, 60, 40);
    let picture = draw(
        Edges {
            radius: 0.0,
            border: Some((Line { width: 4.0, double: false }, half)),
            cut: false,
        }
        .marks(place, vec![fill(place, FILL)], Vec::new()),
    );
    // 200 * 128 / 255 + 240 * 127 / 255, each rounded: 100 + 120 = 220 of red; 240 * 127 / 255 = 120 of green and blue.
    assert_eq!(rgb(&picture, 11, 30), [220, 120, 120]);
}
