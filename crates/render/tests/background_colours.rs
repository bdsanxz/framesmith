//! The colours of backgrounds at worked places, on the processor and on the card (`docs/spec/render-command.md`, section
//! 4.6): layers drawn the first listed on top, the direction of a linear gradient, a hard edge, a fade to a clear colour,
//! a circle's centre and radius, and tiles. Each composition is 96 by 54 pixels on grey; the card's tests need a graphics
//! card that frx7 can draw with: reference machine only.

mod common;

use common::project_with;
use draw::{Picture, Renderer};
use draw_cpu::CpuRenderer;
use draw_gpu::CardRenderer;
use project::Project;
use render::Scene;

/// Frame 0 of a composition of 96 by 54 pixels holding `inside`, drawn by `renderer`.
fn drawn(name: &str, inside: &str, renderer: &mut dyn Renderer) -> Picture {
    let text = format!(
        "<composition version=\"1\" width=\"96\" height=\"54\" fps=\"10\" duration=\"1\" background=\"#808080\">\n{inside}\n</composition>\n"
    );
    let folder = project_with("background-colours", name, &text);
    let scene = Scene::from_project(&Project::open(&folder).unwrap()).unwrap_or_else(|reports| panic!("{reports:?}"));
    scene.picture(0, renderer).unwrap()
}

fn card() -> CardRenderer {
    CardRenderer::new().unwrap_or_else(|error| panic!("the card could not be started: {error}"))
}

/// Whether `found` is within `within` levels of `wanted` in every value.
fn near(found: [u8; 4], wanted: [u8; 4], within: u8) -> bool {
    found.iter().zip(wanted).all(|(found, wanted)| found.abs_diff(wanted) <= within)
}

fn at(picture: &Picture, x: u32, y: u32) -> [u8; 4] {
    picture.pixel(x, y).unwrap()
}

/// `⌊255 v + 0.5⌋`, the level of a value from 0 to 1.
fn level(value: f64) -> u8 {
    (value * 255.0 + 0.5).floor() as u8
}

fn the_first_layer_listed_is_drawn_over_the_second(renderer: &mut dyn Renderer, within: u8) {
    let picture = drawn(
        "layers",
        r##"<box left="0" top="0" width="40" height="54" background="#00ff00, #ff0000"/>
<box left="48" top="0" width="48" height="54" background="#ff000080, #0000ff"/>"##,
        renderer,
    );
    // An opaque first layer hides the second.
    assert!(near(at(&picture, 20, 27), [0, 255, 0, 255], within), "{:?}", at(&picture, 20, 27));
    // A half clear red over blue: red at 128 of 255, and the blue it lets through, 255 × 127 / 255.
    assert!(
        near(at(&picture, 70, 27), [128, 0, 127, 255], within.max(1)),
        "{:?}",
        at(&picture, 70, 27)
    );
}

