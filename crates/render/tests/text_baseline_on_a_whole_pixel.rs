//! The baseline of an absolute text is drawn on a whole pixel (`docs/spec/text.md`, section 4): the exact baseline is rounded
//! to a whole pixel, halves up. Worked by hand from the test font's stem (`docs/spec/test-font.md`, sections 3 and 5): the
//! stem of `H` runs from 40 units above the baseline to the cap height's 660, so at a size of 30 pixels (0.03 per unit) its
//! ink ends 1.2 pixels above the baseline and its top lies 19.8 pixels above it; at 25 pixels, 1.0 and 16.5.
//!
//! The baseline of `H` at 30 pixels, with its top at 0, is 0.85 * 30 = 25.5, which rounds up to 26. The stem then covers 80
//! per cent of row 6 (its top at 6.2) and of row 24 (its foot at 24.8). Without the rounding, at 25.5, row 5 would hold 30 per
//! cent (its top at 5.7) and row 24 would hold 30 per cent (its foot at 24.3), so the test tells the two apart. At 25 pixels the baseline is 0.85 * 25 = 21.25, which rounds down to 21: the
//! stem covers 50 per cent of row 4 (its top at 4.5) and ends at the foot of row 19.

mod common;

use common::project_copied_from_fixture;
use draw_cpu::CpuRenderer;
use project::Project;
use render::Scene;

/// The red channel of the processor's frame 0 of the fixture, as rows of pixels, the picture being grey.
fn frame_rows() -> (usize, Vec<Vec<u8>>) {
    let folder = project_copied_from_fixture(
        "text-baseline-on-a-whole-pixel",
        "text-baseline-whole-pixel",
        "text-baseline-whole-pixel",
    );
    let project = Project::open(&folder).unwrap();
    let scene = Scene::from_project(&project, &render::system_codecs()).unwrap_or_else(|reports| panic!("{reports:?}"));
    let picture = scene.picture(0, &mut CpuRenderer).unwrap();
    let width = picture.width() as usize;
    let rows = picture
        .pixels()
        .chunks(width * 4)
        .map(|row| row.chunks(4).map(|pixel| pixel[0]).collect())
        .collect();
    (width, rows)
}

#[test]
fn the_baseline_of_a_text_at_30_pixels_rounds_half_up_to_row_26() {
    let (_, rows) = frame_rows();
    // Column 13 lies inside the left stem of the `H` at 30 pixels: rows 7 to 23 are fully ink, row 5 and row 25 are empty.
    let stem: Vec<u8> = (3..=27).map(|row| rows[row][13]).collect();
    // Rows 3 and 4 are above the stem, row 5 is empty, row 6 holds 80 per cent (51 of 255), rows 7 to 23 are ink, row 24
    // holds 80 per cent, row 25 is empty and row 26 is below the glyph's foot.
    assert_eq!(stem[..3], [255, 255, 255], "rows 3 to 5 are empty above the stem: {stem:?}");
    assert_eq!(stem[3], 51, "row 6 holds 80 per cent of the stem: {stem:?}");
    assert!(stem[4..21].iter().all(|&value| value == 0), "rows 7 to 23 are fully ink: {stem:?}");
    assert_eq!(stem[21], 51, "row 24 holds 80 per cent of the stem: {stem:?}");
    assert!(stem[22..].iter().all(|&value| value == 255), "rows 25 onwards are empty: {stem:?}");
}

#[test]
fn the_baseline_of_a_text_at_25_pixels_rounds_to_row_21() {
    let (_, rows) = frame_rows();
    // Column 62 lies inside the left stem of the `H` at 25 pixels, which starts at 60 pixels.
    let stem: Vec<u8> = (2..=22).map(|row| rows[row][62]).collect();
    // Row 4 holds half of the stem (127.5 of 255, which is 127 or 128); rows 5 to 19 are fully ink; row 20 is empty.
    assert_eq!(stem[..2], [255, 255], "rows 2 and 3 are empty above the stem: {stem:?}");
    assert!((127..=128).contains(&stem[2]), "row 4 holds half of the stem: {stem:?}");
    assert!(stem[3..18].iter().all(|&value| value == 0), "rows 5 to 19 are fully ink: {stem:?}");
    assert!(stem[18..].iter().all(|&value| value == 255), "row 20 onwards is empty: {stem:?}");
}
