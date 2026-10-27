//! The rows of a set of columns and the place of each cell in its row and column (`docs/spec/render-command.md`
//! section 4.4, rules 3, 4 and 7).

use layout::{columns, Across, Amount, Cell, Columns, Rect, Sides, Size, Track, Written};

fn tracks(list: Vec<Track>) -> Columns {
    Columns {
        tracks: list,
        gap: 0.0,
        padding: Sides::default(),
        across: Across::Stretch,
    }
}

fn rect(left: i32, top: i32, width: u32, height: u32) -> Rect {
    Rect { left, top, width, height }
}

#[test]
fn leftover_height_is_shared_equally_among_the_rows() {
    // A set 100 high with two rows of 20 each: 60 is left over, so each row is 50 high. A cell with no height
    // of its own stretches to its row's height.
    let set = tracks(vec![Track::Pixels(50.0)]);
    let cell = Cell {
        measured: Some(Size { width: 0.0, height: 20.0 }),
        ..Cell::default()
    };
    let height_100 = Written {
        height: Some(Amount::Pixels((100.0).into())),
        ..Written::default()
    };
    let flowed = columns(layout::frame(300, 300), &height_100, &set, &[cell.clone(), cell]).expect("placed");
    assert_eq!(flowed.items, vec![rect(0, 0, 50, 50), rect(0, 50, 50, 50)]);
}

#[test]
fn a_cell_with_a_written_width_stands_at_the_start_of_its_column() {
    // Two columns of 100 and one row 40 high. The second cell writes a width of 30 and a height of 10, and is
    // aligned at the end of the row: it stands at the start of the second column and at the bottom of the row.
    let set = Columns {
        across: Across::End,
        ..tracks(vec![Track::Pixels(100.0), Track::Pixels(100.0)])
    };
    let tall = Cell {
        measured: Some(Size { width: 0.0, height: 40.0 }),
        ..Cell::default()
    };
    let small = Cell {
        written: Written {
            width: Some(Amount::Pixels((30.0).into())),
            height: Some(Amount::Pixels((10.0).into())),
            ..Written::default()
        },
        ..Cell::default()
    };
    let flowed = columns(layout::frame(300, 300), &Written::default(), &set, &[tall, small]).expect("placed");
    assert_eq!(flowed.items, vec![rect(0, 0, 100, 40), rect(100, 30, 30, 10)]);
}
