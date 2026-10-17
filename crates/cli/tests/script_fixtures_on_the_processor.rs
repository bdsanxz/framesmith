//! The script fixtures on the processor: `check` accepts each of the five that draw with the machine's font, and
//! `render --backend cpu` writes a film of ten frames for each. The emoji fixture is refused by both commands with the
//! line of its text (`docs/spec/text.md`, sections 12.1 and 12.4). The fixtures name the machine's font by its path, so
//! the test is compiled for macOS only.

#![cfg(target_os = "macos")]

mod common;

use std::fs;
use std::path::Path;

use common::err;
use common::framesmith;
use common::out;
use common::project as make;

const GROUP: &str = "script-fixtures-on-the-processor";

/// The five fixtures that draw with the machine's font; the emoji fixture is named apart.
const SCRIPTS: [&str; 5] = ["text-cyrillic", "text-arabic", "text-hebrew-latin", "text-japanese", "text-thai"];

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
fn the_emoji_fixture_is_refused_by_check_and_by_render_with_its_line() {
    let (project, cwd) = make(GROUP, "emoji-check", &fixture("text-emoji"));
    let checked = framesmith(&cwd, ["check", project.to_str().unwrap()], &[]);
    assert_eq!(checked.status.code(), Some(2), "{}{}", out(&checked), err(&checked));
    assert!(out(&checked).contains("line 3: FS-0035"), "{}", out(&checked));

    let (project, cwd) = make(GROUP, "emoji-render", &fixture("text-emoji"));
    let rendered = framesmith(&cwd, ["render", project.to_str().unwrap(), "--backend", "cpu"], &[]);
    assert_eq!(rendered.status.code(), Some(2), "{}{}", out(&rendered), err(&rendered));
    assert!(out(&rendered).contains("line 3: FS-0035"), "{}", out(&rendered));
    assert!(!out(&rendered).contains("Rendered "), "{}", out(&rendered));
}
