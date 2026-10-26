//! A perspective of 0 or below (`docs/spec/render-command.md`, section 4.5, rule 10; `docs/spec/check-command.md`):
//! `check` reports it as `FS-0053` on the attribute's line, `render` refuses it the same way and writes no film.

mod common;

use std::fs;
use std::path::Path;

use common::{err, framesmith, out, project};

const COMPOSITION: &str = r##"<composition version="1" width="200" height="100" fps="30" duration="1" background="#ffffff">
<box left="0" top="0" width="40" height="40" background="#ff0000" perspective="0" rotate-y="30"/>
<box left="50" top="0" width="40" height="40" background="#ff0000"
     perspective="-200" rotate-x="30"/>
<box left="100" top="0" width="40" height="40" background="#ff0000" perspective="300" rotate-x="30"/>
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
fn a_perspective_not_above_zero_is_reported_on_its_line_by_check_and_render() {
    let (folder, cwd) = project("perspective-reports", "two", COMPOSITION);
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
        lines[0].contains("line 2: FS-0053") && lines[0].contains("`perspective=\"0\"` is not above 0"),
        "{lines:?}"
    );
    assert!(
        lines[1].contains("line 4: FS-0053") && lines[1].contains("`perspective=\"-200\"` is not above 0"),
        "{lines:?}"
    );
}

#[test]
fn a_perspective_above_zero_is_accepted_by_check() {
    let (folder, cwd) = project(
        "perspective-reports",
        "accepted",
        &COMPOSITION.replace("\"0\"", "\"1\"").replace("\"-200\"", "\"0.5\""),
    );
    let checked = framesmith(&cwd, ["check", folder.to_str().unwrap()], &[]);
    assert_eq!(checked.status.code(), Some(0), "{}{}", out(&checked), err(&checked));
}
