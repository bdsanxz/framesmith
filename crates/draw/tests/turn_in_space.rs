//! The projective map of a turn in space (`docs/spec/render-command.md`, section 4.5, rule 10): where it draws points,
//! that its inverse takes them back, when the element is edge-on, and when a corner comes to the eye.

use draw::{Map, Turn};
use layout::Rect;

fn square(perspective: f64, rotate_x: f64, rotate_y: f64) -> Turn {
    Turn {
        rect: Rect {
            left: 50,
            top: 50,
            width: 100,
            height: 100,
        },
        origin: (100.0, 100.0),
        perspective,
        rotate_x,
        rotate_y,
    }
}

fn near(found: (f64, f64), wanted: (f64, f64)) -> bool {
    (found.0 - wanted.0).abs() < 1e-9 && (found.1 - wanted.1).abs() < 1e-9
}

#[test]
fn a_turn_about_the_upright_line_draws_the_far_side_nearer_the_origin() {
    let turn = square(400.0, 0.0, 60.0);
    let (sin, cos) = 60f64.to_radians().sin_cos();
    // The right corner goes back by 50 sin 60 and is drawn at 50 cos 60, scaled by P / (P + 50 sin 60).
    let scale = 400.0 / (400.0 + 50.0 * sin);
    assert!(near(
        turn.apply((150.0, 50.0)).unwrap(),
        (100.0 + 50.0 * cos * scale, 100.0 - 50.0 * scale)
    ));
    let scale = 400.0 / (400.0 - 50.0 * sin);
    assert!(near(
        turn.apply((50.0, 150.0)).unwrap(),
        (100.0 - 50.0 * cos * scale, 100.0 + 50.0 * scale)
    ));
    assert!(
        near(turn.apply((100.0, 100.0)).unwrap(), (100.0, 100.0)),
        "the origin stays where it is"
    );
}

#[test]
fn a_positive_turn_about_the_level_line_takes_the_top_away() {
    let turn = square(400.0, 45.0, 0.0);
    assert!(turn.depth((100.0, 50.0)) > 400.0 && turn.depth((100.0, 150.0)) < 400.0);
    let (top, bottom) = (turn.apply((150.0, 50.0)).unwrap(), turn.apply((150.0, 150.0)).unwrap());
    assert!(top.0 - 100.0 < bottom.0 - 100.0, "the top edge is shorter: {top:?} {bottom:?}");
}

#[test]
fn both_turns_are_the_level_line_first_then_the_upright_line() {
    let turn = square(300.0, 50.0, 40.0);
    let (sin_a, cos_a) = 50f64.to_radians().sin_cos();
    let (sin_b, cos_b) = 40f64.to_radians().sin_cos();
    let (u, v) = (30.0, -20.0);
    // About the level line: (u, v cos a, v sin a); then about the upright line.
    let (x, y, z) = (u, v * cos_a, v * sin_a);
    let (x, z) = (x * cos_b + z * sin_b, -x * sin_b + z * cos_b);
    let scale = 300.0 / (300.0 - z);
    assert!(near(
        turn.apply((100.0 + u, 100.0 + v)).unwrap(),
        (100.0 + x * scale, 100.0 + y * scale)
    ));
}

#[test]
fn the_inverse_finds_the_flat_point_drawn_at_each_worked_place() {
    for (rotate_x, rotate_y) in [(0.0, 60.0), (45.0, 0.0), (50.0, 40.0), (0.0, 135.0), (-30.0, 200.0)] {
        let turn = square(400.0, rotate_x, rotate_y);
        let inverse = turn.inverse().unwrap();
        let ((sin_a, cos_a), (sin_b, cos_b)) = (f64::to_radians(rotate_x).sin_cos(), f64::to_radians(rotate_y).sin_cos());
        for (u, v) in [(-50.0, -50.0), (50.0, -50.0), (20.0, 30.0), (-40.0, 49.0)] {
            // Where rule 10 draws (u, v), worked here and not by the turn under test.
            let (x, y, z) = (u * cos_b + v * sin_a * sin_b, v * cos_a, -u * sin_b + v * sin_a * cos_b);
            let drawn = (100.0 + x * 400.0 / (400.0 - z), 100.0 + y * 400.0 / (400.0 - z));
            let back = turn.back(&inverse, drawn).unwrap();
            assert!(near(back, (100.0 + u, 100.0 + v)), "{rotate_x} {rotate_y}: ({u}, {v}) {back:?}");
        }
    }
}

#[test]
fn at_90_degrees_the_element_is_edge_on_and_not_drawn() {
    for (rotate_x, rotate_y) in [(0.0, 90.0), (90.0, 0.0), (-90.0, 30.0), (0.0, 270.0)] {
        let turn = square(400.0, rotate_x, rotate_y);
        assert!(
            turn.is_edge_on() && turn.inverse().is_none() && !turn.draws(Map::IDENTITY),
            "{rotate_x} {rotate_y}"
        );
    }
    assert!(!square(400.0, 0.0, 89.0).is_edge_on() && square(400.0, 0.0, 89.0).draws(Map::IDENTITY));
}

#[test]
fn a_corner_at_the_eye_or_behind_it_draws_nothing() {
    // With a perspective of 40, a corner 50 to the left turned 60 degrees comes 43.3 nearer: behind the eye.
    let turn = square(40.0, 0.0, 60.0);
    assert!(turn.depth((50.0, 100.0)) <= 0.0 && !turn.draws(Map::IDENTITY) && turn.apply((50.0, 100.0)).is_none());
    assert!(square(60.0, 0.0, 60.0).draws(Map::IDENTITY));
}

#[test]
fn the_turn_is_about_the_origin_taken_by_the_map() {
    let turn = square(400.0, 0.0, 30.0).about(Map::moving(10.0, -5.0));
    assert_eq!(turn.origin, (110.0, 95.0));
    assert!(turn.draws(Map::moving(10.0, -5.0)));
    assert!(square(400.0, 0.0, 0.0).is_none() && !square(400.0, 0.0, 30.0).is_none());
}
