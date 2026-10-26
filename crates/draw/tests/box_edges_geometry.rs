//! The geometry of a box's border, corners and cut (`docs/spec/box-edges.md`, section 2).

use draw::{Line, Ring, Rounded};
use layout::Rect;

fn rect(left: i32, top: i32, width: u32, height: u32) -> Rect {
    Rect { left, top, width, height }
}

#[test]
fn the_radius_used_is_at_most_half_of_the_shorter_side() {
    assert_eq!(Rounded::of_box(rect(0, 0, 80, 40), 12.0).unwrap().radius, 12.0);
    assert_eq!(Rounded::of_box(rect(0, 0, 80, 40), 30.0).unwrap().radius, 20.0);
    assert_eq!(Rounded::of_box(rect(0, 0, 40, 40), 500.0).unwrap().radius, 20.0);
    assert_eq!(Rounded::of_box(rect(0, 0, 40, 40), -3.0).unwrap().radius, 0.0);
}

#[test]
fn a_box_with_no_size_has_no_edge() {
    assert_eq!(Rounded::of_box(rect(0, 0, 0, 40), 4.0), None);
    assert_eq!(Rounded::of_box(rect(0, 0, 40, 0), 4.0), None);
    assert_eq!(Rounded::of_box(rect(0, 0, 40, 40), f64::NAN), None);
}

#[test]
fn the_inner_edge_lies_the_border_width_inside_with_the_radius_less_the_width() {
    let outer = Rounded::of_box(rect(10, 20, 100, 60), 16.0).unwrap();
    let line = Line { width: 6.0, double: false };
    let inner = line.inner_edge(outer).unwrap();
    assert_eq!(
        (inner.left, inner.top, inner.right, inner.bottom, inner.radius),
        (16.0, 26.0, 104.0, 74.0, 10.0)
    );
    let wide = Line {
        width: 20.0,
        double: false,
    }
    .inner_edge(outer)
    .unwrap();
    assert_eq!(wide.radius, 0.0);
}

#[test]
fn a_border_of_one_line_is_one_ring_from_the_edge_to_the_inner_edge() {
    let outer = Rounded::of_box(rect(0, 0, 40, 30), 0.0).unwrap();
    let rings = Line { width: 6.0, double: false }.rings(outer);
    assert_eq!(
        rings,
        vec![Ring {
            outer,
            inner: outer.inset(6.0)
        }]
    );
}

#[test]
fn a_border_of_two_lines_is_two_rings_each_a_third_of_the_width() {
    let outer = Rounded::of_box(rect(0, 0, 60, 40), 0.0).unwrap();
    let rings = Line { width: 9.0, double: true }.rings(outer);
    assert_eq!(rings.len(), 2);
    assert_eq!(rings[0].outer, outer);
    assert_eq!(rings[0].inner, outer.inset(3.0));
    assert_eq!(rings[1].outer, outer.inset(6.0).unwrap());
    assert_eq!(rings[1].inner, outer.inset(9.0));
}

#[test]
fn a_border_wider_than_half_the_box_fills_it() {
    let outer = Rounded::of_box(rect(0, 0, 20, 10), 0.0).unwrap();
    let rings = Line {
        width: 50.0,
        double: false,
    }
    .rings(outer);
    assert_eq!(rings, vec![Ring { outer, inner: None }]);
    assert_eq!(
        Line {
            width: 50.0,
            double: false
        }
        .inner_edge(outer),
        None
    );
}

#[test]
fn a_border_of_width_zero_draws_nothing() {
    let outer = Rounded::of_box(rect(0, 0, 20, 10), 4.0).unwrap();
    assert!(Line { width: 0.0, double: false }.rings(outer).is_empty());
    assert_eq!(Line { width: 0.0, double: true }.inner_edge(outer), Some(outer));
}

#[test]
fn a_square_shape_on_whole_pixels_is_a_rectangle() {
    let outer = Rounded::of_box(rect(3, 4, 20, 10), 0.0).unwrap();
    assert_eq!(outer.whole_rect(), Some(rect(3, 4, 20, 10)));
    assert_eq!(Rounded::of_box(rect(3, 4, 20, 10), 2.0).unwrap().whole_rect(), None);
    assert_eq!(outer.inset(0.5).unwrap().whole_rect(), None);
}

#[test]
fn a_box_taller_than_wide_is_held_to_half_of_its_width() {
    // 30 wide, 110 high: half the width, 15, is the shorter half.
    assert_eq!(Rounded::of_box(rect(0, 0, 30, 110), 400.0).unwrap().radius, 15.0);
    assert_eq!(Rounded::of_box(rect(0, 0, 30, 110), 16.0).unwrap().radius, 15.0);
    assert_eq!(Rounded::of_box(rect(0, 0, 30, 110), 14.0).unwrap().radius, 14.0);
    // Its inner edge under a border of 4 is 4 inside, with the radius 15 less 4.
    let outer = Rounded::of_box(rect(0, 0, 30, 110), 400.0).unwrap();
    let inner = Line { width: 4.0, double: false }.inner_edge(outer).unwrap();
    assert_eq!((inner.left, inner.right, inner.radius), (4.0, 26.0, 11.0));
}

#[test]
fn a_border_of_two_lines_wider_than_half_the_box_is_held_to_half_and_keeps_its_gap() {
    // 20 by 10 with `50 double`: the width used is 5, a third of it 5/3; the gap runs from 5/3 to 10/3, the inner line from
    // 10/3 to the middle, which leaves no inside.
    let outer = Rounded::of_box(rect(0, 0, 20, 10), 0.0).unwrap();
    let rings = Line { width: 50.0, double: true }.rings(outer);
    assert_eq!(rings.len(), 2);
    assert_eq!(rings[0].outer, outer);
    assert_eq!(rings[0].inner, outer.inset(5.0 / 3.0));
    assert!(rings[0].inner.is_some(), "the outer line leaves room for the gap");
    assert_eq!(rings[1].outer, outer.inset(10.0 / 3.0).unwrap());
    assert_eq!(rings[1].inner, None);
}
