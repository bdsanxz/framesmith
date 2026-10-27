//! A shape of path data is drawn in its box as `docs/spec/shape.md` gives it (sections 3 and 4): the corners of a square
//! path lie at the box's corners, a fill is the colour inside the outline and nothing outside, and a stroke is a line of
//! the written width centred on the path. The expected pixels are worked by hand from those sections.

use draw::{Colour, Frame, Mark, Path, Picture, Renderer, Rgba, ShapeMark, ViewBox};
use draw_cpu::CpuRenderer;
use layout::Rect;

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
const INK_BYTES: [u8; 3] = [224, 160, 48];
const UNDER_BYTES: [u8; 3] = [16, 32, 64];

fn shape(rect: Rect, view_box: ViewBox, data: &str, stroke_width: Option<f64>, dashes: Vec<f64>) -> Mark {
    Mark::Shape(ShapeMark {
        shadow: None,
        left: f64::from(rect.left),
        top: f64::from(rect.top),
        width: f64::from(rect.width),
        height: f64::from(rect.height),
        view_box,
        path: Path::parse(data).unwrap(),
        colour: INK,
        stroke_width,
        dashes,
    })
}

fn draw(size: u32, mark: Mark) -> Picture {
    CpuRenderer
        .draw_frame(&Frame {
            width: size,
            height: size,
            background: UNDER,
            marks: vec![mark],
        })
        .unwrap()
}

fn rgb(picture: &Picture, x: u32, y: u32) -> [u8; 3] {
    let [red, green, blue, alpha] = picture.pixel(x, y).unwrap();
    assert_eq!(alpha, 255);
    [red, green, blue]
}

/// The number of pixels of the colour in the picture.
fn count_of(picture: &Picture, colour: [u8; 3]) -> usize {
    let size = picture.width();
    (0..size)
        .flat_map(|y| (0..size).map(move |x| (x, y)))
        .filter(|&(x, y)| rgb(picture, x, y) == colour)
        .count()
}

fn view(width: f64, height: f64) -> ViewBox {
    ViewBox {
        x: 0.0,
        y: 0.0,
        width,
        height,
    }
}

#[test]
fn a_square_path_in_a_view_box_of_the_box_s_shape_has_its_corners_at_the_box_s_corners() {
    // The box is 4 to 36 on both axes; the square path is 0 to 24 in a view box of 24, so it is drawn 4 to 36.
    let mark = shape(
        Rect {
            left: 4,
            top: 4,
            width: 32,
            height: 32,
        },
        view(24.0, 24.0),
        "M 0 0 H 24 V 24 H 0 Z",
        None,
        Vec::new(),
    );
    let picture = draw(40, mark);
    assert_eq!(rgb(&picture, 4, 4), INK_BYTES, "the top-left corner is inside");
    assert_eq!(rgb(&picture, 35, 35), INK_BYTES, "the bottom-right corner is inside");
    assert_eq!(rgb(&picture, 3, 3), UNDER_BYTES, "one pixel above and left is outside");
    assert_eq!(rgb(&picture, 36, 36), UNDER_BYTES, "one pixel below and right is outside");
    assert_eq!(count_of(&picture, INK_BYTES), 32 * 32);
}

#[test]
fn a_fill_shows_its_colour_inside_the_outline_and_nothing_outside() {
    // A half-size square in a view box of 12 is 16 by 16 in a box of 32 by 32 (scale 32 / 12 is 8 / 3 per unit, so the
    // square 0 to 6 is 16 pixels): it covers 4 to 20 on both axes.
    let mark = shape(
        Rect {
            left: 4,
            top: 4,
            width: 32,
            height: 32,
        },
        view(12.0, 12.0),
        "M 0 0 H 6 V 6 H 0 Z",
        None,
        Vec::new(),
    );
    let picture = draw(40, mark);
    assert_eq!(rgb(&picture, 4, 4), INK_BYTES);
    assert_eq!(rgb(&picture, 19, 19), INK_BYTES);
    assert_eq!(rgb(&picture, 20, 20), UNDER_BYTES, "just past the square is outside");
    assert_eq!(rgb(&picture, 30, 30), UNDER_BYTES, "the box is larger than the square");
    assert_eq!(count_of(&picture, INK_BYTES), 16 * 16);
}

