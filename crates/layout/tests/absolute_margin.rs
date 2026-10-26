//! The margin of an absolutely placed item moves it in from each edge it is placed by (`docs/spec/render-command.md`
//! section 4.1, rule 8; `notation.md` N10). Every rectangle below is worked out by hand from the rule: with `left`,
//! the left edge lies at `left` plus the left margin; with `right` and no `left`, the right edge lies `right` plus the
//! right margin in from the parent's right edge; the same down; with both edges and no size, the size is what lies
//! between them less the two margins; on an axis with no edge the item lies at its static position (the parent's near
//! edge in a plain box; the place the container would give it as its only item in a row, a column or a set) moved in by
//! its near margin (B41).

use layout::{
    columns, flow, flow_absolute, frame, place, place_absolute, Across, Along, Amount, Cell, Columns, Direction, Flex, Item, Rect, Sides,
    Track, Written,
};

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
fn on_an_axis_with_no_edge_the_near_margin_moves_the_item_from_the_parent_s_near_edge() {
    // No `left` or `right`: across the item starts at the parent's left edge moved in by its left margin of 9, 20
    // wide; the right margin moves nothing. Down, top 5 + 1 = 6.
    let written = Written {
        top: px(5.0),
        width: px(20.0),
        height: px(10.0),
        ..Written::default()
    };
    assert_eq!(
        place_absolute(frame(96, 54), written, sides(1.0, 9.0, 1.0, 9.0)),
        Ok(rect(9, 6, 20, 10))
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

/// An item 10 by 6 that writes no edge, with `margin`.
fn edgeless(margin: Sides) -> Item {
    Item {
        written: Written {
            width: px(10.0),
            height: px(6.0),
            ..Written::default()
        },
        margin,
        absolute: true,
        ..Item::default()
    }
}

#[test]
fn an_absolute_item_of_a_row_with_no_edge_lies_where_the_row_would_put_it_as_its_only_item() {
    // A row 100 by 40 with padding 4, `justify-content` center and `align-items` center: a content box 92 by 32 from 4, 4.
    // The item 10 by 6 with margin 2 0 0 4 is 14 wide and 8 high with its margins. Along: 4 + (92 - 14) / 2 = 43, and
    // its left margin puts its left edge at 47. Across: 4 + (32 - 8) / 2 = 16, and its top margin puts its top at 18.
    // The flowed item before it changes nothing, as the static position is that of an only item. With `top="0"` it is
    // placed by that edge down: 0 + 2 = 2, and keeps its static place across.
    let row = Flex {
        direction: Direction::Row,
        along: Along::Center,
        across: Across::Center,
        gap: 0.0,
        padding: sides(4.0, 4.0, 4.0, 4.0),
    };
    let sized = Written {
        width: px(100.0),
        height: px(40.0),
        ..Written::default()
    };
    let flowed_item = Item {
        written: Written {
            width: px(30.0),
            height: px(10.0),
            ..Written::default()
        },
        ..Item::default()
    };
    let item = edgeless(sides(2.0, 0.0, 0.0, 4.0));
    let mut topped = item;
    topped.written.top = px(0.0);
    let laid = flow(frame(400, 300), sized, &row, &[flowed_item, item, topped]).expect("placed");
    assert_eq!(laid.items[1], rect(47, 18, 10, 6));
    assert_eq!(laid.items[2], rect(47, 2, 10, 6));
}

#[test]
fn an_absolute_item_of_a_column_with_no_edge_lies_at_the_end_where_the_column_puts_its_items() {
    // A column 40 by 100, `justify-content` flex-end and `align-items` flex-end. The item 10 by 6 with margin 1 2 3 4 is
    // 10 high and 16 wide with its margins. Down: 100 - 10 = 90, top 91. Across: 40 - 16 = 24, left 28.
    let column = Flex {
        direction: Direction::Column,
        along: Along::End,
        across: Across::End,
        gap: 0.0,
        padding: Sides::default(),
    };
    let sized = Written {
        width: px(40.0),
        height: px(100.0),
        ..Written::default()
    };
    let laid = flow(frame(400, 300), sized, &column, &[edgeless(sides(1.0, 2.0, 3.0, 4.0))]).expect("placed");
    assert_eq!(laid.items, vec![rect(28, 91, 10, 6)]);
}

#[test]
fn an_absolute_cell_of_a_set_with_no_edge_lies_where_the_set_would_put_it_as_its_only_cell() {
    // A set of columns 30 and 50, gap 5, padding 2, 40 high, `align-items` center: one row 36 high from 2. The cell 10 by
    // 6 with margin 2 0 0 3 stands at the start of the first column: 2 + 3 = 5. Down, 8 high with its margins, in the
    // middle of the row: 2 + (36 - 8) / 2 = 16, and its top margin puts it at 18.
    let set = Columns {
        tracks: vec![Track::Pixels(30.0), Track::Pixels(50.0)],
        gap: 5.0,
        padding: sides(2.0, 2.0, 2.0, 2.0),
        across: Across::Center,
    };
    let cell = Cell {
        written: Written {
            width: px(10.0),
            height: px(6.0),
            ..Written::default()
        },
        margin: sides(2.0, 0.0, 0.0, 3.0),
        absolute: true,
        ..Cell::default()
    };
    let height_40 = Written {
        height: px(40.0),
        ..Written::default()
    };
    let laid = columns(frame(300, 300), height_40, &set, &[cell]).expect("placed");
    assert_eq!(laid.items, vec![rect(5, 18, 10, 6)]);
}

#[test]
fn a_row_placed_in_a_plain_box_is_moved_in_by_its_margin() {
    // A row at left 4, top 2, 20 by 10 with margin 3 0 0 5 lies at 9, 5; with no edge and the same margin, at 5, 3.
    let written = Written {
        left: px(4.0),
        top: px(2.0),
        width: px(20.0),
        height: px(10.0),
        ..Written::default()
    };
    let margin = sides(3.0, 0.0, 0.0, 5.0);
    let laid = flow_absolute(frame(96, 54), written, margin, &Flex::default(), &[]).expect("placed");
    assert_eq!(laid.container, rect(9, 5, 20, 10));
    let edgeless_row = Written {
        left: None,
        top: None,
        ..written
    };
    let laid = flow_absolute(frame(96, 54), edgeless_row, margin, &Flex::default(), &[]).expect("placed");
    assert_eq!(laid.container, rect(5, 3, 20, 10));
}
