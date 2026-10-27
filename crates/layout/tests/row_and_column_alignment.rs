//! Where the group of children sits along the main axis (`justify-content`), and where each child sits
//! across (`align-items`, `align-self`, stretch) (`docs/spec/render-command.md` section 4.3, rules 5, 7
//! and 10).

use layout::{flow, frame, Across, Along, Amount, Direction, Flex, Item, Rect, Sides, Written};

fn sized(width: f64, height: f64) -> Item {
    Item {
        written: Written {
            width: Some(Amount::Pixels((width).into())),
            height: Some(Amount::Pixels((height).into())),
            ..Written::default()
        },
        ..Item::default()
    }
}

fn known(width: f64, height: f64) -> Written {
    Written {
        width: Some(Amount::Pixels((width).into())),
        height: Some(Amount::Pixels((height).into())),
        ..Written::default()
    }
}

fn rect(left: i32, top: i32, width: u32, height: u32) -> Rect {
    Rect { left, top, width, height }
}

#[test]
fn a_centred_group_has_the_same_room_on_both_sides() {
    let flex = Flex {
        along: Along::Center,
        gap: 10.0,
        ..Flex::default()
    };
    let items = [sized(20.0, 10.0), sized(30.0, 10.0)];
    let flowed = flow(frame(400, 300), &known(100.0, 10.0), &flex, &items).expect("placed");
    assert_eq!(flowed.items, vec![rect(20, 0, 20, 10), rect(50, 0, 30, 10)]);
}

#[test]
fn an_end_group_sits_at_the_end_edge_of_the_content_box() {
    let flex = Flex {
        along: Along::End,
        gap: 10.0,
        ..Flex::default()
    };
    let items = [sized(20.0, 10.0), sized(30.0, 10.0)];
    let flowed = flow(frame(400, 300), &known(100.0, 10.0), &flex, &items).expect("placed");
    assert_eq!(flowed.items, vec![rect(40, 0, 20, 10), rect(70, 0, 30, 10)]);
}

#[test]
fn across_a_row_each_child_sits_at_the_start_centre_or_end() {
    let container = known(100.0, 50.0);
    let item = [sized(20.0, 10.0)];
    let start = flow(frame(400, 300), &container, &Flex::default(), &item).expect("placed");
    let centre = flow(
        frame(400, 300),
        &container,
        &Flex {
            across: Across::Center,
            ..Flex::default()
        },
        &item,
    )
    .expect("placed");
    let end = flow(
        frame(400, 300),
        &container,
        &Flex {
            across: Across::End,
            ..Flex::default()
        },
        &item,
    )
    .expect("placed");
    assert_eq!(start.items, vec![rect(0, 0, 20, 10)]);
    assert_eq!(centre.items, vec![rect(0, 20, 20, 10)]);
    assert_eq!(end.items, vec![rect(0, 40, 20, 10)]);
}

#[test]
fn a_stretched_child_with_no_height_fills_the_content_box_across() {
    let item = Item {
        written: Written {
            width: Some(Amount::Pixels((20.0).into())),
            ..Written::default()
        },
        ..Item::default()
    };
    let flowed = flow(frame(400, 300), &known(100.0, 50.0), &Flex::default(), &[item]).expect("placed");
    assert_eq!(flowed.items, vec![rect(0, 0, 20, 50)]);
}

#[test]
fn a_stretched_child_keeps_the_height_it_writes_and_sits_at_the_start() {
    let flowed = flow(frame(400, 300), &known(100.0, 50.0), &Flex::default(), &[sized(20.0, 10.0)]).expect("placed");
    assert_eq!(flowed.items, vec![rect(0, 0, 20, 10)]);
}

#[test]
fn a_row_of_stretched_children_alone_has_no_height_across() {
    let item = Item {
        written: Written {
            width: Some(Amount::Pixels((20.0).into())),
            ..Written::default()
        },
        ..Item::default()
    };
    let flowed = flow(frame(400, 300), &Written::default(), &Flex::default(), &[item]).expect("placed");
    assert_eq!(flowed.container, rect(0, 0, 20, 0));
    assert_eq!(flowed.items, vec![rect(0, 0, 20, 0)]);
}

#[test]
fn a_stretched_child_fills_the_content_box_less_its_margins() {
    let item = Item {
        written: Written {
            width: Some(Amount::Pixels((20.0).into())),
            ..Written::default()
        },
        margin: Sides {
            top: 4.0,
            bottom: 6.0,
            ..Sides::default()
        },
        ..Item::default()
    };
    let flowed = flow(frame(400, 300), &known(100.0, 50.0), &Flex::default(), &[item]).expect("placed");
    assert_eq!(flowed.items, vec![rect(0, 4, 20, 40)]);
}

#[test]
fn a_child_own_align_self_takes_the_place_of_align_items() {
    let item = Item {
        across: Some(Across::End),
        ..sized(20.0, 10.0)
    };
    let flex = Flex {
        across: Across::Start,
        ..Flex::default()
    };
    let flowed = flow(frame(400, 300), &known(100.0, 50.0), &flex, &[item]).expect("placed");
    assert_eq!(flowed.items, vec![rect(0, 40, 20, 10)]);
}

#[test]
fn a_column_centres_its_children_across_its_width() {
    let flex = Flex {
        direction: Direction::Column,
        across: Across::Center,
        ..Flex::default()
    };
    let flowed = flow(frame(400, 300), &known(100.0, 50.0), &flex, &[sized(20.0, 10.0)]).expect("placed");
    assert_eq!(flowed.items, vec![rect(40, 0, 20, 10)]);
}

#[test]
fn a_column_stretches_a_child_with_no_width_across_its_width() {
    let item = Item {
        written: Written {
            height: Some(Amount::Pixels((10.0).into())),
            ..Written::default()
        },
        ..Item::default()
    };
    let flex = Flex {
        direction: Direction::Column,
        ..Flex::default()
    };
    let flowed = flow(frame(400, 300), &known(100.0, 50.0), &flex, &[item]).expect("placed");
    assert_eq!(flowed.items, vec![rect(0, 0, 100, 10)]);
}

#[test]
fn a_column_with_no_width_is_as_wide_as_its_widest_child_and_stretches_the_others() {
    // The second child writes no width, so it stretches to the 20 of the first across the column.
    let second = Item {
        written: Written {
            height: Some(Amount::Pixels((10.0).into())),
            ..Written::default()
        },
        ..Item::default()
    };
    let flex = Flex {
        direction: Direction::Column,
        gap: 4.0,
        ..Flex::default()
    };
    let flowed = flow(frame(400, 300), &Written::default(), &flex, &[sized(20.0, 10.0), second]).expect("placed");
    assert_eq!(flowed.container, rect(0, 0, 20, 24));
    assert_eq!(flowed.items, vec![rect(0, 0, 20, 10), rect(0, 14, 20, 10)]);
}