#[test]
fn a_view_box_of_another_size_scales_the_path_to_the_element_s_box() {
    // The same square drawn in a view box of 24 and in one of 12 (each the full box) gives the same picture.
    let rect = Rect {
        left: 4,
        top: 4,
        width: 32,
        height: 32,
    };
    let wide = draw(40, shape(rect, view(24.0, 24.0), "M 0 0 H 24 V 24 H 0 Z", None, Vec::new()));
    let narrow = draw(40, shape(rect, view(12.0, 12.0), "M 0 0 H 12 V 12 H 0 Z", None, Vec::new()));
    assert_eq!(wide.pixels(), narrow.pixels());
}

#[test]
fn a_stroke_is_a_line_of_the_written_width_centred_on_the_path() {
    // The path is the square 10 to 30 with its corners at 10 and 30; a stroke of 4 covers 8 to 12 on each side of it.
    let mark = shape(
        Rect {
            left: 0,
            top: 0,
            width: 40,
            height: 40,
        },
        view(40.0, 40.0),
        "M 10 10 H 30 V 30 H 10 Z",
        Some(4.0),
        Vec::new(),
    );
    let picture = draw(40, mark);
    assert_eq!(rgb(&picture, 7, 20), UNDER_BYTES, "one pixel outside the line is not inked");
    assert_eq!(rgb(&picture, 8, 20), INK_BYTES, "the line starts 2 units outside the path");
    assert_eq!(rgb(&picture, 11, 20), INK_BYTES, "and ends 2 units inside it");
    assert_eq!(rgb(&picture, 12, 20), UNDER_BYTES, "the inside of the square is not inked");
    assert_eq!(rgb(&picture, 8, 8), INK_BYTES, "the mitred corner is inked");
    assert_eq!(rgb(&picture, 20, 20), UNDER_BYTES, "a stroke is not a fill");
}

#[test]
fn a_stroke_is_not_a_fill_and_a_fill_is_not_a_stroke() {
    let filled = draw(
        40,
        shape(
            Rect {
                left: 0,
                top: 0,
                width: 40,
                height: 40,
            },
            view(40.0, 40.0),
            "M 10 10 H 30 V 30 H 10 Z",
            None,
            Vec::new(),
        ),
    );
    assert_eq!(rgb(&filled, 20, 20), INK_BYTES, "a fill is inked inside");
    assert_eq!(rgb(&filled, 9, 20), UNDER_BYTES, "and not outside");
}

#[test]
fn a_view_box_fits_evenly_and_is_centred_in_a_box_of_another_shape() {
    // A box 40 wide and 20 high holds a 10 by 10 view box at the smaller ratio, 20 / 10 = 2: the square is 20 by 20, centred
    // across, so it covers 10 to 30 on the row axis and 0 to 20 down.
    let mark = shape(
        Rect {
            left: 0,
            top: 0,
            width: 40,
            height: 20,
        },
        view(10.0, 10.0),
        "M 0 0 H 10 V 10 H 0 Z",
        None,
        Vec::new(),
    );
    let picture = draw(40, mark);
    assert_eq!(rgb(&picture, 10, 0), INK_BYTES);
    assert_eq!(rgb(&picture, 29, 19), INK_BYTES);
    assert_eq!(rgb(&picture, 9, 10), UNDER_BYTES, "one pixel left of the square is outside");
    assert_eq!(rgb(&picture, 30, 10), UNDER_BYTES, "one pixel right of the square is outside");
    assert_eq!(count_of(&picture, INK_BYTES), 20 * 20);
}
