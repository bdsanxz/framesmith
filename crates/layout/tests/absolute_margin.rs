//! The margin of an absolutely placed item moves it in from each edge it is placed by (`docs/spec/render-command.md`
//! section 4.1, rule 8; `notation.md` N10). Every rectangle below is worked out by hand from the rule: with `left`,
//! the left edge lies at `left` plus the left margin; with `right` and no `left`, the right edge lies `right` plus the
//! right margin in from the parent's right edge; the same down; with both edges and no size, the size is what lies
//! between them less the two margins; a margin on an axis with no edge moves nothing.

use layout::{columns, flow, frame, place, place_absolute, Across, Amount, Cell, Columns, Flex, Item, Rect, Sides, Track, Written};

fn rect(left: i32, top: i32, width: u32, height: u32) -> Rect {
    Rect { left, top, width, height }
}

fn px(value: f64) -> Option<Amount> {
    Some(Amount::Pixels(value))
}

fn sides(top: f64, right: f64, bottom: f64, left: f64) -> Sides {
    Sides { top, right, bottom, left }
}

#[test]
fn a_margin_moves_an_item_placed_by_left_and_top_in_by_the_left_and_top_margins() {
    // left 50 + 30 = 80, top 10 + 30 = 40; the size stays 20 by 20.
    let written = Written {
        left: px(50.0),
        top: px(10.0),
        width: px(20.0),
        height: px(20.0),
        ..Written::default()
    };
    assert_eq!(
        place_absolute(frame(96, 54), written, sides(30.0, 30.0, 30.0, 30.0)),
        Ok(rect(80, 40, 20, 20))
    );
    // With no margin it lies where its edges put it, as `place` puts it.
    assert_eq!(
        place_absolute(frame(96, 54), written, Sides::default()),
        place(frame(96, 54), written)
    );
    assert_eq!(place(frame(96, 54), written), Ok(rect(50, 10, 20, 20)));
}

#[test]
fn a_margin_moves_an_item_placed_by_right_and_bottom_in_by_the_right_and_bottom_margins() {
    // right 6 + 4 = 10 from the right edge of 96: the right edge at 86, the left at 86 - 12 = 74.
    // bottom 3 + 5 = 8 from the foot of 54: the foot at 46, the top at 46 - 10 = 36.
    let written = Written {
        right: px(6.0),
        bottom: px(3.0),
        width: px(12.0),
        height: px(10.0),
        ..Written::default()
    };
    assert_eq!(
        place_absolute(frame(96, 54), written, sides(1.0, 4.0, 5.0, 2.0)),
        Ok(rect(74, 36, 12, 10))
    );
}

#[test]
fn with_both_edges_and_no_size_the_size_is_what_lies_between_them_less_the_two_margins() {
    // Across: 96 - 10 - 20 - 3 - 7 = 56, from 10 + 7 = 17. Down: 54 - 4 - 6 - 2 - 8 = 34, from 4 + 2 = 6.
    let written = Written {
        left: px(10.0),
        right: px(20.0),
        top: px(4.0),
        bottom: px(6.0),
        ..Written::default()
    };
    assert_eq!(
        place_absolute(frame(96, 54), written, sides(2.0, 3.0, 8.0, 7.0)),
        Ok(rect(17, 6, 56, 34))
    );
}

#[test]
fn with_both_edges_and_a_size_the_near_edge_and_its_margin_place_the_item() {
    // As without a margin, a written size keeps the near edge: left 10 + 7 = 17, top 4 + 2 = 6.
    let written = Written {
        left: px(10.0),
        right: px(20.0),
        top: px(4.0),
        bottom: px(6.0),
        width: px(5.0),
        height: px(5.0),
        ..Written::default()
    };
    assert_eq!(
        place_absolute(frame(96, 54), written, sides(2.0, 3.0, 8.0, 7.0)),
        Ok(rect(17, 6, 5, 5))
    );
}

