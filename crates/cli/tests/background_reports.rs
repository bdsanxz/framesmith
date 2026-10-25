//! The reports of backgrounds that are not drawn (`docs/spec/render-command.md`, section 4.6, rule 10;
//! `docs/spec/check-command.md`): a list of settings with 2 entries for 3 layers (`FS-0065`), a setting on a box with no
//! `background` (`FS-0066`), a gradient with one stop (`FS-0064`) and a conic gradient, which this version does not draw
//! (`FS-0035`). `check` reports each on its line, `render` refuses them the same way and writes no film.

mod common;

use std::fs;
use std::path::Path;

use common::{err, framesmith, out, project};

const GROUP: &str = "background-reports";

/// A composition with the four backgrounds that are not drawn, one box on each of lines 2 to 5.
const COMPOSITION: &str = r##"<composition version="1" width="320" height="120" fps="30" duration="1" background="#ffffff">
<box left="0" top="0" width="40" height="40" background="#ff0000, #00ff00, #0000ff" background-size="10 10, 20 20"/>
<box left="50" top="0" width="40" height="40" background-size="10 10"/>
<box left="100" top="0" width="40" height="40" background="linear-gradient(90deg, #ff0000 0%)"/>
<box left="150" top="0" width="40" height="40" background="conic-gradient(#ff0000 0%, #0000ff 100%)"/>
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
fn backgrounds_not_drawn_are_each_reported_on_their_line_by_check_and_render() {
    let (folder, cwd) = project(GROUP, "four", COMPOSITION);
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
    let printed = out(&checked);
    let wanted = [
        ("line 2: FS-0065", "`background-size=\"10 10, 20 20\"` has 2 entries for 3 layers"),
        (
            "line 3: FS-0066",
            "`background-size=\"10 10\"` is written on <box>, which has no `background`",
        ),
        ("line 4: FS-0064", "a gradient needs two stops or more"),
        ("line 5: FS-0035", "holds a conic gradient, which this version does not draw"),
    ];
    for (place, words) in wanted {
        assert!(
            lines.iter().any(|line| line.contains(place)) && printed.contains(words),
            "{place} with {words:?} in:\n{printed}"
        );
    }
    assert_eq!(lines.len(), 4, "{lines:?}");
}

#[test]
fn every_background_drawn_is_accepted_by_check() {
    let accepted = r##"<composition version="1" width="320" height="120" fps="30" duration="1" background="#ffffff">
<box left="0" top="0" width="40" height="40" background="#ff000080, linear-gradient(#000000 0%, #ffffff 100%)"/>
<box left="50" top="0" width="40" height="40" background="radial-gradient(circle 10 at 25% 75%, #ff0000 0%, #0000ff 10), #ffffff" background-size="20 20, auto" background-position="50% 50%, 0 0" background-repeat="no-repeat"/>
<box left="100" top="0" width="40" height="40" background="radial-gradient(ellipse 50% 30%, #ff0000 0%, #0000ff 100%)"/>
<box left="150" top="0" width="40" height="40" background="#00ff00" background-repeat="repeat"/>
</composition>
"##;
    let (folder, cwd) = project(GROUP, "accepted", accepted);
    let checked = framesmith(&cwd, ["check", folder.to_str().unwrap()], &[]);
    assert_eq!(checked.status.code(), Some(0), "{}{}", out(&checked), err(&checked));
}
