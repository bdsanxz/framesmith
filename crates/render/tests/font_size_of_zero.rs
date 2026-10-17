//! A font size of zero is reported, never drawn: FS-0053 on the text's line, invalid, with no frame made
//! (`docs/spec/text.md`, section 3.1; `docs/spec/notation.md`, N6).

mod common;

use std::fs;
use std::path::PathBuf;

use common::fixtures;
use project::{Code, Kind, Project};
use render::Scene;

fn zero_size_project(name: &str, body: &str) -> PathBuf {
    let folder = common::scratch("font-size-of-zero", name);
    fs::create_dir_all(folder.join("fonts")).unwrap();
    for file in ["framesmith-test-stroke-400.ttf", "framesmith-test-stroke-700.ttf"] {
        fs::copy(fixtures().join("fonts").join(file), folder.join("fonts").join(file)).unwrap();
    }
    let composition = format!(
        "<composition version=\"1\" width=\"200\" height=\"90\" fps=\"10\" duration=\"1\" background=\"#ffffff\">\n\
         <font src=\"fonts/framesmith-test-stroke-400.ttf\" family=\"Framesmith Test Stroke\"/>\n\
         {body}\n</composition>\n"
    );
    fs::write(folder.join("composition.xml"), composition).unwrap();
    folder
}

#[test]
fn a_text_with_a_font_size_of_zero_is_reported_with_its_line_as_invalid() {
    let folder = zero_size_project(
        "text",
        "<text left=\"10\" top=\"10\" font-family=\"Framesmith Test Stroke\" font-size=\"0\">AB</text>",
    );
    let reports = Scene::from_project(&Project::open(&folder).unwrap()).unwrap_err();
    assert_eq!(reports.len(), 1, "{reports:#?}");
    let problem = reports[0].problem();
    assert_eq!(problem.code(), Code::SizeNotAboveZero);
    assert_eq!(problem.kind(), Kind::Invalid);
    assert_eq!(problem.line(), 3);
    assert!(
        problem.message().contains("`font-size=\"0\"` must be above 0"),
        "{}",
        problem.message()
    );
}

#[test]
fn a_negative_font_size_is_reported_as_a_zero_one_is() {
    let folder = zero_size_project(
        "negative",
        "<text left=\"10\" top=\"10\" font-family=\"Framesmith Test Stroke\" font-size=\"-4\">AB</text>",
    );
    let reports = Scene::from_project(&Project::open(&folder).unwrap()).unwrap_err();
    assert_eq!(reports.len(), 1, "{reports:#?}");
    assert_eq!(reports[0].problem().code(), Code::SizeNotAboveZero);
}
