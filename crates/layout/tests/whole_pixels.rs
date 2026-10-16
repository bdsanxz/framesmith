//! Each edge of a placed box is rounded to a whole pixel, a half up, and the box covers the pixels from
//! the rounded near edge up to the rounded far edge (`docs/spec/render-command.md` section 4.1, rule 3).

use layout::{frame, place, Amount, Rect, Written};

#[test]
fn an_edge_that_lies_half_way_rounds_to_the_larger_whole_pixel() {
    // 12.5% of 100 is 12.5, which rounds to 13.
    let rect = place(
        frame(100, 10),
        Written {
            left: Some(Amount::Percent(12.5)),
            width: Some(Amount::Pixels(10.0)),
            ..Written::default()
        },
    )
    .unwrap();
    assert_eq!(
        rect,
        Rect {
            left: 13,
            top: 0,
            width: 10,
            height: 10
        }
    );
}

#[test]
fn a_negative_half_pixel_rounds_to_the_larger_whole_pixel_too() {
    // -0.5 rounds to 0 and 0.5 to 1: the box covers the one pixel at 0.
    let rect = place(
        frame(100, 10),
        Written {
            left: Some(Amount::Pixels(-0.5)),
            width: Some(Amount::Pixels(1.0)),
            ..Written::default()
        },
    )
    .unwrap();
    assert_eq!(
        rect,
        Rect {
            left: 0,
            top: 0,
            width: 1,
            height: 10
        }
    );
}

#[test]
fn two_boxes_that_meet_at_a_fraction_share_one_edge() {
    // The first covers 0 to 33.3 and the second 33.3 to 66.6: the shared edge 33.3 rounds to 33 for both,
    // so the second starts where the first ends (33 to 67 in whole pixels) and neither leaves a gap.
    let first = place(
        frame(100, 10),
        Written {
            width: Some(Amount::Percent(33.3)),
            ..Written::default()
        },
    )
    .unwrap();
    let second = place(
        frame(100, 10),
        Written {
            left: Some(Amount::Percent(33.3)),
            width: Some(Amount::Percent(33.3)),
            ..Written::default()
        },
    )
    .unwrap();
    assert_eq!(first.left + first.width as i32, second.left);
    assert_eq!((first.width, second.width), (33, 34));
}

#[test]
fn a_box_of_no_width_covers_no_pixel_wherever_it_lies() {
    let rect = place(
        frame(100, 10),
        Written {
            left: Some(Amount::Percent(33.4)),
            width: Some(Amount::Percent(0.0)),
            ..Written::default()
        },
    )
    .unwrap();
    assert_eq!(rect.width, 0);
    assert_eq!(rect.visible(100, 10), None);
}
