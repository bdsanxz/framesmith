//! Naming a project by a folder, a project file or a composition file: the project folder that is
//! found, the composition that is read, the base of every written path, and what is reported for a
//! path that is not there (`docs/spec/project-location.md`).

use project::{Code, Kind, NoProject, OpenError, Project};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

const FIVE_SECONDS: &str = r#"<composition version="1" id="five" width="1280" height="720" duration="5"/>"#;
const TWO_SECONDS: &str = r#"<composition version="1" id="two" width="640" height="360" duration="2"/>"#;
/// A root with an odd width: one report.
const ODD_WIDTH: &str = r#"<composition version="1" width="641" height="360" duration="2"/>"#;

/// A new folder named `name` for one test, under the tests' scratch folder. No `framesmith.yaml`
/// may be at or above it, or a test about "none above" would judge the machine and not the code.
fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("locating-the-project").join(name);
    if dir.exists() {
        make_writable(&dir);
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    let real = fs::canonicalize(&dir).unwrap();
    for above in real.ancestors() {
        assert!(
            !above.join("framesmith.yaml").exists(),
            "the machine has a framesmith.yaml in {}, above the scratch folder of the tests",
            above.display()
        );
    }
    dir
}

/// Gives every folder under `dir` back its owner's permissions, so that it can be removed.
fn make_writable(dir: &Path) {
    let _ = fs::set_permissions(dir, fs::Permissions::from_mode(0o755));
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                make_writable(&entry.path());
            }
        }
    }
}

/// Writes `text` to `dir/relative`, making the folders on the way.
fn write(dir: &Path, relative: &str, text: &str) -> PathBuf {
    let path = dir.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, text).unwrap();
    path
}

/// `statement` of a code: what follows the place on a line.
fn statement(code: Code, message: &str) -> String {
    format!("{code}: {}: {message}. {}", code.kind().as_str(), code.remedy())
}

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

fn real(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap()
}

#[test]
fn a_folder_a_project_file_and_a_composition_file_open_the_same_project() {
    let dir = scratch("same-project");
    write(&dir, "framesmith.yaml", "name: launch\n");
    let composition = write(&dir, "composition.xml", FIVE_SECONDS);
    let forms = [dir.clone(), dir.join("framesmith.yaml"), composition];
    let opened: Vec<Project> = forms.iter().map(|form| Project::open(form).unwrap()).collect();
    for project in &opened {
        assert_eq!(project.folder(), dir);
        assert_eq!(project.composition(), "composition.xml");
        assert_eq!(project.name(), "launch");
        assert_eq!(
            (project.root().width, project.root().height, project.root().frames),
            (1280, 720, 150)
        );
    }
    assert_eq!(opened[0].root(), opened[1].root());
    assert_eq!(opened[1].root(), opened[2].root());
}

#[test]
fn a_folder_is_the_project_folder_and_is_not_searched_upward() {
    let dir = scratch("folder-not-searched");
    write(&dir, "framesmith.yaml", "name: outer\n");
    write(&dir, "composition.xml", FIVE_SECONDS);
    let inner = write(&dir, "scenes/composition.xml", TWO_SECONDS).parent().unwrap().to_path_buf();
    // `scenes` holds no project file; the project file of the folder above does not make it a
    // part of that project, and it is read as a project of its own.
    let project = Project::open(&inner).unwrap();
    assert_eq!(project.folder(), inner);
    assert_eq!(project.name(), "scenes");
    assert_eq!(project.root().frames, 60);
}

#[test]
fn a_composition_file_in_a_subfolder_takes_the_nearest_folder_above_that_holds_a_project_file() {
    let dir = scratch("subfolder-composition");
    write(&dir, "framesmith.yaml", "name: launch\n");
    write(&dir, "composition.xml", FIVE_SECONDS);
    let intro = write(&dir, "scenes/intro.xml", TWO_SECONDS);
    let project = Project::open(&intro).unwrap();
    assert_eq!(project.folder(), real(&dir));
    assert_eq!(project.composition(), "scenes/intro.xml");
    assert_eq!(project.composition_path(), real(&dir).join("scenes").join("intro.xml"));
    // The project file of `dir` was read: its name is the project's. The composition read is
    // `intro.xml`, not the `composition.xml` beside the project file.
    assert_eq!(project.name(), "launch");
    assert_eq!(project.root().id.as_deref(), Some("two"));
    assert_eq!(project.root().frames, 60);
}

