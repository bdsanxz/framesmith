//! A clip in a column whose window has not begun keeps its place in the column: the clip is not drawn before its
//! window, the box after it stays where it is on the frame before the window and on the frame after its start, and
//! the place is the same in every frame (`docs/spec/render-command.md` section 4.3, rule 11; `docs/spec/notation.md`,
//! Q3). The clip starts at 0.3 seconds, which is frame 3 at 10 frames a second, and ends at 0.6 seconds, frame 6.

mod common;

use common::project_with;
use draw::Step;
use project::Project;
use render::Scene;

/// The clip's rectangle: the first child, 10 tall, at the top of the column.
const CLIP: (i32, i32, u32, u32) = (0, 0, 40, 10);

/// The box after the clip: it keeps the place the clip leaves for it, 10 pixels down.
const AFTER_THE_CLIP: (i32, i32, u32, u32) = (0, 10, 40, 10);

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

/// The scene of the column, in a project folder of its own for the test that asks for it, so that tests running at
/// the same time never share a folder.
fn scene_of_the_column(name: &str) -> Scene {
    let inside = "<box id=\"column\" left=\"0\" top=\"0\" width=\"40\" height=\"54\" flex-direction=\"column\">\n\
                  <box start=\"0.3\" duration=\"0.3\" height=\"10\" background=\"#ff8060\"/>\n\
                  <box height=\"10\" background=\"#80ff80\"/>\n\
                  </box>";
    let composition =
        format!("<composition version=\"1\" width=\"96\" height=\"54\" fps=\"10\" duration=\"1\">\n{inside}\n</composition>\n");
    let folder = project_with("clip-not-yet-shown-keeps-its-place", name, &composition);
    let project = Project::open(&folder).unwrap_or_else(|error| panic!("{error}"));
    Scene::from_project(&project).unwrap_or_else(|reports| panic!("{reports:?}"))
}

#[test]
fn a_clip_not_yet_shown_keeps_its_place_on_the_frames_around_its_start() {
    let scene = scene_of_the_column("around-its-start");
    // The frame before the window, the first frame of it, the frame after its start, and the frame after its end.
    assert_eq!(filled_in_frame(&scene, 2), vec![AFTER_THE_CLIP], "the frame before its start");
    assert_eq!(filled_in_frame(&scene, 3), vec![CLIP, AFTER_THE_CLIP], "the frame of its start");
    assert_eq!(filled_in_frame(&scene, 4), vec![CLIP, AFTER_THE_CLIP], "the frame after its start");
    assert_eq!(filled_in_frame(&scene, 6), vec![AFTER_THE_CLIP], "the frame after its end");
}

#[test]
fn the_clip_is_drawn_only_in_its_window_and_the_box_after_it_never_moves() {
    let scene = scene_of_the_column("drawn-only-in-its-window");
    for number in 0..scene.frames() {
        let drawn = filled_in_frame(&scene, number);
        let in_window = (3..6).contains(&number);
        let expected = if in_window {
            vec![CLIP, AFTER_THE_CLIP]
        } else {
            vec![AFTER_THE_CLIP]
        };
        assert_eq!(drawn, expected, "frame {number}");
    }
}
