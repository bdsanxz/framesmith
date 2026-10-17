//! Two texts of different sizes in a row with `align-items="baseline"` have the baselines of their first lines at one
//! height. The texts are read from the picture of the first frame: each word has flat feet, so the bottom of its ink
//! is its baseline. Without the alignment the same words stand on different heights (`docs/spec/render-command.md`
//! section 4.3, rule 7; `docs/spec/text.md`, section 8).

mod common;

use common::{extent_of, project_copied_from_fixture};
use draw::Picture;
use draw_cpu::CpuRenderer;
use project::Project;
use render::Scene;
use std::fs;

const GROUP: &str = "shared-baseline-texts-picture";

/// The colour of the small blue word, `#102040`, and of the large red word, `#c03020`.
const SMALL: [u8; 4] = [0x10, 0x20, 0x40, 0xff];
const LARGE: [u8; 4] = [0xc0, 0x30, 0x20, 0xff];

/// The picture of the first frame of a copy of the fixture, in a project folder of its own.
fn first_picture(name: &str, composition_change: Option<(&str, &str)>) -> Picture {
    let folder = project_copied_from_fixture(GROUP, name, "shared-baseline-texts");
    if let Some((from, to)) = composition_change {
        let path = folder.join("composition.xml");
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains(from), "the composition has `{from}`");
        fs::write(&path, text.replace(from, to)).unwrap();
    }
    let project = Project::open(&folder).unwrap_or_else(|error| panic!("{error}"));
    let scene = Scene::from_project(&project).unwrap_or_else(|reports| panic!("{reports:?}"));
    scene.picture(0, &mut CpuRenderer).unwrap_or_else(|error| panic!("{error}"))
}

#[test]
fn the_first_lines_of_two_texts_of_different_sizes_stand_on_one_baseline() {
    let picture = first_picture("aligned-on-the-baseline", None);
    let small = extent_of(&picture, SMALL).expect("the small word is drawn");
    let large = extent_of(&picture, LARGE).expect("the large word is drawn");
    assert_eq!(
        small.3, large.3,
        "the bottoms of the two words are their baselines: {small:?} and {large:?}"
    );
    assert!(
        large.3 - large.1 > small.3 - small.1,
        "the large word is the taller: {small:?} and {large:?}"
    );
}

#[test]
fn without_the_shared_baseline_the_same_words_stand_on_different_heights() {
    // The control: the same row, with its items aligned at the start instead of on the baseline.
    let picture = first_picture(
        "aligned-at-the-start",
        Some(("align-items=\"baseline\"", "align-items=\"flex-start\"")),
    );
    let small = extent_of(&picture, SMALL).expect("the small word is drawn");
    let large = extent_of(&picture, LARGE).expect("the large word is drawn");
    assert_ne!(small.3, large.3, "the control has the baselines apart: {small:?} and {large:?}");
}
