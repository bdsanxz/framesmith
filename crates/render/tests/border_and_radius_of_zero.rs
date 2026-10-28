//! A border of width 0 draws nothing, and a `border-radius` of 0 is a square corner (`docs/spec/box-edges.md`, section 1),
//! held to pictures on the processor and on the card. The expected pixels are worked by hand from the box's edges: a box at
//! (8, 8) of 40 by 30 pixels, a border of 6 pixels is the 6 columns and rows inside its edge (section 5), and a corner of
//! radius 12 leaves its corner pixel outside the rounded shape, so the root's background shows there. The card's test needs
//! a graphics card that frx7 can draw with: reference machine only.

mod common;

use common::project_with;
use draw::{Picture, Renderer};
use draw_cpu::CpuRenderer;
use draw_gpu::CardRenderer;
use project::Project;
use render::Scene;

/// Frame 0 of a composition of 96 by 64 pixels on black holding one box with `attribute`, drawn by `renderer`.
fn drawn(name: &str, attribute: &str, renderer: &mut dyn Renderer) -> Picture {
    let text = format!(
        "<composition version=\"1\" width=\"96\" height=\"64\" fps=\"10\" duration=\"1\" background=\"#000000\">\n<box left=\"8\" top=\"8\" width=\"40\" height=\"30\" background=\"#ffffff\" {attribute}/>\n</composition>\n"
    );
    let folder = project_with("border-and-radius-of-zero", name, &text);
    let scene = Scene::from_project(&Project::open(&folder).unwrap()).unwrap_or_else(|reports| panic!("{reports:?}"));
    scene.picture(0, renderer).unwrap()
}

fn zero_width_border_draws_nothing(renderer: &mut dyn Renderer) {
    // The box is white to its edge; a border of width 0 adds no ring, so the corner and the edge keep the background.
    let picture = drawn("zero-width", "border=\"0 #ff0000\"", renderer);
    assert_eq!(picture.pixel(8, 8).unwrap(), [255, 255, 255, 255]);
    assert_eq!(picture.pixel(47, 37).unwrap(), [255, 255, 255, 255]);
    assert_eq!(picture.pixel(7, 8).unwrap(), [0, 0, 0, 255]);
}

fn zero_radius_is_a_square_corner(renderer: &mut dyn Renderer) {
    // A border of 6 on a square corner: the corner pixel (8, 8) is inside the border, and the 6 columns and rows inside the
    // edge are red (section 5); on the row 20, the pixel just past the left border, (14, 20), is the white of the box.
    let picture = drawn("square-corner", "border=\"6 #ff0000\" border-radius=\"0\"", renderer);
    assert_eq!(picture.pixel(8, 8).unwrap(), [255, 0, 0, 255]);
    assert_eq!(picture.pixel(13, 20).unwrap(), [255, 0, 0, 255]);
    assert_eq!(picture.pixel(14, 20).unwrap(), [255, 255, 255, 255]);
    assert_eq!(picture.pixel(47, 37).unwrap(), [255, 0, 0, 255]);
}

fn a_rounded_corner_leaves_its_corner_pixel_to_the_background(renderer: &mut dyn Renderer) {
    // The same border with a radius of 12: the corner pixel lies outside the quarter circle, so the root's black shows; the
    // middle of each side is still red.
    let picture = drawn("rounded-corner", "border=\"6 #ff0000\" border-radius=\"12\"", renderer);
    assert_eq!(picture.pixel(8, 8).unwrap(), [0, 0, 0, 255]);
    assert_eq!(picture.pixel(27, 8).unwrap(), [255, 0, 0, 255]);
    assert_eq!(picture.pixel(8, 23).unwrap(), [255, 0, 0, 255]);
}

#[test]
fn zero_width_border_and_zero_radius_are_drawn_as_the_spec_says_on_the_processor() {
    let mut renderer = CpuRenderer;
    zero_width_border_draws_nothing(&mut renderer);
    zero_radius_is_a_square_corner(&mut renderer);
    a_rounded_corner_leaves_its_corner_pixel_to_the_background(&mut renderer);
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_zero_width_border_and_zero_radius_are_drawn_as_the_spec_says_on_the_card() {
    let mut card = CardRenderer::new().unwrap_or_else(|error| panic!("the card could not be started: {error}"));
    zero_width_border_draws_nothing(&mut card);
    zero_radius_is_a_square_corner(&mut card);
    a_rounded_corner_leaves_its_corner_pixel_to_the_background(&mut card);
}
