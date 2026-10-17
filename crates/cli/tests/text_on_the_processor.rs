//! A composition with text: `check` accepts it, `render --backend cpu` draws and writes it, and a family that no font
//! of the composition declares is reported by `check` and refused by `render` with the line of the `<text>`
//! (`docs/spec/text.md`, sections 2.3 and 9). The card does not draw text in this version, so its refusal is a test
//! for the reference machine, which has the card.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use common::err;
use common::framesmith;
use common::out;
use common::project as make;

const GROUP: &str = "text-on-the-processor";

/// The composition of a text fixture, read from the fixtures folder.
fn fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name)
        .join("composition.xml");
    fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// A project of `text` whose folder also holds the two test font files, as a text fixture does.
fn project_with_fonts(name: &str, text: &str) -> (PathBuf, PathBuf) {
    let (project, cwd) = make(GROUP, name, text);
    fs::create_dir_all(project.join("fonts")).unwrap();
    for file in ["framesmith-test-stroke-400.ttf", "framesmith-test-stroke-700.ttf"] {
        let from = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/fonts").join(file);
        fs::copy(&from, project.join("fonts").join(file)).unwrap();
    }
    (project, cwd)
}

#[test]
fn check_accepts_a_composition_whose_texts_it_can_place() {
    let (project, cwd) = project_with_fonts("check-accepts", &fixture("text-colour-and-weight"));
    let output = framesmith(&cwd, ["check", project.to_str().unwrap()], &[]);
    assert_eq!(output.status.code(), Some(0), "{}{}", out(&output), err(&output));
    assert!(!out(&output).contains("FS-"), "{}", out(&output));
}

#[test]
fn render_with_the_processor_draws_a_text_and_writes_its_film() {
    let (project, cwd) = project_with_fonts("processor-film", &fixture("text-colour-and-weight"));
    let output = framesmith(&cwd, ["render", project.to_str().unwrap(), "--backend", "cpu"], &[]);
    assert_eq!(output.status.code(), Some(0), "{}{}", out(&output), err(&output));
    let printed = out(&output);
    assert!(printed.contains("renderer: processor"), "{printed}");
    assert!(printed.contains("frames: 10"), "{printed}");
}

#[test]
fn a_family_no_font_declares_is_reported_by_check_and_refused_by_render_with_its_line() {
    let text = fixture("text-colour-and-weight").replace(
        "font-family=\"Framesmith Test Stroke\" font-size=\"30\" font-weight=\"400\"",
        "font-family=\"sans-serif\" font-size=\"30\"",
    );
    let (project, cwd) = project_with_fonts("family-not-declared", &text);
    let checked = framesmith(&cwd, ["check", project.to_str().unwrap()], &[]);
    assert_eq!(checked.status.code(), Some(2), "{}{}", out(&checked), err(&checked));
    let printed = out(&checked);
    assert!(printed.contains("line 7: FS-0035"), "{printed}");
    assert!(printed.contains("no font of the composition is named sans-serif"), "{printed}");

    let rendered = framesmith(&cwd, ["render", project.to_str().unwrap(), "--backend", "cpu"], &[]);
    assert_eq!(rendered.status.code(), Some(2), "{}{}", out(&rendered), err(&rendered));
    assert!(out(&rendered).contains("line 7: FS-0035"), "{}", out(&rendered));
    assert!(!out(&rendered).contains("Rendered "), "{}", out(&rendered));
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_the_card_refuses_a_text_before_any_frame_and_writes_no_file() {
    let (project, cwd) = project_with_fonts("card-refuses-text", &fixture("text-colour-and-weight"));
    let output = framesmith(&cwd, ["render", project.to_str().unwrap(), "--backend", "gpu"], &[]);
    assert_eq!(output.status.code(), Some(2), "{}{}", out(&output), err(&output));
    let printed = out(&output);
    assert!(printed.contains("line 7: FS-0035"), "{printed}");
    assert!(printed.contains("is not drawn by the card renderer"), "{printed}");
    assert!(printed.contains("--backend cpu"), "{printed}");
    assert!(!printed.contains("Rendered "), "{printed}");
}
