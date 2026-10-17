//! Spreading the children of a row between its edges (`docs/spec/render-command.md` section 4.3, rule 5,
//! `justify-content="space-between"`).

use layout::{flow, frame, Along, Amount, Flex, Item, Rect, Written};

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

fn row_of(width: f64) -> Written {
    Written {
        width: Some(Amount::Pixels(width)),
        ..Written::default()
    }
}

fn space_between(gap: f64) -> Flex {
    Flex {
        along: Along::SpaceBetween,
        gap,
        ..Flex::default()
    }
}

fn rect(left: i32, top: i32, width: u32, height: u32) -> Rect {
    Rect { left, top, width, height }
}

#[test]
fn the_first_item_sits_at_the_start_and_the_last_at_the_end() {
    let items = [sized(20.0, 10.0), sized(30.0, 10.0), sized(20.0, 10.0)];
    let flowed = flow(frame(400, 300), row_of(200.0), &space_between(0.0), &items).expect("placed");
    assert_eq!(flowed.items[0].left, 0);
    assert_eq!(flowed.items[2].left + flowed.items[2].width as i32, 200);
}

#[test]
fn the_room_on_both_sides_of_the_middle_item_is_equal() {
    let items = [sized(20.0, 10.0), sized(30.0, 10.0), sized(20.0, 10.0)];
    let flowed = flow(frame(400, 300), row_of(200.0), &space_between(0.0), &items).expect("placed");
    assert_eq!(flowed.items, vec![rect(0, 0, 20, 10), rect(85, 0, 30, 10), rect(180, 0, 20, 10)]);
    let before = flowed.items[1].left - (flowed.items[0].left + flowed.items[0].width as i32);
    let after = flowed.items[2].left - (flowed.items[1].left + flowed.items[1].width as i32);
    assert_eq!(before, after);
}

#[test]
fn the_gap_is_kept_on_top_of_the_shared_room() {
    // The room left over is 200 less 70 less two gaps of 10, which is 110: 55 shared between each two
    // neighbours on top of the 10 of the gap, so each neighbour is 65 from the next item.
    let items = [sized(20.0, 10.0), sized(30.0, 10.0), sized(20.0, 10.0)];
    let flowed = flow(frame(400, 300), row_of(200.0), &space_between(10.0), &items).expect("placed");
    assert_eq!(flowed.items, vec![rect(0, 0, 20, 10), rect(85, 0, 30, 10), rect(180, 0, 20, 10)]);
}

#[test]
fn one_item_is_placed_at_the_start_as_flex_start_does() {
    let items = [sized(20.0, 10.0)];
    let flowed = flow(frame(400, 300), row_of(200.0), &space_between(0.0), &items).expect("placed");
    assert_eq!(flowed.items, vec![rect(0, 0, 20, 10)]);
}

#[test]
fn a_group_larger_than_the_row_runs_over_the_end_edge_from_the_start() {
    let items = [sized(120.0, 10.0), sized(120.0, 10.0)];
    let flowed = flow(frame(400, 300), row_of(200.0), &space_between(0.0), &items).expect("placed");
    assert_eq!(flowed.items, vec![rect(0, 0, 120, 10), rect(120, 0, 120, 10)]);
}
