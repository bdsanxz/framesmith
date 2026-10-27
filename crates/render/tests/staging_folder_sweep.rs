//! The sweep of the hidden staging folders of renders: a folder whose lock no render holds is removed, whatever process id
//! its name carries, a folder whose lock a render holds is left as it is, and a name that is not a staging folder, or a
//! staging folder with no lock file, is never touched (`docs/spec/render-command.md`, section 6).

mod common;

use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::Command;

use common::scratch;
use render::remove_stale_staging;

/// A hidden staging folder of the process `pid` in `folder`, with its lock file and a staging file.
fn staging_folder(folder: &Path, pid: u32) -> PathBuf {
    let hidden = folder.join(format!(".framesmith-{pid}"));
    fs::create_dir_all(&hidden).unwrap();
    File::create(hidden.join("lock")).unwrap();
    fs::write(hidden.join("film.part.mp4"), b"what a killed render left").unwrap();
    hidden
}

/// The process id of a program that has been waited for: it does not run any more.
fn finished_pid() -> u32 {
    let mut finished = Command::new("/usr/bin/true").spawn().unwrap();
    let pid = finished.id();
    finished.wait().unwrap();
    pid
}

#[test]
fn a_staging_folder_that_no_render_holds_is_removed() {
    let folder = scratch("staging-folder-sweep", "no-holder");
    let hidden = staging_folder(&folder, finished_pid());

    remove_stale_staging(&folder);

    assert!(!hidden.exists(), "a staging folder that no render holds is still there");
}

#[test]
fn a_staging_folder_whose_lock_a_render_holds_is_left_as_it_is() {
    let folder = scratch("staging-folder-sweep", "held");
    let hidden = staging_folder(&folder, finished_pid());
    let held = File::open(hidden.join("lock")).unwrap();
    held.try_lock().unwrap();

    remove_stale_staging(&folder);

    assert!(
        hidden.join("film.part.mp4").is_file(),
        "the staging folder of a render that holds its lock was touched"
    );
    drop(held);
}

#[test]
fn a_dead_render_whose_process_id_a_live_process_has_now_is_removed() {
    let folder = scratch("staging-folder-sweep", "reused-process-id");
    let hidden = staging_folder(&folder, std::process::id());

    remove_stale_staging(&folder);

    assert!(
        !hidden.exists(),
        "a staging folder whose lock no render holds was kept because its process id is running"
    );
}

#[test]
fn a_staging_folder_with_no_lock_file_is_left_as_it_is() {
    let folder = scratch("staging-folder-sweep", "no-lock-file");
    let hidden = folder.join(format!(".framesmith-{}", finished_pid()));
    fs::create_dir_all(&hidden).unwrap();

    remove_stale_staging(&folder);

    assert!(hidden.is_dir(), "a folder with no lock file was removed");
}

#[test]
fn a_name_that_is_not_a_staging_folder_is_never_touched() {
    let folder = scratch("staging-folder-sweep", "other-names");
    let pid = finished_pid();
    let as_a_file = folder.join(format!(".framesmith-{pid}"));
    fs::write(&as_a_file, b"a file, not a folder").unwrap();
    let not_a_number = folder.join(".framesmith-soon");
    fs::create_dir_all(&not_a_number).unwrap();
    fs::write(not_a_number.join("lock"), b"").unwrap();
    let other = folder.join("notes");
    fs::create_dir_all(&other).unwrap();
    fs::write(other.join("lock"), b"").unwrap();

    remove_stale_staging(&folder);

    assert!(as_a_file.is_file(), "a file with the name of a staging folder was removed");
    assert!(not_a_number.is_dir(), "a folder that is not named with a process id was removed");
    assert!(other.is_dir(), "an ordinary folder was removed");
}
