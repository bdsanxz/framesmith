//! A `<shape>` takes the box look of its attribute table, each drawn as on a box (`docs/spec/shape.md`,
//! sections 9 and 10; ticket T062 criterion 11): a `background` under the shape, a `border` at the box's
//! edges, rounded corners that cut the box, and the `drop-shadow` under the shape's outlines.

mod common;

use common::project_with;
use draw_cpu::CpuRenderer;
use project::Project;
use render::Scene;

const BACKGROUND: [u8; 4] = [16, 32, 64, 255];
const INK: [u8; 4] = [224, 160, 48, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];
const WHITE: [u8; 4] = [255, 255, 255, 255];
const BLACK: [u8; 4] = [0, 0, 0, 255];

/// The frame of a 64 by 64 composition whose `<shape>` fills a 48 by 48 box at (8, 8) with a 12 by 12
/// view-box-unit square at its centre, over a green `background`, with a white border of 6, rounded
/// corners of radius 6, and a black `drop-shadow` moved by (4, 4) view-box units with no blur.
fn frame(name: &str) -> draw::Picture {
    let folder = project_with(
        "shape-box-look",
        name,
        "<composition version=\"1\" width=\"64\" height=\"64\" fps=\"10\" duration=\"1\" background=\"#102040\">\n<shape left=\"8\" top=\"8\" width=\"48\" height=\"48\" view-box=\"0 0 24 24\" d=\"M 6 6 H 18 V 18 H 6 Z\" color=\"#E0A030\" background=\"#00FF00\" border=\"6 #FFFFFF\" border-radius=\"6\" drop-shadow=\"4 4 0 #000000\"/>\n</composition>\n",
    );
    let project = Project::open(&folder).unwrap();
    let scene = Scene::from_project(&project).unwrap_or_else(|reports| panic!("{reports:?}"));
    scene.picture(0, &mut CpuRenderer).unwrap()
}

#[test]
fn the_background_of_a_shape_shows_where_its_path_does_not_cover() {
    let picture = frame("background");
    // Inside the box and past the border, outside the path and past the shadow: the green background.
    assert_eq!(picture.pixel(40, 16).unwrap(), GREEN);
    // Outside the box: the frame's background.
    assert_eq!(picture.pixel(2, 2).unwrap(), BACKGROUND);
}

#[test]
fn the_shape_is_drawn_over_its_own_background() {
    let picture = frame("shape");
    // Inside the path: the orange shape, though the shadow and the background lie under it.
    assert_eq!(picture.pixel(32, 32).unwrap(), INK);
}

#[test]
fn the_border_of_a_shape_rings_its_box_as_on_a_box() {
    let picture = frame("border");
    // Two pixels inside the left edge, clear of the path and of the shadow: the white border.
    assert_eq!(picture.pixel(10, 40).unwrap(), WHITE);
}

#[test]
fn the_rounded_corners_of_a_shape_cut_its_box_as_on_a_box() {
    let picture = frame("corners");
    // The corner pixel lies 7.78 pixels from the arc's centre of radius 6, wholly outside the
    // rounded box, so the frame's background shows where a square corner would be white.
    assert_eq!(picture.pixel(8, 8).unwrap(), BACKGROUND);
}

#[test]
fn the_shadow_of_a_shape_falls_under_its_outlines_in_view_box_units() {
    let picture = frame("shadow");
    // The shadow is the shape's coverage moved by (4, 4) view-box units, two pixels a unit: inside
    // the shadow's 24 by 24 pixel square, past the path and the border, black over green.
    assert_eq!(picture.pixel(48, 48).unwrap(), BLACK);
}
