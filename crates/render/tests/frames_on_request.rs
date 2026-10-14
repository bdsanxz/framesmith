//! Any single frame is drawn on request, with no command for it, and is the same picture whatever was
//! asked for before (`docs/spec/render-command.md` sections 4 and 8).

mod common;

use common::project_from_fixture;
use draw::{Picture, Renderer};
use draw_cpu::CpuRenderer;
use project::Project;
use render::{PictureError, Scene};

fn scene() -> Scene {
    let folder = project_from_fixture("frames-on-request", "first-render", "first-render");
    Scene::from_project(&Project::open(&folder).unwrap()).unwrap()
}

#[test]
fn the_first_a_middle_and_the_last_frame_are_the_same_pictures_in_order_and_in_reverse() {
    let scene = scene();
    assert_eq!(scene.frames(), 10);
    let (first, middle, last) = (0, 4, scene.frames() - 1);
    let mut renderer = CpuRenderer;

    let forward: Vec<Picture> = [first, middle, last]
        .iter()
        .map(|&n| scene.picture(n, &mut renderer).unwrap())
        .collect();
    let backward: Vec<Picture> = [last, middle, first]
        .iter()
        .map(|&n| scene.picture(n, &mut renderer).unwrap())
        .collect();
    assert_eq!(forward[0], backward[2], "frame 0");
    assert_eq!(forward[1], backward[1], "frame 4");
    assert_eq!(forward[2], backward[0], "frame 9");

    // A renderer that is new for each request gives the same pictures too.
    for (number, picture) in [(first, &forward[0]), (middle, &forward[1]), (last, &forward[2])] {
        assert_eq!(&scene.picture(number, &mut CpuRenderer).unwrap(), picture, "frame {number}");
    }
}

#[test]
fn a_frame_asked_for_alone_is_the_frame_asked_for_after_every_other() {
    let scene = scene();
    let mut renderer = CpuRenderer;
    let alone = scene.picture(7, &mut CpuRenderer).unwrap();
    for number in (0..scene.frames()).rev() {
        scene.picture(number, &mut renderer).unwrap();
    }
    assert_eq!(scene.picture(7, &mut renderer).unwrap(), alone);
}

#[test]
fn the_steps_of_a_frame_are_the_same_list_whatever_was_asked_for_before() {
    let scene = scene();
    let at_the_end = scene.list(9).unwrap();
    assert_eq!(scene.list(0).unwrap(), at_the_end);
    assert_eq!(scene.list(9).unwrap(), at_the_end);
}

#[test]
fn a_frame_past_the_last_is_not_a_frame_and_the_message_names_the_number_and_the_last_frame() {
    let scene = scene();
    let error = scene.picture(10, &mut CpuRenderer).unwrap_err();
    assert_eq!(error, PictureError::NoSuchFrame { number: 10, frames: 10 });
    assert_eq!(error.to_string(), "there is no frame 10: the frames are numbered from 0 to 9");
    assert!(scene.list(u64::MAX).is_err());
    assert!(scene.picture(9, &mut CpuRenderer).is_ok());
}

#[test]
fn a_renderer_that_cannot_draw_is_told_with_its_own_words() {
    struct Broken;
    impl Renderer for Broken {
        fn name(&self) -> &'static str {
            "broken"
        }
        fn draw(&mut self, _: &draw::DisplayList) -> Result<Picture, draw::Error> {
            Err(draw::Error::new("no memory"))
        }
    }
    let error = scene().picture(0, &mut Broken).unwrap_err();
    assert_eq!(error.to_string(), "no memory");
}

#[test]
fn the_scene_says_what_the_composition_says() {
    let scene = scene();
    assert_eq!(
        (scene.width(), scene.height(), scene.fps(), scene.frames(), scene.boxes()),
        (96, 54, 10, 10, 1)
    );
}
