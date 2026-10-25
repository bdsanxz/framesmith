//! A background layer's numbers for a box of a given size, and its colour at a point (`docs/spec/render-command.md`,
//! section 4.6, rules 2 to 8).

use draw::{level, Amount, Layer, Paint, Rgba, Shape, Size, Stop};

const BLACK: Rgba = Rgba {
    red: 0,
    green: 0,
    blue: 0,
    alpha: 255,
};
const WHITE: Rgba = Rgba {
    red: 255,
    green: 255,
    blue: 255,
    alpha: 255,
};
const CLEAR: Rgba = Rgba {
    red: 0,
    green: 0,
    blue: 0,
    alpha: 0,
};

fn stop(colour: Rgba, percent: f64) -> Stop {
    Stop {
        colour,
        place: Amount::Percent(percent),
    }
}

fn layer(paint: Paint) -> Layer {
    Layer {
        paint,
        size: Size::Auto,
        position: (Amount::Pixels(0.0), Amount::Pixels(0.0)),
        repeat: true,
    }
}

fn linear(angle: f64) -> Layer {
    layer(Paint::Linear {
        angle,
        stops: vec![stop(BLACK, 0.0), stop(WHITE, 100.0)],
    })
}

fn close(found: [f64; 4], wanted: [f64; 4]) -> bool {
    found.iter().zip(wanted).all(|(found, wanted)| (found - wanted).abs() < 1e-9)
}

fn grey(value: f64) -> [f64; 4] {
    [value, value, value, 1.0]
}

#[test]
fn a_linear_gradient_s_line_crosses_the_box_the_way_its_angle_names() {
    // 90deg: left to right over the width; 0deg: bottom to top over the height; 180deg: top to bottom.
    let right = linear(90.0).resolve(200.0, 100.0);
    assert!(close(right.colour_at(50.0, 10.0).unwrap(), grey(0.25)));
    let up = linear(0.0).resolve(200.0, 100.0);
    assert!(close(up.colour_at(10.0, 75.0).unwrap(), grey(0.25)));
    let down = linear(180.0).resolve(200.0, 100.0);
    assert!(close(down.colour_at(10.0, 75.0).unwrap(), grey(0.75)));
}

#[test]
fn a_diagonal_line_is_as_long_as_the_box_seen_along_it() {
    // At 45deg the line of a square of side 100 is 100 √2 long and runs from the bottom left corner to the top right, so a
    // quarter of the way along the diagonal is a quarter of the way along the line.
    let diagonal = linear(45.0).resolve(100.0, 100.0);
    assert!(close(diagonal.colour_at(25.0, 75.0).unwrap(), grey(0.25)));
    assert!(close(diagonal.colour_at(75.0, 25.0).unwrap(), grey(0.75)));
    assert!(close(diagonal.colour_at(50.0, 50.0).unwrap(), grey(0.5)));
}

#[test]
fn a_place_that_goes_back_is_taken_as_the_one_before_it_and_makes_a_hard_edge() {
    let edge = layer(Paint::Linear {
        angle: 90.0,
        stops: vec![stop(BLACK, 60.0), stop(WHITE, 20.0)],
    })
    .resolve(100.0, 10.0);
    assert_eq!(edge.stops[1].0, 60.0);
    assert!(close(edge.colour_at(59.9, 5.0).unwrap(), grey(0.0)));
    assert!(close(edge.colour_at(60.0, 5.0).unwrap(), grey(1.0)));
}

#[test]
fn a_fade_to_a_clear_colour_is_worked_premultiplied() {
    let red = Rgba {
        red: 255,
        green: 0,
        blue: 0,
        alpha: 255,
    };
    let fade = layer(Paint::Linear {
        angle: 90.0,
        stops: vec![stop(red, 0.0), stop(CLEAR, 100.0)],
    })
    .resolve(100.0, 10.0);
    // Half way: half the red at half the alpha, so that over white it shows as red, not as a darker red.
    assert!(close(fade.colour_at(50.0, 5.0).unwrap(), [0.5, 0.0, 0.0, 0.5]));
}

#[test]
fn a_circle_and_an_ellipse_reach_their_radii() {
    let circle = layer(Paint::Radial {
        shape: Shape::Circle(Some(20.0)),
        centre: Some((25.0, 50.0)),
        stops: vec![stop(BLACK, 0.0), stop(WHITE, 100.0)],
    })
    .resolve(200.0, 100.0);
    assert!(close(circle.colour_at(50.0, 50.0).unwrap(), grey(0.0)));
    assert!(close(circle.colour_at(60.0, 50.0).unwrap(), grey(0.5)));
    assert!(close(circle.colour_at(50.0, 70.0).unwrap(), grey(1.0)));
    // An ellipse with no radii passes through the corners of the tile.
    let ellipse = layer(Paint::Radial {
        shape: Shape::Ellipse(None),
        centre: None,
        stops: vec![stop(BLACK, 0.0), stop(WHITE, 100.0)],
    })
    .resolve(200.0, 100.0);
    assert!(close(ellipse.colour_at(0.0, 0.0).unwrap(), grey(1.0)));
    assert!(close(ellipse.colour_at(100.0, 50.0).unwrap(), grey(0.0)));
}

#[test]
fn a_tile_repeats_from_its_position_or_shows_once() {
    let mut tiled = linear(90.0);
    tiled.size = Size::Given(Amount::Pixels(20.0), Amount::Pixels(10.0));
    tiled.position = (Amount::Pixels(5.0), Amount::Percent(100.0));
    let resolved = tiled.resolve(100.0, 50.0);
    assert_eq!((resolved.left, resolved.top), (5.0, 40.0));
    assert!(close(
        resolved.colour_at(10.0, 40.0).unwrap(),
        resolved.colour_at(70.0, 0.0).unwrap()
    ));
    assert!(close(resolved.colour_at(0.0, 0.0).unwrap(), grey(0.75)));
    tiled.repeat = false;
    let once = tiled.resolve(100.0, 50.0);
    assert!(once.colour_at(0.0, 0.0).is_none());
    assert!(once.colour_at(30.0, 45.0).is_none());
    assert!(close(once.colour_at(15.0, 45.0).unwrap(), grey(0.5)));
}

#[test]
fn a_value_is_made_8_bits_by_rounding_half_up() {
    assert_eq!(level(0.0), 0);
    assert_eq!(level(0.5), 128);
    assert_eq!(level(1.0), 255);
    assert_eq!(level(1.5 / 255.0), 2);
}
