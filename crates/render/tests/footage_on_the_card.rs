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
    let scene = Scene::from_project(&project).unwrap_or_else(|reports| panic!("{reports:?}"));
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
