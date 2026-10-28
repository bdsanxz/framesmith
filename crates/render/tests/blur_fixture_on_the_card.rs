//! The card's pictures of the blur fixture agree with the processor's on every frame, at the tolerance of `PLAN.md`
//! section 5 (`docs/spec/card-renderer.md`, section 10.7). It needs a graphics card that frx7 can draw with: reference
//! machine only.

mod common;

use common::agreement::compare_scene;
use common::project_copied_from_fixture;
use draw_cpu::CpuRenderer;
use draw_gpu::CardRenderer;
use project::Project;
use render::Scene;

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_the_card_agrees_with_the_processor_on_blurred_brightened_and_faded_elements() {
    let folder = project_copied_from_fixture("blur-fixture-on-the-card", "blur-and-brightness", "blur-and-brightness");
    let scene =
        Scene::from_project(&Project::open(&folder).unwrap(), &render::system_codecs()).unwrap_or_else(|reports| panic!("{reports:?}"));
    let mut card = CardRenderer::new().unwrap_or_else(|error| panic!("the card could not be started: {error}"));
    let comparison = compare_scene(&scene, &mut CpuRenderer, &mut card, None);
    let report = comparison.report();
    println!("{}", report.join("\n"));
    assert!(comparison.passed(), "{}", report.join("\n"));
}
