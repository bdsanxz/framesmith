//! The sweep of the hidden staging folders of renders: a folder whose process no longer runs is removed, and a folder
//! whose process still runs, the process of the caller included, is left as it is; a name that is not a staging folder
//! is never touched (`docs/spec/render-command.md`, section 6).

mod common;

use std::fs;
use std::process::{Command, Stdio};

use common::scratch;
use render::{process_runs, remove_stale_staging};

/// A hidden staging folder of the process `pid` in `folder`, holding a staging file.
fn staging_folder(folder: &std::path::Path, pid: u32) -> std::path::PathBuf {
    let hidden = folder.join(format!(".framesmith-{pid}"));
    fs::create_dir_all(&hidden).unwrap();
    fs::write(hidden.join("film.part.mp4"), b"what a killed render left").unwrap();
    hidden
}

#[test]
fn a_staging_folder_whose_process_no_longer_runs_is_removed() {
    let folder = scratch("staging-folder-sweep", "dead-process");
    let mut finished = Command::new("/usr/bin/true").spawn().unwrap();
    let pid = finished.id();
    finished.wait().unwrap();
    assert!(!process_runs(pid), "a process that has been waited for does not run");
    let hidden = staging_folder(&folder, pid);

    remove_stale_staging(&folder);

    assert!(!hidden.exists(), "the staging folder of a process that does not run is still there");
}

#[test]
fn a_staging_folder_whose_process_still_runs_is_left_as_it_is() {
    let folder = scratch("staging-folder-sweep", "live-process");
    let mut running = Command::new("/bin/sleep")
        .arg("30")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let pid = running.id();
    assert!(process_runs(pid), "a process that runs is seen running");
    let hidden = staging_folder(&folder, pid);

    remove_stale_staging(&folder);
    running.kill().unwrap();
    running.wait().unwrap();

    assert!(
        hidden.join("film.part.mp4").is_file(),
        "the staging folder of a running process was touched"
    );
}

#[test]
fn the_staging_folder_of_the_caller_is_left_as_it_is() {
    let folder = scratch("staging-folder-sweep", "own-process");
    let hidden = staging_folder(&folder, std::process::id());

    remove_stale_staging(&folder);

    assert!(
        hidden.join("film.part.mp4").is_file(),
        "the staging folder of the running caller was touched"
    );
}

#[test]
fn a_name_that_is_not_a_staging_folder_is_never_touched() {
    let folder = scratch("staging-folder-sweep", "other-names");
    let mut finished = Command::new("/usr/bin/true").spawn().unwrap();
    let pid = finished.id();
    finished.wait().unwrap();
    let as_a_file = folder.join(format!(".framesmith-{pid}"));
    fs::write(&as_a_file, b"a file, not a folder").unwrap();
    let not_a_number = folder.join(".framesmith-soon");
    fs::create_dir_all(&not_a_number).unwrap();
    let other = folder.join("notes");
    fs::create_dir_all(&other).unwrap();

    remove_stale_staging(&folder);

    assert!(as_a_file.is_file(), "a file with the name of a staging folder was removed");
    assert!(not_a_number.is_dir(), "a folder that is not named with a process id was removed");
    assert!(other.is_dir(), "an ordinary folder was removed");
}
