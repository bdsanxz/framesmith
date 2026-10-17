//! The files a composition shows are drawn at the place of their element, inside its window and cut to its
//! rectangle: the points of the shown-composition fixture, read from its picture of each frame, hold the colour
//! the files give them (`docs/spec/notation.md` section 10; `docs/spec/timeline.md` section 4).

mod common;

use common::project_copied_from_fixture;
use draw_cpu::CpuRenderer;
use project::Project;
use render::Scene;

const FIXTURE: &str = "shown-composition";

const BACK: [u8; 4] = [48, 64, 96, 255];
const ORANGE: [u8; 4] = [255, 128, 0, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];
const MAGENTA: [u8; 4] = [255, 0, 255, 255];
const RED: [u8; 4] = [255, 0, 0, 255];
const YELLOW: [u8; 4] = [255, 255, 0, 255];
const TEAL: [u8; 4] = [0, 160, 160, 255];

/// Each point: the frame, the column and the row of the point, and the colour the picture holds there.
/// The strip is at the foot of the frame from 0 to 29; the corner is at the top right from 15 to 44; the
/// inset is at 8, 6 from 30 to 74, with its frame, its inner box from 45 (in the inset from 0.5 seconds, which
/// its own length cuts at 75) and its badge from 30 to 59; the overhang is cut at the inset's right edge, 40.
const POINTS: [(u64, u32, u32, [u8; 4]); 17] = [
    (0, 36, 26, BACK),
    (0, 5, 45, TEAL),
    (14, 88, 4, BACK),
    (15, 88, 4, YELLOW),
    (29, 5, 45, TEAL),
    (30, 5, 45, BACK),
    (30, 12, 10, ORANGE),
    (30, 30, 8, RED),
    (30, 36, 26, MAGENTA),
    (30, 44, 26, BACK),
    (44, 88, 4, YELLOW),
    (44, 12, 10, ORANGE),
    (45, 12, 10, GREEN),
    (59, 30, 8, RED),
    (60, 30, 8, ORANGE),
    (75, 12, 10, BACK),
    (119, 12, 10, BACK),
];

#[test]
fn a_shown_file_is_drawn_at_its_element_inside_its_window_and_cut_to_its_rectangle() {
    let folder = project_copied_from_fixture("shown-composition-points", FIXTURE, FIXTURE);
    let project = Project::open(&folder).unwrap_or_else(|error| panic!("{error}"));
    let scene = Scene::from_project(&project).unwrap_or_else(|reports| panic!("{reports:?}"));
    for (number, column, row, colour) in POINTS {
        let picture = scene.picture(number, &mut CpuRenderer).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(
            picture.pixel(column, row),
            Some(colour),
            "{FIXTURE}: frame {number}, the point {column}, {row}"
        );
    }
}
