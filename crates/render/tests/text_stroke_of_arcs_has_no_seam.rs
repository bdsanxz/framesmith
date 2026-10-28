//! The stroke of the glyph 8 of the test font is one ring around each loop, with no seam where the pieces of an arc meet
//! (`docs/spec/test-font.md`, section 4, rules 3 and 5; `docs/spec/text.md`, section 6.6). The fixture sets the glyph at 250
//! pixels, its baseline at row 233 and its left at 40, so the upper loop is centred at (105, 108) with its outline at radii
//! 50 and 30 pixels; a stroke of 16 pixels is 8 pixels to each side of the outline.
//!
//! Worked from the spec, at row 108, the centre row of the upper loop: the outer stroke covers the radii 42 to 58, so the
//! columns 47 to 63; the inner stroke covers the radii 22 to 38, so the columns 67 to 83; the columns 64 to 66 lie between
//! them and stay clear. On the right, the outer stroke covers the columns 147 to 163 and the inner one the columns 127 to
//! 143, so column 145 is clear. The old generator stroked a radial seam across the ink there, from x = 135 to x = 155.
//!
//! The worked picture of the whole frame is `fixtures/text-stroke-arcs/worked/frame-0000.png`, made from the spec by an
//! independent script (`evidence/textgaps/arc_ref.py`), with no product code.

mod common;

use common::{compare, expected, fixtures, png, project_copied_from_fixture};
use draw::Picture;
use draw_cpu::CpuRenderer;
use project::Project;
use render::Scene;

const FIXTURE: &str = "text-stroke-arcs";

/// The processor's frame 0 of the fixture.
fn drawn() -> Picture {
    let folder = project_copied_from_fixture("text-stroke-of-arcs", FIXTURE, FIXTURE);
    let project = Project::open(&folder).unwrap();
    let scene = Scene::from_project(&project).unwrap_or_else(|reports| panic!("{reports:?}"));
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
fn the_frame_is_within_one_level_of_its_worked_picture() {
    let path = fixtures().join(FIXTURE).join("worked").join("frame-0000.png");
    let bytes = std::fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let (width, height, pixels) = png::decode(&bytes).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let worked = Picture::new(width, height, pixels).unwrap();
    let drawn = drawn();
    assert_eq!((drawn.width(), drawn.height()), (worked.width(), worked.height()));
    let mut worst = (0u8, 0u32, 0u32);
    for y in 0..drawn.height() {
        for x in 0..drawn.width() {
            let difference = red(&drawn, x, y).abs_diff(red(&worked, x, y));
            if difference > worst.0 {
                worst = (difference, x, y);
            }
        }
    }
    assert!(
        worst.0 <= 1,
        "the largest difference from the worked picture is {} levels, at ({}, {})",
        worst.0,
        worst.1,
        worst.2
    );
}

#[test]
fn the_stroke_of_the_upper_loop_at_its_centre_row_lies_where_the_spec_puts_it() {
    let drawn = drawn();
    assert_eq!(red(&drawn, 50, 108), 0, "column 50 lies in the outer stroke, 47 to 63");
    assert_eq!(red(&drawn, 64, 108), 255, "column 64 lies between the strokes");
    assert_eq!(red(&drawn, 66, 108), 255, "column 66 lies between the strokes");
    assert_eq!(red(&drawn, 70, 108), 0, "column 70 lies in the inner stroke, 67 to 83");
}

#[test]
fn the_stroke_has_no_seam_where_the_pieces_of_the_right_side_meet() {
    // Column 145 is between the strokes on the right: the old generator stroked a radial seam there, black from 135 to 155.
    assert_eq!(
        red(&drawn(), 145, 108),
        255,
        "column 145 is clear of the stroke: no seam across the ink"
    );
}