#[test]
fn margins_larger_than_the_room_between_two_edges_leave_a_size_of_zero() {
    // 96 - 40 - 40 - 10 - 10 = -4: the size is 0, at 40 + 10 = 50.
    let written = Written {
        left: px(40.0),
        right: px(40.0),
        top: px(0.0),
        height: px(4.0),
        ..Written::default()
    };
    assert_eq!(
        place_absolute(frame(96, 54), written, sides(0.0, 10.0, 0.0, 10.0)),
        Ok(rect(50, 0, 0, 4))
    );
}

#[test]
fn a_margin_on_an_axis_with_no_edge_moves_nothing() {
    // No `left` or `right`: across the item starts at the parent's left edge, 20 wide, whatever its side margins.
    let written = Written {
        top: px(5.0),
        width: px(20.0),
        height: px(10.0),
        ..Written::default()
    };
    assert_eq!(
        place_absolute(frame(96, 54), written, sides(1.0, 9.0, 1.0, 9.0)),
        Ok(rect(0, 6, 20, 10))
    );
}

#[test]
fn a_margin_is_added_to_an_edge_written_as_a_share_of_the_parent() {
    // left 25 % of 96 = 24, + 6 = 30; top 50 % of 54 = 27, + 3 = 30.
    let written = Written {
        left: Some(Amount::Percent(25.0)),
        top: Some(Amount::Percent(50.0)),
        width: px(10.0),
        height: px(10.0),
        ..Written::default()
    };
    assert_eq!(
        place_absolute(frame(96, 54), written, sides(3.0, 0.0, 0.0, 6.0)),
        Ok(rect(30, 30, 10, 10))
    );
}

#[test]
fn an_absolute_item_of_a_row_is_moved_in_by_its_margin_and_a_flowed_one_keeps_its_place() {
    // The row is at 0, 0. The absolute item at right 0, top 0, 8 by 6, with margin 2 3 4 5: its right edge 3 in from
    // the row's right edge, its top 2 down. The row is as wide as its flowed items, 30 + 30 = 60: right edge 57, left 49.
    let flowed_item = Item {
        written: Written {
            width: px(30.0),
            height: px(10.0),
            ..Written::default()
        },
        ..Item::default()
    };
    let absolute = Item {
        written: Written {
            right: px(0.0),
            top: px(0.0),
            width: px(8.0),
            height: px(6.0),
            ..Written::default()
        },
        margin: sides(2.0, 3.0, 4.0, 5.0),
        absolute: true,
        ..Item::default()
    };
    let laid = flow(
        frame(400, 300),
        Written::default(),
        &Flex::default(),
        &[flowed_item, absolute, flowed_item],
    )
    .expect("placed");
    assert_eq!(laid.container, rect(0, 0, 60, 10));
    assert_eq!(laid.items, vec![rect(0, 0, 30, 10), rect(49, 2, 8, 6), rect(30, 0, 30, 10)]);
}

#[test]
fn an_absolute_cell_of_a_set_is_moved_in_by_its_margin() {
    // A set of one column 50 wide, 40 high. The absolute cell at left 4, bottom 4, 10 by 10, margin 3 on every side:
    // left 4 + 3 = 7, foot 40 - 4 - 3 = 33, top 33 - 10 = 23.
    let set = Columns {
        tracks: vec![Track::Pixels(50.0)],
        gap: 0.0,
        padding: Sides::default(),
        across: Across::Stretch,
    };
    let cell = Cell {
        written: Written {
            left: px(4.0),
            bottom: px(4.0),
            width: px(10.0),
            height: px(10.0),
            ..Written::default()
        },
        margin: sides(3.0, 3.0, 3.0, 3.0),
        absolute: true,
        ..Cell::default()
    };
    let height_40 = Written {
        height: px(40.0),
        ..Written::default()
    };
    let laid = columns(frame(300, 300), height_40, &set, &[cell]).expect("placed");
    assert_eq!(laid.items, vec![rect(7, 23, 10, 10)]);
}
