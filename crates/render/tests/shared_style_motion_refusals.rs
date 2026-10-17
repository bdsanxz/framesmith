//! A movement attribute that a shared style gives a box (`docs/spec/notation.md` N5). This version does
//! not draw the movement attributes of a box, so each one the style gives is refused by `render` on the
//! box's line, and the report says which style gives it: nothing is drawn in part (`docs/spec/render-command.md`
//! section 9; `docs/spec/check-command.md` section 3c).

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
fn a_movement_attribute_a_style_gives_a_box_is_refused_on_the_box_line_and_names_the_style() {
    let folder = project_with(
        "shared-style-motion-refusals",
        "style-gives-movement",
        "<composition version=\"1\" width=\"160\" height=\"90\" fps=\"10\" duration=\"1\">\n\
         <style id=\"moving\" x=\"20\" opacity=\"0.5\"/>\n\
         <box class=\"moving\" left=\"90\" top=\"50\" width=\"20\" height=\"16\" background=\"#ffffff\"/>\n\
         </composition>\n",
    );
    let reports = Scene::from_project(&Project::open(&folder).unwrap()).unwrap_err();
    let shown: Vec<String> = reports.iter().map(shown).collect();
    assert_eq!(
        shown,
        [
            "composition.xml 3 FS-0035 <box> has an attribute this version does not support: `x`, which the style `moving` gives it",
            "composition.xml 3 FS-0035 <box> has an attribute this version does not support: `opacity`, which the style `moving` gives it",
        ]
    );
}

#[test]
fn a_movement_attribute_the_box_writes_itself_is_refused_without_naming_a_style() {
    let folder = project_with(
        "shared-style-motion-refusals",
        "box-writes-movement",
        "<composition version=\"1\" width=\"160\" height=\"90\" fps=\"10\" duration=\"1\">\n\
         <box left=\"90\" top=\"50\" width=\"20\" height=\"16\" x=\"20\"/>\n\
         </composition>\n",
    );
    let reports = Scene::from_project(&Project::open(&folder).unwrap()).unwrap_err();
    let shown: Vec<String> = reports.iter().map(shown).collect();
    assert_eq!(
        shown,
        ["composition.xml 2 FS-0035 <box> has an attribute this version does not support: `x`"]
    );
}
