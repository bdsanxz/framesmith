//! A composition with text: `check` accepts it, `render --backend cpu` draws and writes it, and a family that no font
//! of the composition declares is reported by `check` and refused by `render` with the line of the `<text>`
//! (`docs/spec/text.md`, sections 2.3 and 9). The card draws text too, and the real card is tested here, on the reference
//! machine. A renderer that draws no text is refused on every machine, with a started double, in `text_refused_by_a_renderer_without_text.rs`.

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

/// A name no font of the composition declares is looked up among the generic names and the fonts of the machine
/// (`docs/spec/text.md`, sections 2.3 and 2.7): on macOS `sans-serif` is Helvetica, so the text is drawn and nothing is
/// reported.
#[cfg(target_os = "macos")]
#[test]
fn a_generic_family_no_font_declares_is_drawn_from_the_machine() {
    let text = fixture("text-colour-and-weight").replace(
        "font-family=\"Framesmith Test Stroke\" font-size=\"30\" font-weight=\"400\"",
        "font-family=\"sans-serif\" font-size=\"30\"",
    );
    let (project, cwd) = project_with_fonts("family-not-declared", &text);
    let checked = framesmith(&cwd, ["check", project.to_str().unwrap()], &[]);
    assert_eq!(checked.status.code(), Some(0), "{}{}", out(&checked), err(&checked));
    assert!(!out(&checked).contains("FS-"), "{}", out(&checked));

    let rendered = framesmith(&cwd, ["render", project.to_str().unwrap(), "--backend", "cpu"], &[]);
    assert_eq!(rendered.status.code(), Some(0), "{}{}", out(&rendered), err(&rendered));
    assert!(out(&rendered).contains("frames: 10"), "{}", out(&rendered));
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_the_card_draws_a_text_and_writes_its_film() {
    let (project, cwd) = project_with_fonts("card-draws-text", &fixture("text-colour-and-weight"));
    let output = framesmith(&cwd, ["render", project.to_str().unwrap(), "--backend", "gpu"], &[]);
    assert_eq!(output.status.code(), Some(0), "{}{}", out(&output), err(&output));
    let printed = out(&output);
    assert!(printed.contains("renderer: card"), "{printed}");
    assert!(printed.contains("frames: 10"), "{printed}");
    assert!(!printed.contains("FS-"), "{printed}");
}
