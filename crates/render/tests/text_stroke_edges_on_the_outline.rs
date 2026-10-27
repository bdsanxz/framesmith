//! The stroke of a text lies on the outline of its glyph, exactly: a stroke's edge on a straight stem is drawn at its worked share
//! of a pixel (`docs/spec/text.md`, section 6.6, rule 10). The stem's outline lies at 10 + 60 * 0.096 = 15.76 pixels (the H of the
//! test font at 96 pixels, `docs/spec/test-font.md`, sections 3 and 5), so the stroke of 2 pixels has its outer edge at 14.76 and
//! its inner edge at 16.76. Column 14 is covered by 0.24 of the stroke, so it is drawn at 255 * 0.76 = 193.8 on white, and column 16
//! by 0.76, drawn at 255 * 0.24 = 61.2. A pixel is held to its exact share within one level (`docs/spec/text.md`, section 6.6,
//! rule 7). Rows 32 to 72 lie on the straight stem, clear of its top and of its foot, which begins at row 77; the crossbar starts
//! to the right of columns 14 to 16.

mod common;

use common::project_copied_from_fixture;
use draw_cpu::CpuRenderer;
use project::Project;
use render::Scene;

const FIXTURE: &str = "text-stroke-edges";

/// The rows of the straight stem, clear of its top and of its foot.
fn stem_rows() -> impl Iterator<Item = usize> {
    32..=72
}

/// The red channel of the processor's frame 0 of the fixture, as rows of pixels, the picture being grey.
fn frame_rows() -> Vec<Vec<u8>> {
    let folder = project_copied_from_fixture("text-stroke-edges", FIXTURE, FIXTURE);
    let project = Project::open(&folder).unwrap();
    let scene = Scene::from_project(&project).unwrap_or_else(|reports| panic!("{reports:?}"));
    let picture = scene.picture(0, &mut CpuRenderer).unwrap();
    let width = picture.width() as usize;
    picture
        .pixels()
        .chunks(width * 4)
        .map(|row| row.chunks(4).map(|pixel| pixel[0]).collect())
        .collect()
}

#[test]
fn the_outer_edge_of_the_stroke_on_the_stem_is_drawn_at_its_share() {
    let rows = frame_rows();
    for row in stem_rows() {
        let value = f64::from(rows[row][14]);
        assert!(
            (value - 193.8).abs() <= 1.0,
            "row {row} of column 14 is {value}, its share is 193.8"
        );
    }
}

#[test]
fn the_inner_edge_of_the_stroke_on_the_stem_is_drawn_at_its_share() {
    let rows = frame_rows();
    for row in stem_rows() {
        let value = f64::from(rows[row][16]);
        assert!((value - 61.2).abs() <= 1.0, "row {row} of column 16 is {value}, its share is 61.2");
    }
}

#[test]
fn the_stem_is_inked_in_full_between_the_two_edges() {
    let rows = frame_rows();
    for row in stem_rows() {
        assert_eq!(rows[row][15], 0, "row {row} of column 15 lies inside the stroke");
    }
}
