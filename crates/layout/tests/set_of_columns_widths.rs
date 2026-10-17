//! The columns of a set of columns: a column of a stated width and a share of the rest, the padding of a
//! written size, and the widths a set takes when it has no size of its own (`docs/spec/render-command.md`
//! section 4.4, rules 1, 2 and 8, and section 4.3, rule 8 with decision B17).

use layout::{columns, measure_columns, Across, Amount, Cell, Columns, Rect, Sides, Size, Track, Written};

fn tracks(list: Vec<Track>) -> Columns {
    Columns {
        tracks: list,
        gap: 0.0,
        padding: Sides::default(),
        across: Across::Stretch,
    }
}

fn frame_of(width: u32, height: u32) -> Rect {
    layout::frame(width, height)
}

fn cell(height: f64) -> Cell {
    Cell {
        written: Written {
            height: Some(Amount::Pixels(height)),
            ..Written::default()
        },
        ..Cell::default()
    }
}

fn rect(left: i32, top: i32, width: u32, height: u32) -> Rect {
    Rect { left, top, width, height }
}

#[test]
fn four_cells_lie_two_to_a_row_with_a_stated_column_and_a_share_of_the_rest() {
    let set = tracks(vec![Track::Pixels(640.0), Track::Share(1.0)]);
    let cells = [cell(30.0), cell(30.0), cell(30.0), cell(30.0)];
    let width_1000 = Written {
        width: Some(Amount::Pixels(1000.0)),
        ..Written::default()
    };
    let flowed = columns(frame_of(1000, 300), width_1000, &set, &cells).expect("placed");
    assert_eq!(
        flowed.items,
        vec![
            rect(0, 0, 640, 30),
            rect(640, 0, 360, 30),
            rect(0, 30, 640, 30),
            rect(640, 30, 360, 30),
        ]
    );
}

#[test]
fn the_padding_of_a_set_lies_inside_its_written_width() {
    // Written 1000 wide with padding 8 on each side: the content box is 984 wide, so the share is 984 less 640.
    let set = Columns {
        padding: Sides {
            top: 8.0,
            right: 8.0,
            bottom: 8.0,
            left: 8.0,
        },
        ..tracks(vec![Track::Pixels(640.0), Track::Share(1.0)])
    };
    let mut first = cell(30.0);
    first.baseline = Some(12.0);
    let cells = [first, cell(30.0), cell(30.0), cell(30.0)];
    let width_1000 = Written {
        width: Some(Amount::Pixels(1000.0)),
        ..Written::default()
    };
    let flowed = columns(frame_of(1000, 300), width_1000, &set, &cells).expect("placed");
    // Two rows of 30 with the padding of 8 above and below.
    assert_eq!(flowed.container, rect(0, 0, 1000, 76));
    assert_eq!(&flowed.items[..2], &[rect(8, 8, 640, 30), rect(648, 8, 344, 30)]);
    assert_eq!(flowed.baseline, Some(20.0));
}

#[test]
fn a_set_with_no_width_of_its_own_is_as_wide_as_its_widest_cells_per_column() {
    // Columns of 100 px, a share of 1 and a share of 2. Cells 1 and 4 share the second column; the widest is 70 px
    // with its margins 5 each side, that is 80. Cell 2 alone makes the third column 30 px.
    let set = tracks(vec![Track::Pixels(100.0), Track::Share(1.0), Track::Share(2.0)]);
    let sized = |width: f64| Cell {
        measured: Some(Size { width, height: 10.0 }),
        ..Cell::default()
    };
    let mut wide = sized(70.0);
    wide.margin = Sides {
        top: 0.0,
        right: 5.0,
        bottom: 0.0,
        left: 5.0,
    };
    let cells = [sized(50.0), wide, sized(30.0), sized(40.0), sized(20.0)];
    let size = measure_columns(&set, &cells).expect("measured");
    assert_eq!(
        size,
        Size {
            width: 100.0 + 80.0 + 30.0,
            height: 20.0
        }
    );
}
