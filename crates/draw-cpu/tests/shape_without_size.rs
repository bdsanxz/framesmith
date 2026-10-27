//! A `<shape>` with no width or no height has no size and shows nothing (`docs/spec/shape.md`, section 1; `notation.md`
//! 7.11), and that holds with a shadow too.

use draw::{Colour, Map, Path, Rgba, ShapeMark, ShapeShadow, ViewBox};
use draw_cpu::shape_layer;

fn sized(width: f64, height: f64, shadow: Option<ShapeShadow>) -> ShapeMark {
    ShapeMark {
        left: 0.0,
        top: 0.0,
        width,
        height,
        view_box: ViewBox {
            x: 0.0,
            y: 0.0,
            width: 24.0,
            height: 24.0,
        },
        path: Path::parse("M 4 4 H 20 V 20 H 4 Z").unwrap(),
        colour: Rgba {
            red: 224,
            green: 160,
            blue: 48,
            alpha: 255,
        },
        stroke_width: None,
        dashes: Vec::new(),
        shadow,
    }
}

fn shadow() -> ShapeShadow {
    ShapeShadow {
        across: 2.0,
        down: 2.0,
        blur: 1.0,
        colour: Colour { red: 0, green: 0, blue: 0 },
    }
}

fn is_clear(layer: &[u8]) -> bool {
    layer.iter().all(|&value| value == 0)
}

#[test]
fn a_shape_with_no_width_shows_nothing() {
    let layer = shape_layer(48, 48, &sized(0.0, 48.0, None), Map::IDENTITY).unwrap();
    assert!(is_clear(&layer));
}

#[test]
fn a_shape_with_no_height_shows_nothing() {
    let layer = shape_layer(48, 48, &sized(48.0, 0.0, None), Map::IDENTITY).unwrap();
    assert!(is_clear(&layer));
}

#[test]
fn a_shape_with_no_size_shows_nothing_even_with_a_shadow() {
    let layer = shape_layer(48, 48, &sized(0.0, 0.0, Some(shadow())), Map::IDENTITY).unwrap();
    assert!(is_clear(&layer));
}
