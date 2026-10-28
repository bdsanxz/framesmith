//! Two renders of the order proof's composition print the same mark for every frame, and the marks are those of its frames
//! drawn in order; and no command of the product offers drawing a single frame on request (`docs/spec/render-command.md`,
//! sections 1, 7.1 and 8).

mod common;
mod frame_marks;

use common::{err, framesmith, out};
use draw_cpu::CpuRenderer;
use frame_marks::{fixture_project, mark_lines};
use project::Project;
use render::Scene;
use std::path::Path;

const GROUP: &str = "frame-order-renders";
const FIXTURE: &str = "frame-order";

/// The words of a render of `project` with `--marks` on `backend`, the film written to `film-<run>.mp4` beside it.
fn marked(project: &Path, backend: &str, run: u32) -> Vec<String> {
    let film = project.parent().unwrap().join(format!("film-{run}.mp4"));
    [
        "render",
        project.to_str().unwrap(),
        "--backend",
        backend,
        "--marks",
        "--output",
        film.to_str().unwrap(),
    ]
    .map(str::to_string)
    .to_vec()
}

/// The mark lines of two renders of one project on `backend`, each asserted to have succeeded.
fn two_renders(name: &str, backend: &str) -> (Vec<String>, Vec<String>) {
    let (project, cwd) = fixture_project(GROUP, name, FIXTURE);
    let runs: Vec<Vec<String>> = (1..=2)
        .map(|run| {
            let output = framesmith(&cwd, marked(&project, backend, run), &[]);
            assert_eq!(output.status.code(), Some(0), "{}{}", out(&output), err(&output));
            mark_lines(&output)
        })
        .collect();
    (runs[0].clone(), runs[1].clone())
}

#[test]
fn two_renders_of_the_project_on_the_processor_print_the_same_mark_for_every_frame() {
    let (first, second) = two_renders("processor", "cpu");
    assert_eq!(first.len(), 60, "one mark for each of the 60 frames");
    assert_eq!(first, second);
    // They are the marks of the frames drawn in order on request.
    let (project, _) = fixture_project(GROUP, "processor-in-order", FIXTURE);
    let scene = Scene::from_project(&Project::open(&project).unwrap(), &render::system_codecs()).unwrap();
    for (number, line) in first.iter().enumerate() {
        let mark = scene.picture(number as u64, &mut CpuRenderer).unwrap().mark();
        assert_eq!(line, &format!("mark {number:04}: {mark:016x}"));
    }
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_two_renders_of_the_project_on_the_card_print_the_same_mark_for_every_frame() {
    let (first, second) = two_renders("card", "gpu");
    assert_eq!(first.len(), 60, "one mark for each of the 60 frames");
    assert_eq!(first, second);
}

#[test]
fn no_command_draws_a_single_frame_on_request() {
    let (project, cwd) = fixture_project(GROUP, "no-single-frame", FIXTURE);
    let path = project.to_str().unwrap();
    // The usage names two commands, check and render, and render names no option that picks a frame.
    let usage = framesmith(&cwd, Vec::<String>::new(), &[]);
    let usage_text = err(&usage);
    let commands: Vec<&str> = usage_text
        .lines()
        .filter_map(|line| line.strip_prefix("usage: framesmith "))
        .map(|line| line.split(' ').next().unwrap())
        .collect();
    assert_eq!(commands, ["check", "render"], "{usage_text}");
    let options: Vec<&str> = usage_text
        .split(['[', ']', ' ', '\n'])
        .filter(|word| word.starts_with("--"))
        .collect();
    assert_eq!(
        options,
        ["--output", "--backend", "--quality", "--encoder", "--codec", "--workers", "--marks"],
        "{usage_text}"
    );
    // Words a single-frame command could have are not commands.
    for command in ["frame", "snapshot", "still", "preview", "picture"] {
        let output = framesmith(&cwd, [command, path, "0"], &[]);
        assert_eq!(output.status.code(), Some(1), "{command}: {}{}", out(&output), err(&output));
        assert!(
            err(&output).contains(&format!("`{command}` is not a command")),
            "{command}: {}",
            err(&output)
        );
    }
    // Options that would pick a frame are not options of render, and no film is drawn for them.
    for option in ["--frame", "--frames", "--at", "--start", "--end"] {
        let output = framesmith(&cwd, ["render", path, option, "3"], &[]);
        assert_eq!(output.status.code(), Some(1), "{option}: {}{}", out(&output), err(&output));
        assert!(
            err(&output).contains(&format!("`{option}` is not an option of `render`")),
            "{option}: {}",
            err(&output)
        );
    }
}
