//! Blur and brightness of an element with its children (`docs/spec/render-command.md`, section 4.5, rule 11), measured in
//! the frame on the processor and on the card: the values across a blurred edge, colours halved and stopped at white, the
//! order of the two, an element faded as one picture, and an element at opacity 0. The expected values are worked here
//! from a Gaussian of our own (`common/gaussian.rs`, by numerical integration of its density); none comes from the renderer. The card's tests
//! need a graphics card that frx7 can draw with: reference machine only.

mod common;

use common::gaussian::share;
use common::project_with;
use draw::{Picture, Renderer};
use draw_cpu::CpuRenderer;
use draw_gpu::CardRenderer;
use project::Project;
use render::Scene;

/// Frame 0 of a composition of 200 by 100 pixels on `background` holding `inside`, drawn by `renderer`.
fn drawn(name: &str, background: &str, inside: &str, renderer: &mut dyn Renderer) -> Picture {
    let text = format!(
        "<composition version=\"1\" width=\"200\" height=\"100\" fps=\"10\" duration=\"1\" background=\"{background}\">\n{inside}\n</composition>\n"
    );
    let folder = project_with("blur-and-brightness", name, &text);
    let scene = Scene::from_project(&Project::open(&folder).unwrap()).unwrap_or_else(|reports| panic!("{reports:?}"));
    scene.picture(0, renderer).unwrap()
}

fn card() -> CardRenderer {
    CardRenderer::new().unwrap_or_else(|error| panic!("the card could not be started: {error}"))
}

fn near(found: u8, wanted: f64, within: f64) -> bool {
    (f64::from(found) - wanted).abs() <= within
}

fn a_blurred_hard_edge_has_the_values_of_a_gaussian(renderer: &mut dyn Renderer, within: f64) {
    // A white box from (50, 20) to (150, 80) on black, blurred with a standard deviation of 4 pixels: each value is 255
    // times the share of the Gaussian over the box, across and down.
    let picture = drawn(
        "edge",
        "#000000",
        r##"<box left="50" top="20" width="100" height="60" background="#ffffff" blur="4"/>"##,
        renderer,
    );
    let mut softened = 0;
    for y in 0..100 {
        for x in 0..200 {
            let (cx, cy) = (f64::from(x) + 0.5, f64::from(y) + 0.5);
            let wanted = 255.0 * share(50.0, 150.0, cx, 4.0) * share(20.0, 80.0, cy, 4.0);
            let found = picture.pixel(x, y).unwrap();
            assert!(
                found[..3].iter().all(|&value| near(value, wanted, within)),
                "({x}, {y}): {found:?} against {wanted:.3}"
            );
            softened += usize::from(wanted > 5.0 && wanted < 250.0);
        }
    }
    assert!(softened > 2000, "the edge is spread over {softened} pixels");
    // Across the left edge on the middle row: 255 times the share right of the edge.
    let row: Vec<u8> = (40..60).map(|x| picture.pixel(x, 50).unwrap()[0]).collect();
    assert!(row.windows(2).all(|pair| pair[0] <= pair[1]), "{row:?}");
}

fn brightness_multiplies_each_colour_and_stops_at_white(renderer: &mut dyn Renderer) {
    let picture = drawn(
        "brightness",
        "#000000",
        r##"<box left="0" top="0" width="50" height="100" background="#c86432" brightness="0.5"/>
<box left="50" top="0" width="50" height="100" background="#808080" brightness="3"/>
<box left="100" top="0" width="50" height="100" background="#3c6480" brightness="3"/>
<box left="150" top="0" width="50" height="50" background="#c86432" brightness="0"/>
<box left="150" top="50" width="50" height="50" background="#80808080" brightness="3"/>"##,
        renderer,
    );
    // 200, 100, 50 halved; 128 three times is past white; 60, 100, 128 three times are 180, 300 and 384; 0 makes it black.
    // A grey of 128 at half strength (alpha 128) three times is past white too: the colour stops at white and the alpha
    // stays, so it is white at half strength over black, 128.
    assert_eq!(picture.pixel(25, 50).unwrap(), [100, 50, 25, 255]);
    assert_eq!(picture.pixel(75, 50).unwrap(), [255, 255, 255, 255]);
    assert_eq!(picture.pixel(125, 50).unwrap(), [180, 255, 255, 255]);
    assert_eq!(picture.pixel(175, 25).unwrap(), [0, 0, 0, 255]);
    assert_eq!(picture.pixel(175, 75).unwrap(), [128, 128, 128, 255]);
}

