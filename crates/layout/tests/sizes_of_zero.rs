//! A size of 0 draws nothing of its box, and a size that two distances leave below 0 is 0
//! (`docs/spec/render-command.md` section 4.1, rule 5 and rule 1).

use layout::{frame, place, Amount, Rect, Written};

#[test]
fn a_width_of_zero_and_a_height_of_zero_are_taken_as_written() {
    let rect = place(
        frame(100, 60),
        Written {
            left: Some(Amount::Pixels(10.0)),
            width: Some(Amount::Pixels(0.0)),
            height: Some(Amount::Pixels(0.0)),
            ..Written::default()
        },
    )
    .unwrap();
    assert_eq!(
        rect,
        Rect {
            left: 10,
            top: 0,
            width: 0,
            height: 0
        }
    );
    assert_eq!(rect.visible(100, 60), None);
}

#[test]
fn a_width_of_zero_percent_is_a_width_of_zero() {
    let rect = place(
        frame(100, 60),
        Written {
            width: Some(Amount::Percent(0.0)),
            ..Written::default()
        },
    )
    .unwrap();
    assert_eq!(rect.width, 0);
}

#[test]
fn two_edges_that_leave_no_room_give_a_size_of_zero_and_not_a_negative_one() {
    // The edges are 60 from each side of a 100-pixel frame: the space between them is -20, which is 0.
    // The near edge is still where it is written, 60.
    let rect = place(
        frame(100, 60),
        Written {
            left: Some(Amount::Pixels(60.0)),
            right: Some(Amount::Pixels(60.0)),
            ..Written::default()
        },
    )
    .unwrap();
    assert_eq!(
        rect,
        Rect {
            left: 60,
            top: 0,
            width: 0,
            height: 60
        }
    );
}

#[test]
fn a_zero_size_keeps_the_place_of_its_edges_and_draws_nothing() {
    let rect = place(
        frame(100, 60),
        Written {
            right: Some(Amount::Pixels(10.0)),
            width: Some(Amount::Pixels(0.0)),
            ..Written::default()
        },
    )
    .unwrap();
    assert_eq!(
        rect,
        Rect {
            left: 90,
            top: 0,
            width: 0,
            height: 60
        }
    );
    assert_eq!(rect.visible(100, 60), None);
}
