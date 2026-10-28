//! The sweep of the hidden staging folders of renders: the lock file of a run decides. A lock file no render holds is
//! removed with its folder, whatever process id its name carries, a lock file a render holds is left with its folder, and a
//! name that is not a run's lock file, or a folder with no lock file, is never touched (`docs/spec/render-command.md`,
//! section 6).

mod common;

use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::Command;

use common::scratch;
use render::remove_stale_staging;

/// The lock file of the process `pid` in `folder`, with the hidden staging folder and a staging file of the run beside it.
fn run_left_behind(folder: &Path, pid: u32) -> (PathBuf, PathBuf) {
    let lock = folder.join(format!(".framesmith-{pid}.lock"));
    File::create(&lock).unwrap();
    let hidden = folder.join(format!(".framesmith-{pid}"));
    fs::create_dir_all(&hidden).unwrap();
    fs::write(hidden.join("film.part.mp4"), b"what a killed render left").unwrap();
    (lock, hidden)
}

/// The process id of a program that has been waited for: it does not run any more.
fn finished_pid() -> u32 {
    let mut finished = Command::new("/usr/bin/true").spawn().unwrap();
    let pid = finished.id();
    finished.wait().unwrap();
    pid
}

#[test]
fn a_run_whose_lock_no_render_holds_is_removed_with_its_folder() {
    let folder = scratch("staging-folder-sweep", "no-holder");
    let (lock, hidden) = run_left_behind(&folder, finished_pid());

    remove_stale_staging(&folder);

    assert!(!hidden.exists(), "the staging folder of a run that no render holds is still there");
    assert!(!lock.exists(), "the lock file of a run that no render holds is still there");
}

#[test]
fn a_lock_file_left_with_no_folder_is_removed() {
    let folder = scratch("staging-folder-sweep", "lock-only");
    let lock = folder.join(format!(".framesmith-{}.lock", finished_pid()));
    File::create(&lock).unwrap();

    remove_stale_staging(&folder);

    assert!(
        !lock.exists(),
        "a lock file that no render holds, left by a render killed before its folder, is still there"
    );
}

#[test]
fn a_run_whose_lock_a_render_holds_is_left_as_it_is() {
    let folder = scratch("staging-folder-sweep", "held");
    let (lock, hidden) = run_left_behind(&folder, finished_pid());
    let held = File::open(&lock).unwrap();
    held.try_lock().unwrap();

    remove_stale_staging(&folder);

    assert!(
        hidden.join("film.part.mp4").is_file(),
        "the staging folder of a render that holds its lock was touched"
    );
    assert!(lock.is_file(), "the lock file of a render that holds its lock was removed");
    drop(held);
}

#[test]
fn a_dead_render_whose_process_id_a_live_process_has_now_is_removed() {
    let folder = scratch("staging-folder-sweep", "reused-process-id");
    let (lock, hidden) = run_left_behind(&folder, std::process::id());

    remove_stale_staging(&folder);

    assert!(
        !hidden.exists() && !lock.exists(),
        "a run whose lock no render holds was kept because its process id is running"
    );
}

#[test]
fn a_folder_with_no_lock_file_is_left_as_it_is() {
    let folder = scratch("staging-folder-sweep", "no-lock-file");
    let hidden = folder.join(format!(".framesmith-{}", finished_pid()));
    fs::create_dir_all(&hidden).unwrap();

    remove_stale_staging(&folder);

    assert!(hidden.is_dir(), "a folder with no lock file was removed");
}

#[test]
fn a_name_that_is_not_a_run_lock_file_is_never_touched() {
    let folder = scratch("staging-folder-sweep", "other-names");
    let pid = finished_pid();
    let as_a_folder = folder.join(format!(".framesmith-{pid}.lock"));
    fs::create_dir_all(&as_a_folder).unwrap();
    let not_a_number = folder.join(".framesmith-soon.lock");
    File::create(&not_a_number).unwrap();
    let other = folder.join("notes.lock");
    File::create(&other).unwrap();

    remove_stale_staging(&folder);

    assert!(as_a_folder.is_dir(), "a folder with the name of a run's lock file was removed");
    assert!(
        not_a_number.is_file(),
        "a lock file that is not named with a process id was removed"
    );
    assert!(other.is_file(), "an ordinary file was removed");
}
