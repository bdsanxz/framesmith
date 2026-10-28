//! The development example `read_back_film` with `--checked-in` (`docs/spec/render-command.md`, section 9): a frame with
//! no checked-in picture is named, with the file looked for, and the example exits with 1 and writes no sheet; frames that
//! all have their pictures give a sheet. The example's code is compiled into this test and run in it, so the test needs no
//! example built apart from it.

mod common;

#[allow(dead_code, clippy::duplicate_mod)]
#[path = "../examples/read_back_film.rs"]
mod read_back_film;

use std::path::{Path, PathBuf};

use common::{fixtures, project_from_fixture};
use draw_cpu::CpuRenderer;
use project::Project;
use render::{write_film, Scene};

const GROUP: &str = "read-back-film-checked-in";

/// What a run of the example printed as refusals, and its exit code.
struct Output {
    code: i32,
    stderr: Vec<u8>,
}

/// The film of the fixture `first-render`, written in a folder of its own under `name`.
fn film(name: &str) -> PathBuf {
    let folder = project_from_fixture(GROUP, name, "first-render");
    let scene = Scene::from_project(&Project::open(&folder).unwrap(), &render::system_codecs()).unwrap();
    let output = folder.join("film.mp4");
    write_film(&scene, &mut CpuRenderer, &output, 192).unwrap();
    output
}

fn read_back(film: &Path, sheet: &Path, frames: &[&str]) -> Output {
    let checked_in = fixtures().join("first-render").join("expected");
    let mut words: Vec<String> = [film, sheet, Path::new("--checked-in"), &checked_in]
        .iter()
        .map(|word| word.display().to_string())
        .collect();
    words.extend(frames.iter().map(|frame| frame.to_string()));
    let mut stderr = Vec::new();
    let code = read_back_film::run(words.into_iter(), &mut stderr);
    Output { code, stderr }
}

#[test]
fn a_frame_with_no_checked_in_picture_is_named_and_the_example_exits_with_1_and_writes_no_sheet() {
    let film = film("missing");
    let sheet = film.with_file_name("sheet.png");
    let output = read_back(&film, &sheet, &["0", "5"]);
    let err = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.code, 1, "{err}");
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

/// The line naming frame `number` as having no checked-in picture.
fn no_picture(number: u32) -> String {
    let missing = fixtures()
        .join("first-render")
        .join("expected")
        .join(format!("frame-{number:04}.png"));
    format!(
        "read_back_film: frame {number} has no checked-in picture: {} does not exist",
        missing.display()
    )
}

#[test]
fn several_frames_with_no_checked_in_picture_among_ones_that_have_it_are_each_named_in_the_order_asked() {
    let film = film("several-missing");
    let sheet = film.with_file_name("sheet.png");
    let output = read_back(&film, &sheet, &["9", "7", "0", "2", "4", "5"]);
    let err = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.code, 1, "{err}");
    let wanted: Vec<String> = [7, 2, 5].into_iter().map(no_picture).collect();
    assert_eq!(err.lines().collect::<Vec<_>>(), wanted, "{err}");
    assert!(!err.contains("panicked"), "{err}");
    assert!(!sheet.exists());
}

#[test]
fn a_frame_with_no_checked_in_picture_is_named_before_a_file_that_is_not_a_film_is_read() {
    let folder = project_from_fixture(GROUP, "not-a-film", "first-render");
    let not_a_film = folder.join("not-a-film.mp4");
    std::fs::write(&not_a_film, b"these bytes are not a film").unwrap();
    let absent = folder.join("absent.mp4");
    for film in [&not_a_film, &absent] {
        let sheet = folder.join("sheet.png");
        let output = read_back(film, &sheet, &["0", "5"]);
        let err = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.code, 1, "{}: {err}", film.display());
        assert_eq!(err.trim_end(), no_picture(5), "{}", film.display());
        assert!(!err.contains("panicked"), "{err}");
        assert!(!sheet.exists());
    }
}

#[test]
fn frames_that_all_have_a_checked_in_picture_give_a_sheet() {
    let film = film("present");
    let sheet = film.with_file_name("sheet.png");
    let output = read_back(&film, &sheet, &["0", "9"]);
    assert_eq!(output.code, 0, "{}", String::from_utf8_lossy(&output.stderr));
    assert!(sheet.is_file());
}
