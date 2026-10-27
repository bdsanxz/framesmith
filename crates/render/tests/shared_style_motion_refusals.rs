//! A movement attribute that a shared style gives a box (`docs/spec/notation.md` N5). This version moves a box
//! (`docs/spec/render-command.md` section 4.5), so a movement attribute is taken whether the style or the box writes
//! it, while an attribute this version does not draw is still refused on the box's line, naming the style that gives
//! it (`docs/spec/render-command.md` section 9; `docs/spec/check-command.md` section 3c).

mod common;

use common::project_with;
use project::Project;
use render::Scene;

fn shown(report: &project::Report) -> String {
    format!(
        "{} {} {} {}",
        report.file(),
        report.problem().line(),
        report.problem().code(),
        report.problem().message()
    )
}

#[test]
fn a_movement_attribute_a_style_gives_a_box_is_taken_beside_a_refused_one_that_names_the_style() {
    let folder = project_with(
        "shared-style-motion-refusals",
        "style-gives-movement",
        "<composition version=\"1\" width=\"160\" height=\"90\" fps=\"10\" duration=\"1\">\n\
         <style id=\"moving\" x=\"20\" opacity=\"0.5\" grain=\"0.5\"/>\n\
         <box class=\"moving\" left=\"90\" top=\"50\" width=\"20\" height=\"16\" background=\"#ffffff\"/>\n\
         </composition>\n",
    );
    let reports = Scene::from_project(&Project::open(&folder).unwrap()).unwrap_err();
    let shown: Vec<String> = reports.iter().map(shown).collect();
    assert_eq!(
        shown,
        ["composition.xml 3 FS-0035 <box> has an attribute this version does not support: `grain`, which the style `moving` gives it"]
    );
}

#[test]
fn a_movement_attribute_the_box_writes_itself_is_taken() {
    let folder = project_with(
        "shared-style-motion-refusals",
        "box-writes-movement",
        "<composition version=\"1\" width=\"160\" height=\"90\" fps=\"10\" duration=\"1\">\n\
         <box left=\"90\" top=\"50\" width=\"20\" height=\"16\" x=\"20\"/>\n\
         </composition>\n",
    );
    let scene = Scene::from_project(&Project::open(&folder).unwrap());
    assert!(
        scene.is_ok(),
        "{:?}",
        scene.err().map(|reports| reports.iter().map(shown).collect::<Vec<_>>())
    );
}
