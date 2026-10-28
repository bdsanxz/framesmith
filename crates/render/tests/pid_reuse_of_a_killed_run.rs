//! A render killed outright leaves its hidden folder and lock file under its process id. When a later render has that
//! same process id, its check and its render still complete, and leave nothing of the earlier run beside the output path
//! (`docs/spec/render-command.md`, section 6).

mod common;

use std::fs::{self, File};
use std::path::PathBuf;

use common::{project_from_fixture, scratch};
use draw_cpu::CpuRenderer;
use project::Project;
use render::{check_place, staging_folder_of, write_film, Scene};

/// The names of the files in a folder, in order.
fn names_in(folder: &std::path::Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(folder)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    names
}

/// The hidden folder and the lock file of a run under the process id of this process, as a killed run left them: the
/// lock is held by nothing, and the folder holds the part of a film.
fn killed_run_under_this_process(output: &std::path::Path) -> (PathBuf, PathBuf) {
    let folder = staging_folder_of(output);
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join("film.part.mp4"), b"what a killed render left").unwrap();
    let lock = output.with_file_name(format!(".framesmith-{}.lock", std::process::id()));
    File::create(&lock).unwrap();
    (folder, lock)
}

#[test]
fn a_killed_run_under_this_process_id_does_not_refuse_the_check_or_the_render_that_follow() {
    let project = project_from_fixture("pid-reuse-of-a-killed-run", "next-render", "first-render");
    let scene = Scene::from_project(&Project::open(&project).unwrap()).unwrap_or_else(|reports| panic!("{reports:?}"));
    let base = scratch("pid-reuse-of-a-killed-run", "output");
    let output = base.join("film.mp4");
    let (folder, lock) = killed_run_under_this_process(&output);

    check_place(&output).unwrap_or_else(|refusal| panic!("the check refused the place: {}", refusal.reason()));
    let check_left = (folder.exists(), lock.exists());

    write_film(&scene, &mut CpuRenderer, &output, 128).unwrap_or_else(|failure| panic!("{failure}"));

    assert_eq!(
        check_left,
        (false, false),
        "the check left the earlier run's folder or lock, or both"
    );
    assert_eq!(
        names_in(&base),
        vec!["film.mp4".to_string()],
        "the render left a file beside the output path"
    );
}
