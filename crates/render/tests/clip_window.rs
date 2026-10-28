//! The frames a box is shown in follow its window: a clip is shown from the frame of its start up to
//! the frame of its end, a box without a window in every frame of its parent's, and a child in no frame
//! outside its parent's window (`docs/spec/render-command.md` section 4.2; `docs/spec/frame-time.md`).
//! Each test draws the frames of the clip-window fixture on the processor and reads the pixels of boxes.

mod common;

use common::project_from_fixture;
use draw_cpu::CpuRenderer;
use project::Project;
use render::Scene;

const FIXTURE: &str = "clip-window";
const CANVAS: [u8; 4] = [0x10, 0x20, 0x40, 255];
const WHITE: [u8; 4] = [255, 255, 255, 255];
const OCHRE: [u8; 4] = [0xe0, 0xa0, 0x30, 255];
const RED: [u8; 4] = [255, 0, 0, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];
const CLIP: [u8; 4] = [0x20, 0x20, 0x20, 255];
const LATER: [u8; 4] = [0x40, 0xc0, 0xff, 255];
const INSIDE: [u8; 4] = [0xff, 0x40, 0xff, 255];

/// The scene of the clip-window fixture, in a project folder of its own named `name`.
fn scene(name: &str) -> Scene {
    let folder = project_from_fixture("clip-window", name, FIXTURE);
    let project = Project::open(&folder).unwrap_or_else(|error| panic!("{error}"));
    Scene::from_project(&project, &render::system_codecs()).unwrap_or_else(|reports| panic!("{reports:?}"))
}

/// The numbers, in order, of the frames in which the pixel at (x, y) has `colour`.
fn frames_showing(scene: &Scene, x: u32, y: u32, colour: [u8; 4]) -> Vec<u64> {
    (0..scene.frames())
        .filter(|&number| scene.picture(number, &mut CpuRenderer).unwrap().pixel(x, y) == Some(colour))
        .collect()
}

/// The numbers from `first` to `last`, both included.
fn frames(first: u64, last: u64) -> Vec<u64> {
    (first..=last).collect()
}

#[test]
fn a_clip_is_shown_from_the_frame_of_its_start_up_to_the_frame_of_its_end() {
    // start="1" and duration="2" at 30 frames a second: frame 30 is the first and frame 89 the last.
    let shown = frames_showing(&scene("clip-start-and-end"), 24, 8, OCHRE);
    assert!(!shown.contains(&29), "{shown:?}");
    assert!(shown.contains(&30) && shown.contains(&89), "{shown:?}");
    assert!(!shown.contains(&90), "{shown:?}");
    assert_eq!(shown, frames(30, 89));
}

#[test]
fn a_box_with_no_window_is_shown_in_every_frame_of_its_parent() {
    let scene = scene("box-without-window");
    // The root is the parent of the box at the top left: the box is shown in all 120 frames.
    assert_eq!(frames_showing(&scene, 8, 8, WHITE), frames(0, scene.frames() - 1));
    // The parent of the box inside the clip is the clip, shown in frames 30 to 59: so is the box.
    assert_eq!(frames_showing(&scene, 40, 31, INSIDE), frames(30, 59));
}

#[test]
fn a_child_starting_half_a_second_into_a_clip_first_appears_on_the_frame_for_one_and_a_half_seconds() {
    // The clip starts at 1 second, frame 30; the child starts 0.5 second into it: 1.5 seconds, frame 45.
    let shown = frames_showing(&scene("child-half-second-in"), 10, 26, LATER);
    assert_eq!(shown.first(), Some(&45), "{shown:?}");
}

#[test]
fn a_child_is_not_drawn_outside_its_parents_window_whatever_its_own_window_says() {
    // The child's own window runs from frame 45 to frame 134, past the end of the clip at frame 59 and
    // past the last frame of the composition, 119. It is drawn from 45 to 59 and in no other frame.
    let scene = scene("child-outside-parent");
    assert_eq!(frames_showing(&scene, 10, 26, LATER), frames(45, 59));
    // The clip itself is drawn in the frames of its own window, 30 to 59, and in no other.
    assert_eq!(frames_showing(&scene, 90, 40, CLIP), frames(30, 59));
    // Where the child would be in frames 60 to 89, its own window's frames, the picture is the canvas:
    // in every frame outside the clip's window the pixel is the canvas, and only there.
    assert_eq!(frames_showing(&scene, 10, 26, CANVAS), [frames(0, 29), frames(60, 119)].concat());
}

#[test]
fn a_start_of_a_third_of_a_second_lands_on_the_frame_the_grid_rule_gives() {
    let scene = scene("third-of-a-second");
    // 0.33333 and 0.33334 are within a thousandth of a frame of frame 10 at 30 frames a second, so both
    // are frame 10. 0.34 is 0.2 of a frame past frame 10, so it is frame 11.
    assert_eq!(frames_showing(&scene, 36, 4, RED), frames(10, 39));
    assert_eq!(frames_showing(&scene, 44, 4, GREEN), frames(10, 39));
    assert_eq!(frames_showing(&scene, 52, 4, BLUE), frames(11, 40));
}
