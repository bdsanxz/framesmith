//! The shadow of a `<shape>` (`docs/spec/shape.md`, section 8): the coverage of its outlines, moved by its offset, blurred by
//! its blur and laid in its colour under the shape. The values are worked by hand in `fixtures/shape-worked-values.md`,
//! section 6.

use draw::{Colour, Map, Path, Rgba, ShapeMark, ShapeShadow, ViewBox};
use draw_cpu::shape_layer;

const INK: Rgba = Rgba {
    red: 224,
    green: 160,
    blue: 48,
    alpha: 255,
};

const BLACK: Colour = Colour { red: 0, green: 0, blue: 0 };

/// A shape in a box of `size` pixels at the origin, of a view box `view_box` units square.
fn shape(size: f64, view_box: f64, data: &str, stroke_width: Option<f64>, dashes: Vec<f64>, shadow: Option<ShapeShadow>) -> ShapeMark {
    ShapeMark {
        left: 0.0,
        top: 0.0,
        width: size,
        height: size,
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
        shadow,
    }
}

/// The premultiplied pixel at `(x, y)` of a layer `size` pixels across.
fn pixel(layer: &[u8], size: u32, x: u32, y: u32) -> [u8; 4] {
    let at = (y * size + x) as usize * 4;
    [layer[at], layer[at + 1], layer[at + 2], layer[at + 3]]
}

#[test]
fn a_shadow_is_moved_by_its_offset_in_view_box_units_with_the_shape_over_it() {
    // Case A of the worked values: a 24-unit view box in 48 pixels, so one unit is two pixels. The shape covers pixels 8 to
    // 39, the shadow pixels 10 to 41.
    let shadow = ShapeShadow {
        across: 1.0,
        down: 1.0,
        blur: 0.0,
        colour: BLACK,
    };
    let mark = shape(48.0, 24.0, "M 4 4 H 20 V 20 H 4 Z", None, Vec::new(), Some(shadow));
    let layer = shape_layer(48, 48, &mark, Map::IDENTITY).unwrap();
    for (x, y) in [(9, 9), (10, 10), (39, 39)] {
        assert_eq!(pixel(&layer, 48, x, y), [224, 160, 48, 255], "ink at ({x}, {y})");
    }
    for (x, y) in [(40, 40), (41, 41), (41, 10)] {
        assert_eq!(pixel(&layer, 48, x, y), [0, 0, 0, 255], "shadow at ({x}, {y})");
    }
    for (x, y) in [(42, 42), (9, 42), (5, 5)] {
        assert_eq!(pixel(&layer, 48, x, y), [0, 0, 0, 0], "clear at ({x}, {y})");
    }
}

#[test]
fn a_shape_with_no_shadow_casts_none() {
    let mark = shape(48.0, 24.0, "M 4 4 H 20 V 20 H 4 Z", None, Vec::new(), None);
    let layer = shape_layer(48, 48, &mark, Map::IDENTITY).unwrap();
    assert_eq!(pixel(&layer, 48, 9, 9), [224, 160, 48, 255]);
    for (x, y) in [(40, 40), (41, 41), (41, 10)] {
        assert_eq!(pixel(&layer, 48, x, y), [0, 0, 0, 0], "no shadow at ({x}, {y})");
    }
}

#[test]
fn a_blur_of_two_softens_the_shadow_edge_by_three_passes_of_a_two_pixel_average() {
    // Case B of the worked values: a 64-pixel frame in a 64-unit view box, the shape's left edge at 16, a blur of 2 pixels
    // (span 2). Along row 32 the shadow's alpha is 0, 32, 128 and 224 at x 12 to 15, and the ink covers x 16 on.
    let shadow = ShapeShadow {
        across: 0.0,
        down: 0.0,
        blur: 2.0,
        colour: Colour {
            red: 40,
            green: 80,
            blue: 120,
        },
    };
    let mark = shape(64.0, 64.0, "M 16 16 H 48 V 48 H 16 Z", None, Vec::new(), Some(shadow));
    let layer = shape_layer(64, 64, &mark, Map::IDENTITY).unwrap();
    for (x, alpha) in [(12, 0), (13, 32), (14, 128), (15, 224)] {
        let [red, green, blue, drawn] = pixel(&layer, 64, x, 32);
        assert_eq!(drawn, alpha, "shadow alpha at x {x}");
        // The premultiplied colour is the shadow's colour times the alpha, to the nearest level.
        for (channel, colour) in [(red, 40.0), (green, 80.0), (blue, 120.0)] {
            let exact = colour * f64::from(alpha) / 255.0;
            assert!(
                (f64::from(channel) - exact).abs() <= 1.0,
                "channel {channel} at x {x}, worked {exact:.2}"
            );
        }
    }
    assert_eq!(pixel(&layer, 64, 16, 32), [224, 160, 48, 255], "the ink covers x 16");
}

