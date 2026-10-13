//! An outline drawn in part, and in dashes: where the cut falls, and what the frame shows.
//! See `docs/spec/native-notation.md`, section 12.

use poc::cpu::Cpu;
use poc::model::Composition;
use poc::outline::{dashed, drawn, lengths};
use poc::scene::Evaluator;
use poc::text::Shaper;
use poc::{layout, xml, Backend};
use std::path::Path;
use vello_cpu::kurbo::{BezPath, PathEl, Point};

fn path(d: &str) -> BezPath {
    BezPath::from_svg(d).expect("the path is valid")
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-3
}

/// Where a path ends.
fn end(path: &BezPath) -> Point {
    match path.elements().last() {
        Some(PathEl::MoveTo(to) | PathEl::LineTo(to) | PathEl::QuadTo(_, to) | PathEl::CurveTo(_, _, to)) => *to,
        _ => panic!("the path ends at a point"),
    }
}

/// Where along x each outline of a path starts, from the left.
fn starts(path: &BezPath) -> Vec<f64> {
    let moves = path.elements().iter().filter_map(|element| match element {
        PathEl::MoveTo(to) => Some(to.x),
        _ => None,
    });
    let mut starts: Vec<f64> = moves.collect();
    starts.sort_by(f64::total_cmp);
    starts
}

#[test]
fn an_outline_is_drawn_from_its_start_to_its_share_of_its_length() {
    let corner = path("M 0 0 L 10 0 L 10 10");
    assert_eq!(lengths(&corner), [20.0]);
    let half = drawn(&corner, 0.5);
    assert!(close(lengths(&half)[0], 10.0));
    assert!(
        close(end(&half).x, 10.0) && close(end(&half).y, 0.0),
        "half way is the corner: {:?}",
        end(&half)
    );
    let most = drawn(&corner, 0.75);
    assert!(close(end(&most).x, 10.0) && close(end(&most).y, 5.0), "{:?}", end(&most));
}

#[test]
fn a_whole_share_is_the_outline_itself_and_none_is_nothing() {
    let corner = path("M 0 0 L 10 0 L 10 10");
    assert_eq!(drawn(&corner, 1.0).elements(), corner.elements());
    assert_eq!(drawn(&corner, 1.5).elements(), corner.elements());
    assert!(drawn(&corner, 0.0).elements().is_empty());
    assert!(drawn(&corner, -0.5).elements().is_empty());
}

#[test]
fn each_outline_of_a_path_is_drawn_to_the_same_share_of_its_own_length() {
    let two = path("M 0 0 L 20 0 M 0 10 L 4 10");
    assert_eq!(lengths(&two), [20.0, 4.0]);
    let part = lengths(&drawn(&two, 0.25));
    assert!(close(part[0], 5.0) && close(part[1], 1.0), "{part:?}");
}

#[test]
fn a_closed_outline_is_drawn_round_to_its_start() {
    let square = path("M 0 0 L 10 0 L 10 10 L 0 10 Z");
    assert_eq!(lengths(&square), [40.0]);
    let nearly = drawn(&square, 0.9);
    assert!(close(lengths(&nearly)[0], 36.0));
    assert!(
        close(end(&nearly).x, 0.0) && close(end(&nearly).y, 4.0),
        "on the closing side: {:?}",
        end(&nearly)
    );
}

#[test]
fn a_curve_is_cut_by_length_along_it() {
    let curve = path("M 0 0 C 0 40, 60 40, 60 0");
    let whole = lengths(&curve)[0];
    assert!(whole > 60.0);
    for share in [0.1, 0.5, 0.8] {
        let part = lengths(&drawn(&curve, share))[0];
        assert!(close(part, whole * share), "{share} of {whole} is {part}");
    }
    // The curve is the same on both sides, so half its length ends in the middle.
    assert!(close(end(&drawn(&curve, 0.5)).x, 30.0));
}

#[test]
fn dashes_are_measured_along_each_outline_from_its_start() {
    let line = path("M 0 0 L 20 0");
    let row = dashed(&line, &[4.0, 6.0]);
    assert_eq!(lengths(&row), [4.0, 4.0]);
    assert_eq!(starts(&row), [0.0, 10.0], "a dash at 0 and one at 10");
    assert_eq!(starts(&dashed(&path("M 0 0 L 25 0"), &[4.0, 6.0])), [0.0, 10.0, 20.0]);
    // The second outline begins with a whole dash, wherever the first one ended.
    let two = path("M 0 0 L 7 0 M 0 10 L 7 10");
    assert_eq!(lengths(&dashed(&two, &[4.0, 6.0])), [4.0, 4.0]);
}

