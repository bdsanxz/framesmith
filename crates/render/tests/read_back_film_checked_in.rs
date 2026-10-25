//! The development example `read_back_film` with `--checked-in` (`docs/spec/render-command.md`, section 9): a frame with
//! no checked-in picture is named, with the file looked for, and the example exits with 1 and writes no sheet; frames that
//! all have their pictures give a sheet. The example is the one `cargo test` builds beside this test.

mod common;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use common::{fixtures, project_from_fixture};
use draw_cpu::CpuRenderer;
use project::Project;
use render::{write_film, Scene};

const GROUP: &str = "read-back-film-checked-in";

/// The example built for this test run: `target/<profile>/examples/read_back_film`.
fn example() -> PathBuf {
    let deps = std::env::current_exe().unwrap().parent().unwrap().to_path_buf();
    deps.parent().unwrap().join("examples").join("read_back_film")
}

/// The film of the fixture `first-render`, written in a folder of its own under `name`.
fn film(name: &str) -> PathBuf {
    let folder = project_from_fixture(GROUP, name, "first-render");
    let scene = Scene::from_project(&Project::open(&folder).unwrap()).unwrap();
    let output = folder.join("film.mp4");
    write_film(&scene, &mut CpuRenderer, &output, 192).unwrap();
    output
}

fn read_back(film: &Path, sheet: &Path, frames: &[&str]) -> Output {
    let checked_in = fixtures().join("first-render").join("expected");
    Command::new(example())
        .arg(film)
        .arg(sheet)
        .arg("--checked-in")
        .arg(&checked_in)
        .args(frames)
        .output()
        .unwrap()
}

#[test]
fn a_frame_with_no_checked_in_picture_is_named_and_the_example_exits_with_1_and_writes_no_sheet() {
    let film = film("missing");
    let sheet = film.with_file_name("sheet.png");
    let output = read_back(&film, &sheet, &["0", "5"]);
    let err = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{err}");
    let missing = fixtures().join("first-render").join("expected").join("frame-0005.png");
    assert_eq!(
        err.trim_end(),
        format!(
            "read_back_film: frame 5 has no checked-in picture: {} does not exist",
            missing.display()
        )
    );
    assert!(!err.contains("panicked"), "{err}");
    assert!(!sheet.exists());
}

#[test]
fn frames_that_all_have_a_checked_in_picture_give_a_sheet() {
    let film = film("present");
    let sheet = film.with_file_name("sheet.png");
    let output = read_back(&film, &sheet, &["0", "9"]);
    assert_eq!(output.status.code(), Some(0), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(sheet.is_file());
}
