//! A view box of another shape than its box is scaled evenly and centred in it (`docs/spec/shape.md`,
//! section 3): one factor, the smaller of the two, and the centre of the view box at the centre of the box.

use draw::{Colour, Frame, Mark, Path, Renderer, Rgba, ShapeMark, ViewBox};
use draw_cpu::CpuRenderer;

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

/// A 48 by 48 box holding a 24 by 12 view box filled whole: the factor is 2, the smaller of 48/24 and
/// 48/12, so the shape is 48 wide and 24 high, centred on row 24: rows 12 to 36.
fn centred_wide() -> draw::Picture {
    CpuRenderer
        .draw_frame(&Frame {
            width: 48,
            height: 48,
            background: BACKGROUND,
            marks: vec![Mark::Shape(ShapeMark {
                shadow: None,
                left: 0.0,
                top: 0.0,
                width: 48.0,
                height: 48.0,
                view_box: ViewBox {
                    x: 0.0,
                    y: 0.0,
                    width: 24.0,
                    height: 12.0,
                },
                path: Path::parse("M 0 0 H 24 V 12 H 0 Z").unwrap(),
                colour: INK,
                stroke_width: None,
                dashes: Vec::new(),
            })],
        })
        .unwrap()
}

#[test]
fn a_wide_view_box_is_scaled_by_the_narrower_factor_and_centred() {
    let picture = centred_wide();
    let ink = [INK.red, INK.green, INK.blue];
    let ground = [BACKGROUND.red, BACKGROUND.green, BACKGROUND.blue];
    for y in [6, 42] {
        let [red, green, blue, alpha] = picture.pixel(24, y).unwrap();
        assert_eq!([red, green, blue, alpha], [ground[0], ground[1], ground[2], 255], "row {y}");
    }
    for y in [18, 30] {
        let [red, green, blue, alpha] = picture.pixel(24, y).unwrap();
        assert_eq!([red, green, blue, alpha], [ink[0], ink[1], ink[2], 255], "row {y}");
    }
    // The shape fills the box edge to edge across: the factor is even, not fitted per axis.
    let [red, green, blue, _] = picture.pixel(2, 24).unwrap();
    assert_eq!([red, green, blue], ink);
    let [red, green, blue, _] = picture.pixel(45, 24).unwrap();
    assert_eq!([red, green, blue], ink);
}
