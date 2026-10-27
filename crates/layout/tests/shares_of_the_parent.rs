//! A place or a size written as a percentage is a share of the parent's width (across) or height
//! (down) (`docs/spec/render-command.md` section 4.1).

use layout::{frame, place, Amount, Rect, Written};

fn across(left: Option<Amount>, width: Option<Amount>, parent: u32) -> (i32, u32) {
    let rect = place(
        frame(parent, 50),
        &Written {
            left,
            width,
            ..Written::default()
        },
    )
    .unwrap();
    (rect.left, rect.width)
}

#[test]
fn a_left_and_a_width_in_percent_are_shares_of_the_parent_width() {
    // 10% of 96 is 9.6 and 50% of it is 48: the edges lie at 9.6 and 57.6, which round to 10 and 58.
    assert_eq!(
        across(Some(Amount::Percent((10.0).into())), Some(Amount::Percent((50.0).into())), 96),
        (10, 48)
    );
}

#[test]
fn a_share_of_a_parent_of_an_odd_width_is_worked_out_exactly_and_then_rounded() {
    // 50% of 99 is 49.5, which rounds half up to 50: the box covers the columns 0 to 49.
    assert_eq!(across(None, Some(Amount::Percent((50.0).into())), 99), (0, 50));
}

#[test]
fn a_share_of_the_height_is_taken_down_the_frame() {
    let rect = place(
        frame(200, 100),
        &Written {
            top: Some(Amount::Percent((25.0).into())),
            height: Some(Amount::Percent((50.0).into())),
            ..Written::default()
        },
    )
    .unwrap();
    assert_eq!(
        rect,
        Rect {
            left: 0,
            top: 25,
            width: 200,
            height: 50
        }
    );
}

#[test]
fn a_share_of_the_parent_is_taken_of_the_parent_and_not_of_the_frame() {
    let parent = Rect {
        left: 20,
        top: 10,
        width: 200,
        height: 100,
    };
    let rect = place(
        parent,
        &Written {
            left: Some(Amount::Percent((10.0).into())),
            width: Some(Amount::Percent((25.0).into())),
            ..Written::default()
        },
    )
    .unwrap();
    // 10% of 200 is 20, so the box starts at 20 + 20 = 40; 25% of 200 is 50.
    assert_eq!(
        rect,
        Rect {
            left: 40,
            top: 10,
            width: 50,
            height: 100
        }
    );
}

#[test]
fn a_percentage_may_be_above_100_or_below_0() {
    // 150% of 100 from the left edge: a box of 100 pixels that starts 150 pixels in.
    assert_eq!(
        across(Some(Amount::Percent((150.0).into())), Some(Amount::Percent((100.0).into())), 100),
        (150, 100)
    );
    // -10% of 100 is a distance of -10 pixels.
    assert_eq!(
        across(Some(Amount::Percent((-10.0).into())), Some(Amount::Pixels((5.0).into())), 100),
        (-10, 5)
    );
}
