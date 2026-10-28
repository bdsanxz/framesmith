//! A hidden staging folder that cannot be removed keeps its lock file beside it, at the end of a run and in the sweep, so
//! that no folder is ever left without its lock file and the next sweep takes both (`docs/spec/render-command.md`,
//! section 6).

mod common;

use std::fs::{self, File, Permissions};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use common::{project_from_fixture, scratch};
use draw_cpu::CpuRenderer;
use project::{Codec, Project};
use render::{remove_stale_staging, staging_folder_of, write_film_stepped, Scene, Step};

/// Makes a folder that its own holder cannot empty: a subfolder with no permissions, holding a file.
fn stuck_folder(folder: &Path) -> std::path::PathBuf {
    let stuck = folder.join("stuck");
    fs::create_dir_all(&stuck).unwrap();
    fs::write(stuck.join("film.part.mp4"), b"a folder that cannot be emptied").unwrap();
    fs::set_permissions(&stuck, Permissions::from_mode(0o000)).unwrap();
    stuck
}

#[test]
fn a_run_whose_folder_cannot_be_removed_keeps_its_lock_file() {
    let project = project_from_fixture("lock-outlives-folder", "run-end", "first-render");
    let scene =
        Scene::from_project(&Project::open(&project).unwrap(), &render::system_codecs()).unwrap_or_else(|reports| panic!("{reports:?}"));
    let base = scratch("lock-outlives-folder", "run-end-output");
    let output = base.join("film.mp4");
    let folder = staging_folder_of(&output);
    let lock = base.join(format!(".framesmith-{}.lock", std::process::id()));
    let mut stuck = None;
    let written = write_film_stepped(&scene, Vec::new(), &mut CpuRenderer, &output, 128, Codec::H264, None, &mut |step| {
        // Once the hidden folder is made, one of its subfolders is made that the run cannot empty at its end.
        if step == Step::Made {
            stuck = Some(stuck_folder(&folder));
        }
    });
    let lock_kept = lock.exists();
    let folder_kept = folder.exists();
    if let Some(stuck) = stuck {
        fs::set_permissions(&stuck, Permissions::from_mode(0o755)).unwrap();
    }
    written.unwrap_or_else(|failure| panic!("{failure}"));
    assert!(folder_kept, "the hidden folder that could not be removed is gone");
    assert!(
        lock_kept,
        "the lock file went with a folder that stayed, so the sweep cannot take the folder"
    );
}

#[test]
fn a_sweep_whose_folder_cannot_be_removed_keeps_the_lock_file_until_it_can() {
    let base = scratch("lock-outlives-folder", "sweep");
    let pid = 4_242_424;
    let lock = base.join(format!(".framesmith-{pid}.lock"));
    File::create(&lock).unwrap();
    let hidden = base.join(format!(".framesmith-{pid}"));
    let stuck = stuck_folder(&hidden);

    remove_stale_staging(&base);
    let lock_kept = lock.exists();
    let folder_kept = hidden.exists();

    fs::set_permissions(&stuck, Permissions::from_mode(0o755)).unwrap();
    remove_stale_staging(&base);

    assert!(
        folder_kept,
        "the folder of a run that no render holds, which could not be removed, is gone"
    );
    assert!(
        lock_kept,
        "the lock file went with a folder that stayed, so the next sweep cannot take the folder"
    );
    assert!(
        !hidden.exists(),
        "the folder is still there after the next sweep, which could remove it"
    );
    assert!(
        !lock.exists(),
        "the lock file is still there after the next sweep, which could remove the folder"
    );
}
