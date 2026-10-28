//! A box's shadow as the processor draws it (`docs/spec/render-command.md`, section 4.7, rules 2 and 3): the shadow of a box is
//! its outer edge moved by the offset, seen only outside the box, in the shadow's colour. The values are worked by hand from
//! the rule, not taken from the product.

use draw::{BoxShadow, Colour, Map, Rounded, Shadow};
use draw_cpu::box_shadow_layer;
use layout::Rect;

/// The premultiplied pixel at (`x`, `y`) of a layer `width` pixels wide.
fn pixel(layer: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let at = ((y * width + x) * 4) as usize;
    [layer[at], layer[at + 1], layer[at + 2], layer[at + 3]]
}

fn black_offset_10_10_hard() -> Shadow {
    Shadow {
        across: 10.0,
        down: 10.0,
        blur: 0.0,
        colour: Colour { red: 0, green: 0, blue: 0 },
    }
}

/// A 20 by 20 box at the top left corner of a 40 by 40 frame, with square corners.
fn square_box(shadow: Shadow) -> BoxShadow {
    let rect = Rect {
        left: 0,
        top: 0,
        width: 20,
        height: 20,
    };
    BoxShadow {
        shape: Rounded::of_box(rect, 0.0).expect("a box with a size has a shape"),
        shadow,
    }
}

#[test]
fn a_hard_shadow_shows_only_outside_the_box_where_the_moved_box_lies() {
    let layer = box_shadow_layer(40, 40, &square_box(black_offset_10_10_hard()), Map::IDENTITY).unwrap();
    // The moved box covers 10 to 29 on both axes. The box itself covers 0 to 19, so the shadow shows at 20 to 29 on either axis.
    assert_eq!(
        pixel(&layer, 40, 25, 15),
        [0, 0, 0, 255],
        "right of the box, in the moved box's span"
    );
    assert_eq!(pixel(&layer, 40, 15, 25), [0, 0, 0, 255], "below the box, in the moved box's span");
    assert_eq!(pixel(&layer, 40, 25, 25), [0, 0, 0, 255], "the corner the two spans share");
    // The edge is hard: the last pixel inside the moved box is full, the first outside it is clear. Any blur fades both.
    assert_eq!(pixel(&layer, 40, 29, 15), [0, 0, 0, 255], "the last pixel inside the moved box");
    assert_eq!(pixel(&layer, 40, 30, 15), [0, 0, 0, 0], "the first pixel beyond the moved box");
}

#[test]
fn a_shadow_is_clear_under_the_box_and_beyond_the_moved_box() {
    let layer = box_shadow_layer(40, 40, &square_box(black_offset_10_10_hard()), Map::IDENTITY).unwrap();
    assert_eq!(
        pixel(&layer, 40, 15, 15),
        [0, 0, 0, 0],
        "under the box: its own coverage is 255, so the shadow is 0"
    );
    assert_eq!(pixel(&layer, 40, 35, 35), [0, 0, 0, 0], "beyond the moved box");
    assert_eq!(pixel(&layer, 40, 5, 25), [0, 0, 0, 0], "left of the moved box");
    assert_eq!(pixel(&layer, 40, 25, 5), [0, 0, 0, 0], "above the moved box");
}

#[test]
fn the_shadow_is_laid_in_its_colour() {
    let blue = Shadow {
        colour: Colour {
            red: 0,
            green: 0,
            blue: 255,
        },
        ..black_offset_10_10_hard()
    };
    let layer = box_shadow_layer(40, 40, &square_box(blue), Map::IDENTITY).unwrap();
    assert_eq!(pixel(&layer, 40, 25, 15), [0, 0, 255, 255], "full alpha in blue");
    assert_eq!(pixel(&layer, 40, 15, 15), [0, 0, 0, 0], "under the box, nothing");
}

/// The same 20 by 20 box at the top left corner, with its corners rounded by `radius`.
fn rounded_box(radius: f64, shadow: Shadow) -> BoxShadow {
    let rect = Rect {
        left: 0,
        top: 0,
        width: 20,
        height: 20,
    };
    BoxShadow {
        shape: Rounded::of_box(rect, radius).expect("a box with a size has a shape"),
        shadow,
    }
}

#[test]
fn a_rounded_box_casts_a_rounded_shadow() {
    // The moved box spans 10 to 29 on both axes, with corners of radius 8. The pixels below lie beyond the original box
    // (0 to 19), so only the shadow's shape decides them.
    let layer = box_shadow_layer(40, 40, &rounded_box(8.0, black_offset_10_10_hard()), Map::IDENTITY).unwrap();
    let square = box_shadow_layer(40, 40, &square_box(black_offset_10_10_hard()), Map::IDENTITY).unwrap();
    // (29, 11) has its centre about 9.9 from (22, 18), the centre of the top right arc: past 8, outside the arc.
    assert_eq!(pixel(&layer, 40, 29, 11), [0, 0, 0, 0], "outside the top right arc");
    assert_eq!(
        pixel(&square, 40, 29, 11),
        [0, 0, 0, 255],
        "the square box's shadow covers that pixel"
    );
    // (10, 29) has its centre about 10.6 from (18, 22), the centre of the bottom left arc: outside it.
    assert_eq!(pixel(&layer, 40, 10, 29), [0, 0, 0, 0], "outside the bottom left arc");
    assert_eq!(
        pixel(&square, 40, 10, 29),
        [0, 0, 0, 255],
        "the square box's shadow covers that pixel"
    );
    // (25, 25) has its centre about 4.9 from (22, 22), the centre of the bottom right arc: inside it.
    assert_eq!(
        pixel(&layer, 40, 25, 25),
        [0, 0, 0, 255],
        "inside the bottom right arc, beyond the box"
    );
}

/// A 1 by 1 box at the top left corner, whose shadow moves it 20 pixels across and down, blurred by 4.
fn point_shadow_blur_4() -> BoxShadow {
    let rect = Rect {
        left: 0,
        top: 0,
        width: 1,
        height: 1,
    };
    BoxShadow {
        shape: Rounded::of_box(rect, 0.0).expect("a box with a size has a shape"),
        shadow: Shadow {
            across: 20.0,
            down: 20.0,
            blur: 4.0,
            colour: Colour { red: 0, green: 0, blue: 0 },
        },
    }
}

#[test]
fn a_blur_of_4_fades_the_shadow_by_the_spec_method() {
    // The expected values are from an independent reference, written from `notation.md` sections 7.5 and 7.6 and not from the
    // product: sigma 2 (half the blur), a span of 4 (one pixel before, two after), three passes of a horizontal then a vertical
    // average, each rounded half up. The centre (20, 20) gives 7, and the sample points lie at 3 pixels left, 3 right of it: 7 and 1.
    let layer = box_shadow_layer(40, 40, &point_shadow_blur_4(), Map::IDENTITY).unwrap();
    assert_eq!(pixel(&layer, 40, 20, 20), [0, 0, 0, 7], "the centre of the moved point");
    assert_eq!(pixel(&layer, 40, 17, 20), [0, 0, 0, 7], "three pixels left of the centre");
    assert_eq!(pixel(&layer, 40, 23, 20), [0, 0, 0, 1], "three pixels right of the centre");
    assert_eq!(pixel(&layer, 40, 20, 14), [0, 0, 0, 1], "six pixels above the centre");
    assert_eq!(pixel(&layer, 40, 30, 30), [0, 0, 0, 0], "beyond the reach of the blur");
}
