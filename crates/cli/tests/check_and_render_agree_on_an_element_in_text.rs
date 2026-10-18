//! `framesmith check` and `framesmith render` report an element other than `<span>` inside a `<text>` with its line, and
//! print the same report for it: no film is written and both exit 1 (`docs/spec/check-command.md`, section 3b;
//! `docs/spec/text.md`, section 1; `docs/spec/render-command.md`, section 3).

mod common;

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use common::{err, framesmith, out, project};

const GROUP: &str = "check-and-render-agree-on-an-element-in-text";

/// The composition of the project: a `<text>` of two words on line 3, with the element of `inside` between them.
fn composition(inside: &str) -> String {
    format!(
        "<composition version=\"1\" id=\"film\" width=\"160\" height=\"60\" fps=\"10\" duration=\"0.5\" background=\"#000000\">\n\
         <font src=\"fonts/framesmith-test-stroke-400.ttf\" family=\"Framesmith Test Stroke\"/>\n\
         <text left=\"10\" top=\"10\" font-family=\"Framesmith Test Stroke\" font-size=\"20\">AB {inside}CD</text>\n\
         </composition>\n"
    )
}

/// A project folder with the composition of `inside` and the test font beside it in `fonts/`. Returns `(project, cwd)`.
fn project_of(name: &str, inside: &str) -> (PathBuf, PathBuf) {
    let (project, cwd) = project(GROUP, name, &composition(inside));
    fs::create_dir_all(project.join("fonts")).unwrap();
    let file = "framesmith-test-stroke-400.ttf";
    let from = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/fonts").join(file);
    fs::copy(&from, project.join("fonts").join(file)).unwrap();
    (project, cwd)
}

/// Each element other than `<span>` inside a `<text>`, with the start of the report that names it.
const ELEMENTS: [(&str, &str, &str); 2] = [
    (
        "box",
        "<box width=\"10\" height=\"10\" color=\"#ff0000\"/>",
        "composition.xml, line 3: FS-0031: invalid: <box> is not known inside <text>",
    ),
    (
        "rect",
        "<rect width=\"10\" height=\"10\" color=\"#ff0000\"/>",
        "composition.xml, line 3: FS-0031: invalid: <rect> is not known inside <text>",
    ),
];

#[test]
fn check_and_render_report_an_element_other_than_span_inside_text_with_its_line_the_same_way() {
    for (name, inside, report) in ELEMENTS {
        let (project, cwd) = project_of(name, inside);
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
        assert_eq!(err(&check), err(&render), "{name}: check and render write different errors");
        assert!(
            out(&check).starts_with(report),
            "{name}: the report does not name the element with its line: {}{}",
            out(&check),
            err(&check)
        );
        assert!(!film.exists(), "{name}: a film was written for a project that is not valid");
    }
}
