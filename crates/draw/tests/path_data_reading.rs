//! Path data is read as the SVG grammar gives it (`docs/spec/shape.md`, section 2). The expected segments are worked by hand
//! from that section, not taken from the reader.

use draw::{Path, PathError, Point, Segment};

fn point(x: f64, y: f64) -> Point {
    Point { x, y }
}

fn close_enough(actual: Point, expected: Point) -> bool {
    (actual.x - expected.x).abs() < 1e-9 && (actual.y - expected.y).abs() < 1e-9
}

#[test]
fn absolute_and_relative_lines_follow_the_current_point() {
    // M 0 0; L 10 0 is absolute; l 0 10 is relative to (10, 0), so it ends at (10, 10); H 0 and V 20 are absolute.
    let path = Path::parse("M 0 0 L 10 0 l 0 10 H 0 V 20").unwrap();
    assert_eq!(
        path.segments,
        vec![
            Segment::Move(point(0.0, 0.0)),
            Segment::Line(point(10.0, 0.0)),
            Segment::Line(point(10.0, 10.0)),
            Segment::Line(point(0.0, 10.0)),
            Segment::Line(point(0.0, 20.0)),
        ]
    );
}

#[test]
fn a_repeated_pair_after_a_move_is_a_line() {
    // Further pairs after M are lines (section 2): 10 0 and 10 10 are two lines.
    let path = Path::parse("M0 0 10 0 10 10").unwrap();
    assert_eq!(
        path.segments,
        vec![
            Segment::Move(point(0.0, 0.0)),
            Segment::Line(point(10.0, 0.0)),
            Segment::Line(point(10.0, 10.0)),
        ]
    );
}

#[test]
fn separators_and_signs_split_numbers_where_the_point_or_the_sign_does() {
    // "-3.5.5" is -3.5 then .5: the second point starts a new number.
    let path = Path::parse("M1,2L-3.5.5").unwrap();
    assert_eq!(path.segments, vec![Segment::Move(point(1.0, 2.0)), Segment::Line(point(-3.5, 0.5))]);
}

#[test]
fn an_exponent_reads_as_a_power_of_ten() {
    // 1e1 is 10 and 2E-1 is 0.2.
    let path = Path::parse("M 0 0 L 1e1 2E-1").unwrap();
    assert_eq!(path.segments, vec![Segment::Move(point(0.0, 0.0)), Segment::Line(point(10.0, 0.2))]);
}

#[test]
fn a_close_returns_a_relative_move_to_the_start_of_the_closed_outline() {
    // After Z the current point is the start (5, 5), so m 1 1 is (6, 6) and L 0 0 is an absolute line.
    let path = Path::parse("M 5 5 L 10 5 Z m 1 1 L 0 0").unwrap();
    assert_eq!(
        path.segments,
        vec![
            Segment::Move(point(5.0, 5.0)),
            Segment::Line(point(10.0, 5.0)),
            Segment::Close,
            Segment::Move(point(6.0, 6.0)),
            Segment::Line(point(0.0, 0.0)),
        ]
    );
}

#[test]
fn a_smooth_cubic_reflects_the_previous_control_point() {
    // C ends at (10, 0) with its second control at (10, 10). S reflects that control about (10, 0), giving (10, -10).
    let path = Path::parse("M0 0 C 0 10 10 10 10 0 S 20 -10 20 0").unwrap();
    let Segment::Cubic(first, second, end) = path.segments[2] else {
        panic!("expected a cubic, got {:?}", path.segments[2]);
    };
    assert!(close_enough(first, point(10.0, -10.0)), "first control was {first:?}");
    assert!(close_enough(second, point(20.0, -10.0)));
    assert!(close_enough(end, point(20.0, 0.0)));
}

#[test]
fn a_quadratic_is_the_cubic_of_the_same_curve() {
    // Q 0 30 30 30 from (0, 0): the cubic controls are 2/3 of the way to the quadratic's control, (0, 20) and (10, 30).
    let path = Path::parse("M0 0 Q 0 30 30 30").unwrap();
    let Segment::Cubic(first, second, end) = path.segments[1] else {
        panic!("expected a cubic, got {:?}", path.segments[1]);
    };
    assert!(close_enough(first, point(0.0, 20.0)), "first control was {first:?}");
    assert!(close_enough(second, point(10.0, 30.0)), "second control was {second:?}");
    assert!(close_enough(end, point(30.0, 30.0)));
}

#[test]
fn a_half_circle_arc_is_cut_into_four_pieces_that_end_on_the_circle() {
    // A 10 10 0 0 1 from (0, 0) to (20, 0): a half circle of radius 10 about (10, 0), cut into four pieces of 45 degrees.
    let path = Path::parse("M 0 0 A 10 10 0 0 1 20 0").unwrap();
    let cubics: Vec<Point> = path.segments[1..]
        .iter()
        .map(|segment| match segment {
            Segment::Cubic(_, _, end) => *end,
            other => panic!("expected cubics, got {other:?}"),
        })
        .collect();
    assert_eq!(cubics.len(), 4);
    for end in &cubics {
        let distance = ((end.x - 10.0).powi(2) + end.y.powi(2)).sqrt();
        assert!((distance - 10.0).abs() < 1e-9, "{end:?} is {distance} from the centre");
    }
    assert!(close_enough(cubics[3], point(20.0, 0.0)));
}

#[test]
fn the_sweep_flag_takes_the_arc_the_positive_way_round() {
    // With sweep 1 the arc from (0, 0) to (20, 0) passes through (10, -10): its third piece ends at 3/2 pi, which is that point.
    let path = Path::parse("M 0 0 A 10 10 0 0 1 20 0").unwrap();
    let Segment::Cubic(_, _, halfway) = path.segments[2] else {
        panic!("expected a cubic");
    };
    assert!(close_enough(halfway, point(10.0, -10.0)), "halfway was {halfway:?}");
}

#[test]
fn a_zero_radius_arc_is_a_straight_line() {
    // Section 2: an arc with a radius of 0 is a straight line to its end.
    let path = Path::parse("M 0 0 A 0 5 0 0 1 4 3").unwrap();
    assert_eq!(path.segments, vec![Segment::Move(point(0.0, 0.0)), Segment::Line(point(4.0, 3.0))]);
}

#[test]
fn a_path_that_does_not_start_with_a_move_is_refused() {
    assert_eq!(Path::parse("L 0 0"), Err(PathError::NoMoveAtStart));
}

#[test]
fn an_unknown_command_letter_is_refused_with_the_letter() {
    assert_eq!(Path::parse("M 0 0 X 1 1"), Err(PathError::UnknownCommand('X')));
}

#[test]
fn a_number_after_a_close_has_no_command() {
    assert_eq!(Path::parse("M 0 0 Z 5"), Err(PathError::NumberWithoutCommand));
}

#[test]
fn a_command_with_a_number_missing_is_refused() {
    assert_eq!(Path::parse("M 0 0 L 1"), Err(PathError::MissingNumber));
}

#[test]
fn a_number_that_does_not_read_is_refused() {
    // A sign with no digits after it is not a number: "-" then a space.
    assert_eq!(Path::parse("M 0 0 L - 1"), Err(PathError::BadNumber));
}

#[test]
fn an_arc_flag_other_than_0_or_1_is_refused() {
    assert_eq!(Path::parse("M 0 0 A 10 10 0 2 1 20 0"), Err(PathError::BadFlag));
}
