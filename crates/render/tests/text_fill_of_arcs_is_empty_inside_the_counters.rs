//! The fill of the glyph 8 of the test font is the ink between each loop's outer circle and its hole, and the counters are
//! empty (`docs/spec/test-font.md`, section 4, rule 5: an outer contour runs clockwise and a hole counter-clockwise, so the
//! winding inside a hole is zero). The fixture sets the glyph at 250 pixels with its upper loop centred at (105, 108) and its
//! outline at radii 50 and 30 pixels, as in `text-stroke-arcs`, but with no stroke.
//!
//! Worked from the spec, at row 108: the ink covers the radii 30 to 50, so the columns 55 to 75 on the left and 135 to 155
//! on the right. The counter lies inside the radius 30, so the columns 80 and 105 (the centre) are clear. Column 50 is
//! outside the outer circle and column 145 is ink.

mod common;

use common::{compare, expected, project_copied_from_fixture};
use draw::Picture;
use draw_cpu::CpuRenderer;
use project::Project;
use render::Scene;

const FIXTURE: &str = "text-fill-arcs";

/// The processor's frame 0 of the fixture.
fn drawn() -> Picture {
    let folder = project_copied_from_fixture("text-fill-of-arcs", FIXTURE, FIXTURE);
    let project = Project::open(&folder).unwrap();
    let scene = Scene::from_project(&project, &render::system_codecs()).unwrap_or_else(|reports| panic!("{reports:?}"));
    scene.picture(0, &mut CpuRenderer).unwrap()
}

/// The red channel of a pixel: the picture is grey.
fn red(picture: &Picture, x: u32, y: u32) -> u8 {
    picture.pixel(x, y).unwrap()[0]
}

#[test]
fn the_frame_equals_its_checked_in_picture_on_the_processor() {
    if let Err(difference) = compare(0, &drawn(), &expected(FIXTURE, 0)) {
        panic!("{difference}");
    }
}

#[test]
fn the_counter_of_the_upper_loop_is_empty_at_its_centre_and_inside_its_hole() {
    let drawn = drawn();
    assert_eq!(red(&drawn, 105, 108), 255, "the centre of the upper loop lies in its hole");
    assert_eq!(red(&drawn, 80, 108), 255, "column 80 lies inside the radius 30, in the hole");
}

#[test]
fn the_ink_of_the_upper_loop_lies_between_its_outer_circle_and_its_hole() {
    let drawn = drawn();
    assert_eq!(red(&drawn, 50, 108), 255, "column 50 lies outside the outer circle");
    assert_eq!(red(&drawn, 64, 108), 0, "column 64 lies in the ring, between 55 and 75");
    assert_eq!(red(&drawn, 66, 108), 0, "column 66 lies in the ring");
    assert_eq!(red(&drawn, 70, 108), 0, "column 70 lies in the ring");
    assert_eq!(
        red(&drawn, 145, 108),
        0,
        "column 145 lies in the ring on the right, between 135 and 155"
    );
}