fn a_linear_gradient_runs_the_way_its_angle_names(renderer: &mut dyn Renderer, within: u8) {
    let mut gradient = |angle: &str| {
        drawn(
            &format!("linear-{}", if angle.is_empty() { "none" } else { angle }),
            &format!(r##"<box left="0" top="0" width="96" height="54" background="linear-gradient({angle}#000000 0%, #ffffff 100%)"/>"##),
            renderer,
        )
    };
    // 90deg: from black at the left to white at the right, a pixel taking the place of its centre, (x + 0.5) / 96.
    let right = gradient("90deg, ");
    for x in [0, 10, 47, 48, 80, 95] {
        let value = level((f64::from(x) + 0.5) / 96.0);
        assert!(
            near(at(&right, x, 27), [value, value, value, 255], within),
            "90deg at {x}: {:?}",
            at(&right, x, 27)
        );
    }
    // 0deg: upward, from black at the bottom to white at the top, (54 − y − 0.5) / 54.
    let up = gradient("0deg, ");
    // No angle: downward, from black at the top, (y + 0.5) / 54.
    let down = gradient("");
    for y in [0, 9, 26, 27, 40, 53] {
        let upward = level((54.0 - f64::from(y) - 0.5) / 54.0);
        let downward = level((f64::from(y) + 0.5) / 54.0);
        assert!(
            near(at(&up, 48, y), [upward, upward, upward, 255], within),
            "0deg at {y}: {:?}",
            at(&up, 48, y)
        );
        assert!(
            near(at(&down, 48, y), [downward, downward, downward, 255], within),
            "none at {y}: {:?}",
            at(&down, 48, y)
        );
    }
}

fn two_stops_at_one_place_give_a_hard_edge(renderer: &mut dyn Renderer, within: u8) {
    let picture = drawn(
        "hard-edge",
        r##"<box left="0" top="0" width="96" height="54" background="linear-gradient(90deg, #ff0000 50%, #0000ff 50%)"/>"##,
        renderer,
    );
    for x in 0..48 {
        assert!(
            near(at(&picture, x, 27), [255, 0, 0, 255], within),
            "red at {x}: {:?}",
            at(&picture, x, 27)
        );
    }
    for x in 48..96 {
        assert!(
            near(at(&picture, x, 27), [0, 0, 255, 255], within),
            "blue at {x}: {:?}",
            at(&picture, x, 27)
        );
    }
}

fn a_fade_to_a_clear_colour_does_not_darken(renderer: &mut dyn Renderer, within: u8) {
    let picture = drawn(
        "fade",
        r##"<box left="0" top="0" width="96" height="54" background="linear-gradient(90deg, #ff0000 0%, #00000000 100%), #ffffff"/>"##,
        renderer,
    );
    // Red at alpha 1 − (x + 0.5) / 96 over white: the red stays full, the white shows through the rest, and no value
    // falls below what a straight fade from red to white gives.
    for x in 0..96 {
        let shown = level((f64::from(x) + 0.5) / 96.0);
        let found = at(&picture, x, 27);
        assert!(
            near(found, [255, shown, shown, 255], within.max(1)),
            "at {x}: {found:?}, against {shown}"
        );
    }
}

fn a_circle_has_its_first_colour_at_the_centre_and_its_last_at_the_radius(renderer: &mut dyn Renderer, within: u8) {
    let picture = drawn(
        "circle",
        r##"<box left="0" top="0" width="96" height="54" background="radial-gradient(circle 20 at 50% 50%, #ffff00 0%, #0000ff 100%)"/>"##,
        renderer,
    );
    // The pixel at (47, 26) has its centre 0.71 pixels from the circle's centre (48, 27): 0.035 of the way.
    let centre = at(&picture, 47, 26);
    let share = 0.5_f64.hypot(0.5) / 20.0;
    let wanted = [level(1.0 - share), level(1.0 - share), level(share), 255];
    assert!(near(centre, wanted, within.max(1)), "centre {centre:?}, against {wanted:?}");
    // At the radius and beyond, the last stop: (68, 27) is 20.5 pixels from the centre, (48, 47) 20.5 below it.
    for (x, y) in [(68, 27), (48, 47), (27, 27), (95, 0)] {
        assert!(
            near(at(&picture, x, y), [0, 0, 255, 255], within),
            "at ({x}, {y}): {:?}",
            at(&picture, x, y)
        );
    }
    // Just inside, 19.5 pixels from the centre, not yet the last.
    assert!(near(
        at(&picture, 67, 27),
        [level(0.025), level(0.025), level(0.975), 255],
        within.max(1)
    ));
}

fn a_sized_layer_tiles_once_or_from_its_position(renderer: &mut dyn Renderer, within: u8) {
    let tiles = |name: &str, settings: &str, renderer: &mut dyn Renderer| {
        drawn(
            name,
            &format!(
                r##"<box left="0" top="0" width="96" height="54" background="linear-gradient(90deg, #ff0000 0%, #0000ff 100%), #ffffff" background-size="20 20, auto" {settings}/>"##
            ),
            renderer,
        )
    };
    let repeated = tiles("tiles", "", renderer);
    let once = tiles("tile-once", r#"background-repeat="no-repeat, repeat""#, renderer);
    let moved = tiles("tiles-moved", r#"background-position="10 5, 0 0""#, renderer);
    for (x, y) in [(0, 0), (3, 7), (12, 19), (19, 2)] {
        let tile = at(&repeated, x, y);
        // The tile repeats every 20 pixels across and down.
        for (across, down) in [(20, 0), (40, 20), (60, 0), (0, 20)] {
            assert!(
                near(at(&repeated, x + across, y + down), tile, within),
                "repeat at ({x}, {y}) + ({across}, {down})"
            );
        }
        // Drawn once, it is where the first tile is, and the white layer under it shows everywhere else.
        assert!(near(at(&once, x, y), tile, within), "once at ({x}, {y})");
        assert!(
            near(at(&once, x + 20, y), [255, 255, 255, 255], within),
            "white beside the one tile at ({x}, {y})"
        );
        assert!(
            near(at(&once, x, y + 20), [255, 255, 255, 255], within),
            "white below the one tile at ({x}, {y})"
        );
        // Its position moves every tile 10 pixels right and 5 down.
        assert!(near(at(&moved, x + 10, y + 5), tile, within), "moved at ({x}, {y})");
    }
    assert!(
        !near(at(&repeated, 2, 2), at(&repeated, 12, 2), within),
        "the tile is not one colour"
    );
}

#[test]
fn the_first_layer_listed_is_drawn_over_the_second_on_the_processor() {
    the_first_layer_listed_is_drawn_over_the_second(&mut CpuRenderer, 0);
}

#[test]
fn a_linear_gradient_runs_the_way_its_angle_names_on_the_processor() {
    a_linear_gradient_runs_the_way_its_angle_names(&mut CpuRenderer, 0);
}

#[test]
fn two_stops_at_one_place_give_a_hard_edge_on_the_processor() {
    two_stops_at_one_place_give_a_hard_edge(&mut CpuRenderer, 0);
}

#[test]
fn a_fade_to_a_clear_colour_does_not_darken_on_the_processor() {
    a_fade_to_a_clear_colour_does_not_darken(&mut CpuRenderer, 0);
}

#[test]
fn a_circle_has_its_first_colour_at_the_centre_and_its_last_at_the_radius_on_the_processor() {
    a_circle_has_its_first_colour_at_the_centre_and_its_last_at_the_radius(&mut CpuRenderer, 0);
}

#[test]
fn a_sized_layer_tiles_once_or_from_its_position_on_the_processor() {
    a_sized_layer_tiles_once_or_from_its_position(&mut CpuRenderer, 0);
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_the_first_layer_listed_is_drawn_over_the_second_on_the_card() {
    the_first_layer_listed_is_drawn_over_the_second(&mut card(), 4);
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_a_linear_gradient_runs_the_way_its_angle_names_on_the_card() {
    a_linear_gradient_runs_the_way_its_angle_names(&mut card(), 4);
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_two_stops_at_one_place_give_a_hard_edge_on_the_card() {
    two_stops_at_one_place_give_a_hard_edge(&mut card(), 4);
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_a_fade_to_a_clear_colour_does_not_darken_on_the_card() {
    a_fade_to_a_clear_colour_does_not_darken(&mut card(), 4);
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_a_circle_has_its_first_colour_at_the_centre_and_its_last_at_the_radius_on_the_card() {
    a_circle_has_its_first_colour_at_the_centre_and_its_last_at_the_radius(&mut card(), 4);
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_a_sized_layer_tiles_once_or_from_its_position_on_the_card() {
    a_sized_layer_tiles_once_or_from_its_position(&mut card(), 4);
}
