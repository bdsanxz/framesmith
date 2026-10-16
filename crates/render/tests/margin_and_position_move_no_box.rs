//! `margin` and `position` on an absolutely placed box have no effect on the picture: the boxes of
//! the fixture are drawn where their edges put them, as they are drawn with no margin and no
//! position, and with other values of them (`docs/spec/render-command.md` section 4.1, and
//! `notation.md` N10).

mod common;

use common::{compare, project_from_fixture, project_with};
use draw::Picture;
use draw_cpu::CpuRenderer;
use project::Project;
use render::Scene;

/// The picture of frame 0 of a composition of the three boxes of the fixture, each given the
/// attributes written in its place.
fn picture_with_attributes(name: &str, plain: &str, absolute: &str, lower: &str) -> Picture {
    let text = format!(
        "<composition version=\"1\" width=\"96\" height=\"54\" fps=\"10\" duration=\"1\" background=\"#102040\">\n\
         <box id=\"plain\" left=\"10\" top=\"10\" width=\"20\" height=\"20\" {plain} background=\"#e0a030\"/>\n\
         <box id=\"absolute\" left=\"50\" top=\"10\" width=\"20\" height=\"20\" {absolute} background=\"#30c0a0\"/>\n\
         <box id=\"static\" left=\"10\" top=\"40\" width=\"20\" height=\"10\" {lower} background=\"#a050e0\"/>\n\
         </composition>\n"
    );
    let folder = project_with("margin-and-position", name, &text);
    let project = Project::open(&folder).unwrap();
    let scene = Scene::from_project(&project).unwrap_or_else(|reports| panic!("{reports:?}"));
    scene.picture(0, &mut CpuRenderer).unwrap()
}

#[test]
fn a_margin_and_a_position_change_no_pixel_of_the_boxes() {
    let folder = project_from_fixture("margin-and-position", "fixture", "placement-margin-and-position");
    let project = Project::open(&folder).unwrap();
    let with_them = Scene::from_project(&project).unwrap().picture(0, &mut CpuRenderer).unwrap();

    let without_them = picture_with_attributes("without", "", "", "");
    assert_eq!(compare(0, &with_them, &without_them), Ok(()));

    let other_values = picture_with_attributes(
        "other-values",
        "margin=\"50\"",
        "margin=\"50\" position=\"static\"",
        "margin=\"0\" position=\"absolute\"",
    );
    assert_eq!(compare(0, &with_them, &other_values), Ok(()));
}
