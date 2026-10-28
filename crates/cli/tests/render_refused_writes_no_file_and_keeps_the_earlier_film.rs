//! A `render` that stops before the first frame writes no file and keeps the bytes of a video
//! already at the output path, with no staging folder left in the output folder: here for an
//! invalid project (the support-table entries are covered by `support_table_refusals`).

mod common;

use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use common::{err, framesmith, out, scratch};

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Copies a folder and what it holds.
fn copy(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// Every file and folder under a folder, as paths from it.
fn files_under(folder: &Path) -> BTreeSet<PathBuf> {
    let mut found = BTreeSet::new();
    let mut pending = vec![folder.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path.clone());
            }
            found.insert(path.strip_prefix(folder).unwrap().to_path_buf());
        }
    }
    found
}

#[test]
fn a_refused_render_writes_no_file_and_keeps_the_earlier_film() {
    let base = scratch("refused-render-writes-nothing", "invalid");
    let project = base.join("demo");
    let cwd = base.join("cwd");
    copy(&repository().join("fixtures/forms-not-listed/position-fixed"), &project);
    fs::create_dir_all(&cwd).unwrap();
    // A video already at the output path, whose bytes the refused render must keep.
    let out_dir = base.join("out");
    fs::create_dir_all(&out_dir).unwrap();
    let film = out_dir.join("film.mp4");
    let earlier = b"a film made before this run".to_vec();
    fs::write(&film, &earlier).unwrap();
    let before = files_under(&out_dir);

    let check = framesmith(&cwd, [OsStr::new("check"), project.as_os_str()], &[("NO_COLOR", "1")]);
    let render = framesmith(
        &cwd,
        [
            OsStr::new("render"),
            project.as_os_str(),
            OsStr::new("--backend"),
            OsStr::new("cpu"),
            OsStr::new("--output"),
            film.as_os_str(),
        ],
        &[("NO_COLOR", "1")],
    );
    assert_eq!(check.status.code(), Some(1), "check: {}{}", out(&check), err(&check));
    assert_eq!(render.status.code(), Some(1), "render: {}{}", out(&render), err(&render));
    assert_eq!(out(&check), out(&render), "check and render print different lists");
    assert!(!out(&render).contains("Rendered "), "{}", out(&render));
    assert_eq!(fs::read(&film).unwrap(), earlier, "the earlier film changed");
    assert_eq!(files_under(&out_dir), before, "the output folder gained a file");
}
