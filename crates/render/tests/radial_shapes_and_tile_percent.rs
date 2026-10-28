//! Radial gradients other than a circle in pixels, and background positions in percent (`docs/spec/render-command.md`,
//! section 4.6, rules 2 and 6; `docs/spec/notation.md`, section 7.4). Every expected value is worked here from the spec's
//! formulas, never from the renderer: for a radial gradient with the stops `#ffffff` at 0 % and `#000000` at 100 %, the
//! share of the way from the centre to the gradient's edge at a pixel centre `(du, dv)` from the centre is
//! `f = √((du / rx)² + (dv / ry)²)`, and the colour is `⌊255 (1 − f) + 0.5⌋` on each channel, held at 0 past the edge.
//! The radii are: `circle` with no radius, the distance from the centre to the farthest corner; `ellipse` with no radii,
//! `√2` times the distances to the farther side across and down; `ellipse A% B%`, `A%` and `B%` of the tile's width and
//! height. The card's tests need a graphics card that frx7 can draw with: reference machine only.

mod common;

use common::project_with;
use draw::{Picture, Renderer};
use draw_cpu::CpuRenderer;
use draw_gpu::CardRenderer;
use project::Project;
use render::Scene;

/// Frame 0 of a composition of 96 by 54 pixels on grey holding `inside`, drawn by `renderer`.
fn drawn(name: &str, inside: &str, renderer: &mut dyn Renderer) -> Picture {
    let text = format!(
        "<composition version=\"1\" width=\"96\" height=\"54\" fps=\"10\" duration=\"1\" background=\"#808080\">\n{inside}\n</composition>\n"
    );
    let folder = project_with("radial-shapes-and-tile-percent", name, &text);
    let scene = Scene::from_project(&Project::open(&folder).unwrap()).unwrap_or_else(|reports| panic!("{reports:?}"));
    scene.picture(0, renderer).unwrap()
}

/// The level of `⌊255 v + 0.5⌋`, for a value from 0 to 1.
fn level(value: f64) -> u8 {
    (value * 255.0 + 0.5).floor() as u8
}

/// The share of the way from the centre to the edge of the gradient at the pixel `(x, y)`, with the centre `(cx, cy)` and
/// the radii `rx` and `ry`, from the spec's formula.
fn share(x: u32, y: u32, (cx, cy): (f64, f64), (rx, ry): (f64, f64)) -> f64 {
    let (du, dv) = (f64::from(x) + 0.5 - cx, f64::from(y) + 0.5 - cy);
    ((du / rx).powi(2) + (dv / ry).powi(2)).sqrt()
}

/// The colour a black and white radial gradient gives at the pixel: white at the centre, black at the edge and past it.
fn expected(x: u32, y: u32, centre: (f64, f64), radii: (f64, f64)) -> u8 {
    level((1.0 - share(x, y, centre, radii)).clamp(0.0, 1.0))
}

/// The box is 80 by 40 pixels at the frame's corner, so its middle is (40, 20) and its farthest corner is 40 across and 20
/// down from that middle.
const WIDTH: u32 = 80;
const HEIGHT: u32 = 40;
const MIDDLE: (f64, f64) = (40.0, 20.0);

fn box_with(background: &str) -> String {
    format!(r##"<box left="0" top="0" width="{WIDTH}" height="{HEIGHT}" background="{background}"/>"##)
}

/// Every pixel of the box equals the worked value within `within` levels.
fn every_pixel_is(picture: &Picture, radii: (f64, f64), within: u8, what: &str) {
    let mut checked = 0;
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let wanted = expected(x, y, MIDDLE, radii);
            let found = picture.pixel(x, y).unwrap();
            assert!(
                found[..3].iter().all(|&value| value.abs_diff(wanted) <= within),
                "{what} at ({x}, {y}): {found:?} against {wanted}"
            );
            checked += 1;
        }
    }
    assert_eq!(checked, (WIDTH * HEIGHT) as usize);
}

fn circle_without_radius_reaches_the_farthest_corner(renderer: &mut dyn Renderer, within: u8) {
    let picture = drawn("circle", &box_with("radial-gradient(circle, #ffffff 0%, #000000 100%)"), renderer);
    // The radius is the distance from the middle to the farthest corner: hypot(40, 20).
    let radius = 40.0_f64.hypot(20.0);
    every_pixel_is(&picture, (radius, radius), within, "circle");
    // The first stop at the middle, and the last at the farthest corner: the pixel just up and left of the middle is at
    // share 0.0158, so it is nearly white.
    assert!(picture.pixel(39, 19).unwrap()[0] >= 250);
}

