//! Shared styles drawn on the processor (`docs/spec/notation.md` section 9; `docs/spec/render-command.md`
//! sections 4 and 9). A box that names a style is drawn with the style's background and size; its own
//! background wins over the style's; of two styles the one named later wins. The frames of the styled
//! composition equal, to the byte, the frames of the same composition with every attribute written on its
//! element, and equal the checked-in pictures at the chosen frames.

mod common;

use std::path::Path;

use common::{compare, count_of, expected, extent_of, numbers_with_a_picture, project_from_fixture, project_with};
use draw_cpu::CpuRenderer;
use project::Project;
use render::Scene;

const PANEL: [u8; 4] = [0xe0, 0xa0, 0x30, 0xff];
const OWN: [u8; 4] = [0x30, 0x60, 0xff, 0xff];
const SECOND: [u8; 4] = [0x30, 0xff, 0x60, 0xff];
const FIRST: [u8; 4] = [0xff, 0x30, 0x30, 0xff];
const CANVAS: [u8; 4] = [0x10, 0x20, 0x40, 0xff];

/// The fixture's composition with no style: each element has every attribute its styles give it,
/// written on it.
const WRITTEN: &str = "<composition version=\"1\" width=\"160\" height=\"90\" fps=\"10\" duration=\"1\" background=\"#102040\">\n\
  <box left=\"10\" top=\"10\" width=\"30\" height=\"20\" background=\"#e0a030\"/>\n\
  <box left=\"50\" top=\"10\" width=\"30\" height=\"20\" background=\"#e0a030\"/>\n\
  <box left=\"90\" top=\"10\" width=\"30\" height=\"20\" background=\"#3060ff\"/>\n\
  <box left=\"10\" top=\"50\" width=\"30\" height=\"12\" background=\"#30ff60\"/>\n\
</composition>\n";

fn scene_of(folder: &Path) -> Scene {
    let project = Project::open(folder).unwrap_or_else(|error| panic!("{error:?}"));
    Scene::from_project(&project, &render::system_codecs()).unwrap_or_else(|reports| panic!("{reports:?}"))
}

fn styled_scene() -> Scene {
    scene_of(&project_from_fixture("shared-style-pictures", "styled", "shared-styles"))
}

fn written_scene() -> Scene {
    scene_of(&project_with("shared-style-pictures", "written", WRITTEN))
}

#[test]
fn the_styled_frames_equal_the_frames_of_the_same_composition_written_out() {
    let styled = styled_scene();
    let written = written_scene();
    assert_eq!(styled.frames(), 10);
    assert_eq!(written.frames(), styled.frames());
    for number in 0..styled.frames() {
        let drawn = styled.picture(number, &mut CpuRenderer).unwrap();
        let wanted = written.picture(number, &mut CpuRenderer).unwrap();
        if let Err(difference) = compare(number, &drawn, &wanted) {
            panic!("{difference}");
        }
    }
}

#[test]
fn two_boxes_that_name_one_style_are_drawn_with_its_background_and_size() {
    let picture = styled_scene().picture(0, &mut CpuRenderer).unwrap();
    // Two boxes of 30 by 20 pixels, at (10, 10) and (50, 10): each has the style's background.
    assert_eq!(count_of(&picture, PANEL), 2 * 30 * 20);
    assert_eq!(picture.pixel(10, 10), Some(PANEL));
    assert_eq!(picture.pixel(39, 29), Some(PANEL));
    assert_eq!(picture.pixel(40, 10), Some(CANVAS));
    assert_eq!(picture.pixel(50, 10), Some(PANEL));
    assert_eq!(picture.pixel(79, 29), Some(PANEL));
}

#[test]
fn a_box_that_writes_its_own_background_over_a_style_is_drawn_with_its_own() {
    let picture = styled_scene().picture(0, &mut CpuRenderer).unwrap();
    // The third box names `panel` and writes `#3060ff`: it is 30 by 20 pixels at (90, 10), in its own colour.
    assert_eq!(extent_of(&picture, OWN), Some((90, 10, 120, 30)));
    assert_eq!(count_of(&picture, OWN), 30 * 20);
    assert_eq!(count_of(&picture, PANEL), 2 * 30 * 20);
}

#[test]
fn of_two_styles_named_in_class_the_later_one_is_drawn() {
    let picture = styled_scene().picture(0, &mut CpuRenderer).unwrap();
    // The box names `first` then `second`: both give a background, and `second` wins. The height of
    // `first` (12 pixels) is kept, and the box is 30 pixels wide from its own width.
    assert_eq!(count_of(&picture, FIRST), 0);
    assert_eq!(extent_of(&picture, SECOND), Some((10, 50, 40, 62)));
    assert_eq!(count_of(&picture, SECOND), 30 * 12);
}

#[test]
fn the_styled_frames_at_chosen_times_equal_their_checked_in_pictures() {
    let scene = styled_scene();
    let numbers = numbers_with_a_picture("shared-styles");
    assert_eq!(numbers, vec![0, 4, 9], "the first, a middle and the last frame have a picture");
    for number in numbers {
        let drawn = scene.picture(number, &mut CpuRenderer).unwrap();
        if let Err(difference) = compare(number, &drawn, &expected("shared-styles", number)) {
            panic!("{difference}");
        }
    }
}
