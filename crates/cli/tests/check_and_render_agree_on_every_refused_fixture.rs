//! `check` and `render` print the same list and exit the same for every fixture project that either
//! command refuses: every invalid form of `fixtures/forms-not-listed`, the mixed and unnamed refusal
//! fixtures, and every fixture of the support table. Each failure names the fixture whose lists differ.

mod common;

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use common::{err, framesmith, out, scratch};

/// Every invalid fixture: an invalid form of `fixtures/forms-not-listed`, and the mixed and unnamed
/// refusal fixtures. `check` and `render` both exit 1 on each.
const INVALID: [&str; 15] = [
    "forms-not-listed/duration-without-start",
    "forms-not-listed/font-variant-numeric-normal-in-a-list",
    "forms-not-listed/font-variant-numeric-slashed-zero",
    "forms-not-listed/left-with-pixels",
    "forms-not-listed/margin-five-numbers",
    "forms-not-listed/margin-percent",
    "forms-not-listed/position-fixed",
    "forms-not-listed/start-without-duration",
    "forms-not-listed/text-shadow-negative-blur",
    "forms-not-listed/text-shadow-two-words",
    "forms-not-listed/text-transform-lowercase",
    "forms-not-listed/white-space-pre",
    "forms-not-listed/width-with-em",
    "refusals-mixed",
    "refusals-unnamed",
];

/// The unsupported forms of `fixtures/forms-not-listed` (the rest of that folder is invalid).
const UNSUPPORTED_FORMS: [&str; 2] = ["forms-not-listed/left-fraction", "forms-not-listed/text-stroke-alpha"];

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Every fixture folder of the support table: each folder with a composition, so that a form added
/// beside its entry (such as `s37-tween` beside `s37`) is compared too.
fn support_fixtures() -> Vec<PathBuf> {
    let mut found = Vec::new();
    for entry in fs::read_dir(repository().join("fixtures/support-table")).unwrap() {
        let folder = entry.unwrap().path();
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

fn label_of(folder: &Path) -> String {
    folder
        .strip_prefix(repository().join("fixtures"))
        .unwrap_or(folder)
        .to_string_lossy()
        .replace('/', "_")
}

/// Both commands over one refused fixture: the same printed list, the same exit, nothing on the
/// error stream.
fn agrees(name: &str, from: &Path, exit: i32) {
    let (project, cwd) = project_of("check-and-render-agree", name, from);
    let check = framesmith(&cwd, [OsStr::new("check"), project.as_os_str()], &[("NO_COLOR", "1")]);
    // No `--output`: `render` uses the project's own output, so a setting it refuses (such as S34)
    // is the one in force.
    let render = framesmith(
        &cwd,
        [
            OsStr::new("render"),
            project.as_os_str(),
            OsStr::new("--backend"),
            OsStr::new("cpu"),
        ],
        &[("NO_COLOR", "1")],
    );
    assert_eq!(check.status.code(), Some(exit), "{name}: check: {}{}", out(&check), err(&check));
    assert_eq!(render.status.code(), Some(exit), "{name}: render: {}{}", out(&render), err(&render));
    assert_eq!(out(&check), out(&render), "{name}: check and render print different lists");
    assert_eq!(err(&check), "", "{name}: check wrote to the error stream");
    assert_eq!(err(&render), "", "{name}: render wrote to the error stream");
}

#[test]
fn check_and_render_print_the_same_list_and_exit_for_every_refused_fixture() {
    for name in INVALID {
        agrees(name, &repository().join("fixtures").join(name), 1);
    }
    for name in UNSUPPORTED_FORMS {
        agrees(name, &repository().join("fixtures").join(name), 2);
    }
    for folder in support_fixtures() {
        agrees(&label_of(&folder), &folder, 2);
    }
}
