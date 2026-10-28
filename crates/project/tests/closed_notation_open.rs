//! A project is opened for a render by the reading `check` does: a composition that differs from a
//! valid one only by a form the notation does not list is not opened, so nothing can be rendered
//! from it (`docs/spec/check-command.md` section 3a).

use project::{Code, OpenError, Project, COMPOSITION_FILE};
use std::fs;
use std::path::{Path, PathBuf};

fn folder(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("closed-notation-open").join(name);
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn composition(body: &str) -> String {
    format!("<composition version=\"1\" width=\"640\" height=\"360\" duration=\"2\">\n{body}\n</composition>\n")
}

fn open(name: &str, text: &str) -> Result<Project, OpenError> {
    let dir = folder(name);
    fs::write(dir.join(COMPOSITION_FILE), text).unwrap();
    Project::open(&dir)
}

#[test]
fn a_composition_with_an_unknown_attribute_is_not_opened_and_its_valid_twin_is() {
    let valid = "<box left=\"10\" top=\"10\" width=\"100\" height=\"50\" background=\"#ff0000\"/>";
    let project = open("twin-valid", &composition(valid)).expect("the valid composition is opened");
    assert_eq!(project.root().frames, 60);

    // Each form that the notation does not list, added to the valid composition alone.
    for (name, body, code, line) in [
        ("attribute", valid.replace("/>", " glow=\"1\"/>"), Code::AttributeNotTaken, 2),
        ("element", format!("{valid}\n<circle/>"), Code::ElementNotKnown, 3),
        (
            "place",
            format!("<box>\n<font src=\"f.ttf\"/>\n</box>\n{valid}"),
            Code::ElementNotHere,
            3,
        ),
        ("number", valid.replace("/>", " x=\"inf\"/>"), Code::NotANumber, 2),
        ("time", valid.replace("/>", " start=\"5.\" duration=\"1\"/>"), Code::NotATime, 2),
    ] {
        match open(&format!("twin-{name}"), &composition(&body)) {
            Ok(_) => panic!("{name}: the composition was opened"),
            Err(OpenError::Invalid(reports)) => {
                assert_eq!(reports.len(), 1, "{name}: {reports:?}");
                assert_eq!(reports[0].problem().code(), code, "{name}");
                assert_eq!(reports[0].problem().line(), line, "{name}");
                assert_eq!(reports[0].file(), COMPOSITION_FILE);
            }
            Err(other) => panic!("{name}: {other}"),
        }
    }
}

#[test]
fn a_form_in_a_composition_the_project_file_names_is_reported_with_that_files_name() {
    let dir = folder("named-composition");
    fs::write(dir.join("framesmith.yaml"), "composition: scenes/main.xml\n").unwrap();
    fs::create_dir_all(dir.join("scenes")).unwrap();
    fs::write(dir.join("scenes").join("main.xml"), composition("<rect colour=\"red\"/>")).unwrap();
    match Project::open(&dir) {
        Err(OpenError::Invalid(reports)) => {
            assert_eq!(reports.len(), 1, "{reports:?}");
            assert_eq!(reports[0].file(), "scenes/main.xml");
            assert_eq!(reports[0].problem().code(), Code::AttributeNotTaken);
            assert_eq!(reports[0].problem().line(), 2);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn two_elements_with_one_id_open() {
    let text = composition("<box id=\"a\"/>\n<box id=\"a\"/>");
    assert!(open("same-id", &text).is_ok());
}
