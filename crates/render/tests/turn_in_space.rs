//! An element turned in space (`docs/spec/render-command.md`, section 4.5, rule 10), measured in the frame on the processor
//! and on the card: the edges of a square turned about the upright and the level line, an edge-on and a mirrored square,
//! the order of the two turns, and a turn with no perspective. Each composition is 200 by 200 pixels on white, with a blue
//! square of 100 pixels whose middle, its origin, is at (100, 100). The card's tests need a graphics card that frx7 can
//! draw with: reference machine only.

mod common;

use common::project_with;
use draw::{Picture, Renderer};
use draw_cpu::CpuRenderer;
use draw_gpu::CardRenderer;
use project::Project;
use render::Scene;

/// The origin of the square, in the frame.
const ORIGIN: (f64, f64) = (100.0, 100.0);

/// Frame 0 of the composition holding the square with `attributes`, drawn by `renderer`.
fn drawn(name: &str, attributes: &str, renderer: &mut dyn Renderer) -> Picture {
    let text = format!(
        "<composition version=\"1\" width=\"200\" height=\"200\" fps=\"10\" duration=\"1\" background=\"#ffffff\">\n\
         <box left=\"50\" top=\"50\" width=\"100\" height=\"100\" background=\"#2050c0\" {attributes}/>\n</composition>\n"
    );
    let folder = project_with("turn-in-space", name, &text);
    let scene =
        Scene::from_project(&Project::open(&folder).unwrap(), &render::system_codecs()).unwrap_or_else(|reports| panic!("{reports:?}"));
    scene.picture(0, renderer).unwrap()
}

fn card() -> CardRenderer {
    CardRenderer::new().unwrap_or_else(|error| panic!("the card could not be started: {error}"))
}

/// The share of the pixel the square covers, from its red: 255 on white, 32 in the square.
fn covered(picture: &Picture, x: u32, y: u32) -> f64 {
    (255.0 - f64::from(picture.pixel(x, y).unwrap()[0])) / (255.0 - 32.0)
}

fn column_length(picture: &Picture, x: u32) -> f64 {
    (0..picture.height()).map(|y| covered(picture, x, y)).sum()
}

fn row_length(picture: &Picture, y: u32) -> f64 {
    (0..picture.width()).map(|x| covered(picture, x, y)).sum()
}

/// Where the turn draws the point `(u, v)` from the origin, turned by `first` then `second` (each an axis, `x` or `y`, and
/// degrees), with the eye `perspective` above the origin: rule 10's formula, turn by turn.
fn turned(u: f64, v: f64, turns: &[(char, f64)], perspective: f64) -> (f64, f64) {
    let (mut x, mut y, mut z) = (u, v, 0.0_f64);
    for &(axis, degrees) in turns {
        let (sin, cos) = degrees.to_radians().sin_cos();
        // About the level line a positive angle takes the top (y below 0) away; about the upright line, the right.
        (x, y, z) = match axis {
            'x' => (x, y * cos - z * sin, y * sin + z * cos),
            _ => (x * cos + z * sin, y, -x * sin + z * cos),
        };
    }
    let scale = perspective / (perspective - z);
    (ORIGIN.0 + x * scale, ORIGIN.1 + y * scale)
}

