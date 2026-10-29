//! `framesmith check` run from the project folder itself, or from a folder inside it: the log is
//! then under that folder (decision A2), and nothing else of the project changes
//! (`docs/spec/run-log.md` section 1, `docs/spec/check-command.md` section 6).

mod common;

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use common::*;

const GROUP: &str = "run-log-in-the-project";

/// Every file and folder under `dir`, as a path from `dir`, with the bytes of the files.
fn entries(dir: &Path) -> Vec<(PathBuf, Option<Vec<u8>>)> {
    let mut found = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(next) = pending.pop() {
        for entry in fs::read_dir(&next).unwrap() {
            let path = entry.unwrap().path();
            let name = path.strip_prefix(dir).unwrap().to_path_buf();
            if path.is_dir() {
                found.push((name, None));
                pending.push(path);
            } else {
                found.push((name, Some(fs::read(&path).unwrap())));
            }
        }
    }
    found.sort();
    found
}

/// A project with a composition, a project file and a media file, and an empty folder beside it
/// to run from. Returns `(project, elsewhere)`.
fn project_with_files(name: &str, text: &str) -> (PathBuf, PathBuf) {
    let (project, elsewhere) = project(GROUP, name, text);
    fs::create_dir(project.join("media")).unwrap();
    fs::write(project.join("media").join("clip.bin"), [1u8, 2, 3]).unwrap();
    fs::write(project.join("framesmith.yaml"), "name: demo\n").unwrap();
    (project, elsewhere)
}

#[test]
fn run_from_the_project_folder_the_result_is_the_same_and_the_log_is_the_one_thing_added() {
    for (label, text) in [("valid", VALID), ("invalid", INVALID)] {
        let (project, elsewhere) = project_with_files(label, text);
        let before = entries(&project);
        // A valid project is checked to the size probe's pair; an invalid one is refused before it.
        let per_run = if label == "valid" { 4 } else { 2 };

        // The result from outside, with the project given by its path.
        let outside = framesmith(&elsewhere, [OsStr::new("check"), project.as_os_str()], &[]);
        assert!(!outside.stdout.is_empty());
        assert_eq!(entries(&project), before, "{label}: a run from outside adds nothing to the project");

        // The same project, run from inside it with `.`.
        let inside = framesmith(&project, ["check", "."], &[]);
        assert_eq!(inside.status.code(), outside.status.code(), "{label}");
        assert_eq!(inside.stdout, outside.stdout, "{label}");
        assert_eq!(err(&inside), "", "{label}");

        let after = entries(&project);
        let new: Vec<_> = after.iter().filter(|entry| !before.contains(entry)).collect();
        let names: Vec<_> = new.iter().map(|(name, _)| name.to_str().unwrap()).collect();
        assert_eq!(names, ["logs", "logs/framesmith.log"], "{label}");
        // Every file the project had keeps its name and its bytes.
        assert!(before.iter().all(|entry| after.contains(entry)), "{label}");
        assert_eq!(lines(&project).len(), per_run, "{label}");

        // A second run adds lines to the log and changes nothing else.
        let log_before = log_text(&project);
        let again = framesmith(&project, ["check", "."], &[]);
        assert_eq!(again.stdout, outside.stdout, "{label}");
        let log_after = log_text(&project);
        assert!(
            log_after.starts_with(&log_before) && log_after.lines().count() == 2 * per_run,
            "{label}"
        );
        let others =
            |all: &[(PathBuf, Option<Vec<u8>>)]| -> Vec<_> { all.iter().filter(|(name, _)| !name.starts_with("logs")).cloned().collect() };
        assert_eq!(others(&entries(&project)), others(&after), "{label}");
        let log_dir: Vec<_> = fs::read_dir(project.join("logs"))
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(log_dir, ["framesmith.log"], "{label}");
    }
}

#[test]
fn run_from_a_folder_inside_the_project_the_log_is_under_that_folder() {
    let (project, _) = project_with_files("inside", VALID);
    let inner = project.join("media");
    let before = entries(&project);

    let output = framesmith(&inner, ["check", ".."], &[]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(out(&output), "Ready: the project is valid, 150 frames, 1280x720, 30 fps\n");

    assert!(inner.join("logs").join("framesmith.log").is_file());
    assert!(!project.join("logs").exists());
    let after = entries(&project);
    let new: Vec<_> = after
        .iter()
        .filter(|entry| !before.contains(entry))
        .map(|(name, _)| name.to_str().unwrap())
        .collect();
    assert_eq!(new, ["media/logs", "media/logs/framesmith.log"]);
    assert!(before.iter().all(|entry| after.contains(entry)));
}
