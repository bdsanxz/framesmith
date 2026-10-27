//! The children of a row or a column follow each other in document order with the gap between
//! neighbours, and a column stacks them down (`docs/spec/render-command.md` section 4.3, rules 1, 2
//! and 4).

use layout::{flow, frame, Amount, Direction, Flex, Item, Rect, Written};

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
fn row_children_follow_each_other_in_document_order_with_the_gap_between() {
    let flex = Flex {
        gap: 10.0,
        ..Flex::default()
    };
    let items = [sized(30.0, 20.0), sized(40.0, 20.0), sized(50.0, 20.0)];
    let flowed = flow(frame(400, 300), &Written::default(), &flex, &items).expect("placed");
    assert_eq!(flowed.container, rect(0, 0, 140, 20));
    assert_eq!(flowed.items, vec![rect(0, 0, 30, 20), rect(40, 0, 40, 20), rect(90, 0, 50, 20)]);
}

#[test]
fn column_children_stack_down_with_the_gap_between() {
    let flex = Flex {
        direction: Direction::Column,
        gap: 5.0,
        ..Flex::default()
    };
    let items = [sized(50.0, 10.0), sized(50.0, 20.0), sized(50.0, 30.0)];
    let flowed = flow(frame(400, 300), &Written::default(), &flex, &items).expect("placed");
    assert_eq!(flowed.container, rect(0, 0, 50, 70));
    assert_eq!(flowed.items, vec![rect(0, 0, 50, 10), rect(0, 15, 50, 20), rect(0, 40, 50, 30)]);
}

#[test]
fn a_gap_of_zero_leaves_the_children_touching() {
    let items = [sized(10.0, 10.0), sized(10.0, 10.0)];
    let flowed = flow(frame(400, 300), &Written::default(), &Flex::default(), &items).expect("placed");
    assert_eq!(flowed.container, rect(0, 0, 20, 10));
    assert_eq!(flowed.items, vec![rect(0, 0, 10, 10), rect(10, 0, 10, 10)]);
}

#[test]
fn no_gap_is_drawn_before_the_first_child_or_after_the_last() {
    let flex = Flex {
        gap: 5.0,
        ..Flex::default()
    };
    let written = Written {
        width: Some(Amount::Pixels((100.0).into())),
        height: Some(Amount::Pixels((10.0).into())),
        ..Written::default()
    };
    let items = [sized(10.0, 10.0), sized(10.0, 10.0)];
    let flowed = flow(frame(400, 300), &written, &flex, &items).expect("placed");
    assert_eq!(flowed.container, rect(0, 0, 100, 10));
    assert_eq!(flowed.items, vec![rect(0, 0, 10, 10), rect(15, 0, 10, 10)]);
}

#[test]
fn a_row_is_placed_in_its_parent_as_a_box_is() {
    let written = Written {
        left: Some(Amount::Pixels((10.0).into())),
        top: Some(Amount::Pixels((20.0).into())),
        width: Some(Amount::Pixels((50.0).into())),
        height: Some(Amount::Pixels((30.0).into())),
        ..Written::default()
    };
    let items = [sized(20.0, 10.0)];
    let flowed = flow(frame(200, 100), &written, &Flex::default(), &items).expect("placed");
    assert_eq!(flowed.container, rect(10, 20, 50, 30));
    assert_eq!(flowed.items, vec![rect(10, 20, 20, 10)]);
}

#[test]
fn a_child_takes_no_notice_of_its_own_left_and_top() {
    let written = Written {
        left: Some(Amount::Pixels((10.0).into())),
        top: Some(Amount::Pixels((20.0).into())),
        width: Some(Amount::Pixels((50.0).into())),
        height: Some(Amount::Pixels((30.0).into())),
        ..Written::default()
    };
    let item = Item {
        written: Written {
            left: Some(Amount::Pixels((40.0).into())),
            top: Some(Amount::Pixels((35.0).into())),
            width: Some(Amount::Pixels((20.0).into())),
            height: Some(Amount::Pixels((10.0).into())),
            ..Written::default()
        },
        ..Item::default()
    };
    let flowed = flow(frame(200, 100), &written, &Flex::default(), &[item]).expect("placed");
    assert_eq!(flowed.items, vec![rect(10, 20, 20, 10)]);
}
