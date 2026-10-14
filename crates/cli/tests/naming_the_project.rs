//! `framesmith check` with a folder, a `framesmith.yaml` or a composition file as the project:
//! the same project whichever way it is named, the project folder found by one rule, and what is
//! printed for a path that is not there or a folder with nothing to render
//! (`docs/spec/project-location.md`).

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use common::*;

const GROUP: &str = "naming-the-project";

/// A composition of 5 seconds: 150 frames, 1280x720.
const FIVE: &str = r#"<composition version="1" width="1280" height="720" duration="5"/>"#;
/// A composition of 2 seconds: 60 frames, 640x360.
const TWO: &str = r#"<composition version="1" width="640" height="360" duration="2"/>"#;
/// An odd width: one report.
const ODD: &str = r#"<composition version="1" width="641" height="360" duration="2"/>"#;

fn write(dir: &Path, relative: &str, text: &str) {
    let path = dir.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

/// A new base folder holding the folder `demo` and an empty `cwd`; no `framesmith.yaml` may be at
/// or above it.
fn base(name: &str) -> (PathBuf, PathBuf) {
    let base = scratch(GROUP, name);
    fs::create_dir_all(base.join("demo")).unwrap();
    fs::create_dir_all(base.join("cwd")).unwrap();
    for above in fs::canonicalize(&base).unwrap().ancestors() {
        assert!(
            !above.join("framesmith.yaml").exists(),
            "a framesmith.yaml is in {}",
            above.display()
        );
    }
    (base.join("demo"), base.join("cwd"))
}

fn check(cwd: &Path, argument: &str) -> std::process::Output {
    framesmith(cwd, ["check", argument], &[])
}

fn statement(code: project::Code, message: &str) -> String {
    format!("{code}: {}: {message}. {}", code.kind().as_str(), code.remedy())
}

#[test]
fn a_folder_a_project_file_and_a_composition_file_print_the_same_line_and_exit_0() {
    let (demo, cwd) = base("same-line");
    write(&demo, "framesmith.yaml", "name: demo\n");
    write(&demo, "composition.xml", FIVE);
    let mut lines = Vec::new();
    for argument in ["../demo", "../demo/framesmith.yaml", "../demo/composition.xml", "../demo/"] {
        let output = check(&cwd, argument);
        assert_eq!(output.status.code(), Some(0), "{argument}: {}", out(&output));
        assert_eq!(err(&output), "", "{argument}");
        lines.push(out(&output));
    }
    assert_eq!(lines[0], "Ready: the project is valid, 150 frames, 1280x720, 30 fps\n");
    assert!(lines.iter().all(|line| line == &lines[0]), "{lines:?}");

    // Named from inside the project folder, by a bare name.
    for argument in [".", "framesmith.yaml", "composition.xml"] {
        let output = check(&demo, argument);
        assert_eq!(output.status.code(), Some(0), "{argument}");
        assert_eq!(out(&output), lines[0], "{argument}");
    }
}

#[test]
fn a_project_with_no_project_file_is_the_same_project_in_two_forms() {
    let (demo, cwd) = base("no-project-file-two-forms");
    write(&demo, "composition.xml", FIVE);
    let one = check(&cwd, "../demo");
    let two = check(&cwd, "../demo/composition.xml");
    assert_eq!((one.status.code(), two.status.code()), (Some(0), Some(0)));
    assert_eq!(out(&one), out(&two));
}

#[test]
fn a_composition_file_in_a_subfolder_is_the_one_read_and_the_folder_with_the_project_file_is_the_project() {
    let (demo, cwd) = base("subfolder");
    write(&demo, "framesmith.yaml", "name: demo\n");
    write(&demo, "composition.xml", FIVE);
    write(&demo, "scenes/intro.xml", TWO);
    // The composition read is intro.xml: its 60 frames and 640x360, not the 150 of composition.xml.
    let output = check(&cwd, "../demo/scenes/intro.xml");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(out(&output), "Ready: the project is valid, 60 frames, 640x360, 30 fps\n");

    // The project folder is demo: its project file is read (a wrong key in it is reported with
    // the file's name), and the composition is named by its path from demo.
    write(&demo, "framesmith.yaml", "name: demo\nspeed: fast\n");
    write(&demo, "scenes/intro.xml", ODD);
    let output = check(&cwd, "../demo/scenes/intro.xml");
    assert_eq!(output.status.code(), Some(1));
    let printed = out(&output);
    let lines: Vec<&str> = printed.lines().collect();
    assert_eq!(lines.len(), 3, "{printed}");
    assert!(lines[0].starts_with("framesmith.yaml, line 2: FS-0022: invalid: "), "{printed}");
    assert!(lines[1].starts_with("scenes/intro.xml, line 1: FS-0007: invalid: "), "{printed}");
    assert_eq!(lines[2], "Invalid: the project is not valid (framesmith.yaml, scenes/intro.xml)");
}

#[test]
fn a_composition_file_named_from_inside_its_folder_finds_the_project_above_it() {
    let (demo, _) = base("named-from-inside");
    write(&demo, "framesmith.yaml", "speed: fast\n");
    write(&demo, "composition.xml", FIVE);
    write(&demo, "scenes/intro.xml", TWO);
    let scenes = demo.join("scenes");
    for argument in ["intro.xml", "./intro.xml", "../scenes/intro.xml"] {
        let output = check(&scenes, argument);
        assert_eq!(output.status.code(), Some(1), "{argument}");
        let printed = out(&output);
        assert!(
            printed.starts_with("framesmith.yaml, line 1: FS-0022: invalid: "),
            "{argument}: {printed}"
        );
        assert!(
            printed.ends_with("Invalid: the project is not valid (framesmith.yaml)\n"),
            "{argument}: {printed}"
        );
    }
    write(&demo, "framesmith.yaml", "name: demo\n");
    let output = check(&scenes, "intro.xml");
    assert_eq!(out(&output), "Ready: the project is valid, 60 frames, 640x360, 30 fps\n");
    // The file beside the project file is read as well, from the folder above.
    let output = check(&scenes, "../composition.xml");
    assert_eq!(out(&output), "Ready: the project is valid, 150 frames, 1280x720, 30 fps\n");
}

#[test]
fn a_composition_file_with_no_project_file_at_or_above_it_is_read_with_its_own_folder_as_the_project() {
    let (demo, cwd) = base("own-folder");
    write(&demo, "scenes/intro.xml", TWO);
    // A project file below the file's folder or beside it is not the file's.
    write(&demo, "scenes/deeper/framesmith.yaml", "speed: fast\n");
    write(&demo, "other/framesmith.yaml", "speed: fast\n");
    let output = check(&cwd, "../demo/scenes/intro.xml");
    assert_eq!(output.status.code(), Some(0), "{}", out(&output));
    assert_eq!(out(&output), "Ready: the project is valid, 60 frames, 640x360, 30 fps\n");

    // The file is the project's one composition and the file's own folder is its folder: a
    // report names it by its own name, not by a path from a folder above.
    write(&demo, "scenes/intro.xml", ODD);
    let output = check(&cwd, "../demo/scenes/intro.xml");
    assert_eq!(output.status.code(), Some(1));
    let printed = out(&output);
    assert!(printed.starts_with("intro.xml, line 1: FS-0007: invalid: "), "{printed}");
    assert!(printed.ends_with("Invalid: the project is not valid (intro.xml)\n"), "{printed}");
}

#[test]
fn a_path_that_does_not_exist_is_reported_with_the_path_and_exits_1() {
    let (demo, cwd) = base("path-missing");
    write(&demo, "composition.xml", FIVE);
    let before = snapshot_names(&demo);
    for argument in [
        "../nothing-here",
        "../demo/missing.xml",
        "../demo/scenes/intro.xml",
        "../nowhere/framesmith.yaml",
        "../demo/composition.xml/inside.xml",
    ] {
        let output = check(&cwd, argument);
        assert_eq!(output.status.code(), Some(1), "{argument}");
        assert_eq!(
            out(&output),
            format!(
                "No project: {}\n",
                statement(project::Code::PathMissing, &format!("`{argument}` does not exist"))
            ),
            "{argument}"
        );
        assert_eq!(err(&output), "", "{argument}");
    }
    // A folder written with a closing slash is a folder that is missing.
    let output = check(&cwd, "../nothing-here/");
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        out(&output),
        format!(
            "No project: {}\n",
            statement(project::Code::FolderMissing, "the folder `../nothing-here/` does not exist")
        )
    );
    // And a file written with one is not a folder.
    let output = check(&cwd, "../demo/composition.xml/");
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        out(&output),
        format!(
            "No project: {}\n",
            statement(project::Code::NotAFolder, "`../demo/composition.xml/` is not a folder")
        )
    );
    assert_eq!(snapshot_names(&demo), before, "the project was changed");
}

