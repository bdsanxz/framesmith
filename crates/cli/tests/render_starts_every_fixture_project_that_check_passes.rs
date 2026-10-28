//! Every fixture project that `check` passes is one `render` starts, except the minute film named
//! below: `render` on the processor's renderer exits 0 and writes its film. The refused fixtures
//! (`fixtures/forms-not-listed`, `fixtures/refusals-mixed`, `fixtures/refusals-unnamed`,
//! `fixtures/support-table`) are compared by `check_and_render_agree_on_every_refused_fixture`
//! instead and are not walked here.

mod common;

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use common::{err, framesmith, out, scratch};

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Every valid fixture project but one: a top-level fixture folder with a composition, outside the
/// refused sets. `motion-text-a-minute` is not walked here: its sixty seconds take twenty-one minutes
/// to render in a debug build (probe `_swe/edd/n01-t091/work/minute/render.out`, exit 0), longer than
/// the rest of the set together; the reference-machine card suite renders it, and the probe below is
/// its evidence here. Time, not behaviour, is the reason.
fn valid_fixtures() -> Vec<PathBuf> {
    let mut found = Vec::new();
    for entry in fs::read_dir(repository().join("fixtures")).unwrap() {
        let folder = entry.unwrap().path();
        let name = folder.file_name().unwrap().to_string_lossy().to_string();
        if [
            "forms-not-listed",
            "refusals-mixed",
            "refusals-unnamed",
            "support-table",
            "fonts",
            "motion-text-a-minute",
        ]
        .contains(&name.as_str())
        {
            continue;
        }
        if folder.join("composition.xml").is_file() {
            found.push(folder);
        }
    }
    found.sort();
    found
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

/// A copy of a fixture to run, with the repository's test fonts in its `fonts` folder, and the
/// folder to run from.
fn project_of(group: &str, name: &str, from: &Path) -> (PathBuf, PathBuf) {
    let base = scratch(group, name);
    let project = base.join("demo");
    let cwd = base.join("cwd");
    copy(from, &project);
    copy(&repository().join("fixtures/fonts"), &project.join("fonts"));
    fs::remove_file(project.join("expected")).ok();
    fs::create_dir_all(&cwd).unwrap();
    (project, cwd)
}

#[test]
fn render_starts_every_fixture_project_that_check_passes() {
    let fixtures = valid_fixtures();
    assert!(!fixtures.is_empty(), "the fixtures were walked");
    for folder in fixtures {
        let name = folder.file_name().unwrap().to_string_lossy().to_string();
        let (project, cwd) = project_of("check-passes-so-render-starts", &name, &folder);
        let film = cwd.join("film.mp4");
        let check = framesmith(&cwd, [OsStr::new("check"), project.as_os_str()], &[("NO_COLOR", "1")]);
        assert_eq!(check.status.code(), Some(0), "{name}: check: {}{}", out(&check), err(&check));
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
        assert_eq!(render.status.code(), Some(0), "{name}: render: {}{}", out(&render), err(&render));
        assert!(
            out(&render).starts_with(&format!("Rendered {}", film.display())),
            "{name}: {}",
            out(&render)
        );
        assert!(film.is_file(), "{name}: render wrote no film");
    }
}
