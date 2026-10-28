//! Which font draws a text (`docs/spec/text.md`, sections 2.3, 2.4 and 2.7), drawn on both renderers: a list whose first
//! family is absent is drawn with the next, a list whose first family is present with the first, and a character the
//! test font lacks with the face of the machine the fall-back names. The processor's pictures are judged; the card's agree
//! with them within the tolerance of `PLAN.md` section 5 (reference machine only).

mod common;

use std::fs;

use common::project_copied_from_fixture;
use draw::{Mark, Picture};
use draw_cpu::CpuRenderer;
use project::Project;
use render::Scene;

const GROUP: &str = "font-choice-on-both-renderers";

fn scene_of(name: &str, fixture: &str, edit: impl Fn(String) -> String) -> Scene {
    let folder = project_copied_from_fixture(GROUP, name, fixture);
    let file = folder.join("composition.xml");
    fs::write(&file, edit(fs::read_to_string(&file).unwrap())).unwrap();
    Scene::from_project(&Project::open(&folder).unwrap(), &render::system_codecs()).unwrap_or_else(|reports| panic!("{reports:?}"))
}

fn processor_picture(scene: &Scene, number: u64) -> Picture {
    scene.picture(number, &mut CpuRenderer).unwrap()
}

#[test]
fn a_list_draws_its_first_present_family_as_that_family_alone_draws() {
    let listed = scene_of("listed", "text-family-list", |text| text);
    let alone = scene_of("alone", "text-family-list", |text| {
        text.replace("No Such Family, Framesmith Test Stroke", "Framesmith Test Stroke")
            .replace("Framesmith Test Stroke, Helvetica", "Framesmith Test Stroke")
    });
    for number in [0, 9] {
        let a = processor_picture(&listed, number);
        let b = processor_picture(&alone, number);
        assert_eq!(
            a.pixels(),
            b.pixels(),
            "frame {number}: the list draws as its first present family alone"
        );
        assert!(
            a.pixels().chunks(4).any(|pixel| pixel[..3] != [255, 255, 255]),
            "frame {number} draws ink"
        );
    }
}

#[cfg(target_os = "macos")]
#[test]
fn a_character_the_test_font_lacks_is_drawn_from_a_face_of_the_machine_and_the_rest_from_the_test_font() {
    let scene = scene_of("fall-back", "text-fall-back", |text| text);
    let test_font = fs::read(common::fixtures().join("fonts/framesmith-test-stroke-400.ttf")).unwrap();
    let frame = scene.frame(0).unwrap();
    let runs: Vec<_> = frame
        .marks
        .iter()
        .filter_map(|mark| match mark {
            Mark::Text(text) => Some(text.runs.iter()),
            _ => None,
        })
        .flatten()
        .collect();
    let of_the_test_font = runs
        .iter()
        .filter(|run| *run.font.data == test_font)
        .map(|run| run.glyphs.len())
        .sum::<usize>();
    let of_the_machine = runs
        .iter()
        .filter(|run| *run.font.data != test_font)
        .map(|run| run.glyphs.len())
        .sum::<usize>();
    assert_eq!(of_the_test_font, 5, "H, I, H and the two spaces are drawn from the test font");
    assert_eq!(of_the_machine, 2, "É and ก are drawn from faces of the machine");
    let picture = processor_picture(&scene, 0);
    let dark = picture.pixels().chunks(4).filter(|pixel| pixel[0] < 128).count();
    assert!(dark > 200, "the text is drawn: {dark} dark pixels");
}

#[cfg(target_os = "macos")]
mod on_the_card {
    use super::*;
    use common::agreement::compare_scene;
    use draw_gpu::CardRenderer;

    fn agrees(scene: &Scene) {
        let mut card = CardRenderer::new().unwrap_or_else(|error| panic!("the card could not be started: {error}"));
        let comparison = compare_scene(scene, &mut CpuRenderer, &mut card, None);
        assert!(comparison.passed(), "{}", comparison.report().join("\n"));
    }

    #[test]
    #[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
    fn reference_machine_only_the_card_agrees_with_the_processor_on_a_family_list() {
        agrees(&scene_of("card-list", "text-family-list", |text| text));
    }

    #[test]
    #[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
    fn reference_machine_only_the_card_agrees_with_the_processor_on_characters_from_the_machine() {
        agrees(&scene_of("card-fall-back", "text-fall-back", |text| text));
    }
}
