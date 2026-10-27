//! A `<shape>` whose path is missing, whose view box is not four numbers, or whose dashes are not pairs of lengths is refused,
//! each with its own kind and the value it refuses (`docs/spec/shape.md`, section 6). The cases are worked from that
//! section: what is written in the attribute decides the outcome, not the reader's own output.

use draw::{dashes_of, required_path, PathError, ShapeError, ViewBox};

#[test]
fn a_shape_with_no_path_is_refused_as_having_no_d() {
    assert_eq!(required_path(None), Err(ShapeError::NoPath));
}

#[test]
fn a_shape_with_an_empty_or_blank_path_is_refused_as_having_no_d() {
    assert_eq!(required_path(Some("")), Err(ShapeError::NoPath));
    assert_eq!(required_path(Some("   ")), Err(ShapeError::NoPath));
}

#[test]
fn a_path_that_is_not_path_data_is_refused_with_its_reason() {
    assert_eq!(required_path(Some("L 0 0")), Err(ShapeError::BadPath(PathError::NoMoveAtStart)));
    assert_eq!(
        required_path(Some("M 0 0 X 1 1")),
        Err(ShapeError::BadPath(PathError::UnknownCommand('X')))
    );
}

#[test]
fn a_path_that_is_path_data_is_accepted() {
    assert!(required_path(Some("M 0 0 H 24 V 24 H 0 Z")).is_ok());
}

#[test]
fn a_view_box_of_three_numbers_is_refused_with_its_value() {
    assert_eq!(
        ViewBox::parse("0 0 24"),
        Err(ShapeError::ViewBox {
            value: "0 0 24".to_string()
        })
    );
}

#[test]
fn a_view_box_with_a_width_or_height_of_zero_or_less_is_refused_with_its_value() {
    for value in ["0 0 0 24", "0 0 24 0", "0 0 -4 24", "0 0 24 -1"] {
        assert_eq!(
            ViewBox::parse(value),
            Err(ShapeError::ViewBox { value: value.to_string() }),
            "{value} is refused"
        );
    }
}

#[test]
fn a_view_box_with_a_number_that_does_not_read_is_refused_with_its_value() {
    assert_eq!(
        ViewBox::parse("0 0 twenty 24"),
        Err(ShapeError::ViewBox {
            value: "0 0 twenty 24".to_string()
        })
    );
}

#[test]
fn a_view_box_of_four_numbers_is_read_with_its_origin_and_size() {
    assert_eq!(
        ViewBox::parse("-2, 4 24 12.5"),
        Ok(ViewBox {
            x: -2.0,
            y: 4.0,
            width: 24.0,
            height: 12.5
        })
    );
}

#[test]
fn dashes_of_three_lengths_are_refused_with_their_value() {
    assert_eq!(
        dashes_of("10 5 3"),
        Err(ShapeError::Dashes {
            value: "10 5 3".to_string()
        })
    );
}

#[test]
fn dashes_with_a_length_below_zero_are_refused_with_their_value() {
    assert_eq!(
        dashes_of("10 -5"),
        Err(ShapeError::Dashes {
            value: "10 -5".to_string()
        })
    );
}

#[test]
fn dashes_that_are_all_zero_or_absent_are_refused_with_their_value() {
    assert_eq!(dashes_of("0 0"), Err(ShapeError::Dashes { value: "0 0".to_string() }));
    assert_eq!(dashes_of(""), Err(ShapeError::Dashes { value: String::new() }));
}

#[test]
fn dashes_in_pairs_of_lengths_are_accepted_as_written() {
    assert_eq!(dashes_of("10 5"), Ok(vec![10.0, 5.0]));
    assert_eq!(dashes_of("10,5 3 4"), Ok(vec![10.0, 5.0, 3.0, 4.0]));
}

#[test]
fn each_refusal_reads_as_the_report_of_the_specification() {
    assert_eq!(ShapeError::NoPath.to_string(), "<shape> needs `d` or a `src` with paths");
    assert_eq!(
        ShapeError::ViewBox {
            value: "0 0 24".to_string()
        }
        .to_string(),
        "`view-box=\"0 0 24\"` is not a view box of four numbers"
    );
    assert_eq!(
        ShapeError::Dashes {
            value: "10 5 3".to_string()
        }
        .to_string(),
        "`dash=\"10 5 3\"` is not pairs of lengths: a dash and a gap, in view-box units"
    );
    assert_eq!(
        ShapeError::BadPath(PathError::MissingNumber).to_string(),
        "`d` is not path data: a number is missing"
    );
}
