//! A place this version cannot draw, in a composition and in a file it shows, is refused before the first frame:
//! each report names its file and its own line, and the problems of the composition come with those of the file
//! it shows, in one run (`docs/spec/notation.md` section 10; `docs/spec/render-command.md` section 2).

mod common;

use common::scratch;
use project::Project;
use render::Scene;
use std::fs;

#[test]
fn a_wrong_place_in_a_shown_file_is_reported_with_its_name_and_line_with_the_problems_of_the_composition() {
    let dir = scratch("shown-refusals", "one-run");
    fs::write(
        dir.join("composition.xml"),
        "<composition version=\"1\" width=\"96\" height=\"54\" fps=\"30\" duration=\"2\">\n\
         <box left=\"abc\" top=\"0\" width=\"4\" height=\"4\"/>\n\
         <composition src=\"inset.xml\" left=\"0\" top=\"0\"/>\n\
         </composition>\n",
    )
    .unwrap();
    fs::write(
        dir.join("inset.xml"),
        "<composition version=\"1\" width=\"32\" height=\"24\" fps=\"30\" duration=\"2\">\n\
         <box left=\"top\" top=\"0\" width=\"4\" height=\"4\"/>\n\
         </composition>\n",
    )
    .unwrap();
    let project = Project::open(&dir).unwrap_or_else(|error| panic!("{error}"));
    let reports = match Scene::from_project(&project) {
        Ok(_) => panic!("a wrong place must be refused"),
        Err(reports) => reports,
    };
    let places: Vec<(&str, u32)> = reports.iter().map(|report| (report.file(), report.problem().line())).collect();
    assert_eq!(
        places,
        vec![("composition.xml", 2), ("inset.xml", 2)],
        "the reports of the composition, then of the file it shows, each on its line"
    );
    assert!(reports[1].problem().message().contains("`left=\"top\"`"), "{reports:?}");
}
