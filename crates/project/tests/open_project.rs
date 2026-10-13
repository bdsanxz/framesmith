//! Opening a project folder: the composition found, the root judged, and nothing written.

use project::{Code, Kind, NoProject, OpenError, Project, COMPOSITION_FILE};
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

/// What a report says after its place: the code, its kind, the message as a sentence and the
/// remedy (`docs/spec/error-catalogue.md` section 3).
fn statement(code: Code, message: &str) -> String {
    format!("{code}: {}: {message}. {}", code.kind().as_str(), code.remedy())
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
fn a_wrong_root_is_reported_with_the_file_the_line_and_the_code() {
    let dir = folder("wrong-root");
    fs::write(
        dir.join(COMPOSITION_FILE),
        "<composition version=\"1\"\n width=\"641\" height=\"2\" duration=\"1\"/>",
    )
    .unwrap();
    match Project::open(&dir) {
        Err(OpenError::Invalid(reports)) => {
            assert_eq!(reports.len(), 1);
            assert_eq!(reports[0].file(), "composition.xml");
            assert_eq!(reports[0].problem().line(), 2);
            assert_eq!(reports[0].problem().code(), Code::SizeNotAccepted);
            assert_eq!(
                reports[0].to_string(),
                format!(
                    "composition.xml, line 2: {}",
                    statement(
                        Code::SizeNotAccepted,
                        "`width` must be an even whole number from 2 to 8192, not 641"
                    )
                )
            );
            assert_eq!(reports[0].problem().kind(), Kind::Invalid);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn every_problem_of_the_composition_is_one_report_in_the_order_of_the_lines() {
    let dir = folder("several-problems");
    fs::write(
        dir.join(COMPOSITION_FILE),
        "<composition\n version=\"1\"\n width=\"641\"\n height=\"360\"\n fps=\"0\"\n duration=\"2\"\n background=\"red\"/>",
    )
    .unwrap();
    let error = Project::open(&dir).unwrap_err();
    let OpenError::Invalid(reports) = &error else {
        panic!("{error:?}");
    };
    assert_eq!(
        reports
            .iter()
            .map(|report| (report.file(), report.problem().line()))
            .collect::<Vec<_>>(),
        [("composition.xml", 3), ("composition.xml", 5), ("composition.xml", 7)]
    );
    let text = error.to_string();
    assert_eq!(text.lines().count(), 3, "{text}");
    for (line, code) in text.lines().zip(["FS-0007", "FS-0008", "FS-0012"]) {
        assert!(line.starts_with("composition.xml, line "), "{line}");
        assert!(line.contains(&format!(": {code}: invalid: ")), "{line}");
    }
}

#[test]
fn a_folder_that_does_not_exist_is_no_project() {
    let dir = folder("missing").join("nothing-here");
    let error = Project::open(&dir).unwrap_err();
    assert!(matches!(error, OpenError::NoProject(NoProject::FolderMissing(_))), "{error:?}");
    assert_eq!(
        error.to_string(),
        format!(
            "No project: {}",
            statement(Code::FolderMissing, &format!("the folder `{}` does not exist", dir.display()))
        )
    );
    let OpenError::NoProject(reason) = &error else {
        panic!("{error:?}");
    };
    assert_eq!((reason.code(), reason.kind()), (Code::FolderMissing, Kind::Invalid));
}

#[test]
fn a_path_that_is_a_file_is_no_project() {
    let dir = folder("a-file");
    let file = dir.join(COMPOSITION_FILE);
    fs::write(&file, VALID).unwrap();
    let error = Project::open(&file).unwrap_err();
    assert!(matches!(error, OpenError::NoProject(NoProject::NotAFolder(_))), "{error:?}");
    assert_eq!(
        error.to_string(),
        format!(
            "No project: {}",
            statement(Code::NotAFolder, &format!("`{}` is not a folder", file.display()))
        )
    );
}

#[test]
fn a_folder_with_no_composition_is_no_project() {
    let dir = folder("empty");
    let error = Project::open(&dir).unwrap_err();
    assert!(matches!(error, OpenError::NoProject(NoProject::NoComposition(_))), "{error:?}");
    assert_eq!(
        error.to_string(),
        format!(
            "No project: {}",
            statement(Code::NoComposition, &format!("there is no composition.xml in `{}`", dir.display()))
        )
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
        text.starts_with(&format!(
            "No project: FS-0016: failure: composition.xml in `{}` could not be read: ",
            dir.display()
        )),
        "{text}"
    );
    assert!(text.ends_with(Code::CompositionUnreadable.remedy()), "{text}");
    assert!(!text.contains("os error"), "{text}");
    let OpenError::NoProject(reason) = &error else {
        panic!("{error:?}");
    };
    assert_eq!((reason.code(), reason.kind()), (Code::CompositionUnreadable, Kind::Failure));
}
