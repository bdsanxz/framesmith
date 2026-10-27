//! `framesmith check` and `framesmith render` report, by the same code, each attribute that a `<style>`
//! gives and this version does not draw: each is printed by both on its line, with the same code and the
//! same message, both exit with the same status, and no film is written for the project that is refused
//! (`docs/spec/check-command.md`, section 3c; `docs/spec/render-command.md`, section 3). The invalid forms
//! of the styles, which `check` judges as the reader of the file does, are printed by both in the same way,
//! and both exit with 1.

mod common;

use std::ffi::OsStr;

use common::{err, framesmith, out, project};

/// Each project, and the lines of the reports `check` and `render` both print for it, in order. Every
/// report is of the code FS-0035, and each names the style that gives its attribute.
const CASES: [(&str, &str, &[u32]); 3] = [
    (
        "shadow-on-box",
        "<composition version=\"1\" width=\"160\" height=\"90\" fps=\"10\" duration=\"1\">\n\
         <style id=\"moving\" x=\"20\" box-shadow=\"1\" clip-path=\"2\"/>\n\
         <box class=\"moving\" left=\"90\" top=\"50\" width=\"20\" height=\"16\" background=\"#ffffff\"/>\n\
         </composition>\n",
        &[3, 3],
    ),
    (
        "conic-background-on-box",
        "<composition version=\"1\" width=\"160\" height=\"90\" fps=\"10\" duration=\"1\">\n\
         <style id=\"turning\" background=\"conic-gradient(#ffffff 0%, #000000 100%)\"/>\n\
         <box class=\"turning\" left=\"90\" top=\"50\" width=\"20\" height=\"16\"/>\n\
         </composition>\n",
        &[3],
    ),
    (
        "background-not-opaque-on-root",
        "<composition version=\"1\" width=\"160\" height=\"90\" fps=\"10\" duration=\"1\" class=\"faint\">\n\
         <style id=\"faint\" background=\"#ffffff80\"/>\n\
         </composition>\n",
        &[1],
    ),
];

#[test]
fn check_and_render_print_the_same_report_for_each_attribute_a_style_gives_that_is_not_drawn() {
    for (name, text, lines) in CASES {
        let (project, cwd) = project("check-and-render-agree-on-style-refusals", name, text);
        let film = cwd.join("film.mp4");
        let check = framesmith(&cwd, [OsStr::new("check"), project.as_os_str()], &[("NO_COLOR", "1")]);
        let render = framesmith(
            &cwd,
            [
                OsStr::new("render"),
                project.as_os_str(),
                OsStr::new("--backend"),
                OsStr::new("cpu"),
                OsStr::new("--output"),
                film.as_os_str(),
            ],
            &[("NO_COLOR", "1")],
        );
        assert_eq!(check.status.code(), Some(2), "{name}: check: {}{}", out(&check), err(&check));
        assert_eq!(render.status.code(), Some(2), "{name}: render: {}{}", out(&render), err(&render));
        assert_eq!(out(&check), out(&render), "{name}: check and render print different reports");
        assert_eq!(err(&check), "", "{name}: check wrote to the error stream");
        assert_eq!(err(&render), "", "{name}: render wrote to the error stream");
        let printed: Vec<String> = out(&check).lines().map(str::to_string).collect();
        assert_eq!(
            printed.len(),
            lines.len() + 1,
            "{name}: one report per line and one closing line: {printed:?}"
        );
        for (line, report) in lines.iter().zip(&printed) {
            assert!(
                report.starts_with(&format!("composition.xml, line {line}: FS-0035: ")),
                "{name}: {report}"
            );
            assert!(report.contains("which the style `"), "{name}: the report names the style: {report}");
        }
        assert!(
            printed[lines.len()].starts_with("Unsupported: "),
            "{name}: {}",
            printed[lines.len()]
        );
        assert!(!film.exists(), "{name}: a film was written for a project that is refused");
    }
}

