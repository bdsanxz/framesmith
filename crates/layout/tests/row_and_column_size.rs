//! The size of a row or a column with no size of its own: its children, the gaps and the padding
//! (`docs/spec/render-command.md` section 4.3, rule 1). Percentages of a child, and absolute children,
//! which take no room (rules 3 and 6).

use layout::{flow, frame, Amount, Direction, Flex, Item, Rect, Sides, Written};

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

fn rect(left: i32, top: i32, width: u32, height: u32) -> Rect {
    Rect { left, top, width, height }
}

#[test]
fn a_row_with_no_size_is_as_large_as_its_children_gaps_and_padding() {
    let flex = Flex {
        gap: 10.0,
        padding: Sides {
            top: 3.0,
            right: 5.0,
            bottom: 3.0,
            left: 5.0,
        },
        ..Flex::default()
    };
    let items = [sized(30.0, 10.0), sized(30.0, 10.0)];
    let flowed = flow(frame(400, 300), &Written::default(), &flex, &items).expect("placed");
    assert_eq!(flowed.container, rect(0, 0, 80, 16));
    assert_eq!(flowed.items, vec![rect(5, 3, 30, 10), rect(45, 3, 30, 10)]);
}

#[test]
fn a_row_with_no_children_is_as_large_as_its_padding() {
    let flex = Flex {
        padding: Sides {
            top: 4.0,
            right: 4.0,
            bottom: 4.0,
            left: 4.0,
        },
        ..Flex::default()
    };
    let flowed = flow(frame(400, 300), &Written::default(), &flex, &[]).expect("placed");
    assert_eq!(flowed.container, rect(0, 0, 8, 8));
    assert!(flowed.items.is_empty());
}

#[test]
fn a_row_keeps_the_width_it_writes_and_is_as_tall_as_its_children() {
    let written = Written {
        width: Some(Amount::Pixels((100.0).into())),
        ..Written::default()
    };
    let flowed = flow(frame(400, 300), &written, &Flex::default(), &[sized(30.0, 10.0)]).expect("placed");
    assert_eq!(flowed.container, rect(0, 0, 100, 10));
    assert_eq!(flowed.items, vec![rect(0, 0, 30, 10)]);
}

#[test]
fn a_percentage_of_a_row_with_no_size_of_its_own_is_0() {
    let item = Item {
        written: Written {
            width: Some(Amount::Percent((50.0).into())),
            height: Some(Amount::Pixels((10.0).into())),
            ..Written::default()
        },
        ..Item::default()
    };
    let flowed = flow(frame(400, 300), &Written::default(), &Flex::default(), &[item]).expect("placed");
    assert_eq!(flowed.container, rect(0, 0, 0, 10));
    assert_eq!(flowed.items, vec![rect(0, 0, 0, 10)]);
}

#[test]
fn a_percentage_of_a_row_with_a_size_of_its_own_is_of_that_size() {
    let written = Written {
        width: Some(Amount::Pixels((200.0).into())),
        height: Some(Amount::Pixels((40.0).into())),
        ..Written::default()
    };
    let item = Item {
        written: Written {
            width: Some(Amount::Percent((25.0).into())),
            height: Some(Amount::Percent((50.0).into())),
            ..Written::default()
        },
        ..Item::default()
    };
    let flowed = flow(frame(400, 300), &written, &Flex::default(), &[item]).expect("placed");
    assert_eq!(flowed.items, vec![rect(0, 0, 50, 20)]);
}

#[test]
fn an_absolute_child_takes_no_room_and_does_not_move_the_others() {
    let absolute = Item {
        written: Written {
            left: Some(Amount::Pixels((5.0).into())),
            top: Some(Amount::Pixels((7.0).into())),
            width: Some(Amount::Pixels((20.0).into())),
            height: Some(Amount::Pixels((4.0).into())),
            ..Written::default()
        },
        absolute: true,
        ..Item::default()
    };
    let items = [sized(30.0, 10.0), absolute, sized(30.0, 10.0)];
    let flowed = flow(frame(400, 300), &Written::default(), &Flex::default(), &items).expect("placed");
    assert_eq!(flowed.container, rect(0, 0, 60, 10));
    assert_eq!(flowed.items, vec![rect(0, 0, 30, 10), rect(5, 7, 20, 4), rect(30, 0, 30, 10)]);
}

#[test]
fn an_absolute_child_is_not_counted_in_the_gaps() {
    let absolute = Item {
        written: Written {
            left: Some(Amount::Pixels((5.0).into())),
            top: Some(Amount::Pixels((0.0).into())),
            width: Some(Amount::Pixels((2.0).into())),
            height: Some(Amount::Pixels((2.0).into())),
            ..Written::default()
        },
        absolute: true,
        ..Item::default()
    };
    let flex = Flex {
        gap: 10.0,
        ..Flex::default()
    };
    let items = [sized(10.0, 10.0), absolute, sized(10.0, 10.0)];
    let flowed = flow(frame(400, 300), &Written::default(), &flex, &items).expect("placed");
    assert_eq!(flowed.container, rect(0, 0, 30, 10));
    assert_eq!(flowed.items, vec![rect(0, 0, 10, 10), rect(5, 0, 2, 2), rect(20, 0, 10, 10)]);
}

#[test]
fn an_absolute_child_is_counted_from_the_outer_edges_of_the_container() {
    let absolute = Item {
        written: Written {
            left: Some(Amount::Pixels((0.0).into())),
            top: Some(Amount::Pixels((0.0).into())),
            width: Some(Amount::Pixels((4.0).into())),
            height: Some(Amount::Pixels((4.0).into())),
            ..Written::default()
        },
        absolute: true,
        ..Item::default()
    };
    let flex = Flex {
        padding: Sides {
            top: 5.0,
            right: 5.0,
            bottom: 5.0,
            left: 5.0,
        },
        ..Flex::default()
    };
    let items = [sized(30.0, 10.0), absolute];
    let flowed = flow(frame(400, 300), &Written::default(), &flex, &items).expect("placed");
    assert_eq!(flowed.container, rect(0, 0, 40, 20));
    assert_eq!(flowed.items, vec![rect(5, 5, 30, 10), rect(0, 0, 4, 4)]);
}

#[test]
fn an_absolute_child_right_and_bottom_edges_are_counted_from_the_container() {
    let absolute = Item {
        written: Written {
            right: Some(Amount::Pixels((0.0).into())),
            bottom: Some(Amount::Pixels((0.0).into())),
            width: Some(Amount::Pixels((10.0).into())),
            height: Some(Amount::Pixels((5.0).into())),
            ..Written::default()
        },
        absolute: true,
        ..Item::default()
    };
    let written = Written {
        width: Some(Amount::Pixels((100.0).into())),
        height: Some(Amount::Pixels((50.0).into())),
        ..Written::default()
    };
    let flowed = flow(frame(400, 300), &written, &Flex::default(), &[absolute]).expect("placed");
    assert_eq!(flowed.items, vec![rect(90, 45, 10, 5)]);
}

#[test]
fn a_column_is_as_tall_as_its_children_and_as_wide_as_the_widest() {
    let flex = Flex {
        direction: Direction::Column,
        ..Flex::default()
    };
    let items = [sized(10.0, 10.0), sized(40.0, 10.0)];
    let flowed = flow(frame(400, 300), &Written::default(), &flex, &items).expect("placed");
    assert_eq!(flowed.container, rect(0, 0, 40, 20));
    assert_eq!(flowed.items, vec![rect(0, 0, 10, 10), rect(0, 10, 40, 10)]);
}
