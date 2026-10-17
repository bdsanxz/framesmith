//! A text with a widest width breaks its lines so that no line is wider than it, and a text shorter than its widest
//! width keeps its own width. The texts are read from the picture of the first frame, by the extent of their colour
//! (`docs/spec/text.md`, section 5.3; `docs/spec/render-command.md` section 4.3, rule 14).

mod common;

use common::{extent_of, project_copied_from_fixture};
use draw::Picture;
use draw_cpu::CpuRenderer;
use project::Project;
use render::Scene;
use std::fs;

const GROUP: &str = "widest-width-text-picture";

/// The colour of the blue text with a widest width of 60, `#102040`, and of the red one, `#c03020`.
const WIDEST: [u8; 4] = [0x10, 0x20, 0x40, 0xff];
const SHORT: [u8; 4] = [0xc0, 0x30, 0x20, 0xff];

/// The widest width of the blue text in the fixture.
const WIDEST_WIDTH: u32 = 60;

/// The picture of the first frame of a copy of the fixture, after `change` has been made to its composition.
fn first_picture(name: &str, change: Option<(&str, &str)>) -> Picture {
    let folder = project_copied_from_fixture(GROUP, name, "widest-width-text");
    if let Some((from, to)) = change {
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
fn no_line_of_a_text_with_a_widest_width_is_wider_than_that_width() {
    let picture = first_picture("widest-width-lines", None);
    let (left, top, right, bottom) = extent_of(&picture, WIDEST).expect("the blue text is drawn");
    assert!(
        right - left <= WIDEST_WIDTH,
        "the blue text is {} wide, its widest width is {WIDEST_WIDTH}",
        right - left
    );
    // The words broke into lines: the text is three lines high, the red text one line.
    let short = extent_of(&picture, SHORT).expect("the red text is drawn");
    assert!(
        bottom - top > 2 * (short.3 - short.1),
        "the blue text has more than one line: {:?} and {short:?}",
        (left, top, right, bottom)
    );
}

#[test]
fn a_text_shorter_than_its_widest_width_keeps_its_own_width() {
    // The control: the same red text with no widest width, laid out alone.
    let with_widest = first_picture("short-text-with-widest-width", None);
    let without = first_picture("short-text-without-widest-width", Some((" max-width=\"200\"", "")));
    let kept = extent_of(&with_widest, SHORT).expect("the red text is drawn");
    let alone = extent_of(&without, SHORT).expect("the red text is drawn without its widest width");
    assert_eq!(kept, alone, "the red text is as wide as it is with no widest width");
}