/// One project whose styles are invalid: its name, its text, and the line and the code of each report.
type InvalidCase = (&'static str, &'static str, &'static [(u32, &'static str)]);

/// Each project whose styles are invalid, with the line and the code of each report that `check` and
/// `render` both print for it. Every report is of the kind invalid, so both exit with 1.
const INVALID_CASES: [InvalidCase; 4] = [
    (
        "style-value-not-a-number",
        "<composition version=\"1\" width=\"160\" height=\"90\" fps=\"10\" duration=\"1\">\n\
         <style id=\"far\" x=\"wide\"/>\n\
         <box class=\"far\" width=\"10\" height=\"10\"/>\n\
         </composition>\n",
        &[(3, "FS-0032")],
    ),
    (
        "style-attribute-the-element-does-not-take",
        "<composition version=\"1\" width=\"160\" height=\"90\" fps=\"10\" duration=\"1\">\n\
         <style id=\"shade\" object-fit=\"cover\"/>\n\
         <box class=\"shade\" width=\"10\" height=\"10\"/>\n\
         </composition>\n",
        &[(3, "FS-0030")],
    ),
    (
        "style-reports",
        "<composition version=\"1\" width=\"160\" height=\"90\" fps=\"10\" duration=\"1\">\n\
         <style background=\"#ffffff\"/>\n\
         <style id=\"label\" color=\"#000000\"/>\n\
         <style id=\"label\" color=\"#ffffff\"/>\n\
         <box class=\"label missing\"/>\n\
         <style id=\"frame\">\n\
         <box/>\n\
         </style>\n\
         </composition>\n",
        &[(2, "FS-0041"), (4, "FS-0042"), (5, "FS-0043"), (7, "FS-0031")],
    ),
    (
        "style-with-class",
        "<composition version=\"1\" width=\"160\" height=\"90\" fps=\"10\" duration=\"1\">\n\
         <style id=\"other\" color=\"#ffffff\"/>\n\
         <style id=\"chain\" class=\"other\" color=\"#000000\"/>\n\
         </composition>\n",
        &[(3, "FS-0030")],
    ),
];

#[test]
fn check_and_render_print_the_same_report_and_exit_the_same_for_each_invalid_style_form() {
    for (name, text, reports) in INVALID_CASES {
        let (project, cwd) = project("check-and-render-agree-on-style-refusals", name, text);
        let film = cwd.join("film.mp4");
        let check = framesmith(&cwd, [OsStr::new("check"), project.as_os_str()], &[("NO_COLOR", "1")]);
        let render = framesmith(
            &cwd,
            [
                OsStr::new("render"),
                project.as_os_str(),
                OsStr::new("--backend"),
                OsStr::new("cpu"),
                OsStr::new("--output"),
                film.as_os_str(),
            ],
            &[("NO_COLOR", "1")],
        );
        assert_eq!(check.status.code(), Some(1), "{name}: check: {}{}", out(&check), err(&check));
        assert_eq!(render.status.code(), Some(1), "{name}: render: {}{}", out(&render), err(&render));
        assert_eq!(out(&check), out(&render), "{name}: check and render print different reports");
        assert_eq!(err(&check), "", "{name}: check wrote to the error stream");
        assert_eq!(err(&render), "", "{name}: render wrote to the error stream");
        let printed: Vec<String> = out(&check).lines().map(str::to_string).collect();
        assert_eq!(
            printed.len(),
            reports.len() + 1,
            "{name}: one report per line and one closing line: {printed:?}"
        );
        for ((line, code), report) in reports.iter().zip(&printed) {
            assert!(
                report.starts_with(&format!("composition.xml, line {line}: {code}: ")),
                "{name}: {report}"
            );
        }
        assert!(
            printed[reports.len()].starts_with("Invalid: "),
            "{name}: {}",
            printed[reports.len()]
        );
        assert!(!film.exists(), "{name}: a film was written for a project that is refused");
    }
}