fn ellipse_without_radii_passes_through_the_farthest_corner(renderer: &mut dyn Renderer, within: u8) {
    let picture = drawn("ellipse", &box_with("radial-gradient(ellipse, #ffffff 0%, #000000 100%)"), renderer);
    // √2 times the distances to the farther sides: 40 across and 20 down, so 40√2 and 20√2.
    let radii = (40.0 * std::f64::consts::SQRT_2, 20.0 * std::f64::consts::SQRT_2);
    every_pixel_is(&picture, radii, within, "ellipse");
    // The ellipse passes through the farthest corner: that corner is exactly at the last stop, share 1.
    let corner = ((0.0 - MIDDLE.0) / radii.0).powi(2) + ((0.0 - MIDDLE.1) / radii.1).powi(2);
    assert!((corner.sqrt() - 1.0).abs() < 1e-12);
}

fn ellipse_with_radii_in_percent_is_a_share_of_the_box(renderer: &mut dyn Renderer, within: u8) {
    let picture = drawn(
        "ellipse-percent",
        &box_with("radial-gradient(ellipse 50% 25%, #ffffff 0%, #000000 100%)"),
        renderer,
    );
    // 50% of 80 across and 25% of 40 down: 40 and 10, with the middle at (40, 20) when no `at` is written.
    every_pixel_is(&picture, (40.0, 10.0), within, "ellipse 50% 25%");
    // Past the edge of the ellipse, the last stop holds: the pixel (40, 30), 0.5 across and 10.5 down, is past it.
    assert!(picture.pixel(40, 30).unwrap()[..3].iter().all(|&value| value <= within));
}

fn background_position_in_percent_places_the_tile_on_the_room_left(renderer: &mut dyn Renderer, within: u8) {
    // A red tile of 20 by 20 on a white layer, the tile placed at 0 %, 50 % and 100 % across and down. Its room is the
    // box's width less the tile's: 96 − 20 = 76 across and 54 − 20 = 34 down, so 50 % is 38 and 100 % is 76 (and 34 down).
    let tile = |position: &str, renderer: &mut dyn Renderer| {
        drawn(
            "tile",
            &format!(
                r##"<box left="0" top="0" width="96" height="54" background="#ff0000, #ffffff" background-size="20 20, auto" background-position="{position}, 0 0" background-repeat="no-repeat, repeat"/>"##
            ),
            renderer,
        )
    };
    let red = [255, 0, 0, 255];
    let white = [255, 255, 255, 255];
    let near = |found: [u8; 4], wanted: [u8; 4]| found.iter().zip(wanted).all(|(f, w)| f.abs_diff(w) <= within);

    let start = tile("0% 0%", renderer);
    assert!(near(start.pixel(0, 0).unwrap(), red));
    assert!(near(start.pixel(19, 19).unwrap(), red));
    assert!(near(start.pixel(20, 0).unwrap(), white));

    let middle = tile("50% 50%", renderer);
    assert!(near(middle.pixel(37, 19).unwrap(), white), "left of 38");
    assert!(near(middle.pixel(38, 19).unwrap(), red), "from 38 across");
    assert!(near(middle.pixel(57, 19).unwrap(), red), "to 57 across");
    assert!(near(middle.pixel(58, 19).unwrap(), white), "past 57 across");
    assert!(near(middle.pixel(48, 16).unwrap(), white), "above 17 down");
    assert!(near(middle.pixel(48, 17).unwrap(), red), "from 17 down");

    let end = tile("100% 100%", renderer);
    assert!(near(end.pixel(75, 53).unwrap(), white), "left of 76 across");
    assert!(near(end.pixel(76, 34).unwrap(), red), "from 76 across and 34 down");
    assert!(near(end.pixel(95, 53).unwrap(), red), "to the far corner");
    assert!(near(end.pixel(76, 33).unwrap(), white), "above 34 down");
}

#[test]
fn radial_shapes_and_tile_positions_in_percent_are_drawn_as_the_spec_says_on_the_processor() {
    let mut renderer = CpuRenderer;
    circle_without_radius_reaches_the_farthest_corner(&mut renderer, 0);
    ellipse_without_radii_passes_through_the_farthest_corner(&mut renderer, 0);
    ellipse_with_radii_in_percent_is_a_share_of_the_box(&mut renderer, 0);
    background_position_in_percent_places_the_tile_on_the_room_left(&mut renderer, 0);
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_radial_shapes_and_tile_positions_in_percent_are_drawn_as_the_spec_says_on_the_card() {
    let mut card = CardRenderer::new().unwrap_or_else(|error| panic!("the card could not be started: {error}"));
    // The card agrees with the processor within 4 levels of 255 (`PLAN.md`, section 5.2).
    circle_without_radius_reaches_the_farthest_corner(&mut card, 4);
    ellipse_without_radii_passes_through_the_farthest_corner(&mut card, 4);
    ellipse_with_radii_in_percent_is_a_share_of_the_box(&mut card, 4);
    background_position_in_percent_places_the_tile_on_the_room_left(&mut card, 4);
}
