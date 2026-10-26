//! `framesmith check` and `framesmith render` report a border, a corner radius or an `overflow` in a form the notation does
//! not list, each on the line of its attribute with the forms taken, and a border by side (S20) and a corner written as an ellipse (S33) as unsupported, with the same
//! reports and the same exit status; a box with a border, rounded corners and a cut is ready for both
//! (`docs/spec/box-edges.md`, sections 1 and 6).

mod common;

use std::ffi::OsStr;

use common::{err, framesmith, out, project};

/// Runs `check` and `render` on a composition whose root opens on line 1 and whose box stands on lines 2 and 3, with the
/// attribute on line 3. Gives the status and the printed reports of each.
fn check_and_render(name: &str, attribute: &str) -> (i32, String, i32, String) {
    let text = format!(
        "<composition version=\"1\" width=\"96\" height=\"64\" duration=\"1\">\n  <box left=\"8\" top=\"8\" width=\"40\" height=\"30\" background=\"#ffffff\"\n       {attribute}/>\n</composition>\n"
    );
    let (folder, cwd) = project("border-and-corner-reports", name, &text);
    let film = cwd.join("film.mp4");
    let check = framesmith(&cwd, [OsStr::new("check"), folder.as_os_str()], &[("NO_COLOR", "1")]);
    let render = framesmith(
        &cwd,
        [
            OsStr::new("render"),
            folder.as_os_str(),
            OsStr::new("--backend"),
            OsStr::new("cpu"),
            OsStr::new("--output"),
            film.as_os_str(),
        ],
        &[("NO_COLOR", "1")],
    );
    assert_eq!(err(&check), "", "{name}: check wrote to the error stream");
    assert_eq!(err(&render), "", "{name}: render wrote to the error stream");
    assert!(!film.exists(), "{name}: a film was written for a project that is refused");
    (
        check.status.code().unwrap(),
        out(&check),
        render.status.code().unwrap(),
        out(&render),
    )
}

/// The one report of a refused composition: its line, its code and its message, then the closing line of its kind.
fn assert_one_report(name: &str, attribute: &str, code: &str, words: &[&str], exit: i32) {
    let (check_status, check, render_status, render) = check_and_render(name, attribute);
    assert_eq!(check_status, exit, "{name}: check: {check}");
    assert_eq!(render_status, exit, "{name}: render: {render}");
    assert_eq!(check, render, "{name}: check and render print different reports");
    let lines: Vec<&str> = check.lines().collect();
    assert_eq!(lines.len(), 2, "{name}: one report and one closing line: {lines:?}");
    assert!(
        lines[0].starts_with(&format!("composition.xml, line 3: {code}: ")),
        "{name}: {}",
        lines[0]
    );
    for word in words {
        assert!(lines[0].contains(word), "{name}: the report does not say {word:?}: {}", lines[0]);
    }
    let closing = if exit == 2 { "Unsupported: " } else { "Invalid: " };
    assert!(lines[1].starts_with(closing), "{name}: {}", lines[1]);
    println!("{name}: {}", lines[0]);
}

#[test]
fn a_border_in_no_listed_form_is_reported_with_its_line_and_the_forms_taken() {
    let forms = [
        ("unit", "border=\"2px #ff0000\""),
        ("colour-first", "border=\"#ff0000 2\""),
        ("other-word", "border=\"2 solid #ff0000\""),
        ("width-alone", "border=\"2\""),
        ("negative", "border=\"-2 #ff0000\""),
        ("named-colour", "border=\"2 red\""),
    ];
    for (name, attribute) in forms {
        assert_one_report(
            name,
            attribute,
            "FS-0067",
            &["is not a width and a colour", "`width colour`", "`width double colour`"],
            1,
        );
    }
}

#[test]
fn a_negative_radius_and_an_unknown_overflow_are_reported_with_their_line() {
    assert_one_report("radius-negative", "border-radius=\"-4\"", "FS-0034", &["must be 0 or more"], 1);
    assert_one_report("radius-word", "border-radius=\"round\"", "FS-0032", &["is not a number"], 1);
    assert_one_report(
        "overflow-clip",
        "overflow=\"clip\"",
        "FS-0039",
        &["is not one of: visible, hidden"],
        1,
    );
}

#[test]
fn a_border_by_side_is_reported_as_unsupported_with_its_line() {
    for name in [
        "border-top",
        "border-right",
        "border-bottom",
        "border-left",
        "border-width",
        "border-color",
        "border-style",
    ] {
        assert_one_report(
            name,
            &format!("{name}=\"2 #ff0000\""),
            "FS-0035",
            &[&format!("`{name}`"), "a border by side"],
            2,
        );
    }
}

#[test]
fn a_corner_written_as_an_ellipse_is_reported_as_unsupported_with_its_line() {
    for (name, value) in [("ellipse-slash", "24 / 12"), ("ellipse-two", "24 12"), ("ellipse-tight", "24/12")] {
        assert_one_report(
            name,
            &format!("border-radius=\"{value}\""),
            "FS-0035",
            &[&format!("`border-radius=\"{value}\"`"), "a corner written as an ellipse"],
            2,
        );
    }
    // Three numbers are no ellipse: a `border-radius` that is not a number.
    assert_one_report("radius-three", "border-radius=\"24 12 6\"", "FS-0032", &["is not a number"], 1);
}

#[test]
fn a_box_with_a_border_rounded_corners_and_a_cut_is_ready_for_check_and_render() {
    let attribute = "border=\"9 double #ff0000\" border-radius=\"12\" overflow=\"hidden\"";
    let text = format!(
        "<composition version=\"1\" width=\"96\" height=\"64\" duration=\"0.1\">\n  <box left=\"8\" top=\"8\" width=\"40\" height=\"30\" background=\"#ffffff\" {attribute}>\n    <box left=\"-4\" top=\"10\" width=\"60\" height=\"8\" background=\"#0000ff\"/>\n  </box>\n</composition>\n"
    );
    let (folder, cwd) = project("border-and-corner-reports", "ready", &text);
    let film = cwd.join("film.mp4");
    let check = framesmith(&cwd, [OsStr::new("check"), folder.as_os_str()], &[("NO_COLOR", "1")]);
    assert_eq!(check.status.code(), Some(0), "check: {}{}", out(&check), err(&check));
    let render = framesmith(
        &cwd,
        [
            OsStr::new("render"),
            folder.as_os_str(),
            OsStr::new("--backend"),
            OsStr::new("cpu"),
            OsStr::new("--output"),
            film.as_os_str(),
        ],
        &[("NO_COLOR", "1")],
    );
    assert_eq!(render.status.code(), Some(0), "render: {}{}", out(&render), err(&render));
    assert!(film.exists(), "render wrote no film");
}
