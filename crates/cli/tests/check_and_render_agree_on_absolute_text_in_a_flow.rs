//! `framesmith check` and `framesmith render` refuse an absolute `<text>` with `left` and `right` and no `width` inside a
//! row, a column or a set of columns by the same code: FS-0035 on the line of the `<text>`, the same report from both,
//! exit 2, and no film written (`docs/spec/text.md`, section 8; `docs/spec/render-command.md`, sections 4.3 and 4.4).

mod common;

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use common::{err, framesmith, out, project};

const GROUP: &str = "check-and-render-agree-on-absolute-text-in-a-flow";

/// The line of the composition that holds the `<text>` in every case below.
const TEXT_LINE: u32 = 3;

/// The head of each composition: its canvas, and the font that its text draws with, so that the refusal of the
/// placement is the only one a case can give.
const HEAD: &str = r##"<composition version="1" id="film" width="160" height="60" fps="10" duration="0.5" background="#000000">
<font src="fonts/framesmith-test-stroke-400.ttf" family="Framesmith Test Stroke"/>
"##;

/// The absolute text, placed by its two edges and with no width, written on the line of the box that holds it.
const TEXT: &str = r##"<text position="absolute" left="10" right="20" font-family="Framesmith Test Stroke" font-size="20">AB</text>"##;

/// A project folder whose `composition.xml` is `HEAD`, then `flow` holding `TEXT`, then the closing tag, with the test
/// font beside it in `fonts/`. Returns `(project, cwd)`.
fn project_of(name: &str, flow: &str) -> (PathBuf, PathBuf) {
    let (project, cwd) = project(GROUP, name, &format!("{HEAD}{flow}{TEXT}</box>\n</composition>\n"));
    fs::create_dir_all(project.join("fonts")).unwrap();
    let file = "framesmith-test-stroke-400.ttf";
    let from = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/fonts").join(file);
    fs::copy(&from, project.join("fonts").join(file)).unwrap();
    (project, cwd)
}

/// Runs `check` and `render` on the project of `flow` and asserts that both refuse it with the same report, which
/// starts with `message` on the line of the `<text>`.
fn refused_the_same_way(name: &str, flow: &str, message: &str) {
    let (project, cwd) = project_of(name, flow);
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
    let lines: Vec<String> = out(&check).lines().map(str::to_string).collect();
    assert_eq!(lines.len(), 2, "{name}: one report and one closing line: {lines:?}");
    assert!(
        lines[0].starts_with(&format!("composition.xml, line {TEXT_LINE}: FS-0035: unsupported: {message}")),
        "{name}: {}",
        lines[0]
    );
    assert!(lines[1].starts_with("Unsupported: "), "{name}: {}", lines[1]);
    assert!(!film.exists(), "{name}: a film was written for a project that is refused");
}

#[test]
fn check_and_render_refuse_an_absolute_text_with_two_edges_and_no_width_in_a_row() {
    refused_the_same_way(
        "in-a-row",
        "<box flex-direction=\"row\" width=\"160\" height=\"30\" background=\"#202020\">",
        "a <text> with `left` and `right` and no `width`, inside a row, a column or a set of columns, is not drawn by this version",
    );
}

#[test]
fn check_and_render_refuse_an_absolute_text_with_two_edges_and_no_width_in_a_column() {
    refused_the_same_way(
        "in-a-column",
        "<box flex-direction=\"column\" width=\"160\" height=\"60\" background=\"#202020\">",
        "a <text> with `left` and `right` and no `width`, inside a row, a column or a set of columns, is not drawn by this version",
    );
}

#[test]
fn check_and_render_refuse_an_absolute_text_with_two_edges_and_no_width_in_a_set_of_columns() {
    refused_the_same_way(
        "in-a-set-of-columns",
        "<box columns=\"80 1fr\" width=\"160\" height=\"60\" background=\"#202020\">",
        "a <text> with `left` and `right` and no `width`, inside a row, a column or a set of columns, is not drawn by this version",
    );
}
