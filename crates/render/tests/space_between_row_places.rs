//! A row with `justify-content="space-between"` puts its first box at the start edge and its last at the end
//! edge, and shares the room left over between the neighbours of each box equally. Each test reads the rectangles
//! of the filled boxes of a frame from its display list (`docs/spec/render-command.md` section 4.3, rule 5).

mod common;

use common::project_from_fixture;
use draw::Step;
use project::Project;
use render::Scene;

/// The rectangles, as (left, top, width, height), of the filled boxes of one frame, in drawing order.
fn filled_in_frame(scene: &Scene, number: u64) -> Vec<(i32, i32, u32, u32)> {
    scene
        .list(number)
        .unwrap_or_else(|error| panic!("frame {number}: {error:?}"))
        .steps
        .iter()
        .map(|step| match step {
            Step::Fill { rect, .. } => (rect.left, rect.top, rect.width, rect.height),
        })
        .collect()
}

/// The scene of the fixture, in a project folder of the test's own, so that tests running at the same time never share
/// a folder.
fn scene_of_fixture(folder_name: &str) -> Scene {
    let folder = project_from_fixture("space-between-row-places", folder_name, "space-between-row");
    let project = Project::open(&folder).unwrap_or_else(|error| panic!("{error}"));
    Scene::from_project(&project, &render::system_codecs()).unwrap_or_else(|reports| panic!("{reports:?}"))
}

#[test]
fn the_first_box_is_at_the_start_and_the_last_at_the_end_of_the_row() {
    let scene = scene_of_fixture("first-box-at-start");
    let boxes = filled_in_frame(&scene, 0);
    // The row is 96 wide from 0, and its boxes are 10, 12 and 10 wide.
    assert_eq!(boxes.first(), Some(&(0, 10, 10, 20)));
    assert_eq!(boxes.last(), Some(&(86, 10, 10, 20)));
    let (last_left, _, last_width, _) = boxes[2];
    assert_eq!(last_left + last_width as i32, 96, "the last box ends at the end edge of the row");
}

#[test]
fn the_room_either_side_of_the_middle_box_is_equal() {
    let scene = scene_of_fixture("room-either-side");
    let boxes = filled_in_frame(&scene, 0);
    assert_eq!(boxes.len(), 3, "{boxes:?}");
    let (first_left, _, first_width, _) = boxes[0];
    let (middle_left, _, middle_width, _) = boxes[1];
    let (last_left, _, _, _) = boxes[2];
    let room_before = middle_left - (first_left + first_width as i32);
    let room_after = last_left - (middle_left + middle_width as i32);
    assert_eq!(room_before, room_after, "the room either side of the middle box: {boxes:?}");
    // The 64 pixels the boxes leave over are shared between two neighbours, 32 each.
    assert_eq!(room_before, 32);
}

#[test]
fn the_places_of_the_row_are_the_same_in_every_frame() {
    let scene = scene_of_fixture("same-in-every-frame");
    let first = filled_in_frame(&scene, 0);
    for number in 1..scene.frames() {
        assert_eq!(filled_in_frame(&scene, number), first, "frame {number}");
    }
}
