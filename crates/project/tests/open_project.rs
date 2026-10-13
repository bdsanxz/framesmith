//! Opening a project folder: the composition found, the root judged, and nothing written.

use project::{NoProject, OpenError, Project, COMPOSITION_FILE};
use std::fs;
use std::path::{Path, PathBuf};

/// A new folder named `name` for one test, under the tests' scratch folder.
fn folder(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("project-open").join(name);
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    dir
}

const VALID: &str = r#"<composition version="1" id="demo" width="1280" height="720" duration="5"/>"#;

/// Every file under `dir` with its bytes, in order.
fn snapshot(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut files = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(next) = pending.pop() {
        for entry in fs::read_dir(&next).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else {
                files.push((path.clone(), fs::read(&path).unwrap()));
            }
        }
    }
    files.sort();
    files
}

#[test]
fn a_folder_with_a_composition_and_no_project_file_is_a_project() {
    let dir = folder("complete");
    fs::write(dir.join(COMPOSITION_FILE), VALID).unwrap();
    let project = Project::open(&dir).unwrap();
    assert_eq!(project.folder(), dir);
    assert_eq!(project.root().id.as_deref(), Some("demo"));
    assert_eq!((project.root().width, project.root().height), (1280, 720));
    assert_eq!((project.root().fps, project.root().frames), (30, 150));
}

#[test]
fn opening_changes_nothing_in_the_folder() {
    let dir = folder("unchanged");
    fs::write(dir.join(COMPOSITION_FILE), VALID).unwrap();
    fs::create_dir(dir.join("media")).unwrap();
    fs::write(dir.join("media").join("clip.bin"), [0u8, 1, 2, 255]).unwrap();
    let before = snapshot(&dir);
    Project::open(&dir).unwrap();
    assert_eq!(snapshot(&dir), before);
    fs::write(dir.join(COMPOSITION_FILE), VALID.replace("version=\"1\"", "")).unwrap();
    let before = snapshot(&dir);
    assert!(matches!(Project::open(&dir), Err(OpenError::Invalid(_))));
    assert_eq!(snapshot(&dir), before);
}

#[test]
fn a_wrong_root_is_reported_with_its_line() {
    let dir = folder("wrong-root");
    fs::write(
        dir.join(COMPOSITION_FILE),
        "<composition version=\"1\"\n width=\"641\" height=\"2\" duration=\"1\"/>",
    )
    .unwrap();
    match Project::open(&dir) {
        Err(OpenError::Invalid(problem)) => {
            assert_eq!(problem.line(), 2);
            assert_eq!(
                OpenError::Invalid(problem).to_string(),
                "line 2: `width` must be an even whole number from 2 to 8192, not 641"
            );
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_folder_that_does_not_exist_is_no_project() {
    let dir = folder("missing").join("nothing-here");
    let error = Project::open(&dir).unwrap_err();
    assert!(matches!(error, OpenError::NoProject(NoProject::FolderMissing(_))), "{error:?}");
    assert_eq!(
        error.to_string(),
        format!("No project: the folder `{}` does not exist", dir.display())
    );
}

#[test]
fn a_path_that_is_a_file_is_no_project() {
    let dir = folder("a-file");
    let file = dir.join(COMPOSITION_FILE);
    fs::write(&file, VALID).unwrap();
    let error = Project::open(&file).unwrap_err();
    assert!(matches!(error, OpenError::NoProject(NoProject::NotAFolder(_))), "{error:?}");
    assert_eq!(error.to_string(), format!("No project: `{}` is not a folder", file.display()));
}

#[test]
fn a_folder_with_no_composition_is_no_project() {
    let dir = folder("empty");
    let error = Project::open(&dir).unwrap_err();
    assert!(matches!(error, OpenError::NoProject(NoProject::NoComposition(_))), "{error:?}");
    assert_eq!(
        error.to_string(),
        format!("No project: there is no composition.xml in `{}`", dir.display())
    );
    assert!(
        fs::read_dir(&dir).unwrap().next().is_none(),
        "a folder was opened and something was written"
    );
}

#[test]
fn a_composition_that_cannot_be_read_is_no_project_with_the_reason() {
    let dir = folder("unreadable");
    fs::create_dir(dir.join(COMPOSITION_FILE)).unwrap();
    let error = Project::open(&dir).unwrap_err();
    assert!(matches!(error, OpenError::NoProject(NoProject::Unreadable(..))), "{error:?}");
    let text = error.to_string();
    assert!(
        text.starts_with(&format!("No project: composition.xml in `{}` could not be read: ", dir.display())),
        "{text}"
    );
    assert!(!text.contains("os error"), "{text}");
}