fn a_square_turned_about_the_upright_line_has_its_right_edge_shorter(renderer: &mut dyn Renderer) {
    let picture = drawn("rotate-y-60", r#"perspective="400" rotate-y="60""#, renderer);
    let (left_x, left_top) = turned(-50.0, -50.0, &[('y', 60.0)], 400.0);
    let (right_x, right_top) = turned(50.0, -50.0, &[('y', 60.0)], 400.0);
    let (left, right) = (2.0 * (ORIGIN.1 - left_top), 2.0 * (ORIGIN.1 - right_top));
    // 112.14 and 90.23 pixels, 50.59 apart across: narrower than the square, its right edge 21.91 shorter.
    assert!((left - 112.14).abs() < 0.01 && (right - 90.23).abs() < 0.01 && right_x - left_x < 100.0);
    let width = row_length(&picture, 100);
    assert!(
        (width - (right_x - left_x)).abs() <= 1.0,
        "width {width} against {}",
        right_x - left_x
    );
    // The length of each column across the square is the edges' lengths taken along a straight line, within a pixel.
    for x in (left_x.ceil() as u32 + 1)..(right_x.floor() as u32 - 1) {
        let at = f64::from(x) + 0.5;
        let wanted = left + (right - left) * (at - left_x) / (right_x - left_x);
        let found = column_length(&picture, x);
        assert!((found - wanted).abs() <= 1.0, "column {x}: {found} against {wanted}");
    }
}

fn a_positive_turn_about_the_level_line_makes_the_top_edge_shorter(renderer: &mut dyn Renderer) {
    let picture = drawn("rotate-x-45", r#"perspective="400" rotate-x="45""#, renderer);
    let (top_left, top_y) = turned(-50.0, -50.0, &[('x', 45.0)], 400.0);
    let (bottom_left, bottom_y) = turned(-50.0, 50.0, &[('x', 45.0)], 400.0);
    let (top, bottom) = (2.0 * (ORIGIN.0 - top_left), 2.0 * (ORIGIN.0 - bottom_left));
    // 91.88 and 109.70 pixels.
    assert!(top < bottom && (top - 91.88).abs() < 0.01 && (bottom - 109.70).abs() < 0.01);
    for y in (top_y.ceil() as u32 + 1)..(bottom_y.floor() as u32 - 1) {
        let at = f64::from(y) + 0.5;
        let wanted = top + (bottom - top) * (at - top_y) / (bottom_y - top_y);
        let found = row_length(&picture, y);
        assert!((found - wanted).abs() <= 1.0, "row {y}: {found} against {wanted}");
    }
    assert!(row_length(&picture, top_y.ceil() as u32 + 1) < row_length(&picture, bottom_y.floor() as u32 - 2));
}

fn at_90_degrees_nothing_is_drawn_and_past_it_the_square_is_mirrored(renderer: &mut dyn Renderer, within: u8) {
    let blank = drawn("blank", r#"opacity="0""#, renderer);
    for (name, attributes) in [
        ("rotate-y-90", r#"perspective="400" rotate-y="90""#),
        ("rotate-x-90", r#"perspective="400" rotate-x="90""#),
        ("rotate-y-270", r#"perspective="400" rotate-y="-90""#),
    ] {
        assert_eq!(
            drawn(name, attributes, renderer).pixels(),
            blank.pixels(),
            "{name}: nothing is drawn edge-on"
        );
    }
    // Past 90 degrees the square is drawn mirrored: at 135 it is the picture at 45 mirrored about the origin's upright line.
    let front = drawn("rotate-y-45", r#"perspective="400" rotate-y="45""#, renderer);
    let back = drawn("rotate-y-135", r#"perspective="400" rotate-y="135""#, renderer);
    for y in 0..200 {
        for x in 0..200 {
            let (found, mirrored) = (back.pixel(x, y).unwrap(), front.pixel(199 - x, y).unwrap());
            assert!(
                found.iter().zip(mirrored).all(|(a, b)| a.abs_diff(b) <= within),
                "({x}, {y}): {found:?} against {mirrored:?}"
            );
        }
    }
    assert!(
        column_length(&back, 120) > column_length(&back, 80),
        "the edge that was on the left, nearer now, is on the right"
    );
}

/// Whether `point` is inside the quadrilateral `corners` (in order around it) by more than `margin` pixels.
fn inside(corners: [(f64, f64); 4], point: (f64, f64), margin: f64) -> bool {
    (0..4).all(|at| {
        let (a, b) = (corners[at], corners[(at + 1) % 4]);
        let cross = (b.0 - a.0) * (point.1 - a.1) - (b.1 - a.1) * (point.0 - a.0);
        cross / (b.0 - a.0).hypot(b.1 - a.1) > margin
    })
}

fn with_both_turns_the_level_line_comes_first(renderer: &mut dyn Renderer) {
    let picture = drawn("both", r#"perspective="300" rotate-x="50" rotate-y="40""#, renderer);
    let quad =
        |turns: &[(char, f64)]| [(-50.0, -50.0), (50.0, -50.0), (50.0, 50.0), (-50.0, 50.0)].map(|(u, v)| turned(u, v, turns, 300.0));
    let level_first = quad(&[('x', 50.0), ('y', 40.0)]);
    let upright_first = quad(&[('y', 40.0), ('x', 50.0)]);
    let (mut only_level_first, mut only_upright_first) = (0, 0);
    for y in 0..200 {
        for x in 0..200 {
            let centre = (f64::from(x) + 0.5, f64::from(y) + 0.5);
            let share = covered(&picture, x, y);
            let (level, upright) = (inside(level_first, centre, 1.5), inside(upright_first, centre, 1.5));
            let (outside_level, outside_upright) = (inside(level_first, centre, -1.5), inside(upright_first, centre, -1.5));
            if level && !outside_upright {
                only_level_first += 1;
                assert!(
                    share > 0.99,
                    "({x}, {y}) is drawn as the turn about the level line first gives: {share}"
                );
            }
            if upright && !outside_level {
                only_upright_first += 1;
                assert!(share < 0.01, "({x}, {y}) is not drawn: {share}");
            }
        }
    }
    assert!(
        only_level_first > 100 && only_upright_first > 100,
        "{only_level_first} and {only_upright_first} pixels tell the orders apart"
    );
}

fn with_no_perspective_the_turns_change_nothing(renderer: &mut dyn Renderer) {
    let plain = drawn("plain", "", renderer);
    for (name, attributes) in [
        ("no-perspective", r#"rotate-x="30" rotate-y="60""#),
        ("no-perspective-mirrored", r#"rotate-y="135""#),
    ] {
        assert_eq!(drawn(name, attributes, renderer).pixels(), plain.pixels(), "{name}");
    }
    assert_eq!(drawn("no-angles", r#"perspective="400""#, renderer).pixels(), plain.pixels());
}

#[test]
fn a_square_turned_about_the_upright_line_has_its_right_edge_shorter_on_the_processor() {
    a_square_turned_about_the_upright_line_has_its_right_edge_shorter(&mut CpuRenderer);
}

#[test]
fn a_positive_turn_about_the_level_line_makes_the_top_edge_shorter_on_the_processor() {
    a_positive_turn_about_the_level_line_makes_the_top_edge_shorter(&mut CpuRenderer);
}

#[test]
fn at_90_degrees_nothing_is_drawn_and_past_it_the_square_is_mirrored_on_the_processor() {
    at_90_degrees_nothing_is_drawn_and_past_it_the_square_is_mirrored(&mut CpuRenderer, 1);
}

#[test]
fn with_both_turns_the_level_line_comes_first_on_the_processor() {
    with_both_turns_the_level_line_comes_first(&mut CpuRenderer);
}

#[test]
fn with_no_perspective_the_turns_change_nothing_on_the_processor() {
    with_no_perspective_the_turns_change_nothing(&mut CpuRenderer);
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_a_square_turned_about_the_upright_line_has_its_right_edge_shorter_on_the_card() {
    a_square_turned_about_the_upright_line_has_its_right_edge_shorter(&mut card());
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_a_positive_turn_about_the_level_line_makes_the_top_edge_shorter_on_the_card() {
    a_positive_turn_about_the_level_line_makes_the_top_edge_shorter(&mut card());
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_at_90_degrees_nothing_is_drawn_and_past_it_the_square_is_mirrored_on_the_card() {
    at_90_degrees_nothing_is_drawn_and_past_it_the_square_is_mirrored(&mut card(), 4);
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_with_both_turns_the_level_line_comes_first_on_the_card() {
    with_both_turns_the_level_line_comes_first(&mut card());
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_with_no_perspective_the_turns_change_nothing_on_the_card() {
    with_no_perspective_the_turns_change_nothing(&mut card());
}