#[test]
fn the_nearest_project_file_above_wins_over_one_further_up() {
    let dir = scratch("nearest-wins");
    write(&dir, "framesmith.yaml", "name: outer\n");
    write(&dir, "inner/framesmith.yaml", "name: inner\n");
    let deep = write(&dir, "inner/scenes/deeper/intro.xml", TWO_SECONDS);
    let project = Project::open(&deep).unwrap();
    assert_eq!(project.folder(), real(&dir).join("inner"));
    assert_eq!(project.name(), "inner");
    assert_eq!(project.composition(), "scenes/deeper/intro.xml");
}

#[test]
fn a_composition_file_beside_its_project_file_has_that_folder_as_the_project_folder() {
    let dir = scratch("beside");
    write(&dir, "framesmith.yaml", "name: beside\n");
    let other = write(&dir, "other.xml", TWO_SECONDS);
    write(&dir, "composition.xml", FIVE_SECONDS);
    let project = Project::open(&other).unwrap();
    assert_eq!(project.folder(), dir);
    assert_eq!(project.composition(), "other.xml");
    assert_eq!(project.name(), "beside");
}

#[test]
fn a_composition_file_with_no_project_file_at_or_above_it_has_its_own_folder_as_the_project_folder() {
    let dir = scratch("no-project-file");
    let intro = write(&dir, "scenes/intro.xml", TWO_SECONDS);
    // Project files that are not at or above the file are not the file's: one in a folder
    // beside its folder and one in a folder below it.
    write(&dir, "other/framesmith.yaml", "name: other\n");
    write(&dir, "scenes/deeper/framesmith.yaml", "name: deeper\n");
    let project = Project::open(&intro).unwrap();
    assert_eq!(project.folder(), dir.join("scenes"));
    assert_eq!(project.composition(), "intro.xml");
    assert_eq!(project.name(), "scenes");
    // No project file: every key has its default.
    assert_eq!(project.output(), "renders/scenes.mp4");
    assert_eq!(project.root().frames, 60);
}