#[test]
fn a_folder_with_nothing_to_render_is_reported_with_the_folder_and_the_files_looked_for() {
    let (demo, cwd) = base("nothing-to-render");
    let expected = format!(
        "No project: {}\n",
        statement(
            project::Code::NoComposition,
            "there is no composition.xml in `../demo`, and no framesmith.yaml there names a composition"
        )
    );
    // No file at all.
    let output = check(&cwd, "../demo");
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(out(&output), expected);
    assert_eq!(err(&output), "");
    // A project file that names no composition changes nothing about what is looked for.
    write(&demo, "framesmith.yaml", "name: demo\n");
    for argument in ["../demo", "../demo/framesmith.yaml"] {
        let output = check(&cwd, argument);
        assert_eq!(output.status.code(), Some(1), "{argument}");
        assert_eq!(out(&output), expected, "{argument}");
    }
    // A composition that is somewhere else in the folder is not looked for under any other name.
    write(&demo, "scenes/intro.xml", TWO);
    let output = check(&cwd, "../demo");
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(out(&output), expected);
    // The project file naming a composition that is not there is the other reason.
    write(&demo, "framesmith.yaml", "composition: scenes/nothing.xml\n");
    let output = check(&cwd, "../demo");
    assert_eq!(output.status.code(), Some(1));
    assert!(out(&output).starts_with("No project: FS-0025: invalid: "), "{}", out(&output));
}

