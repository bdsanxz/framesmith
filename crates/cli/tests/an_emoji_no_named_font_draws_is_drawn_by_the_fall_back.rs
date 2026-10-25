//! An emoji in a text that no font of the composition draws is drawn by the fall-back of `docs/spec/text.md`, section
//! 2.4: on macOS `check` accepts it and `render` writes the film; where the fonts of the machine are not read, it is
//! drawn as the missing-character mark and `check` prints a note with the line of the `<text>`. The composition names
//! the test font, which is a file of the repository and draws no emoji.

mod common;

use std::fs;
use std::path::Path;

use common::err;
use common::framesmith;
use common::out;
use common::project as make;

const GROUP: &str = "an-emoji-no-named-font-draws-falls-back";

/// The composition: the emoji is on line 3, in a text set in the test font.
fn composition() -> String {
    format!(
        "<composition version=\"1\" width=\"320\" height=\"300\" fps=\"10\" duration=\"1\" background=\"#ffffff\">\n\
         <font src=\"fonts/framesmith-test-stroke-400.ttf\" family=\"Framesmith Test Stroke\"/>\n\
         <text left=\"10\" top=\"10\" width=\"300\" font-family=\"Framesmith Test Stroke\" font-size=\"20\">Smile {} now</text>\n\
         </composition>\n",
        '\u{1f600}'
    )
}

/// Whether the project's `renders` folder holds a film.
fn has_a_film(project: &std::path::Path) -> bool {
    fs::read_dir(project.join("renders"))
        .map(|entries| {
            entries
                .flatten()
                .any(|entry| entry.path().extension().is_some_and(|ext| ext == "mp4"))
        })
        .unwrap_or(false)
}

/// A project of the composition whose folder also holds the test font, as the text fixtures of the repository do. Each
/// test names its own folder: the tests run in parallel, and a shared folder is removed under the other one.
fn project_with_test_font(name: &str) -> (std::path::PathBuf, std::path::PathBuf) {
    let (project, cwd) = make(GROUP, name, &composition());
    fs::create_dir_all(project.join("fonts")).unwrap();
    let file = "framesmith-test-stroke-400.ttf";
    let from = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/fonts").join(file);
    fs::copy(&from, project.join("fonts").join(file)).unwrap();
    (project, cwd)
}

#[test]
fn check_accepts_an_emoji_that_no_named_font_draws() {
    let (project, cwd) = project_with_test_font("check");
    let checked = framesmith(&cwd, ["check", project.to_str().unwrap()], &[]);
    assert_eq!(checked.status.code(), Some(0), "{}{}", out(&checked), err(&checked));
    let printed = out(&checked);
    assert!(!printed.contains("FS-"), "{printed}");
    if cfg!(target_os = "macos") {
        assert!(!printed.contains("note:"), "{printed}");
    } else {
        assert!(
            printed.contains("note: composition.xml, line 3: <text> holds the character U+1F600"),
            "{printed}"
        );
    }
}

#[test]
fn render_draws_an_emoji_that_no_named_font_draws_and_writes_the_film() {
    let (project, cwd) = project_with_test_font("render");
    let rendered = framesmith(&cwd, ["render", project.to_str().unwrap(), "--backend", "cpu"], &[]);
    assert_eq!(rendered.status.code(), Some(0), "{}{}", out(&rendered), err(&rendered));
    assert!(!out(&rendered).contains("FS-"), "{}", out(&rendered));
    assert!(has_a_film(&project), "the film is written");
}
