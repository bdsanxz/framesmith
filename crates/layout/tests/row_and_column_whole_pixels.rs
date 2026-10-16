//! The edges of the children of a row or a column are worked out exactly, then rounded a half up, so
//! that two neighbours which meet at a fraction share one pixel edge (`docs/spec/render-command.md`
//! section 4.3, rule 9).

use layout::{flow, frame, Amount, Flex, Item, Rect, Written};

fn sized(width: f64, height: f64) -> Item {
    Item {
        written: Written {
            width: Some(Amount::Pixels(width)),
            height: Some(Amount::Pixels(height)),
            ..Written::default()
        },
        ..Item::default()
    }
}

fn rect(left: i32, top: i32, width: u32, height: u32) -> Rect {
    Rect { left, top, width, height }
}

#[test]
fn two_children_that_meet_at_a_fraction_share_one_pixel_edge() {
    let items = [sized(33.5, 10.0), sized(33.5, 10.0)];
    let flowed = flow(frame(400, 300), Written::default(), &Flex::default(), &items).expect("placed");
    assert_eq!(flowed.container, rect(0, 0, 67, 10));
    assert_eq!(flowed.items, vec![rect(0, 0, 34, 10), rect(34, 0, 33, 10)]);
}

#[test]
fn a_gap_that_is_a_fraction_starts_its_child_at_the_rounded_edge() {
    let flex = Flex {
        gap: 2.5,
        ..Flex::default()
    };
    let items = [sized(10.0, 10.0), sized(10.0, 10.0)];
    let flowed = flow(frame(400, 300), Written::default(), &flex, &items).expect("placed");
    assert_eq!(flowed.container, rect(0, 0, 23, 10));
    assert_eq!(flowed.items, vec![rect(0, 0, 10, 10), rect(13, 0, 10, 10)]);
}
