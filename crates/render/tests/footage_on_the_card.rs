//! The card's pictures of the footage fixture agree with the processor's, frame by frame, within the tolerance
//! (`docs/spec/card-renderer.md` sections 4 and 12): similarity at least 0.995 and no more than 0.05 % of the values more than
//! 4 levels apart. Both renderers are given the same decoded pictures. The test needs a graphics card that frx7 can draw with,
//! so it is reference machine only (`docs/dev/gates.md` section 4).

mod common;

use common::agreement::figures;
use common::project_copied_from_fixture;
use draw_cpu::CpuRenderer;
use draw_gpu::CardRenderer;
use project::Project;
use render::{Scene, Sources};

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_every_frame_of_the_footage_fixture_is_within_the_tolerance_of_the_processor() {
    let folder = project_copied_from_fixture("footage-card", "footage-on-screen", "footage-on-screen");
    let project = Project::open(&folder).unwrap_or_else(|error| panic!("{error}"));
    let scene = Scene::from_project(&project, &render::system_codecs()).unwrap_or_else(|reports| panic!("{reports:?}"));
    let mut sources = Sources::open(&scene).unwrap();
    let mut card = CardRenderer::new().unwrap_or_else(|error| panic!("the card could not be started: {error}"));
    let mut misses = Vec::new();
    for number in 0..scene.frames() {
        let processor = scene.picture_with(number, &mut CpuRenderer, &mut sources).unwrap();
        let carded = scene.picture_with(number, &mut card, &mut sources).unwrap();
        let measured = figures(number, &processor, &carded).unwrap_or_else(|error| panic!("{error}"));
        println!("{}", measured.line());
        if !measured.within() {
            misses.push(format!("frame {number:04}: {}", measured.misses().join("; ")));
        }
    }
    assert_eq!(scene.frames(), 30);
    assert!(misses.is_empty(), "{}", misses.join("\n"));
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_the_look_and_corners_of_footage_agree_with_the_processor() {
    let folder = common::project_with(
        "footage-card",
        "look",
        "<composition version=\"1\" width=\"96\" height=\"54\" fps=\"10\" duration=\"1\" background=\"#102040\">\n<video src=\"grey.mp4\" width=\"96\" height=\"54\" opacity=\"0.5\" brightness=\"2\" blur=\"4\" border-radius=\"8\"/>\n</composition>\n",
    );
    let grey = measure::Frame {
        width: 96,
        height: 54,
        rgb: [128; 96 * 54 * 3].to_vec(),
    };
    measure::write_video(&folder.join("grey.mp4"), (10, 1), &vec![grey; 10]).unwrap();
    let project = Project::open(&folder).unwrap_or_else(|error| panic!("{error}"));
    let scene = Scene::from_project(&project, &render::system_codecs()).unwrap_or_else(|reports| panic!("{reports:?}"));
    let mut sources = Sources::open(&scene).unwrap();
    let mut card = CardRenderer::new().unwrap_or_else(|error| panic!("the card could not be started: {error}"));
    let processor = scene.picture_with(0, &mut CpuRenderer, &mut sources).unwrap();
    let carded = scene.picture_with(0, &mut card, &mut sources).unwrap();
    let measured = figures(0, &processor, &carded).unwrap_or_else(|error| panic!("{error}"));
    println!("{}", measured.line());
    assert!(measured.within(), "{}", measured.misses().join("; "));
}
