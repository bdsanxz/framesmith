//! `framesmith check` and `framesmith render` judge a form inside a `<text>` by the same code: an element inside a
//! `<text>`, and an attribute of a `<text>` that this version does not draw, are printed by both with the same
//! report, on the same line with the same code, and both exit 2 with no film written (`docs/spec/check-command.md`,
//! section 3b; `docs/spec/text.md`, sections 1 and 8; `docs/spec/render-command.md`, section 3).

mod common;

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use common::{err, framesmith, out, project};

const GROUP: &str = "check-and-render-agree-on-text-forms";

/// The line of the composition that holds the `<text>` in every case below.
const TEXT_LINE: u32 = 3;

/// The head of each composition: its canvas, and the font that its texts draw with.
const HEAD: &str = r##"<composition version="1" id="film" width="160" height="60" fps="10" duration="0.5" background="#000000">
<font src="fonts/framesmith-test-stroke-400.ttf" family="Framesmith Test Stroke"/>
"##;

/// Each form inside a `<text>` that this version does not draw, with the start of the message that both commands
/// give for it.
const TEXT_FORMS: [(&str, &str, &str); 4] = [
    (
        "span-inside-text",
        r##"<text left="10" top="10" font-family="Framesmith Test Stroke" font-size="20">AB <span color="#ff0000">CD</span></text>"##,
        "<span> inside <text> is not supported by this version",
    ),
    (
        "tween-inside-text",
        r##"<text left="10" top="10" font-family="Framesmith Test Stroke" font-size="20">AB<tween at="0" duration="1" from-opacity="0" to-opacity="1"/></text>"##,
        "<tween> inside <text> is not supported by this version",
    ),
    (
        "set-inside-text",
        r##"<text left="10" top="10" font-family="Framesmith Test Stroke" font-size="20">AB<set at="0" opacity="1"/></text>"##,
        "<set> inside <text> is not supported by this version",
    ),
    (
        "background-of-text",
        r##"<text left="10" top="10" font-family="Framesmith Test Stroke" font-size="20" background="#ff0000">AB</text>"##,
        "<text> has an attribute this version does not support: `background`",
    ),
];

/// A project folder whose `composition.xml` is `HEAD`, then `text`, then the closing tag, with the test font beside it
/// in `fonts/`. Returns `(project, cwd)`.
fn project_of(name: &str, text: &str) -> (PathBuf, PathBuf) {
    let (project, cwd) = project(GROUP, name, &format!("{HEAD}{text}\n</composition>\n"));
    fs::create_dir_all(project.join("fonts")).unwrap();
    let file = "framesmith-test-stroke-400.ttf";
    let from = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/fonts").join(file);
    fs::copy(&from, project.join("fonts").join(file)).unwrap();
    (project, cwd)
}

#[test]
fn check_and_render_print_the_same_report_and_refuse_each_text_form_the_same_way() {
    for (name, text, message) in TEXT_FORMS {
        let (project, cwd) = project_of(name, text);
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
}
