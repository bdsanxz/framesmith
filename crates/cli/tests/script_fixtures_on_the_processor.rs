//! The script fixtures on the processor: `check` accepts each of the six that draw with the machine's fonts, and
//! `render --backend cpu` writes a film of ten frames for each. An emoji that its font has no picture for is refused by
//! both commands with the line of its text, FS-0056, and no film (`docs/spec/text.md`, sections 12.1 and 12.4). The
//! fixtures name the machine's fonts by their paths, so the test is compiled for macOS only.

#![cfg(target_os = "macos")]

mod common;

use std::fs;
use std::path::Path;

use common::err;
use common::framesmith;
use common::out;
use common::project as make;

const GROUP: &str = "script-fixtures-on-the-processor";

/// The six fixtures that draw with the machine's fonts: the five scripts and the emoji.
const SCRIPTS: [&str; 6] = [
    "text-cyrillic",
    "text-arabic",
    "text-hebrew-latin",
    "text-japanese",
    "text-thai",
    "text-emoji",
];

/// A composition whose one emoji, U+2640, has no picture in the machine's emoji font: the font gives it no PNG at any size.
const EMOJI_WITHOUT_PICTURE: &str = r##"<composition version="1" width="320" height="300" fps="10" duration="1" background="#ffffff">
<font src="/System/Library/Fonts/Apple Color Emoji.ttc" family="Apple Color Emoji"/>
<text left="10" top="10" width="300" font-family="Apple Color Emoji" font-size="20" line-height="2" color="#1e6fd9">♀</text>
</composition>
"##;

/// The line of a report that names FS-0056, as the command printed it.
fn refusal_line(printed: &str) -> String {
    printed
        .lines()
        .find(|line| line.contains("FS-0056"))
        .unwrap_or_else(|| panic!("no line names FS-0056: {printed}"))
        .to_string()
}

/// Whether the project's `renders` folder holds a film.
fn has_a_film(project: &Path) -> bool {
    fs::read_dir(project.join("renders"))
        .map(|entries| {
            entries
                .flatten()
                .any(|entry| entry.path().extension().is_some_and(|ext| ext == "mp4"))
        })
        .unwrap_or(false)
}

/// The composition of a fixture, read from the fixtures folder.
fn fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name)
        .join("composition.xml");
    fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

#[test]
fn check_accepts_each_script_fixture_drawn_with_the_machine_font() {
    for name in SCRIPTS {
        let (project, cwd) = make(GROUP, &format!("check-{name}"), &fixture(name));
        let output = framesmith(&cwd, ["check", project.to_str().unwrap()], &[]);
        assert_eq!(output.status.code(), Some(0), "{name}: {}{}", out(&output), err(&output));
        assert!(!out(&output).contains("FS-"), "{name}: {}", out(&output));
    }
}

#[test]
fn render_with_the_processor_writes_a_ten_frame_film_for_each_script_fixture() {
    for name in SCRIPTS {
        let (project, cwd) = make(GROUP, &format!("render-{name}"), &fixture(name));
        let output = framesmith(&cwd, ["render", project.to_str().unwrap(), "--backend", "cpu"], &[]);
        assert_eq!(output.status.code(), Some(0), "{name}: {}{}", out(&output), err(&output));
        let printed = out(&output);
        assert!(printed.contains("renderer: processor"), "{name}: {printed}");
        assert!(printed.contains("frames: 10"), "{name}: {printed}");
        assert!(has_a_film(&project), "{name}: the film is written under renders/");
    }
}

#[test]
fn an_emoji_with_no_picture_is_refused_by_check_and_by_render_with_the_same_line_and_no_film() {
    let (project, cwd) = make(GROUP, "emoji-without-picture-check", EMOJI_WITHOUT_PICTURE);
    let checked = framesmith(&cwd, ["check", project.to_str().unwrap()], &[]);
    assert_eq!(checked.status.code(), Some(2), "{}{}", out(&checked), err(&checked));
    let check_line = refusal_line(&out(&checked));
    assert!(
        check_line.starts_with("composition.xml, line 3: FS-0056: unsupported:"),
        "{check_line}"
    );

    let (project, cwd) = make(GROUP, "emoji-without-picture-render", EMOJI_WITHOUT_PICTURE);
    let rendered = framesmith(&cwd, ["render", project.to_str().unwrap(), "--backend", "cpu"], &[]);
    assert_eq!(rendered.status.code(), Some(2), "{}{}", out(&rendered), err(&rendered));
    assert!(!out(&rendered).contains("Rendered "), "{}", out(&rendered));
    assert_eq!(
        refusal_line(&out(&rendered)),
        check_line,
        "render gives the report that check gives"
    );
    assert!(!has_a_film(&project), "no film is written for a refused emoji");
}
