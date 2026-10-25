//! `check` and `render` on a `font-family` list and on a character no font maps (`docs/spec/text.md`, sections 2.3 and
//! 2.4): a list whose first name is absent is drawn with the next name, and a character no font maps is drawn as the
//! missing-character mark, with a note from `check` naming the element's line. A note does not change the exit code.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use common::{err, framesmith, out, project as make};

const GROUP: &str = "font-family-list-and-missing-characters";

fn composition(family: &str, text: &str) -> String {
    format!(
        "<composition version=\"1\" width=\"320\" height=\"120\" fps=\"10\" duration=\"1\" background=\"#ffffff\">\n\
         <font src=\"fonts/framesmith-test-stroke-400.ttf\" family=\"Framesmith Test Stroke\"/>\n\
         <text left=\"10\" top=\"10\" width=\"300\" font-family=\"{family}\" font-size=\"30\" color=\"#000000\">{text}</text>\n\
         </composition>\n"
    )
}

fn project_with_test_font(name: &str, text: &str) -> (PathBuf, PathBuf) {
    let (project, cwd) = make(GROUP, name, text);
    fs::create_dir_all(project.join("fonts")).unwrap();
    let file = "framesmith-test-stroke-400.ttf";
    let from = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/fonts").join(file);
    fs::copy(&from, project.join("fonts").join(file)).unwrap();
    (project, cwd)
}

#[test]
fn a_list_whose_first_family_is_absent_is_accepted_and_drawn() {
    let text = composition("No Such Family, Framesmith Test Stroke", "AB");
    let (project, cwd) = project_with_test_font("absent-first", &text);
    let checked = framesmith(&cwd, ["check", project.to_str().unwrap()], &[]);
    assert_eq!(checked.status.code(), Some(0), "{}{}", out(&checked), err(&checked));
    assert!(!out(&checked).contains("FS-"), "{}", out(&checked));
    assert!(!out(&checked).contains("note:"), "{}", out(&checked));
    let rendered = framesmith(&cwd, ["render", project.to_str().unwrap(), "--backend", "cpu"], &[]);
    assert_eq!(rendered.status.code(), Some(0), "{}{}", out(&rendered), err(&rendered));
}

#[test]
fn a_character_no_font_maps_gives_a_note_with_the_line_of_its_text_and_is_drawn() {
    let text = composition("Framesmith Test Stroke", "A\u{0378}B");
    let (project, cwd) = project_with_test_font("missing", &text);
    let checked = framesmith(&cwd, ["check", project.to_str().unwrap()], &[]);
    assert_eq!(checked.status.code(), Some(0), "{}{}", out(&checked), err(&checked));
    let printed = out(&checked);
    assert!(!printed.contains("FS-"), "{printed}");
    let note = printed
        .lines()
        .find(|line| line.starts_with("note: "))
        .unwrap_or_else(|| panic!("{printed}"));
    assert_eq!(
        note,
        "note: composition.xml, line 3: <text> holds the character U+0378, which no font of its list and no font of the machine maps: it is drawn as the missing-character mark. No rule names a font for it in this version, since the stand-in fonts are chosen in Sprint 5"
    );
    let note_at = printed.find("note: ").unwrap();
    let ready_at = printed.find("Ready").unwrap_or_else(|| panic!("{printed}"));
    assert!(note_at < ready_at, "the note comes before the closing line: {printed}");
    let rendered = framesmith(&cwd, ["render", project.to_str().unwrap(), "--backend", "cpu"], &[]);
    assert_eq!(rendered.status.code(), Some(0), "{}{}", out(&rendered), err(&rendered));
}
