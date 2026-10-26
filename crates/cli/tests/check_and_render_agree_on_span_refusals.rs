//! `framesmith check` and `framesmith render` refuse a `<span>` that writes a margin or a padding this version does not
//! draw, or an attribute that section 6.5 of `text.md` does not draw, with the line of the attribute: both print the same
//! report, and no film is written. A negative padding is invalid and exits 1; the others are unsupported and exit 2
//! (`docs/spec/text.md`, section 6.5, rules 5 and 6; `docs/spec/error-catalogue.md`).

mod common;

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;

use common::{err, framesmith, out, project};

const GROUP: &str = "check-and-render-agree-on-span-refusals";

/// The head of each composition: its canvas, and the font that its texts draw with.
const HEAD: &str = r##"<composition version="1" id="film" width="160" height="60" fps="10" duration="0.5" background="#000000">
<font src="fonts/framesmith-test-stroke-400.ttf" family="Framesmith Test Stroke"/>
"##;

/// Each refused form of a `<span>`: a name for its folder, the span, the start of the report line that both commands
/// give for it on line 3, and the exit code of its kind (1 for invalid, 2 for unsupported).
const SPAN_FORMS: [(&str, &str, &str, i32); 12] = [
    (
        "negative-margin",
        r##"<span margin="-4" color="#ff0000">CD</span>"##,
        "composition.xml, line 3: FS-0035: unsupported: `margin=\"-4\"` on <span> is not drawn by this version: a margin of 0 or more",
        2,
    ),
    (
        "negative-padding",
        r##"<span padding="-4" color="#ff0000">CD</span>"##,
        "composition.xml, line 3: FS-0034: invalid: `padding=\"-4\"` must be 0 or more",
        1,
    ),
    (
        "percentage-padding",
        r##"<span padding="10%" color="#ff0000">CD</span>"##,
        "composition.xml, line 3: FS-0035: unsupported: `padding=\"10%\"` on <span> is not a form this version supports: a number of pixels",
        2,
    ),
    (
        "text-align",
        r##"<span text-align="center" color="#ff0000">CD</span>"##,
        "composition.xml, line 3: FS-0035: unsupported: <span> has an attribute this version does not support: `text-align`",
        2,
    ),
    (
        "background",
        r##"<span background="#ff0000">CD</span>"##,
        "composition.xml, line 3: FS-0035: unsupported: <span> has an attribute this version does not support: `background`",
        2,
    ),
    (
        "background-repeat",
        r##"<span background-repeat="repeat" color="#ff0000">CD</span>"##,
        "composition.xml, line 3: FS-0035: unsupported: <span> has an attribute this version does not support: `background-repeat`",
        2,
    ),
    (
        "border-radius-of-the-look",
        r##"<span border-radius="2" color="#ff0000">CD</span>"##,
        "composition.xml, line 3: FS-0035: unsupported: <span> has an attribute this version does not support: `border-radius`",
        2,
    ),
    (
        "box-shadow-of-the-look",
        r##"<span box-shadow="1 1 #000000" color="#ff0000">CD</span>"##,
        "composition.xml, line 3: FS-0035: unsupported: <span> has an attribute this version does not support: `box-shadow`",
        2,
    ),
    (
        "x-of-the-movement",
        r##"<span x="4" color="#ff0000">CD</span>"##,
        "composition.xml, line 3: FS-0035: unsupported: <span> has an attribute this version does not support: `x`",
        2,
    ),
    (
        "opacity-of-the-movement",
        r##"<span opacity="0.5" color="#ff0000">CD</span>"##,
        "composition.xml, line 3: FS-0035: unsupported: <span> has an attribute this version does not support: `opacity`",
        2,
    ),
    (
        "margin-with-a-negative-side",
        r##"<span margin="0 -4 0 0" color="#ff0000">CD</span>"##,
        "composition.xml, line 3: FS-0035: unsupported: `margin=\"0 -4 0 0\"` on <span> is not drawn by this version: a margin of 0 or more",
        2,
    ),
    (
        "padding-with-a-negative-side",
        r##"<span padding="0 0 0 -4" color="#ff0000">CD</span>"##,
        "composition.xml, line 3: FS-0034: invalid: `padding=\"0 0 0 -4\"` must be 0 or more",
        1,
    ),
];

