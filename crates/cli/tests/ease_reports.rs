//! The `ease` of a `<tween>` (`docs/spec/notation.md` section 8.4; `docs/spec/check-command.md`, the code `FS-0063`): an
//! easing the notation does not take is reported by `check` and refused by `render` on the attribute's line, with the list
//! of the easings taken; every easing it takes, and `out-back(n)`, is accepted by both.

mod common;

use std::fs;
use std::path::Path;

use common::{err, framesmith, out, project};

const GROUP: &str = "ease-reports";

/// A composition whose box, on line 2, holds a tween on line 3 and, on line 4, the tween's `ease` attribute.
fn composition(ease: &str) -> String {
    format!(
        "<composition version=\"1\" width=\"320\" height=\"120\" fps=\"30\" duration=\"1\" background=\"#ffffff\">\n\
         <box left=\"10\" top=\"10\" width=\"40\" height=\"40\" background=\"#ff0000\">\n\
         <tween duration=\"1\" to-x=\"200\"\n\
         ease=\"{ease}\"/>\n\
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

const TAKEN: &str = "linear, in-quad, out-quad, in-out-quad, in-cubic, out-cubic, in-out-cubic, in-quart, out-quart, \
in-out-quart, in-quint, out-quint, in-out-quint, in-sine, out-sine, in-out-sine, in-expo, out-expo, in-out-expo and \
out-back(n), with n a number";

/// Runs `check` and `render` on a project with `ease`; asserts both exit 1, print the same one coded line and write no film;
/// returns that line.
fn refused(name: &str, ease: &str) -> String {
    let (folder, cwd) = project(GROUP, name, &composition(ease));
    let checked = framesmith(&cwd, ["check", folder.to_str().unwrap()], &[]);
    let rendered = framesmith(&cwd, ["render", folder.to_str().unwrap(), "--backend", "cpu"], &[]);
    let coded = |printed: String| -> Vec<String> { printed.lines().filter(|line| line.contains("FS-")).map(str::to_string).collect() };
    assert_eq!(checked.status.code(), Some(1), "{}{}", out(&checked), err(&checked));
    assert_eq!(rendered.status.code(), Some(1), "{}{}", out(&rendered), err(&rendered));
    assert!(!has_a_film(&folder), "no film is written");
    let lines = coded(out(&checked));
    assert_eq!(lines, coded(out(&rendered)), "render reports what check reports");
    assert_eq!(lines.len(), 1, "{lines:?}");
    lines[0].clone()
}

#[test]
fn an_unknown_easing_is_reported_on_its_line_with_the_easings_taken() {
    // `wobble` is no easing of the list and no curve the support table names (`bounce` is S16).
    assert_eq!(
        refused("wobble", "wobble"),
        format!(
            "composition.xml, line 4: FS-0063: invalid: `ease=\"wobble\"` is not a known easing; the easings taken are {TAKEN}. \
             Write one of the easings the message lists, in lower case, such as `ease=\"out-cubic\"`."
        )
    );
}

#[test]
fn an_easing_in_capitals_or_an_out_back_without_a_number_is_not_known() {
    for (name, ease) in [
        ("capitals", "Out-Cubic"),
        ("out-back-empty", "out-back()"),
        ("out-back-word", "out-back(x)"),
        ("out-back-bare", "out-back"),
    ] {
        let line = refused(name, ease);
        assert!(
            line.starts_with(&format!("composition.xml, line 4: FS-0063: invalid: `ease=\"{ease}\"`")),
            "{line}"
        );
    }
}

#[test]
fn every_easing_taken_and_out_back_of_a_number_is_accepted_by_check_and_drawn_by_render() {
    let names = [
        "linear",
        "in-quad",
        "out-quad",
        "in-out-quad",
        "in-cubic",
        "out-cubic",
        "in-out-cubic",
        "in-quart",
        "out-quart",
        "in-out-quart",
        "in-quint",
        "out-quint",
        "in-out-quint",
        "in-sine",
        "out-sine",
        "in-out-sine",
        "in-expo",
        "out-expo",
        "in-out-expo",
        "out-back(1.7)",
        "out-back( 0 )",
    ];
    for name in names {
        let (folder, cwd) = project(
            GROUP,
            &format!("accepted-{}", name.replace(['(', ')', ' ', '.'], "")),
            &composition(name),
        );
        let checked = framesmith(&cwd, ["check", folder.to_str().unwrap()], &[]);
        assert_eq!(checked.status.code(), Some(0), "{name}: {}{}", out(&checked), err(&checked));
        if name == "out-cubic" || name == "out-back(1.7)" {
            let rendered = framesmith(&cwd, ["render", folder.to_str().unwrap(), "--backend", "cpu"], &[]);
            assert_eq!(rendered.status.code(), Some(0), "{name}: {}{}", out(&rendered), err(&rendered));
            assert!(has_a_film(&folder));
        }
    }
}
