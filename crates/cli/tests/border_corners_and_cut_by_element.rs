//! `border`, `border-radius` and `overflow` on a `<text>` or a `<span>`, and `opacity`, `blur` and `brightness` on a `<span>`,
//! are refused as unsupported, each on its line, and no film is written: `check` and `render` print the same report
//! (`docs/spec/notation.md`, section 14, entry S35). On a `<box>`, and `opacity`, `blur` and `brightness` on a `<text>`, they
//! are drawn and the film is written. An animated `blur` is refused as unsupported (entry S37).

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;

use common::{err, framesmith, out, project};

const GROUP: &str = "border-corners-and-cut-by-element";

/// The head of each composition with a text: its canvas, and the font that the text draws with.
const HEAD: &str = r##"<composition version="1" width="160" height="60" fps="10" duration="0.5" background="#000000">
<font src="fonts/framesmith-test-stroke-400.ttf" family="Framesmith Test Stroke"/>
"##;

/// A project whose `composition.xml` is `HEAD`, then `body` on line 3, then the closing tag, with the test font in `fonts/`.
fn text_project(name: &str, body: &str) -> (PathBuf, PathBuf) {
    let (folder, cwd) = project(GROUP, name, &format!("{HEAD}{body}\n</composition>\n"));
    fs::create_dir_all(folder.join("fonts")).unwrap();
    let file = "framesmith-test-stroke-400.ttf";
    let from = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/fonts").join(file);
    fs::copy(&from, folder.join("fonts").join(file)).unwrap();
    (folder, cwd)
}

/// A project whose `composition.xml` is a canvas, a comment on line 2, then `body` on line 3, then the closing tag.
fn box_project(name: &str, body: &str) -> (PathBuf, PathBuf) {
    project(
        GROUP,
        name,
        &format!(
            "<composition version=\"1\" width=\"160\" height=\"60\" fps=\"10\" duration=\"0.5\" background=\"#000000\">\n<!-- a box -->\n{body}\n</composition>\n"
        ),
    )
}

fn has_a_film(folder: &Path) -> bool {
    fs::read_dir(folder.join("renders"))
        .map(|entries| {
            entries
                .flatten()
                .any(|entry| entry.path().extension().is_some_and(|ext| ext == "mp4"))
        })
        .unwrap_or(false)
}

/// Runs `check` and `render` on the project, and returns both outputs.
fn check_and_render(folder: &Path, cwd: &Path) -> (Output, Output) {
    let checked = framesmith(cwd, ["check", folder.to_str().unwrap()], &[]);
    let rendered = framesmith(cwd, ["render", folder.to_str().unwrap(), "--backend", "cpu"], &[]);
    (checked, rendered)
}

fn report_lines(printed: &Output) -> Vec<String> {
    out(printed)
        .lines()
        .filter(|line| line.contains("FS-"))
        .map(str::to_string)
        .collect()
}

#[test]
fn a_look_attribute_on_a_text_is_refused_by_check_and_render_on_its_line_and_no_film_is_written() {
    let forms = [
        ("border", "border=\"2 #ff0000\""),
        ("border-radius", "border-radius=\"2\""),
        ("overflow", "overflow=\"hidden\""),
    ];
    for (name, attribute) in forms {
        let body = format!("<text left=\"10\" top=\"10\" font-family=\"Framesmith Test Stroke\" font-size=\"20\" {attribute}>AB</text>");
        let (folder, cwd) = text_project(&format!("text-{name}"), &body);
        let (checked, rendered) = check_and_render(&folder, &cwd);
        assert_eq!(checked.status.code(), Some(2), "{name}: {}{}", out(&checked), err(&checked));
        assert_eq!(rendered.status.code(), Some(2), "{name}: {}{}", out(&rendered), err(&rendered));
        assert!(
            err(&checked).is_empty() && err(&rendered).is_empty(),
            "{name}: a command wrote to the error stream"
        );
        assert!(!has_a_film(&folder), "{name}: no film is written");
        let lines = report_lines(&checked);
        assert_eq!(lines, report_lines(&rendered), "{name}: render reports what check reports");
        assert_eq!(lines.len(), 1, "{name}: {lines:?}");
        assert!(
            lines[0].starts_with(&format!(
                "composition.xml, line 3: FS-0035: unsupported: <text> has `{attribute}`, which this version does not support (support table S35: "
            )),
            "{name}: {}",
            lines[0]
        );
    }
}

#[test]
fn a_box_takes_the_border_the_corners_the_cut_and_the_look_and_movement_and_is_drawn() {
    let body = "<box left=\"10\" top=\"10\" width=\"40\" height=\"30\" background=\"#ff0000\" border=\"2 #ffffff\" border-radius=\"4\" overflow=\"hidden\" opacity=\"0.3\" blur=\"2\" brightness=\"0.5\"/>";
    let (folder, cwd) = box_project("box-looks", body);
    let (checked, rendered) = check_and_render(&folder, &cwd);
    assert_eq!(checked.status.code(), Some(0), "{}{}", out(&checked), err(&checked));
    assert_eq!(rendered.status.code(), Some(0), "{}{}", out(&rendered), err(&rendered));
    assert!(has_a_film(&folder));
}

#[test]
fn a_text_takes_opacity_blur_and_brightness_and_is_drawn() {
    let body = "<text left=\"10\" top=\"10\" font-family=\"Framesmith Test Stroke\" font-size=\"20\" opacity=\"0.3\" blur=\"2\" brightness=\"0.5\">AB</text>";
    let (folder, cwd) = text_project("text-looks", body);
    let (checked, rendered) = check_and_render(&folder, &cwd);
    assert_eq!(checked.status.code(), Some(0), "{}{}", out(&checked), err(&checked));
    assert_eq!(rendered.status.code(), Some(0), "{}{}", out(&rendered), err(&rendered));
    assert!(has_a_film(&folder));
}

#[test]
fn an_animated_blur_or_brightness_is_refused_by_check_and_render_on_its_line_and_no_film_is_written() {
    let forms = [("blur", "to-blur=\"4\""), ("brightness", "to-brightness=\"0.5\"")];
    for (name, attribute) in forms {
        let body = format!(
            "<box left=\"10\" top=\"10\" width=\"40\" height=\"30\" background=\"#ff0000\"><tween duration=\"1\" {attribute}/></box>"
        );
        let (folder, cwd) = box_project(&format!("animated-{name}"), &body);
        let (checked, rendered) = check_and_render(&folder, &cwd);
        assert_eq!(checked.status.code(), Some(2), "{name}: {}{}", out(&checked), err(&checked));
        assert_eq!(rendered.status.code(), Some(2), "{name}: {}{}", out(&rendered), err(&rendered));
        assert!(!has_a_film(&folder), "{name}: no film is written");
        let lines = report_lines(&checked);
        assert_eq!(lines, report_lines(&rendered), "{name}: render reports what check reports");
        assert_eq!(lines.len(), 1, "{name}: {lines:?}");
        assert!(
            lines[0].starts_with(&format!(
                "composition.xml, line 3: FS-0035: unsupported: <tween> changes `{name}`, which this version does not support (support table S37: "
            )),
            "{name}: {}",
            lines[0]
        );
    }
}