/// A `<text>` on line 3 that holds the word `AB` and then `span`.
fn text_with(span: &str) -> String {
    format!(r##"<text left="10" top="10" font-family="Framesmith Test Stroke" font-size="20">AB {span}</text>"##)
}

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

/// Runs `check` and `render` on the project of `name`. Returns both outputs and the path the film would be written to.
fn check_and_render(name: &str, text: &str) -> (Output, Output, PathBuf) {
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
    (check, render, film)
}

/// What is wrong with the outputs of `check` and `render` for a refused project: the exit code `code`, the same report
/// lines `reports` on the lines before the closing line, and a closing line of the kind of `code`. An empty result means
/// nothing is wrong.
fn problems(check: &Output, render: &Output, film: &Path, reports: &[&str], code: i32) -> Vec<String> {
    let mut found = Vec::new();
    if check.status.code() != Some(code) || render.status.code() != Some(code) {
        found.push(format!(
            "exit codes are {:?} and {:?}, not {code}",
            check.status.code(),
            render.status.code()
        ));
    }
    if out(check) != out(render) {
        found.push("check and render print different reports".to_string());
    }
    if !err(check).is_empty() || !err(render).is_empty() {
        found.push("a command wrote to the error stream".to_string());
    }
    let lines: Vec<String> = out(check).lines().map(str::to_string).collect();
    if lines.len() != reports.len() + 1 {
        found.push(format!(
            "expected {} report line(s) and one closing line, got {lines:?}",
            reports.len()
        ));
    } else {
        for (line, report) in lines.iter().zip(reports) {
            if !line.starts_with(report) {
                found.push(format!("a report line is not `{report}`: {line}"));
            }
        }
        let closing = if code == 1 { "Invalid: " } else { "Unsupported: " };
        if !lines[reports.len()].starts_with(closing) {
            found.push(format!("the closing line does not start `{closing}`: {}", lines[reports.len()]));
        }
    }
    if film.exists() {
        found.push("a film was written for a project that is refused".to_string());
    }
    found
}

#[test]
fn check_and_render_refuse_each_span_form_that_is_not_drawn_the_same_way_on_its_line() {
    let mut failures: Vec<String> = Vec::new();
    for (name, span, report, code) in SPAN_FORMS {
        let (check, render, film) = check_and_render(name, &text_with(span));
        let found = problems(&check, &render, &film, &[report], code);
        if !found.is_empty() {
            failures.push(format!("{name}:\n    {}", found.join("\n    ")));
        }
    }
    assert!(failures.is_empty(), "{} case(s) failed:\n{}", failures.len(), failures.join("\n"));
}

#[test]
fn check_and_render_report_start_and_duration_of_one_span_in_the_order_they_are_written() {
    let start = "composition.xml, line 3: FS-0035: unsupported: <span> has an attribute this version does not support: `start`";
    let duration = "composition.xml, line 3: FS-0035: unsupported: <span> has an attribute this version does not support: `duration`";
    let cases = [
        (
            "start-first",
            r##"<span start="0" duration="1" color="#ff0000">CD</span>"##,
            [start, duration],
        ),
        (
            "duration-first",
            r##"<span duration="1" start="0" color="#ff0000">CD</span>"##,
            [duration, start],
        ),
    ];
    let mut failures: Vec<String> = Vec::new();
    for (name, span, reports) in cases {
        let (check, render, film) = check_and_render(name, &text_with(span));
        let found = problems(&check, &render, &film, &reports, 2);
        if !found.is_empty() {
            failures.push(format!("{name}:\n    {}", found.join("\n    ")));
        }
    }
    assert!(failures.is_empty(), "{} case(s) failed:\n{}", failures.len(), failures.join("\n"));
}

#[test]
fn check_and_render_refuse_a_room_between_two_ideographs_on_the_line_of_its_attribute() {
    // Two ideographs may break between them (UAX 14, rule LB31), and a no-break space after the first keeps no break before
    // it (rule LB12a), so the room of the span cannot keep its place and its break: it is refused where the margin is written.
    // The refusal does not wait for a glyph of the characters, since the room is refused before the glyphs are looked up.
    let text = r##"<text left="10" top="10" font-family="Framesmith Test Stroke" font-size="20">漢<span margin="0 12" color="#c02020">字</span></text>"##;
    let (check, render, film) = check_and_render("ideograph-room", text);
    let report = "composition.xml, line 3: FS-0035: unsupported: <text> is not drawn by this version: a margin or a padding of a <span> where the line may break before the span, after a character other than a space or a hyphen, which this version does not draw";
    let found = problems(&check, &render, &film, &[report], 2);
    assert!(found.is_empty(), "{}", found.join("\n"));
}
