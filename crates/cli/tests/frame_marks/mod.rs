//! What the tests of the frame marks share: a project folder copied from a fixture, a render of it with `--marks` on the
//! processor, and the marks it printed (`docs/spec/render-command.md`, section 7.1).

#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;

use crate::common::{out, scratch};

/// The fixtures of the repository, or the folder `FRAMESMITH_FIXTURES` names.
pub fn fixtures() -> PathBuf {
    std::env::var_os("FRAMESMITH_FIXTURES")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures"))
}

fn copy_files(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            copy_files(&path, &to.join(entry.file_name()));
        } else {
            fs::copy(&path, to.join(entry.file_name())).unwrap();
        }
    }
}

/// A new folder `name` holding a copy of `fixture` as the project `demo`, and an empty working folder `cwd`, side by side.
/// Returns `(project, cwd)`.
pub fn fixture_project(group: &str, name: &str, fixture: &str) -> (PathBuf, PathBuf) {
    let base = scratch(group, name);
    let project = base.join("demo");
    let cwd = base.join("cwd");
    copy_files(&fixtures().join(fixture), &project);
    fs::create_dir_all(&cwd).unwrap();
    (project, cwd)
}

/// The words of a render of `project` on the processor with `--marks`, the film written to `film.mp4` beside it.
pub fn marked_render(project: &Path) -> Vec<String> {
    let film = project.parent().unwrap().join("film.mp4");
    vec![
        "render".to_string(),
        project.to_str().unwrap().to_string(),
        "--backend".to_string(),
        "cpu".to_string(),
        "--marks".to_string(),
        "--output".to_string(),
        film.to_str().unwrap().to_string(),
    ]
}

/// The lines of the output that give a frame's mark, in the order printed.
pub fn mark_lines(output: &Output) -> Vec<String> {
    out(output)
        .lines()
        .filter(|line| line.starts_with("mark "))
        .map(str::to_string)
        .collect()
}
