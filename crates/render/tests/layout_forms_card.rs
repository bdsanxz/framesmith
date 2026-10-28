//! The card's pictures of the layout-form fixtures (space between, a shared baseline, columns of stated widths and
//! shares, and a widest width) agree with the processor's, frame by frame, within the tolerance (`docs/spec/card-renderer.md`
//! sections 4, 5 and 7). The test needs a graphics card that frx7 can draw with, so it is reference machine only
//! (`docs/dev/gates.md` section 4).

mod common;

use common::agreement::compare_scene;
use common::project_copied_from_fixture;
use draw_cpu::CpuRenderer;
use draw_gpu::CardRenderer;
use project::Project;
use render::Scene;

const FIXTURES: [&str; 4] = [
    "space-between-row",
    "shared-baseline-texts",
    "columns-stated-and-shares",
    "widest-width-text",
];

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_every_frame_of_the_layout_form_fixtures_is_within_the_tolerance_of_the_processor() {
    let mut card = CardRenderer::new().unwrap_or_else(|error| panic!("the card could not be started: {error}"));
    for fixture in FIXTURES {
        let folder = project_copied_from_fixture("layout-forms-card", fixture, fixture);
        let project = Project::open(&folder).unwrap_or_else(|error| panic!("{error}"));
        let scene = Scene::from_project(&project, &render::system_codecs()).unwrap_or_else(|reports| panic!("{reports:?}"));
        let comparison = compare_scene(&scene, &mut CpuRenderer, &mut card, None);
        let report = comparison.report();
        for line in &report {
            println!("{fixture}: {line}");
        }
        assert!(comparison.passed(), "{fixture}: {}", report.join("\n"));
        assert_eq!(
            report.last().unwrap(),
            &format!("the comparison passed: all {} frames are within the tolerance", scene.frames()),
            "{fixture}"
        );
    }
}
