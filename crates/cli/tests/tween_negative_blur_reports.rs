//! A `<tween>` that writes a blur or a brightness below 0 is reported by `check` and refused by `render`, each on its
//! attribute's line, with the code of a negative blur or brightness written on an attribute (`FS-0034`;
//! `docs/spec/motion.md`, Properties; `docs/spec/render-command.md`, section 4.5, rule 11). A value an easing carries below
//! 0 on its way is not reported: it is held at 0 (`docs/spec/motion.md`).

mod common;

use std::fs;
use std::path::Path;

use common::{err, framesmith, out, project};

const GROUP: &str = "tween-negative-blur-reports";

/// A composition whose box, on line 2, holds `tween` on line 3.
fn composition(tween: &str) -> String {
    format!(
        "<composition version=\"1\" width=\"320\" height=\"120\" fps=\"30\" duration=\"1\" background=\"#ffffff\">\n\
         <box left=\"10\" top=\"10\" width=\"40\" height=\"40\" background=\"#ff0000\">\n\
         {tween}\n\
         </box>\n\
         </composition>\n"
    )
}

fn has_a_film(project: &Path) -> bool {
    fs::read_dir(project.join("renders"))
        .map(|entries| {
            entries
                .flatten()
                .any(|entry| entry.path().extension().is_some_and(|ext| ext == "mp4"))
        })
        .unwrap_or(false)
}

/// Runs `check` and `render` on a project of `tween`; asserts both fail with the same exit status and write no film, and
/// returns the lines that name a code, as `check` printed them.
fn refused(name: &str, tween: &str) -> Vec<String> {
    let (folder, cwd) = project(GROUP, name, &composition(tween));
    let checked = framesmith(&cwd, ["check", folder.to_str().unwrap()], &[]);
    let rendered = framesmith(&cwd, ["render", folder.to_str().unwrap(), "--backend", "cpu"], &[]);
    assert_ne!(checked.status.code(), Some(0), "{}{}", out(&checked), err(&checked));
    assert_eq!(
        checked.status.code(),
        rendered.status.code(),
        "{}{}",
        out(&rendered),
        err(&rendered)
    );
    assert!(!has_a_film(&folder), "no film is written");
    let coded = |printed: String| -> Vec<String> { printed.lines().filter(|line| line.contains("FS-")).map(str::to_string).collect() };
    let lines = coded(out(&checked));
    assert_eq!(lines, coded(out(&rendered)), "render reports what check reports");
    lines
}

#[test]
fn a_tween_from_a_negative_blur_is_reported_on_its_line_as_below_0() {
    let lines = refused("from-blur", "<tween at=\"0\" duration=\"1\" from-blur=\"-2\" to-blur=\"0\"/>");
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].starts_with("composition.xml, line 3: FS-0034:"), "{}", lines[0]);
    assert!(lines[0].contains("`from-blur=\"-2\"` is below 0"), "{}", lines[0]);
}

#[test]
fn a_tween_to_a_negative_brightness_is_reported_on_its_line_as_below_0() {
    let lines = refused("to-brightness", "<tween at=\"0\" duration=\"1\" to-brightness=\"-0.5\"/>");
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].starts_with("composition.xml, line 3: FS-0034:"), "{}", lines[0]);
    assert!(lines[0].contains("`to-brightness=\"-0.5\"` is below 0"), "{}", lines[0]);
}
