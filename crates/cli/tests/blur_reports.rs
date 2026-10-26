//! A negative blur and a negative brightness (`docs/spec/render-command.md`, section 4.5, rule 11;
//! `docs/spec/check-command.md`): `check` reports each as `FS-0034` on its attribute's line, `render` refuses them the same
//! way and writes no film.

mod common;

use std::fs;
use std::path::Path;

use common::{err, framesmith, out, project};

const COMPOSITION: &str = r##"<composition version="1" width="200" height="100" fps="30" duration="1" background="#ffffff">
<box left="0" top="0" width="40" height="40" background="#ff0000" blur="-2"/>
<text left="50" top="0" font-size="20" color="#000000"
      brightness="-0.5">x</text>
<box left="100" top="0" width="40" height="40" background="#ff0000" blur="0" brightness="0"/>
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

#[test]
fn a_negative_blur_and_a_negative_brightness_are_each_reported_on_their_line_by_check_and_render() {
    let (folder, cwd) = project("blur-reports", "negative", COMPOSITION);
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
    let lines = coded(&out(&checked));
    assert_eq!(lines, coded(&out(&rendered)), "render reports what check reports");
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert!(
        lines[0].contains("line 2: FS-0034") && lines[0].contains("`blur=\"-2\"` is below 0"),
        "{lines:?}"
    );
    assert!(
        lines[1].contains("line 4: FS-0034") && lines[1].contains("`brightness=\"-0.5\"` is below 0"),
        "{lines:?}"
    );
}

#[test]
fn a_blur_and_a_brightness_of_0_or_more_are_accepted_by_check() {
    let accepted = COMPOSITION.replace("\"-2\"", "\"2.5\"").replace("\"-0.5\"", "\"1.5\"");
    let (folder, cwd) = project("blur-reports", "accepted", &accepted);
    let checked = framesmith(&cwd, ["check", folder.to_str().unwrap()], &[]);
    assert_eq!(checked.status.code(), Some(0), "{}{}", out(&checked), err(&checked));
}
