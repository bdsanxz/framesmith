//! Aligning the children of a row by their baselines (`docs/spec/render-command.md` section 4.3, rule 7).

use layout::{flow, frame, Across, Amount, Flex, Item, Rect, Written};

fn item(width: f64, height: f64, baseline: f64) -> Item {
    Item {
        written: Written {
            width: Some(Amount::Pixels(width)),
            height: Some(Amount::Pixels(height)),
            ..Written::default()
        },
        across: Some(Across::Baseline),
        baseline: Some(baseline),
        ..Item::default()
    }
}

fn rect(left: i32, top: i32, width: u32, height: u32) -> Rect {
    Rect { left, top, width, height }
}

fn baseline_row() -> Flex {
    Flex {
        across: Across::Baseline,
        ..Flex::default()
    }
}

#[test]
fn the_first_lines_of_two_items_of_different_heights_lie_on_one_line() {
    // Item A has its baseline 30 down, item B 15 down: the highest baseline is A's, which touches the top.
    let items = [item(20.0, 40.0, 30.0), item(20.0, 20.0, 15.0)];
    let height_100 = Written {
        height: Some(Amount::Pixels(100.0)),
        ..Written::default()
    };
    let flowed = flow(frame(400, 300), height_100, &baseline_row(), &items).expect("placed");
    assert_eq!(flowed.items, vec![rect(0, 0, 20, 40), rect(20, 15, 20, 20)]);
    assert_eq!(flowed.items[0].top + 30, flowed.items[1].top + 15);
}

#[test]
fn a_row_with_no_height_is_as_high_as_its_baselines_need() {
    // Above the line: 30 and 15. Below it: 10 (40 less 30) and 5 (20 less 15). The block is 30 + 10 = 40.
    let items = [item(20.0, 40.0, 30.0), item(20.0, 20.0, 15.0)];
    let flowed = flow(frame(400, 300), Written::default(), &baseline_row(), &items).expect("placed");
    assert_eq!(flowed.container, rect(0, 0, 40, 40));
    assert_eq!(flowed.items, vec![rect(0, 0, 20, 40), rect(20, 15, 20, 20)]);
    assert_eq!(flowed.baseline, Some(30.0));
}

#[test]
fn in_a_column_a_baseline_is_placed_at_the_start_edge() {
    let column = Flex {
        direction: layout::Direction::Column,
        across: Across::Baseline,
        ..Flex::default()
    };
    let items = [item(20.0, 40.0, 30.0), item(20.0, 20.0, 15.0)];
    let size_100 = Written {
        width: Some(Amount::Pixels(100.0)),
        height: Some(Amount::Pixels(100.0)),
        ..Written::default()
    };
    let flowed = flow(frame(400, 300), size_100, &column, &items).expect("placed");
    assert_eq!(flowed.items, vec![rect(0, 0, 20, 40), rect(0, 40, 20, 20)]);
}
