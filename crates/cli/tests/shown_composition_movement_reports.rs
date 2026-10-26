//! `framesmith check` and `framesmith render` on a `<composition src>` that moves (`docs/spec/render-command.md`, section
//! 4.5, rule 11): its movement attributes and a `<tween>` or `<set>` inside it are no longer listed and are drawn, while an
//! element of another kind inside it, and the motion of a `<box>` that holds one, are still refused on their lines.

mod common;

use std::fs;
use std::path::Path;

use common::{err, framesmith, out, project};

const SHOWN: &str = r##"<composition version="1" width="40" height="30" fps="30" duration="1" background="#000000">
<box left="0" top="0" width="20" height="30" background="#ff0000"/>
</composition>
"##;

fn has_a_film(project: &Path) -> bool {
    fs::read_dir(project.join("renders"))
        .map(|entries| {
            entries
                .flatten()
                .any(|entry| entry.path().extension().is_some_and(|ext| ext == "mp4"))
        })
        .unwrap_or(false)
}

fn coded(printed: &str) -> Vec<String> {
    printed.lines().filter(|line| line.contains("FS-")).map(str::to_string).collect()
}

/// The project `name` holding `body` in a root of 160 by 100, with `shown.xml` beside it.
fn with_shown(name: &str, body: &str) -> (std::path::PathBuf, std::path::PathBuf) {
    let text = format!(
        "<composition version=\"1\" width=\"160\" height=\"100\" fps=\"30\" duration=\"1\" background=\"#000000\">\n{body}\n</composition>\n"
    );
    let (folder, cwd) = project("shown-composition-movement-reports", name, &text);
    fs::write(folder.join("shown.xml"), SHOWN).unwrap();
    (folder, cwd)
}

#[test]
fn a_moving_shown_composition_is_valid_and_rendered() {
    let (folder, cwd) = with_shown(
        "moves",
        "<composition src=\"shown.xml\" left=\"10\" top=\"20\" opacity=\"0.5\" rotation=\"10\" perspective=\"400\" rotate-y=\"30\" transform-origin=\"0% 0%\">\n\
         <tween duration=\"1\" to-x=\"60\" to-scale=\"2\"/>\n<set at=\"0.5\" skew-y=\"10\"/>\n</composition>",
    );
    let checked = framesmith(&cwd, ["check", folder.to_str().unwrap()], &[]);
    assert_eq!(checked.status.code(), Some(0), "{}{}", out(&checked), err(&checked));
    assert_eq!(out(&checked), "Ready: the project is valid, 30 frames, 160x100, 30 fps\n");
    let rendered = framesmith(&cwd, ["render", folder.to_str().unwrap(), "--backend", "cpu"], &[]);
    assert_eq!(rendered.status.code(), Some(0), "{}{}", out(&rendered), err(&rendered));
    assert!(has_a_film(&folder), "a film is written");
}

/// Runs `check` and `render` on the project and returns its coded lines, after asserting both exit with `code` and agree.
fn refused(folder: &Path, cwd: &Path, code: i32) -> Vec<String> {
    let checked = framesmith(cwd, ["check", folder.to_str().unwrap()], &[]);
    let rendered = framesmith(cwd, ["render", folder.to_str().unwrap(), "--backend", "cpu"], &[]);
    assert_eq!(checked.status.code(), Some(code), "{}{}", out(&checked), err(&checked));
    assert_eq!(rendered.status.code(), Some(code), "{}{}", out(&rendered), err(&rendered));
    assert!(!has_a_film(folder), "no film is written");
    let lines = coded(&out(&checked));
    assert_eq!(lines, coded(&out(&rendered)), "render reports what check reports");
    lines
}

#[test]
fn another_element_inside_a_shown_composition_is_still_invalid() {
    let (folder, cwd) = with_shown(
        "box-inside",
        "<composition src=\"shown.xml\" left=\"10\" top=\"20\">\n\
         <box left=\"0\" top=\"0\" width=\"4\" height=\"4\" background=\"#ffffff\"/>\n</composition>",
    );
    let lines = refused(&folder, &cwd, 1);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(
        lines[0].contains("line 3: FS-0031") && lines[0].contains("<box> is not known inside <composition src>"),
        "{lines:?}"
    );
}

#[test]
fn a_moving_box_that_holds_a_shown_composition_is_still_refused() {
    let (folder, cwd) = with_shown(
        "box-holds",
        "<box left=\"80\" top=\"0\" width=\"60\" height=\"60\" x=\"5\">\n\
         <composition src=\"shown.xml\" left=\"0\" top=\"0\"/>\n</box>",
    );
    let lines = refused(&folder, &cwd, 2);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(
        lines[0].contains("line 2: FS-0035") && lines[0].contains("holds a <composition src>"),
        "{lines:?}"
    );
}