fn blur_comes_before_brightness(renderer: &mut dyn Renderer, within: f64) {
    // Two greys side by side, 200 left of x = 100 and 40 right of it, in an element blurred by 3 and brightened by 2: the
    // blur mixes 200 and 40 first, and the mix is doubled and stopped at white. Brightened first, 200 would stop at 255
    // before the blur, and the seam would be lower.
    let picture = drawn(
        "order",
        "#000000",
        r##"<box left="0" top="0" width="200" height="100" blur="3" brightness="2">
<box left="0" top="0" width="100" height="100" background="#c8c8c8"/>
<box left="100" top="0" width="100" height="100" background="#282828"/>
</box>"##,
        renderer,
    );
    let mut apart = 0;
    for x in 85..115 {
        let c = f64::from(x) + 0.5;
        let (left, right) = (share(-1e9, 100.0, c, 3.0), share(100.0, 200.0, c, 3.0));
        let (vertical, alpha) = (
            share(0.0, 100.0, 50.5, 3.0),
            share(-1e9, 200.0, c, 3.0) * share(0.0, 100.0, 50.5, 3.0),
        );
        let blurred_first = (2.0 * (200.0 * left + 40.0 * right) * vertical).min(255.0 * alpha);
        let brightened_first = (255.0 * left + 80.0 * right) * vertical;
        let found = picture.pixel(x, 50).unwrap()[0];
        assert!(near(found, blurred_first, within), "x {x}: {found} against {blurred_first:.3}");
        apart += usize::from((blurred_first - brightened_first).abs() > 4.0);
    }
    assert!(apart >= 6, "the two orders differ at {apart} columns of the seam");
}

fn an_element_fades_as_one_picture(renderer: &mut dyn Renderer, within: u8) {
    // A red box and a blue one over it, overlapping from x = 80 to 120, in an element at opacity 0.5 on white.
    let picture = drawn(
        "fade",
        "#ffffff",
        r##"<box left="0" top="0" width="200" height="100" opacity="0.5">
<box left="20" top="20" width="100" height="60" background="#ff0000"/>
<box left="80" top="20" width="100" height="60" background="#0000ff"/>
</box>"##,
        renderer,
    );
    // Half of 255 is 127.5, rounded to 128; white shows through by 255 - 128 = 127.
    let close = |found: [u8; 4], wanted: [u8; 4]| found.iter().zip(wanted).all(|(a, b)| a.abs_diff(b) <= within);
    let (red, blue, overlap) = (
        picture.pixel(50, 50).unwrap(),
        picture.pixel(150, 50).unwrap(),
        picture.pixel(100, 50).unwrap(),
    );
    assert!(close(red, [255, 127, 127, 255]), "red alone: {red:?}");
    assert!(close(blue, [127, 127, 255, 255]), "blue alone: {blue:?}");
    assert_eq!(overlap, blue, "the overlap is as strong as blue alone");
    assert_eq!(picture.pixel(10, 10).unwrap(), [255, 255, 255, 255]);
}

fn an_element_at_opacity_0_draws_nothing(renderer: &mut dyn Renderer) {
    let blank = drawn("blank", "#ffffff", "", renderer);
    for (name, attributes) in [
        ("opacity-0", r#"opacity="0""#),
        ("opacity-0-blurred", r#"opacity="0" blur="6" brightness="2""#),
    ] {
        let picture = drawn(
            name,
            "#ffffff",
            &format!(
                r##"<box left="10" top="10" width="180" height="80" background="#204080" {attributes}>
<box left="20" top="20" width="60" height="40" background="#ff0000"/>
<text left="100" top="20" font-size="20" color="#000000">x</text>
</box>"##
            ),
            renderer,
        );
        assert_eq!(picture.pixels(), blank.pixels(), "{name}");
    }
}

#[test]
fn a_blurred_hard_edge_has_the_values_of_a_gaussian_on_the_processor() {
    a_blurred_hard_edge_has_the_values_of_a_gaussian(&mut CpuRenderer, 1.0);
}

#[test]
fn brightness_multiplies_each_colour_and_stops_at_white_on_the_processor() {
    brightness_multiplies_each_colour_and_stops_at_white(&mut CpuRenderer);
}

#[test]
fn blur_comes_before_brightness_on_the_processor() {
    blur_comes_before_brightness(&mut CpuRenderer, 1.0);
}

#[test]
fn an_element_fades_as_one_picture_on_the_processor() {
    an_element_fades_as_one_picture(&mut CpuRenderer, 0);
}

#[test]
fn an_element_at_opacity_0_draws_nothing_on_the_processor() {
    an_element_at_opacity_0_draws_nothing(&mut CpuRenderer);
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_a_blurred_hard_edge_has_the_values_of_a_gaussian_on_the_card() {
    a_blurred_hard_edge_has_the_values_of_a_gaussian(&mut card(), 1.0);
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_brightness_multiplies_each_colour_and_stops_at_white_on_the_card() {
    brightness_multiplies_each_colour_and_stops_at_white(&mut card());
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_blur_comes_before_brightness_on_the_card() {
    blur_comes_before_brightness(&mut card(), 1.0);
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_an_element_fades_as_one_picture_on_the_card() {
    an_element_fades_as_one_picture(&mut card(), 4);
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_an_element_at_opacity_0_draws_nothing_on_the_card() {
    an_element_at_opacity_0_draws_nothing(&mut card());
}
