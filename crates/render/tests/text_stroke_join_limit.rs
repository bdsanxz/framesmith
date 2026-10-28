//! The join of a text's stroke at a corner: a corner whose mitre ratio is below the limit of 4 keeps its mitre, and one
//! past the limit is bevelled (`docs/spec/text.md`, section 6.6, rule 6). The corners are those of the test-only glyph
//! U+E000 of the test font (`docs/spec/test-font.md`, section 2), at 250 pixels with a stroke of 16 pixels, so half of
//! the stroke is 8 pixels.
//!
//! Worked from the spec: the baseline is at 0.85 * 250 = 212.5, which halves up to 213; each apex lies 10 pixels above
//! it, at y = 203. A mitre tip lies 8 times its ratio below its apex, and a bevel is cut 8 / ratio below it.
//!
//! The first apex lies at x = 45.5. Its apex angle is 2 * arctan(50 / 193) = 29.05 degrees, and its ratio is 3.987: the
//! tip lies at 203 + 31.9 = 234.9. Its tip is a wedge of half-angle t with tan t = 1 / sqrt(ratio squared - 1) =
//! 0.2591, so the column of x from 45 to 46 holds, in row 233 (depths 0.9 to 1.9 above the tip), the area of 2 * 0.2591 times the depth
//! integrated from 0.9 to 1.9 = 0.725, which is 255 * 0.275 = 70.1; in row 234 (depths 0 to 0.9), 0.2591 *
//! 2 * 0.81 / 2 = 0.210, which is 255 * 0.790 = 201.5.
//!
//! The second apex lies at x = 107. Its apex angle is 2 * arctan(96 / 373) = 28.87 degrees, and its ratio is 4.012,
//! past the limit: the bevel cuts the corner 8 / 4.012 = 1.99 pixels below the apex, at y = 204.99, so row 204 is 99
//! per cent covered (2 of 255) and the rows from 205 down lie clear.
//!
//! The third apex lies at x = 193.5. Its apex angle is 2 * arctan(35 / 198) = 20.05 degrees, and its ratio is 5.745,
//! past the limit: the bevel is cut 8 / 5.745 = 1.39 pixels below the apex, at y = 204.39, so row 204 is 39 per cent
//! covered: 255 * 0.607 = 154.9.
//!
//! The fourth apex lies at x = 258.5. Its apex angle is 2 * arctan(55 / 150) = 40.27 degrees, and its ratio is 2.905,
//! below the limit: the tip lies 8 * 2.905 = 23.2 pixels below the apex, at y = 226.24, with tan t = 1 / sqrt(2.905
//! squared - 1) = 0.3666. Row 225 (depths 0.24 to 1.24) holds 0.3666 * 2 * (1.24 squared - 0.24 squared) / 2 = 0.542,
//! which is 255 * 0.458 = 116.7; row 226 (depths 0 to 0.24) holds 0.3666 * 2 * 0.24 squared / 2 = 0.021, which is 255 *
//! 0.979 = 249.6.

mod common;

use common::{compare, expected, project_copied_from_fixture};
use draw_cpu::CpuRenderer;
use project::Project;
use render::Scene;

const FIXTURE: &str = "text-join-corner";

/// The red channel of the processor's frame 0 of the fixture, as rows of pixels, the picture being grey.
fn frame_rows() -> Vec<Vec<u8>> {
    let folder = project_copied_from_fixture("text-stroke-join-limit", FIXTURE, FIXTURE);
    let project = Project::open(&folder).unwrap();
    let scene = Scene::from_project(&project, &render::system_codecs()).unwrap_or_else(|reports| panic!("{reports:?}"));
    let picture = scene.picture(0, &mut CpuRenderer).unwrap();
    let width = picture.width() as usize;
    picture
        .pixels()
        .chunks(width * 4)
        .map(|row| row.chunks(4).map(|pixel| pixel[0]).collect())
        .collect()
}

#[test]
fn a_corner_below_the_limit_keeps_its_mitre_down_to_the_worked_tip() {
    let rows = frame_rows();
    // Column 45 holds the apex at 45.5: inside the mitre from row 206 to row 232, the wedge's partial rows 233 and 234, and
    // clear below the tip at 234.9.
    for (row, line) in rows.iter().enumerate().take(233).skip(206) {
        assert_eq!(line[45], 0, "row {row} lies inside the mitre of the first corner");
    }
    assert!(
        (69..=71).contains(&rows[233][45]),
        "row 233 holds the wedge's 72.5 per cent: {}",
        rows[233][45]
    );
    assert!(
        (201..=203).contains(&rows[234][45]),
        "row 234 holds the wedge's 21 per cent: {}",
        rows[234][45]
    );
    for (row, line) in rows.iter().enumerate().take(240).skip(235) {
        assert_eq!(line[45], 255, "row {row} lies below the mitre tip of the first corner");
    }
}

#[test]
fn a_corner_past_the_limit_is_bevelled_at_the_worked_cut() {
    let rows = frame_rows();
    // Columns 106 and 107 hold the apex at 107: the row 204 is covered by the bevel, and the rows from 205 down are clear.
    for column in [106, 107] {
        assert!(rows[204][column] < 128, "row 204 of column {column} is covered by the bevel");
        for (row, line) in rows.iter().enumerate().take(240).skip(205) {
            assert_eq!(
                line[column], 255,
                "row {row} of column {column} lies past the bevel of the second corner"
            );
        }
    }
}

#[test]
fn a_twenty_degree_corner_is_bevelled_at_the_worked_cut() {
    let rows = frame_rows();
    // Column 193 holds the apex at 193.5, with its ratio of 5.745: the bevel covers 39 per cent of row 204 and nothing below.
    assert_eq!(rows[203][193], 0, "row 203 is the apex's own row, inside the stroke");
    assert!(
        (154..=156).contains(&rows[204][193]),
        "row 204 holds the bevel's 39 per cent: {}",
        rows[204][193]
    );
    for (row, line) in rows.iter().enumerate().take(240).skip(205) {
        assert_eq!(line[193], 255, "row {row} lies past the bevel of the third corner");
    }
}

#[test]
fn a_forty_degree_corner_keeps_its_mitre_down_to_the_worked_tip() {
    let rows = frame_rows();
    // Column 258 holds the apex at 258.5, with its ratio of 2.905: the tip covers row 225 by 54 per cent and row 226 by 2 per
    // cent, and the rows from 227 down are clear.
    for (row, line) in rows.iter().enumerate().take(224).skip(204) {
        assert_eq!(line[258], 0, "row {row} lies inside the mitre of the fourth corner");
    }
    assert!(
        (116..=118).contains(&rows[225][258]),
        "row 225 holds the tip's 54 per cent: {}",
        rows[225][258]
    );
    assert!(
        (249..=251).contains(&rows[226][258]),
        "row 226 holds the tip's 2 per cent: {}",
        rows[226][258]
    );
    for (row, line) in rows.iter().enumerate().take(240).skip(227) {
        assert_eq!(line[258], 255, "row {row} lies below the mitre tip of the fourth corner");
    }
}

#[test]
fn the_frame_equals_its_checked_in_picture_on_the_processor() {
    let folder = project_copied_from_fixture("text-stroke-join-limit", FIXTURE, FIXTURE);
    let project = Project::open(&folder).unwrap();
    let scene = Scene::from_project(&project, &render::system_codecs()).unwrap_or_else(|reports| panic!("{reports:?}"));
    let drawn = scene.picture(0, &mut CpuRenderer).unwrap();
    if let Err(difference) = compare(0, &drawn, &expected(FIXTURE, 0)) {
        panic!("{difference}");
    }
}