#[test]
fn a_stroked_shadow_follows_the_line_and_its_dashes() {
    // A line of width 4 at y 16 from x 4 to 60, dashed 10 and 10, casts its shadow 6 pixels down: rows 20 to 23.
    let shadow = ShapeShadow {
        across: 0.0,
        down: 6.0,
        blur: 0.0,
        colour: BLACK,
    };
    let mark = shape(64.0, 64.0, "M 4 16 H 60", Some(4.0), vec![10.0, 10.0], Some(shadow));
    let layer = shape_layer(64, 64, &mark, Map::IDENTITY).unwrap();
    assert_eq!(pixel(&layer, 64, 8, 15), [224, 160, 48, 255], "the line's first dash");
    assert_eq!(pixel(&layer, 64, 19, 15), [0, 0, 0, 0], "the line's gap");
    assert_eq!(pixel(&layer, 64, 8, 21), [0, 0, 0, 255], "the shadow of the first dash");
    assert_eq!(pixel(&layer, 64, 19, 21), [0, 0, 0, 0], "the shadow of the gap");
    assert_eq!(pixel(&layer, 64, 28, 21), [0, 0, 0, 255], "the shadow of the next dash");
}

#[test]
fn a_shadow_moves_with_the_element_under_its_map() {
    // The shape of case A, moved five pixels across: its ink and its shadow move with it.
    let shadow = ShapeShadow {
        across: 1.0,
        down: 1.0,
        blur: 0.0,
        colour: BLACK,
    };
    let mark = shape(48.0, 24.0, "M 4 4 H 20 V 20 H 4 Z", None, Vec::new(), Some(shadow));
    let layer = shape_layer(48, 48, &mark, Map::moving(5.0, 0.0)).unwrap();
    assert_eq!(
        pixel(&layer, 48, 14, 9),
        [224, 160, 48, 255],
        "the ink's corner moves with the shape"
    );
    assert_eq!(
        pixel(&layer, 48, 44, 20),
        [224, 160, 48, 255],
        "the ink's last column moves with the shape"
    );
    assert_eq!(
        pixel(&layer, 48, 45, 40),
        [0, 0, 0, 255],
        "the shadow's first column moves with the shape"
    );
    assert_eq!(
        pixel(&layer, 48, 46, 40),
        [0, 0, 0, 255],
        "the shadow's second column moves with the shape"
    );
    assert_eq!(pixel(&layer, 48, 47, 40), [0, 0, 0, 0], "clear past the shadow");
    assert_eq!(pixel(&layer, 48, 12, 40), [0, 0, 0, 0], "clear before the shape");
}

#[test]
fn a_blur_in_view_box_units_is_scaled_into_pixels() {
    // The case-B edge at a scale of two: a 24-unit view box in 48 pixels, the shape's left edge at unit 8 (pixel 16), and a
    // blur of one unit, which is two pixels. The alpha along row 32 is the same as in case B.
    let shadow = ShapeShadow {
        across: 0.0,
        down: 0.0,
        blur: 1.0,
        colour: BLACK,
    };
    let mark = shape(48.0, 24.0, "M 8 8 H 24 V 24 H 8 Z", None, Vec::new(), Some(shadow));
    let layer = shape_layer(48, 48, &mark, Map::IDENTITY).unwrap();
    for (x, alpha) in [(12, 0), (13, 32), (14, 128), (15, 224)] {
        assert_eq!(pixel(&layer, 48, x, 32), [0, 0, 0, alpha], "shadow alpha at x {x}");
    }
}
