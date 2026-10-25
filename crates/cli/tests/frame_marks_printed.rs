//! `framesmith render --marks` prints the mark of every frame drawn after the summary, one line a frame in the order of the
//! frames; each mark is the mark of the picture the renderer drew, and two runs print the same marks. `--marks` takes no
//! value and is given once (`docs/spec/render-command.md`, sections 1 and 7.1).

mod common;
mod frame_marks;

use common::{err, framesmith, out};
use draw_cpu::CpuRenderer;
use frame_marks::{fixture_project, mark_lines, marked_render};
use project::Project;
use render::Scene;

const FIXTURE: &str = "declared-font-file";

#[test]
fn every_frame_s_mark_is_printed_after_the_summary_and_is_the_mark_of_its_picture() {
    let (project, cwd) = fixture_project("frame-marks", "printed", FIXTURE);
    let output = framesmith(&cwd, marked_render(&project), &[]);
    assert_eq!(output.status.code(), Some(0), "{}{}", out(&output), err(&output));
    let printed = out(&output);
    let lines: Vec<&str> = printed.lines().collect();
    let elapsed = lines.iter().position(|line| line.starts_with("elapsed: ")).unwrap();
    let marks = mark_lines(&output);
    assert_eq!(
        lines[elapsed + 1..].to_vec(),
        marks,
        "the marks follow the elapsed line, and nothing else does"
    );

    let scene = Scene::from_project(&Project::open(&project).unwrap()).unwrap_or_else(|reports| panic!("{reports:?}"));
    assert_eq!(marks.len() as u64, scene.frames(), "one mark for each frame");
    for (number, line) in marks.iter().enumerate() {
        let picture = scene.picture(number as u64, &mut CpuRenderer).unwrap();
        assert_eq!(line, &format!("mark {number:04}: {:016x}", picture.mark()));
    }
}

#[test]
fn two_renders_of_one_project_print_the_same_marks() {
    let (first, first_cwd) = fixture_project("frame-marks", "first", FIXTURE);
    let (second, second_cwd) = fixture_project("frame-marks", "second", FIXTURE);
    let first_marks = mark_lines(&framesmith(&first_cwd, marked_render(&first), &[]));
    let second_marks = mark_lines(&framesmith(&second_cwd, marked_render(&second), &[]));
    assert_eq!(first_marks.len(), 10);
    assert_eq!(first_marks, second_marks);
}

#[test]
fn a_render_without_marks_prints_none() {
    let (project, cwd) = fixture_project("frame-marks", "without", FIXTURE);
    let mut words = marked_render(&project);
    words.retain(|word| word != "--marks");
    let output = framesmith(&cwd, words, &[]);
    assert_eq!(output.status.code(), Some(0), "{}", out(&output));
    assert!(mark_lines(&output).is_empty(), "{}", out(&output));
}

#[test]
fn marks_with_a_value_or_given_twice_is_an_option_not_taken() {
    let (project, cwd) = fixture_project("frame-marks", "refused", FIXTURE);
    let path = project.to_str().unwrap();
    for (words, why) in [
        (
            vec!["render", path, "--marks=yes"],
            "`--marks` takes no value, and is written alone",
        ),
        (vec!["render", path, "--marks", "--marks"], "`--marks` is given twice; give it once"),
    ] {
        let output = framesmith(&cwd, &words, &[]);
        assert_eq!(output.status.code(), Some(1), "{words:?}");
        assert_eq!(out(&output), "", "{words:?}");
        let printed = err(&output);
        assert!(printed.starts_with("framesmith: FS-0037: invalid: "), "{printed}");
        assert!(printed.contains(why), "{printed}");
        assert!(!project.join("renders").exists());
    }
}
