//! A `<set>` that cannot be applied is reported by `check` and refused by `render`, each with its line
//! (`docs/spec/notation.md` section 8.3; `docs/spec/check-command.md`, the codes `FS-0061` and `FS-0062`;
//! `docs/spec/render-command.md` section 3): a set with no property, a set on a property that cannot be animated, a set on
//! a property its element does not take, and a set on an animated property this version does not move. A set that can be
//! applied is accepted by both.

mod common;

use std::fs;
use std::path::Path;

use common::{err, framesmith, out, project};

const GROUP: &str = "set-reports";

/// A composition whose box, on line 2, holds `set` on line 3.
fn composition(set: &str) -> String {
    format!(
        "<composition version=\"1\" width=\"320\" height=\"120\" fps=\"30\" duration=\"1\" background=\"#ffffff\">\n\
         <box left=\"10\" top=\"10\" width=\"40\" height=\"40\" background=\"#ff0000\">\n\
         {set}\n\
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

/// Runs `check` and `render` on a project of `set`; asserts both exit with `status`, print the same lines that name a
/// code, and write no film; returns those lines.
fn refused(name: &str, set: &str, status: i32) -> Vec<String> {
    let (folder, cwd) = project(GROUP, name, &composition(set));
    let checked = framesmith(&cwd, ["check", folder.to_str().unwrap()], &[]);
    let rendered = framesmith(&cwd, ["render", folder.to_str().unwrap(), "--backend", "cpu"], &[]);
    let coded = |printed: String| -> Vec<String> { printed.lines().filter(|line| line.contains("FS-")).map(str::to_string).collect() };
    assert_eq!(checked.status.code(), Some(status), "{}{}", out(&checked), err(&checked));
    assert_eq!(rendered.status.code(), Some(status), "{}{}", out(&rendered), err(&rendered));
    assert!(!has_a_film(&folder), "no film is written");
    let lines = coded(out(&checked));
    assert_eq!(lines, coded(out(&rendered)), "render reports what check reports");
    lines
}

#[test]
fn a_set_with_no_property_is_reported_on_its_line() {
    let lines = refused("no-property", "<set at=\"1\"/>", 1);
    assert_eq!(
        lines,
        ["composition.xml, line 3: FS-0061: invalid: <set> changes no property; a set needs at least one property, such as `x`. Write at least one property on the set, such as `x=\"300\"`."]
    );
}

#[test]
fn a_set_on_a_property_that_cannot_be_animated_is_reported_once_on_its_line() {
    let lines = refused("width", "<set at=\"1\" width=\"300\"/>", 1);
    assert_eq!(lines.len(), 1, "one report, not a second for the set with no property: {lines:?}");
    assert!(
        lines[0].starts_with("composition.xml, line 3: FS-0030: invalid: <set> has no attribute `width`"),
        "{}",
        lines[0]
    );
}

#[test]
fn a_set_on_a_property_its_element_does_not_take_is_reported_on_its_line() {
    let lines = refused("count-on-a-box", "<set at=\"1\" count=\"5\"/>", 1);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(
        lines[0].starts_with("composition.xml, line 3: FS-0062: invalid: `count` changes `count`, which <box> does not take; a set changes only the properties of the element it moves."),
        "{}",
        lines[0]
    );
}

#[test]
fn a_set_on_an_animated_property_this_version_does_not_move_is_refused_on_its_line() {
    let lines = refused("blur", "<set at=\"1\" blur=\"4\"/>", 2);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(
        lines[0].starts_with("composition.xml, line 3: FS-0035: unsupported: <set> changes `blur`, which this version does not move: it moves x, y, scale, scale-x, rotation, skew-y, opacity, rotate-x and rotate-y."),
        "{}",
        lines[0]
    );
}

#[test]
fn a_set_of_the_seven_properties_is_accepted_by_check_and_drawn_by_render() {
    let (folder, cwd) = project(
        GROUP,
        "accepted",
        &composition("<set at=\"0.5\" x=\"100\" y=\"10\" scale=\"1.5\" scale-x=\"0.5\" rotation=\"30\" skew-y=\"10\" opacity=\"0.5\"/>"),
    );
    let checked = framesmith(&cwd, ["check", folder.to_str().unwrap()], &[]);
    assert_eq!(checked.status.code(), Some(0), "{}{}", out(&checked), err(&checked));
    let rendered = framesmith(&cwd, ["render", folder.to_str().unwrap(), "--backend", "cpu"], &[]);
    assert_eq!(rendered.status.code(), Some(0), "{}{}", out(&rendered), err(&rendered));
    assert!(has_a_film(&folder));
}
