//! The padding of a row or a column, read as the sides of section 4 (one to four numbers, in the order
//! of CSS), and the margin of a child, which moves it and counts in its outer size
//! (`docs/spec/render-command.md` section 4.3, rules 2, 8 and 10).

use layout::{flow, frame, Across, Along, Amount, Direction, Flex, Item, Rect, Sides, Written};

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

fn known(width: f64, height: f64) -> Written {
    Written {
        width: Some(Amount::Pixels(width)),
        height: Some(Amount::Pixels(height)),
        ..Written::default()
    }
}

fn rect(left: i32, top: i32, width: u32, height: u32) -> Rect {
    Rect { left, top, width, height }
}

#[test]
fn one_number_of_padding_is_the_same_on_all_four_sides() {
    assert_eq!(
        Sides::from_values(&[4.0]),
        Some(Sides {
            top: 4.0,
            right: 4.0,
            bottom: 4.0,
            left: 4.0
        })
    );
}

#[test]
fn two_numbers_of_padding_are_the_top_and_bottom_then_the_left_and_right() {
    assert_eq!(
        Sides::from_values(&[4.0, 8.0]),
        Some(Sides {
            top: 4.0,
            right: 8.0,
            bottom: 4.0,
            left: 8.0
        })
    );
}

#[test]
fn three_numbers_of_padding_are_the_top_then_the_sides_then_the_bottom() {
    assert_eq!(
        Sides::from_values(&[4.0, 8.0, 2.0]),
        Some(Sides {
            top: 4.0,
            right: 8.0,
            bottom: 2.0,
            left: 8.0
        })
    );
}

#[test]
fn four_numbers_of_padding_are_the_top_right_bottom_and_left() {
    assert_eq!(
        Sides::from_values(&[1.0, 2.0, 3.0, 4.0]),
        Some(Sides {
            top: 1.0,
            right: 2.0,
            bottom: 3.0,
            left: 4.0
        })
    );
}

#[test]
fn a_count_of_sides_other_than_one_to_four_is_no_sides() {
    assert_eq!(Sides::from_values(&[]), None);
    assert_eq!(Sides::from_values(&[1.0, 2.0, 3.0, 4.0, 5.0]), None);
}

#[test]
fn padding_moves_the_children_in_from_the_edges_of_a_row_or_column() {
    let flex = Flex {
        across: Across::Start,
        padding: Sides {
            top: 5.0,
            right: 10.0,
            bottom: 5.0,
            left: 10.0,
        },
        ..Flex::default()
    };
    let flowed = flow(frame(400, 300), known(100.0, 50.0), &flex, &[sized(20.0, 10.0)]).expect("placed");
    assert_eq!(flowed.container, rect(0, 0, 100, 50));
    assert_eq!(flowed.items, vec![rect(10, 5, 20, 10)]);
}

#[test]
fn a_centred_group_is_centred_in_the_content_box_inside_the_padding() {
    let flex = Flex {
        along: Along::Center,
        padding: Sides {
            top: 0.0,
            right: 30.0,
            bottom: 0.0,
            left: 10.0,
        },
        ..Flex::default()
    };
    let flowed = flow(frame(400, 300), known(100.0, 10.0), &flex, &[sized(20.0, 10.0)]).expect("placed");
    assert_eq!(flowed.items, vec![rect(30, 0, 20, 10)]);
}

#[test]
fn a_margin_across_a_row_moves_the_child_and_counts_in_its_outer_size() {
    let margined = Item {
        written: Written {
            width: Some(Amount::Pixels(20.0)),
            height: Some(Amount::Pixels(10.0)),
            ..Written::default()
        },
        margin: Sides {
            top: 0.0,
            right: 7.0,
            bottom: 0.0,
            left: 3.0,
        },
        ..Item::default()
    };
    let flowed = flow(
        frame(400, 300),
        Written::default(),
        &Flex::default(),
        &[margined, sized(10.0, 10.0)],
    )
    .expect("placed");
    assert_eq!(flowed.container, rect(0, 0, 40, 10));
    assert_eq!(flowed.items, vec![rect(3, 0, 20, 10), rect(30, 0, 10, 10)]);
}

#[test]
fn a_margin_down_a_column_moves_the_child_and_counts_in_its_outer_size() {
    let margined = Item {
        margin: Sides {
            top: 4.0,
            right: 0.0,
            bottom: 6.0,
            left: 0.0,
        },
        ..sized(10.0, 10.0)
    };
    let flex = Flex {
        direction: Direction::Column,
        ..Flex::default()
    };
    let flowed = flow(frame(400, 300), Written::default(), &flex, &[margined]).expect("placed");
    assert_eq!(flowed.container, rect(0, 0, 10, 20));
    assert_eq!(flowed.items, vec![rect(0, 4, 10, 10)]);
}
