//! The card's pictures of the exact-halves fixture agree with the processor's on every frame, at the tolerance of
//! `PLAN.md` section 5 (`docs/spec/render-command.md`, section 4.1, rule 3). Needs a graphics card that frx7 can draw with:
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
fn reference_machine_only_the_card_agrees_with_the_processor_on_exact_halves_of_percentages() {
    let folder = project_copied_from_fixture("exact-half-edges-on-the-card", "placement-exact-halves", "placement-exact-halves");
    let scene = Scene::from_project(&Project::open(&folder).unwrap()).unwrap_or_else(|reports| panic!("{reports:?}"));
    let mut card = CardRenderer::new().unwrap_or_else(|error| panic!("the card could not be started: {error}"));
    let comparison = compare_scene(&scene, &mut CpuRenderer, &mut card, None);
    let report = comparison.report();
    println!("placement-exact-halves:\n{}", report.join("\n"));
    assert!(comparison.passed(), "{}", report.join("\n"));
}
