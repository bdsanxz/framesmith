//! `max-width` caps the width of a box that writes no width of its own, from its parent, from its two edges, or
//! not at all (`docs/spec/render-command.md` section 4.1, rule 7).

use layout::{frame, place, Amount, Rect, Written};

fn rect(left: i32, top: i32, width: u32, height: u32) -> Rect {
    Rect { left, top, width, height }
}

#[test]
fn a_box_that_writes_no_width_is_as_wide_as_its_parent_up_to_max_width() {
    let written = Written {
        max_width: Some(50.0),
        ..Written::default()
    };
    let placed = place(frame(200, 100), written).expect("placed");
    assert_eq!(placed, rect(0, 0, 50, 100));
}

#[test]
fn a_box_with_two_edges_is_capped_at_max_width_and_keeps_its_near_edge() {
    let written = Written {
        left: Some(Amount::Pixels(10.0)),
        right: Some(Amount::Pixels(10.0)),
        max_width: Some(50.0),
        ..Written::default()
    };
    let placed = place(frame(200, 100), written).expect("placed");
    assert_eq!(placed, rect(10, 0, 50, 100));
}

#[test]
fn a_box_with_a_width_of_its_own_ignores_max_width() {
    let written = Written {
        width: Some(Amount::Pixels(80.0)),
        max_width: Some(50.0),
        ..Written::default()
    };
    let placed = place(frame(200, 100), written).expect("placed");
    assert_eq!(placed, rect(0, 0, 80, 100));
}
