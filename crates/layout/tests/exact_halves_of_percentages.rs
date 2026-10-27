//! A percentage is worked out exactly, and a half of a pixel rounds up (`docs/spec/render-command.md`, section 4.1, rule 3):
//! the values below are worked by hand from that rule, and each is one a float gets a whole pixel low.

use layout::{frame, place, Amount, Rect, Written};

fn across(parent: Rect, left: Option<Amount>, width: Option<Amount>, right: Option<Amount>) -> (i32, u32) {
    let rect = place(
        parent,
        &Written {
            left,
            width,
            right,
            ..Written::default()
        },
    )
    .unwrap();
    (rect.left, rect.width)
}

#[test]
fn one_point_four_percent_of_2750_is_38_point_5_and_its_edge_rounds_up_to_39() {
    // 1.4% of 2750 is exactly 38.5, which a half up rounds to 39; the far edge is 38.5 + 10 = 48.5, which rounds to 49.
    let parent = frame(2750, 10);
    assert_eq!(
        across(
            parent,
            Some(Amount::Percent((1.4).into())),
            Some(Amount::Pixels((10.0).into())),
            None
        ),
        (39, 10)
    );
}

#[test]
fn a_half_of_a_pixel_rounds_up_for_each_of_these_percentages() {
    // 2.3% of 1500 = 34.5 -> 35; 4.6% of 750 = 34.5 -> 35; 8.2% of 1750 = 143.5 -> 144.
    assert_eq!(
        across(
            frame(1500, 10),
            Some(Amount::Percent((2.3).into())),
            Some(Amount::Pixels((20.0).into())),
            None
        ),
        (35, 20)
    );
    assert_eq!(
        across(
            frame(750, 10),
            Some(Amount::Percent((4.6).into())),
            Some(Amount::Pixels((20.0).into())),
            None
        ),
        (35, 20)
    );
    assert_eq!(
        across(
            frame(1750, 10),
            Some(Amount::Percent((8.2).into())),
            Some(Amount::Pixels((10.0).into())),
            None
        ),
        (144, 10)
    );
}

#[test]
fn a_width_of_half_the_parent_in_an_odd_parent_is_49_point_5_and_rounds_up_to_50() {
    // In a parent 99 pixels wide, width 50% is 49.5, which rounds to 50 (the spec's own example).
    assert_eq!(
        across(
            frame(99, 10),
            Some(Amount::Pixels((0.0).into())),
            Some(Amount::Percent((50.0).into())),
            None
        ),
        (0, 50)
    );
}

#[test]
fn a_negative_half_rounds_up_to_the_larger_whole_number() {
    // -0.5% of 2900 is -14.5, which rounds to -14; the far edge is -14.5 + 20 = 5.5, which rounds to 6, so the width is 20.
    assert_eq!(
        across(
            frame(2900, 10),
            Some(Amount::Percent((-0.5).into())),
            Some(Amount::Pixels((20.0).into())),
            None
        ),
        (-14, 20)
    );
}

#[test]
fn a_far_edge_written_in_percent_is_worked_from_the_parent_far_edge_exactly() {
    // right 1.4% of 2750 is 38.5 from the far edge, so the near edge is 2750 - 38.5 - 10 = 2701.5, which rounds to 2702.
    assert_eq!(
        across(
            frame(2750, 10),
            None,
            Some(Amount::Pixels((10.0).into())),
            Some(Amount::Percent((1.4).into()))
        ),
        (2702, 10)
    );
}

#[test]
fn a_percentage_inside_a_box_that_starts_on_a_whole_pixel_adds_exactly() {
    // A parent at left 10 with a child at left 1.4% of its 2750 width: 10 + 38.5 = 48.5, which rounds to 49.
    let parent = Rect {
        left: 10,
        top: 0,
        width: 2750,
        height: 10,
    };
    assert_eq!(
        across(
            parent,
            Some(Amount::Percent((1.4).into())),
            Some(Amount::Pixels((10.0).into())),
            None
        ),
        (49, 10)
    );
}

#[test]
fn a_far_edge_half_in_percent_rounds_up_from_the_exact_sum() {
    // right 64.4% of 125 is exactly 80.5, so the near edge is 125 - 80.5 - 10 = 34.5, which rounds up to 35. A float gives
    // 80.50000000000001 for the inset and 34.49999999999999 for the edge, which rounds down to 34.
    assert_eq!(
        across(
            frame(125, 10),
            None,
            Some(Amount::Pixels((10.0).into())),
            Some(Amount::Percent((64.4).into()))
        ),
        (35, 10)
    );
}