#[test]
fn the_composition_given_is_the_one_read_whatever_the_project_file_names() {
    let dir = scratch("given-wins");
    write(&dir, "framesmith.yaml", "composition: scenes/other.xml\n");
    let composition = write(&dir, "composition.xml", FIVE_SECONDS);
    // Named by the folder, the file the project file names is read, and it is not there.
    match Project::open(&dir) {
        Err(OpenError::NoProject(reason @ NoProject::NamedCompositionMissing(..))) => {
            assert_eq!(reason.code(), Code::NamedCompositionMissing);
        }
        other => panic!("{other:?}"),
    }
    // Named by the file, that file is the composition and the key picks nothing.
    let project = Project::open(&composition).unwrap();
    assert_eq!(project.composition(), "composition.xml");
    assert_eq!(project.root().frames, 150);
    // A `composition` value that is wrong is still reported with its line, and the composition
    // given is read all the same: its problems come after those of the project file.
    write(&dir, "framesmith.yaml", "composition: []\n");
    write(&dir, "composition.xml", ODD_WIDTH);
    match Project::open(&composition) {
        Err(OpenError::Invalid(reports)) => {
            let places: Vec<(&str, u32, Code)> = reports
                .iter()
                .map(|report| (report.file(), report.problem().line(), report.problem().code()))
                .collect();
            assert_eq!(
                places,
                [
                    ("framesmith.yaml", 1, Code::ValueNotAccepted),
                    ("composition.xml", 1, Code::SizeNotAccepted)
                ]
            );
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_report_names_a_composition_in_a_subfolder_by_its_path_from_the_project_folder() {
    let dir = scratch("report-names");
    write(&dir, "framesmith.yaml", "");
    let intro = write(&dir, "scenes/intro.xml", ODD_WIDTH);
    let error = Project::open(&intro).unwrap_err();
    let text = error.to_string();
    assert!(
        text.starts_with(&format!("scenes/intro.xml, line 1: {}", Code::SizeNotAccepted)),
        "{text}"
    );
    let OpenError::Invalid(reports) = &error else {
        panic!("{error:?}");
    };
    assert_eq!(reports[0].file(), "scenes/intro.xml");

    // With no project file above, the file's own folder is the project folder, and the file is
    // named by its own name.
    let alone = scratch("report-names-alone");
    let intro = write(&alone, "scenes/intro.xml", ODD_WIDTH);
    let OpenError::Invalid(reports) = Project::open(&intro).unwrap_err() else {
        panic!("not a report");
    };
    assert_eq!(reports[0].file(), "intro.xml");
}

#[test]
fn a_src_is_resolved_from_the_project_folder_and_not_from_the_compositions_folder() {
    let dir = scratch("src-base");
    write(&dir, "framesmith.yaml", "");
    // The same file name in both places, with different bytes.
    write(&dir, "logo.bin", "the project folder's logo");
    write(&dir, "scenes/logo.bin", "the composition folder's logo");
    write(&dir, "media/clip.bin", "the project folder's clip");
    let intro = write(
        &dir,
        "scenes/intro.xml",
        r#"<composition version="1" width="640" height="360" duration="2"><image src="logo.bin"/><video src="media/clip.bin"/></composition>"#,
    );
    let project = Project::open(&intro).unwrap();
    assert_eq!(project.composition(), "scenes/intro.xml");
    assert_eq!(
        fs::read_to_string(project.resolve("logo.bin")).unwrap(),
        "the project folder's logo"
    );
    assert_ne!(project.resolve("logo.bin"), real(&dir).join("scenes").join("logo.bin"));
    assert_eq!(project.resolve("logo.bin"), real(&dir).join("logo.bin"));
    assert_eq!(
        fs::read_to_string(project.resolve("media/clip.bin")).unwrap(),
        "the project folder's clip"
    );
    // The same base whichever way the project is named.
    for form in [dir.clone(), dir.join("framesmith.yaml")] {
        write(&dir, "framesmith.yaml", "composition: scenes/intro.xml\n");
        let project = Project::open(&form).unwrap();
        assert_eq!(project.composition(), "scenes/intro.xml");
        assert_eq!(
            fs::read_to_string(project.resolve("logo.bin")).unwrap(),
            "the project folder's logo"
        );
    }
    // A written path that begins with `/` is a path of the machine and is taken as it is.
    assert_eq!(project.resolve("/media/clip.bin"), PathBuf::from("/media/clip.bin"));
}

#[test]
fn with_no_project_file_the_compositions_own_folder_is_the_base_of_a_src() {
    let dir = scratch("src-base-alone");
    write(&dir, "logo.bin", "the folder above");
    write(&dir, "scenes/logo.bin", "the composition's own folder");
    let intro = write(&dir, "scenes/intro.xml", TWO_SECONDS);
    let project = Project::open(&intro).unwrap();
    assert_eq!(
        fs::read_to_string(project.resolve("logo.bin")).unwrap(),
        "the composition's own folder"
    );
}

#[test]
fn the_paths_of_the_project_file_are_relative_to_the_project_folder() {
    let dir = scratch("project-file-paths");
    write(
        &dir,
        "framesmith.yaml",
        "name: film\ncomposition: scenes/main.xml\npaths:\n  renders: out\nrender:\n  output: out/final/film.mp4\n",
    );
    write(&dir, "scenes/main.xml", TWO_SECONDS);
    let project = Project::open(&dir.join("framesmith.yaml")).unwrap();
    assert_eq!(project.composition(), "scenes/main.xml");
    assert_eq!(project.composition_path(), dir.join("scenes").join("main.xml"));
    assert_eq!(project.output(), "out/final/film.mp4");
    assert_eq!(project.resolve(&project.output()), dir.join("out").join("final").join("film.mp4"));
    assert_eq!(project.root().frames, 60);
}

#[test]
fn a_path_that_does_not_exist_is_no_project_and_the_reason_names_the_path() {
    let dir = scratch("path-missing");
    write(&dir, "composition.xml", FIVE_SECONDS);
    for missing in [
        dir.join("nothing.xml"),
        dir.join("scenes").join("intro.xml"),
        dir.join("nowhere").join("framesmith.yaml"),
        dir.join("nothing-here"),
        // A name on the way that is a file.
        dir.join("composition.xml").join("inside.xml"),
    ] {
        let error = Project::open(&missing).unwrap_err();
        assert!(matches!(error, OpenError::NoProject(NoProject::PathMissing(_))), "{error:?}");
        assert_eq!(
            error.to_string(),
            format!(
                "No project: {}",
                statement(Code::PathMissing, &format!("`{}` does not exist", missing.display()))
            )
        );
        let OpenError::NoProject(reason) = &error else {
            panic!("{error:?}");
        };
        assert_eq!((reason.code(), reason.kind()), (Code::PathMissing, Kind::Invalid));
    }
}

#[test]
fn a_closing_slash_names_a_folder() {
    let dir = scratch("closing-slash");
    write(&dir, "composition.xml", FIVE_SECONDS);
    // A folder written with a closing slash opens as the folder.
    let with_slash = PathBuf::from(format!("{}/", dir.display()));
    let project = Project::open(&with_slash).unwrap();
    assert_eq!(project.root().frames, 150);
    // One that is not there is a folder that is missing, and a file is not a folder.
    let missing = PathBuf::from(format!("{}/nothing/", dir.display()));
    assert!(matches!(
        Project::open(&missing),
        Err(OpenError::NoProject(NoProject::FolderMissing(_)))
    ));
    let file = PathBuf::from(format!("{}/composition.xml/", dir.display()));
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
fn a_folder_with_no_composition_names_the_folder_and_both_files_that_were_looked_for() {
    let empty = scratch("nothing-to-render");
    for form in [empty.clone(), empty.join("framesmith.yaml")] {
        // The second form needs a project file to be a path that exists.
        if form.ends_with("framesmith.yaml") {
            write(&empty, "framesmith.yaml", "name: nothing\n");
        }
        let error = Project::open(&form).unwrap_err();
        assert!(matches!(error, OpenError::NoProject(NoProject::NoComposition(_))), "{error:?}");
        let text = error.to_string();
        assert_eq!(
            text,
            format!(
                "No project: {}",
                statement(
                    Code::NoComposition,
                    &format!(
                        "there is no composition.xml in `{}`, and no framesmith.yaml there names a composition",
                        empty.display()
                    )
                )
            )
        );
        for looked_for in [empty.to_str().unwrap(), "composition.xml", "framesmith.yaml"] {
            assert!(text.contains(looked_for), "{looked_for} is not in: {text}");
        }
    }
    assert!(!empty.join("composition.xml").exists(), "something was written");
}

#[test]
fn a_composition_file_that_cannot_be_read_is_a_failure_that_names_the_path() {
    let dir = scratch("no-access");
    let file = write(&dir, "intro.xml", TWO_SECONDS);
    fs::set_permissions(&file, fs::Permissions::from_mode(0o000)).unwrap();
    let error = Project::open(&file).unwrap_err();
    fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(matches!(error, OpenError::NoProject(NoProject::PathUnreadable(..))), "{error:?}");
    let text = error.to_string();
    assert!(
        text.starts_with(&format!(
            "No project: {}: failure: `{}` could not be read: ",
            Code::PathUnreadable,
            file.display()
        )),
        "{text}"
    );
    assert!(text.ends_with(Code::PathUnreadable.remedy()), "{text}");
    assert!(!text.contains("os error"), "{text}");
    let OpenError::NoProject(reason) = &error else {
        panic!("{error:?}");
    };
    assert_eq!((reason.code(), reason.kind()), (Code::PathUnreadable, Kind::Failure));
}

#[test]
fn a_project_file_above_a_composition_file_that_cannot_be_read_is_reported_with_its_folder() {
    let dir = scratch("project-file-above-unreadable");
    fs::create_dir_all(dir.join("framesmith.yaml")).unwrap();
    let intro = write(&dir, "scenes/intro.xml", TWO_SECONDS);
    match Project::open(&intro) {
        Err(OpenError::NoProject(reason @ NoProject::ProjectFileUnreadable(..))) => {
            assert_eq!((reason.code(), reason.kind()), (Code::ProjectFileUnreadable, Kind::Failure));
            assert!(
                reason
                    .to_string()
                    .starts_with(&format!("framesmith.yaml in `{}` could not be read: ", real(&dir).display())),
                "{reason}"
            );
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn something_that_is_neither_a_folder_nor_a_file_is_not_read() {
    let dir = scratch("named-pipe");
    let pipe = dir.join("pipe.xml");
    let made = std::process::Command::new("/usr/bin/mkfifo").arg(&pipe).status().unwrap();
    assert!(made.success());
    // Opening it for reading would wait for ever for a writer: the answer must come at once.
    let (sender, receiver) = std::sync::mpsc::channel();
    let given = pipe.clone();
    std::thread::spawn(move || {
        let _ = sender.send(Project::open(&given).map(|_| ()));
    });
    let answer = receiver
        .recv_timeout(std::time::Duration::from_secs(20))
        .expect("the named pipe was waited for");
    let error = answer.unwrap_err();
    assert!(matches!(error, OpenError::NoProject(NoProject::NotAFolder(_))), "{error:?}");
}

#[test]
fn a_link_to_a_folder_is_followed_to_the_real_folder_when_the_project_file_is_looked_for() {
    let dir = scratch("link");
    write(&dir, "real/framesmith.yaml", "name: real\n");
    write(&dir, "real/scenes/intro.xml", TWO_SECONDS);
    std::os::unix::fs::symlink(dir.join("real").join("scenes"), dir.join("alias")).unwrap();
    let project = Project::open(&dir.join("alias").join("intro.xml")).unwrap();
    assert_eq!(project.folder(), real(&dir).join("real"));
    assert_eq!(project.composition(), "scenes/intro.xml");
    assert_eq!(project.name(), "real");
}

#[test]
fn a_path_with_dots_in_it_finds_the_same_project_as_the_plain_path() {
    let dir = scratch("dots");
    write(&dir, "framesmith.yaml", "name: dots\n");
    write(&dir, "scenes/intro.xml", TWO_SECONDS);
    write(&dir, "elsewhere/keep", "");
    let dotted = dir.join("elsewhere").join("..").join("scenes").join(".").join("intro.xml");
    let project = Project::open(&dotted).unwrap();
    assert_eq!(project.folder(), real(&dir));
    assert_eq!(project.composition(), "scenes/intro.xml");
}

#[test]
fn naming_a_project_in_any_form_changes_nothing() {
    let dir = scratch("unchanged");
    write(&dir, "framesmith.yaml", "name: unchanged\n");
    write(&dir, "composition.xml", FIVE_SECONDS);
    write(&dir, "scenes/intro.xml", TWO_SECONDS);
    write(&dir, "media/clip.bin", "bytes");
    let before = snapshot(&dir);
    for form in [
        dir.clone(),
        dir.join("framesmith.yaml"),
        dir.join("composition.xml"),
        dir.join("scenes").join("intro.xml"),
        dir.join("nothing.xml"),
    ] {
        let _ = Project::open(&form);
    }
    assert_eq!(snapshot(&dir), before);
    assert!(fs::read_dir(&dir).unwrap().count() == 4, "a file or a folder is new");
}
