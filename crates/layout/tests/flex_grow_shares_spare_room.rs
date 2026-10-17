//! Sharing the spare room of a row between its children by their `flex-grow` values, and capping a share by
//! `max-width` after it is added (`docs/spec/render-command.md` section 4.3, rules 12 and 13).

use layout::{flow, frame, Amount, Flex, Item, Rect, Written};

fn sized(width: f64, height: f64, grow: f64) -> Item {
    Item {
        written: Written {
            width: Some(Amount::Pixels(width)),
            height: Some(Amount::Pixels(height)),
            ..Written::default()
        },
        grow,
        ..Item::default()
    }
}

fn row_of(width: f64) -> Written {
    Written {
        width: Some(Amount::Pixels(width)),
        ..Written::default()
    }
}

fn rect(left: i32, top: i32, width: u32, height: u32) -> Rect {
    Rect { left, top, width, height }
}

#[test]
fn the_spare_room_is_shared_in_proportion_to_the_grow_values() {
    // The group is 20 + 20 + 20 = 60 and the spare room 140. Shares 1 : 3 : 0 give 35 and 105.
    let items = [sized(20.0, 10.0, 1.0), sized(20.0, 10.0, 3.0), sized(20.0, 10.0, 0.0)];
    let flowed = flow(frame(400, 300), row_of(200.0), &Flex::default(), &items).expect("placed");
    assert_eq!(flowed.items, vec![rect(0, 0, 55, 10), rect(55, 0, 125, 10), rect(180, 0, 20, 10)]);
}

#[test]
fn a_share_of_the_spare_room_is_capped_by_max_width_after_it_is_added() {
    // The first item writes no width, so the group is 20 and the spare room 180. Each of the two items grows
    // by 90; the first is then capped at 40, and the second keeps its share of 110, placed after the first.
    let capped = Item {
        written: Written {
            height: Some(Amount::Pixels(10.0)),
            ..Written::default()
        },
        grow: 1.0,
        max_width: Some(40.0),
        ..Item::default()
    };
    let items = [capped, sized(20.0, 10.0, 1.0)];
    let flowed = flow(frame(400, 300), row_of(200.0), &Flex::default(), &items).expect("placed");
    assert_eq!(flowed.items, vec![rect(0, 0, 40, 10), rect(40, 0, 110, 10)]);
}
