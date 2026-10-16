//! `framesmith check` and `framesmith render` judge the forms of a place, a size, a margin and a
//! position by the same code: each invalid form is printed by both with the same report, on the same
//! line with the same code, and both exit with the same status; each valid placement fixture is
//! ready for both (`docs/spec/check-command.md`, section 3a; `docs/spec/render-command.md`, section 3).

mod common;

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use common::{err, framesmith, out, scratch};

/// Each invalid form of `fixtures/forms-not-listed`, with the line of its report, its code, and the
/// exit status that `check` and `render` both give: 1 invalid, 2 unsupported.
const INVALID_FORMS: [(&str, u32, &str, i32); 8] = [
    ("left-with-pixels", 5, "FS-0034", 1),
    ("width-with-em", 5, "FS-0034", 1),
    ("position-fixed", 5, "FS-0039", 1),
    ("margin-percent", 5, "FS-0032", 1),
    ("margin-five-numbers", 5, "FS-0032", 1),
    ("left-fraction", 5, "FS-0035", 2),
    ("start-without-duration", 5, "FS-0040", 1),
    ("duration-without-start", 5, "FS-0040", 1),
];

/// The folder of the fixtures in the repository.
fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

#[test]
fn check_and_render_print_the_same_report_and_exit_the_same_for_each_invalid_form() {
    for (name, line, code, exit) in INVALID_FORMS {
        let project = fixtures().join("forms-not-listed").join(name);
        let cwd = scratch("check-and-render-agree", name);
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
        assert_eq!(check.status.code(), Some(exit), "{name}: check: {}{}", out(&check), err(&check));
        assert_eq!(render.status.code(), Some(exit), "{name}: render: {}{}", out(&render), err(&render));
        assert_eq!(out(&check), out(&render), "{name}: check and render print different reports");
        assert_eq!(err(&check), "", "{name}: check wrote to the error stream");
        assert_eq!(err(&render), "", "{name}: render wrote to the error stream");
        let lines: Vec<String> = out(&check).lines().map(str::to_string).collect();
        assert_eq!(lines.len(), 2, "{name}: one report and one closing line: {lines:?}");
        assert!(
            lines[0].starts_with(&format!("composition.xml, line {line}: {code}: ")),
            "{name}: {}",
            lines[0]
        );
        let closing = if exit == 2 { "Unsupported: " } else { "Invalid: " };
        assert!(lines[1].starts_with(closing), "{name}: {}", lines[1]);
        assert!(!film.exists(), "{name}: a film was written for a project that is refused");
    }
}

#[test]
fn check_is_ready_for_every_placement_fixture_that_render_draws() {
    let names = [
        "clip-window",
        "first-render",
        "overlapping-boxes",
        "placement-between-edges",
        "placement-from-the-bottom-and-right",
        "placement-in-percentages",
        "placement-in-pixels",
        "placement-inside-a-box",
        "placement-margin-and-position",
        "placement-zero-sizes",
    ];
    for name in names {
        let project = fixtures().join(name);
        let cwd = scratch("check-ready-on-fixtures", name);
        let check = framesmith(&cwd, [OsStr::new("check"), project.as_os_str()], &[("NO_COLOR", "1")]);
        assert_eq!(check.status.code(), Some(0), "{name}: {}{}", out(&check), err(&check));
        assert!(out(&check).starts_with("Ready: the project is valid, "), "{name}: {}", out(&check));
    }
}
