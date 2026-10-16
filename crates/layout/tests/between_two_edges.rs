//! A box written with both edges of an axis and no size spans the space between them, and a box with
//! a far edge and a size is placed from the far edge (`docs/spec/render-command.md` section 4.1, rules
//! 1 and 2; `docs/spec/notation.md` Q1).

use layout::{frame, place, Amount, Rect, Written};

fn px(value: f64) -> Option<Amount> {
    Some(Amount::Pixels(value))
}

#[test]
fn a_left_and_a_right_with_no_width_span_the_space_between_them() {
    let rect = place(
        frame(100, 60),
        Written {
            left: px(10.0),
            right: px(10.0),
            ..Written::default()
        },
    )
    .unwrap();
    assert_eq!(
        rect,
        Rect {
            left: 10,
            top: 0,
            width: 80,
            height: 60
        }
    );
}

#[test]
fn a_top_and_a_bottom_with_no_height_span_the_space_between_them() {
    let rect = place(
        frame(100, 60),
        Written {
            top: px(5.0),
            bottom: px(15.0),
            ..Written::default()
        },
    )
    .unwrap();
    assert_eq!(
        rect,
        Rect {
            left: 0,
            top: 5,
            width: 100,
            height: 40
        }
    );
}

#[test]
fn a_bottom_with_a_height_and_no_top_lies_that_distance_from_the_parents_bottom_edge() {
    // The bottom edge is 10 from the bottom of the 100-pixel frame; the box is 20 high, so its top
    // edge is 100 - 10 - 20 = 70.
    let rect = place(
        frame(100, 100),
        Written {
            bottom: px(10.0),
            height: px(20.0),
            ..Written::default()
        },
    )
    .unwrap();
    assert_eq!(
        rect,
        Rect {
            left: 0,
            top: 70,
            width: 100,
            height: 20
        }
    );
}

#[test]
fn a_right_with_a_width_and_no_left_lies_that_distance_from_the_parents_right_edge() {
    let rect = place(
        frame(100, 100),
        Written {
            right: px(30.0),
            width: px(20.0),
            ..Written::default()
        },
    )
    .unwrap();
    assert_eq!(
        rect,
        Rect {
            left: 50,
            top: 0,
            width: 20,
            height: 100
        }
    );
}

#[test]
fn a_top_and_a_bottom_with_a_height_place_the_box_from_the_top_and_ignore_the_bottom() {
    let rect = place(
        frame(100, 100),
        Written {
            top: px(10.0),
            bottom: px(10.0),
            height: px(20.0),
            ..Written::default()
        },
    )
    .unwrap();
    assert_eq!(
        rect,
        Rect {
            left: 0,
            top: 10,
            width: 100,
            height: 20
        }
    );
}

#[test]
fn a_left_and_a_right_with_a_width_place_the_box_from_the_left_and_ignore_the_right() {
    let rect = place(
        frame(100, 100),
        Written {
            left: px(10.0),
            right: px(10.0),
            width: px(30.0),
            ..Written::default()
        },
    )
    .unwrap();
    assert_eq!(
        rect,
        Rect {
            left: 10,
            top: 0,
            width: 30,
            height: 100
        }
    );
}

#[test]
fn a_left_alone_does_not_reduce_the_width() {
    let rect = place(
        frame(100, 100),
        Written {
            left: px(40.0),
            ..Written::default()
        },
    )
    .unwrap();
    assert_eq!(
        rect,
        Rect {
            left: 40,
            top: 0,
            width: 100,
            height: 100
        }
    );
}