fn load(source: &str) -> Composition {
    let mut composition = xml::parse(source, Path::new(".")).expect("the scene is valid");
    layout::lay_out(&mut composition, &mut Shaper::new(), &[]);
    composition
}

/// Whether the line is there at `x` of each frame asked for: a 64 by 16 frame, a white line
/// from x = 8 to x = 56 at y = 8.
fn lit(shape: &str, frame: u64, xs: &[usize]) -> Vec<bool> {
    let composition = load(&format!(
        r##"<composition width="64" height="16" fps="10" duration="2" background="#000000">
  <shape left="0" top="0" width="64" height="16" view-box="0 0 64 16" d="M 8 8 L 56 8" color="#ffffff" stroke-width="2" {shape}
</composition>"##
    ));
    let mut cpu = Cpu::new(&composition, &[]);
    cpu.draw(frame, &Evaluator::new(&composition).frame(frame).ops)
        .expect("the frame is drawn");
    let pixels = cpu.pixels().expect("the frame is read");
    xs.iter().map(|x| pixels[(8 * 64 + x) * 4] > 200).collect()
}

const DRAWS: &str = r#"><tween at="0" duration="1" ease="linear" from-draw="0" to-draw="1"/></shape>"#;

#[test]
fn a_line_that_draws_itself_grows_from_its_start() {
    assert_eq!(
        lit(DRAWS, 0, &[9, 20, 32, 44, 54]),
        [false; 5],
        "nothing in the first frame, not even a dot"
    );
    assert_eq!(lit(DRAWS, 3, &[9, 20, 32, 44, 54]), [true, true, false, false, false]);
    assert_eq!(lit(DRAWS, 6, &[9, 20, 32, 44, 54]), [true, true, true, false, false]);
    assert_eq!(lit(DRAWS, 10, &[9, 20, 32, 44, 54]), [true; 5]);
    assert_eq!(lit(DRAWS, 19, &[9, 20, 32, 44, 54]), [true; 5]);
}

#[test]
fn the_attribute_alone_draws_that_share() {
    assert_eq!(lit(r#"draw="0.5"/>"#, 0, &[12, 28, 36, 52]), [true, true, false, false]);
    assert_eq!(lit("/>", 0, &[12, 28, 36, 52]), [true; 4], "without it the line is whole");
}

#[test]
fn a_dashed_line_shows_its_dashes_and_not_its_gaps() {
    // Dashes of 8 at 8, 24 and 40; gaps of 8 between them.
    assert_eq!(
        lit(r#"dash="8 8"/>"#, 0, &[12, 20, 28, 36, 44, 52]),
        [true, false, true, false, true, false]
    );
}

#[test]
fn a_dashed_line_drawn_in_part_keeps_its_dashes_where_they_were() {
    // Half the line ends at 32: the dash from 24 is whole, the one from 40 is not there.
    assert_eq!(
        lit(r#"dash="8 8" draw="0.5"/>"#, 0, &[12, 20, 28, 36, 44]),
        [true, false, true, false, false]
    );
}

#[test]
fn what_is_not_a_line_or_not_dashes_is_refused_with_its_line() {
    let source = r##"<composition width="64" height="16" fps="10" duration="1">
  <shape width="16" height="16" d="M 0 0 L 8 8" draw="0.5"/>
  <rect width="16" height="16" dash="4 4"/>
  <rect width="16" height="16">
    <tween at="0" duration="1" to-draw="1"/>
  </rect>
  <shape width="16" height="16" d="M 0 0 L 8 8" stroke-width="2" dash="4"/>
  <shape width="16" height="16" d="M 0 0 L 8 8" stroke-width="2" dash="4 wide"/>
  <shape width="16" height="16" d="M 0 0 L 8 8" stroke-width="2" dash="0 0"/>
</composition>"##;
    let problems = xml::parse(source, Path::new(".")).err().expect("the file has problems");
    let found: Vec<(u32, &str)> = problems.iter().map(|p| (p.line, p.message.as_str())).collect();
    assert_eq!(
        found,
        [
            (2, "`draw` is for a <shape> with `stroke-width`"),
            (3, "`dash` is for a <shape> with `stroke-width`"),
            (4, "`draw` is for a <shape> with `stroke-width`"),
            (7, "`dash=\"4\"` is not pairs of lengths: a dash and a gap, in view-box units"),
            (8, "`dash=\"4 wide\"` is not pairs of lengths: a dash and a gap, in view-box units"),
            (9, "`dash=\"0 0\"` is not pairs of lengths: a dash and a gap, in view-box units"),
        ]
    );
}
