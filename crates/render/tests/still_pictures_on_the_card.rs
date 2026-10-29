//! The card's pictures of the fixture `still-pictures-fitting` agree with the processor's on every frame, at the tolerance
//! of `PLAN.md` section 5 (`docs/spec/still-pictures.md`, section 6). It needs a graphics card that frx7 can draw with:
//! reference machine only.

mod common;

use common::agreement::compare_scene;
use common::project_copied_from_fixture;
use draw_cpu::CpuRenderer;
use draw_gpu::CardRenderer;
use project::Project;
use render::Scene;

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_the_card_agrees_with_the_processor_on_pictures_filled_covered_and_contained() {
    let fixture = "still-pictures-fitting";
    let folder = project_copied_from_fixture("still-pictures-on-the-card", fixture, fixture);
    let scene =
        Scene::from_project(&Project::open(&folder).unwrap(), &render::system_codecs()).unwrap_or_else(|reports| panic!("{reports:?}"));
    let mut card = CardRenderer::new().unwrap_or_else(|error| panic!("the card could not be started: {error}"));
    let comparison = compare_scene(&scene, &mut CpuRenderer, &mut card, None);
    let report = comparison.report();
    println!("{fixture}:\n{}", report.join("\n"));
    assert!(comparison.passed(), "{fixture}: {}", report.join("\n"));
}
