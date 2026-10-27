//! The size of a row or a column that has none of its own, worked out from its children (`docs/spec/render-command.md`
//! section 4.3, rule 2), taken from the inside out, and the children laid out inside a rectangle whose size their
//! row or column measured (`layout::measure` and `layout::inside`).

use layout::{flow, frame, inside, measure, Across, Amount, Direction, Flex, Item, Measured, Rect, Sides, Size, Written};

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
fn a_row_is_as_large_as_its_children_gaps_and_padding() {
    let flex = Flex {
        gap: 2.0,
        padding: Sides::from_values(&[4.0]).unwrap(),
        ..Flex::default()
    };
    let size = measure(&flex, &[sized(10.0, 10.0), sized(10.0, 10.0)]).expect("measured");
    assert_eq!(size, Size { width: 30.0, height: 18.0 });
}

#[test]
fn a_column_is_as_tall_as_its_children_and_gaps_and_as_wide_as_the_widest() {
    let flex = Flex {
        direction: Direction::Column,
        gap: 1.0,
        ..Flex::default()
    };
    let size = measure(&flex, &[sized(20.0, 5.0), sized(10.0, 7.0)]).expect("measured");
    assert_eq!(size, Size { width: 20.0, height: 13.0 });
}

#[test]
fn a_child_that_stretches_across_counts_only_its_margins_in_the_size_across() {
    let stretched = |top: f64| Item {
        written: Written {
            width: Some(Amount::Pixels((10.0).into())),
            ..Written::default()
        },
        margin: Sides { top, ..Sides::default() },
        ..Item::default()
    };
    let row = Flex::default();
    // The sized child is 20 high; a stretched child with no margin adds nothing, and one with a margin of 25 counts 25.
    assert_eq!(measure(&row, &[sized(10.0, 20.0), stretched(0.0)]).expect("measured").height, 20.0);
    assert_eq!(measure(&row, &[sized(10.0, 20.0), stretched(25.0)]).expect("measured").height, 25.0);
}

#[test]
fn a_child_that_is_measured_first_is_counted_at_its_measured_size() {
    // The inner row is 10 and 2 and 10 wide, 22 by 10, because its boxes and its gap say so; the outer row
    // counts that 22, its gap and the box after it.
    let inner = measure(
        &Flex {
            gap: 2.0,
            ..Flex::default()
        },
        &[sized(10.0, 10.0), sized(10.0, 10.0)],
    )
    .expect("measured");
    assert_eq!(inner, Size { width: 22.0, height: 10.0 });
    let outer = Flex {
        gap: 4.0,
        ..Flex::default()
    };
    let items = [
        Item {
            written: Written {
                width: Some(Amount::Pixels((inner.width).into())),
                ..Written::default()
            },
            ..Item::default()
        },
        sized(10.0, 10.0),
    ];
    assert_eq!(measure(&outer, &items).expect("measured"), Size { width: 36.0, height: 10.0 });
    let flowed = flow(frame(400, 300), &Written::default(), &outer, &items).expect("placed");
    // The first child is 22 wide from the frame's edge at 0, the gap is 4, so the second lies at 26.
    assert_eq!(flowed.items[1], rect(26, 0, 10, 10));
}

#[test]
fn a_measured_row_inside_a_rectangle_places_its_children_in_it() {
    // The inner row is 22 by 10 and placed at 4, 4 with its width measured: its boxes lie at 4 and 16.
    let flex = Flex {
        gap: 2.0,
        ..Flex::default()
    };
    let measured = Measured {
        width: true,
        height: false,
    };
    let flowed = inside(rect(4, 4, 22, 10), measured, &flex, &[sized(10.0, 10.0), sized(10.0, 10.0)]).expect("placed");
    assert_eq!(flowed.container, rect(4, 4, 22, 10));
    assert_eq!(flowed.items, vec![rect(4, 4, 10, 10), rect(16, 4, 10, 10)]);
}

#[test]
fn a_percentage_of_a_child_is_0_on_an_axis_the_row_is_measured_on() {
    let half = Item {
        written: Written {
            width: Some(Amount::Percent((50.0).into())),
            height: Some(Amount::Pixels((10.0).into())),
            ..Written::default()
        },
        ..Item::default()
    };
    let measured_width = Measured {
        width: true,
        height: false,
    };
    let known_width = Measured::default();
    let on_measured = inside(rect(4, 4, 30, 10), measured_width, &Flex::default(), &[half.clone()]).expect("placed");
    assert_eq!(on_measured.items, vec![rect(4, 4, 0, 10)]);
    let on_known = inside(rect(4, 4, 30, 10), known_width, &Flex::default(), &[half]).expect("placed");
    assert_eq!(on_known.items, vec![rect(4, 4, 15, 10)]);
}

