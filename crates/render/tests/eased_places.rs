//! Where an eased box is drawn, frame by frame (`docs/spec/notation.md` section 8.4; `docs/spec/motion.md`, "Easing
//! names"): a tween with no `ease` draws the same frames as one with `ease="linear"`, and one with `ease="out-cubic"` is
//! three quarters or more of the way at the middle of its time. Places are read from the processor's picture.

mod common;

use common::project_with;
use draw::Picture;
use draw_cpu::CpuRenderer;
use project::Project;
use render::Scene;

const GROUP: &str = "eased-places";

/// A scene of a 400 by 60 canvas at 30 frames a second, white, with a red 20 by 20 box at left 10 whose tween moves it
/// 300 pixels right over 2 seconds from 0, with `ease` written as given.
fn scene(name: &str, ease: &str) -> Scene {
    let text = format!(
        "<composition version=\"1\" width=\"400\" height=\"60\" fps=\"30\" duration=\"3\" background=\"#ffffff\">\n\
         <box left=\"10\" top=\"20\" width=\"20\" height=\"20\" background=\"#ff0000\">\n\
         <tween duration=\"2\" to-x=\"300\"{ease}/>\n\
         </box>\n</composition>\n"
    );
    let folder = project_with(GROUP, name, &text);
    Scene::from_project(&Project::open(&folder).unwrap(), &render::system_codecs()).unwrap_or_else(|reports| panic!("{reports:?}"))
}

fn picture(scene: &Scene, number: u64) -> Picture {
    scene.picture(number, &mut CpuRenderer).unwrap()
}

/// The first column of row 30 whose pixel is pure red, if any.
fn first_red(picture: &Picture) -> Option<u32> {
    (0..picture.width()).find(|x| picture.pixel(*x, 30) == Some([255, 0, 0, 255]))
}

#[test]
fn a_tween_with_no_ease_draws_the_same_frames_as_one_with_linear() {
    let none = scene("no-ease", "");
    let linear = scene("linear", " ease=\"linear\"");
    for number in 0..90 {
        assert!(picture(&none, number) == picture(&linear, number), "frame {number}");
    }
    assert_eq!(first_red(&picture(&none, 30)), Some(160), "half way at the middle");
}

#[test]
fn a_tween_with_out_cubic_is_three_quarters_or_more_of_the_way_at_the_middle_of_its_time() {
    let scene = scene("out-cubic", " ease=\"out-cubic\"");
    let left = first_red(&picture(&scene, 30)).expect("the box is drawn");
    // Three quarters of 300 pixels from left 10 is 235; out-cubic gives 7/8 of the way, 272.5, so the first whole red
    // column is 273.
    assert!(left >= 235, "left {left}");
    assert_eq!(left, 273);
    assert_eq!(first_red(&picture(&scene, 0)), Some(10));
    assert_eq!(first_red(&picture(&scene, 60)), Some(310));
}

#[test]
fn a_tween_with_out_back_runs_past_its_end_and_settles_on_it() {
    let scene = scene("out-back", " ease=\"out-back(1.7)\"");
    let most = (0..=60).filter_map(|number| first_red(&picture(&scene, number))).max().unwrap();
    // 1.0998 of 300 pixels is about 330 pixels from left 10.
    assert!(most > 310 && most <= 341, "most {most}");
    assert_eq!(first_red(&picture(&scene, 60)), Some(310));
}