#[test]
fn the_command_line_names_a_path_in_its_usage() {
    let (_, cwd) = base("usage");
    let output = framesmith(&cwd, ["check"], &[]);
    assert_eq!(output.status.code(), Some(1));
    let printed = err(&output);
    assert!(printed.contains("`check` needs the path of a project"), "{printed}");
    assert!(printed.ends_with("usage: framesmith check <path>\n"), "{printed}");
    assert!(
        printed.contains("Run `framesmith check <path>` with the folder, the project file or the composition file of one project."),
        "{printed}"
    );
}

/// The names of the files and folders under `dir`, in order.
fn snapshot_names(dir: &Path) -> Vec<PathBuf> {
    let mut names = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(next) = pending.pop() {
        for entry in fs::read_dir(&next).unwrap() {
            let path = entry.unwrap().path();
            names.push(path.clone());
            if path.is_dir() {
                pending.push(path);
            }
        }
    }
    names.sort();
    names
}

#[test]
fn the_run_log_says_which_form_the_path_was_and_how_many_folders_were_searched_and_holds_no_path() {
    let (demo, cwd) = base("log");
    write(&demo, "framesmith.yaml", "name: demo\n");
    write(&demo, "composition.xml", FIVE);
    write(&demo, "scenes/deeper/intro.xml", TWO);
    let located = |argument: &str| -> String {
        let output = framesmith(&cwd, ["check", argument], &[("FRAMESMITH_LOG", "debug")]);
        assert_eq!(output.status.code(), Some(0), "{argument}");
        let log = log_text(&cwd);
        let this_run = parse(&log)
            .into_iter()
            .rfind(|line| line.text.starts_with("project located"))
            .unwrap();
        fs::remove_dir_all(cwd.join("logs")).unwrap();
        assert_eq!(this_run.level, "DEBUG");
        assert_eq!(this_run.component, "project");
        this_run.text
    };
    assert_eq!(located("../demo"), r#"project located form="folder" folders_searched=0"#);
    assert_eq!(
        located("../demo/framesmith.yaml"),
        r#"project located form="project file" folders_searched=0"#
    );
    // The file's own folder holds the project file: one folder looked in.
    assert_eq!(
        located("../demo/composition.xml"),
        r#"project located form="composition file" folders_searched=1"#
    );
    // Two folders above the file's own folder hold it: three looked in.
    assert_eq!(
        located("../demo/scenes/deeper/intro.xml"),
        r#"project located form="composition file" folders_searched=3"#
    );

    // The folder found above the file is not written. The words of the command line are (they are
    // written as typed), and nothing else of the machine is.
    let output = framesmith(&cwd, ["check", "../demo/scenes/deeper/intro.xml"], &[("FRAMESMITH_LOG", "trace")]);
    assert_eq!(output.status.code(), Some(0));
    let written = after_run_id(&log_text(&cwd));
    let absolute = fs::canonicalize(&demo).unwrap();
    assert!(!written.contains(absolute.to_str().unwrap()), "{written}");
}