#[test]
fn a_row_with_no_height_is_as_high_as_its_tallest_child_and_its_stretched_child_fills_that_height() {
    // The second child writes no height, so it stretches to the 20 of the first and sits at 14 plus the gap.
    let second = Item {
        written: Written {
            width: Some(Amount::Pixels((10.0).into())),
            ..Written::default()
        },
        ..Item::default()
    };
    let flex = Flex {
        gap: 4.0,
        ..Flex::default()
    };
    let flowed = flow(frame(400, 300), &Written::default(), &flex, &[sized(10.0, 20.0), second]).expect("placed");
    assert_eq!(flowed.container, rect(0, 0, 24, 20));
    assert_eq!(flowed.items, vec![rect(0, 0, 10, 20), rect(14, 0, 10, 20)]);
}

#[test]
fn a_stretched_child_in_a_row_with_no_height_is_not_counted_in_the_height() {
    let stretched = Item {
        written: Written {
            width: Some(Amount::Pixels((10.0).into())),
            ..Written::default()
        },
        across: Some(Across::Stretch),
        ..Item::default()
    };
    let flowed = flow(frame(400, 300), &Written::default(), &Flex::default(), &[stretched]).expect("placed");
    assert_eq!(flowed.container, rect(0, 0, 10, 0));
    assert_eq!(flowed.items, vec![rect(0, 0, 10, 0)]);
}

/// A row with no size, padded by 2, whose only child is a row with no size across, padded by 3 and holding a
/// box of 10 by 10. The inner row is 16 by 16 (its items and padding), so the outer row is 20 by 20, and the
/// inner row stretches to fill the outer row's content box across: 16 by 16 at 2 by 2 inside the outer row.
#[test]
fn a_stretched_row_with_no_size_across_counts_the_size_its_items_give_it() {
    let outer = Flex {
        padding: Sides::from_values(&[2.0]).unwrap(),
        ..Flex::default()
    };
    let inner_flex = Flex {
        padding: Sides::from_values(&[3.0]).unwrap(),
        ..Flex::default()
    };
    let inner_size = measure(&inner_flex, &[sized(10.0, 10.0)]).expect("measured");
    assert_eq!(inner_size, Size { width: 16.0, height: 16.0 });
    let inner = || Item {
        written: Written {
            width: Some(Amount::Pixels((inner_size.width).into())),
            ..Written::default()
        },
        across: Some(Across::Stretch),
        measured_across: Some(inner_size.height),
        ..Item::default()
    };
    assert_eq!(measure(&outer, &[inner()]).expect("measured"), Size { width: 20.0, height: 20.0 });
    let flowed = flow(frame(400, 300), &Written::default(), &outer, &[inner()]).expect("placed");
    assert_eq!(flowed.container, rect(0, 0, 20, 20));
    assert_eq!(flowed.items, vec![rect(2, 2, 16, 16)]);
}

/// The column form of the case above: a column with no width holds a column with no width across, which
/// stretches to fill the outer column's content box across, 16 by 16 at 2 by 2 inside the outer column.
#[test]
fn a_stretched_column_with_no_size_across_counts_the_size_its_items_give_it() {
    let outer = Flex {
        direction: Direction::Column,
        padding: Sides::from_values(&[2.0]).unwrap(),
        ..Flex::default()
    };
    let inner_flex = Flex {
        direction: Direction::Column,
        padding: Sides::from_values(&[3.0]).unwrap(),
        ..Flex::default()
    };
    let inner_size = measure(&inner_flex, &[sized(10.0, 10.0)]).expect("measured");
    assert_eq!(inner_size, Size { width: 16.0, height: 16.0 });
    let inner = || Item {
        written: Written {
            height: Some(Amount::Pixels((inner_size.height).into())),
            ..Written::default()
        },
        across: Some(Across::Stretch),
        measured_across: Some(inner_size.width),
        ..Item::default()
    };
    assert_eq!(measure(&outer, &[inner()]).expect("measured"), Size { width: 20.0, height: 20.0 });
    let flowed = flow(frame(400, 300), &Written::default(), &outer, &[inner()]).expect("placed");
    assert_eq!(flowed.container, rect(0, 0, 20, 20));
    assert_eq!(flowed.items, vec![rect(2, 2, 16, 16)]);
}
