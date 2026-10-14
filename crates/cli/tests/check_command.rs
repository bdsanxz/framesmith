//! The `framesmith check` command, run as a program the way its user runs it.

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use project::Code;

/// What a report says after its place: the code, its kind, the message as a sentence and the
/// remedy (`docs/spec/error-catalogue.md` section 3).
fn statement(code: Code, message: &str) -> String {
    format!("{code}: {}: {message}. {}", code.kind().as_str(), code.remedy())
}

/// One report as the command prints it: the place, then the statement.
fn report(place: &str, code: Code, message: &str) -> String {
    format!("{place}: {}", statement(code, message))
}

/// A new folder named `name` for one test, under the tests' scratch folder.
fn folder(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("check-command").join(name);
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// A project folder whose `composition.xml` is `text`, next to an empty folder to run from.
fn project(name: &str, text: &str) -> (PathBuf, PathBuf) {
    let base = folder(name);
    let project = base.join("demo");
    let cwd = base.join("cwd");
    fs::create_dir_all(&project).unwrap();
    fs::create_dir_all(&cwd).unwrap();
    fs::write(project.join("composition.xml"), text).unwrap();
    (project, cwd)
}

/// Runs `framesmith` with `args` from `cwd`.
fn framesmith<I, S>(cwd: &Path, args: I) -> Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    Command::new(env!("CARGO_BIN_EXE_framesmith"))
        .args(args)
        .current_dir(cwd)
        // A terminal that asks for colour changes nothing.
        .env("CLICOLOR_FORCE", "1")
        .env("FORCE_COLOR", "1")
        .env("TERM", "xterm-256color")
        .env_remove("NO_COLOR")
        .output()
        .unwrap()
}

fn out(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).unwrap()
}

fn err(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).unwrap()
}

/// Every file and folder under `dir` with the bytes of the files and the modification times.
fn snapshot(dir: &Path) -> Vec<(PathBuf, Option<Vec<u8>>, std::time::SystemTime)> {
    let mut entries = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(next) = pending.pop() {
        for entry in fs::read_dir(&next).unwrap() {
            let path = entry.unwrap().path();
            let modified = fs::metadata(&path).unwrap().modified().unwrap();
            if path.is_dir() {
                entries.push((path.clone(), None, modified));
                pending.push(path);
            } else {
                entries.push((path.clone(), Some(fs::read(&path).unwrap()), modified));
            }
        }
    }
    entries.sort();
    entries
}

const VALID: &str = r#"<composition version="1" id="demo" width="1280" height="720" duration="5"/>"#;

#[test]
fn a_valid_root_prints_one_line_with_frames_size_and_rate_and_exits_0() {
    let (project, cwd) = project("valid", VALID);
    let output = framesmith(&cwd, [OsStr::new("check"), project.as_os_str()]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(out(&output), "Ready: the project is valid, 150 frames, 1280x720, 30 fps\n");
    assert_eq!(err(&output), "");
}

#[test]
fn five_seconds_with_no_fps_is_150_frames_at_30_and_1_01_seconds_at_30_is_31_frames() {
    let (project, cwd) = project("five-seconds", VALID);
    let output = framesmith(&cwd, [OsStr::new("check"), project.as_os_str()]);
    assert!(
        out(&output).contains("150 frames") && out(&output).contains("30 fps"),
        "{}",
        out(&output)
    );

    fs::write(
        project.join("composition.xml"),
        r#"<composition version="1" width="1280" height="720" fps="30" duration="1.01"/>"#,
    )
    .unwrap();
    let output = framesmith(&cwd, [OsStr::new("check"), project.as_os_str()]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(out(&output), "Ready: the project is valid, 31 frames, 1280x720, 30 fps\n");
}

#[test]
fn a_root_with_no_background_and_no_fps_is_valid_and_the_line_shows_30_fps() {
    let (project, cwd) = project("defaults", r#"<composition version="1" width="640" height="360" duration="2"/>"#);
    let output = framesmith(&cwd, [OsStr::new("check"), project.as_os_str()]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(out(&output), "Ready: the project is valid, 60 frames, 640x360, 30 fps\n");
}

#[test]
fn one_frame_is_worded_in_the_singular() {
    let (project, cwd) = project(
        "one-frame",
        r#"<composition version="1" width="2" height="2" fps="1" duration="1"/>"#,
    );
    let output = framesmith(&cwd, [OsStr::new("check"), project.as_os_str()]);
    assert_eq!(out(&output), "Ready: the project is valid, 1 frame, 2x2, 1 fps\n");
}

/// Runs `check` on a project with this root and returns what was printed, with the exit code.
fn check_root(name: &str, root: &str) -> (String, Option<i32>) {
    let (project, cwd) = project(name, root);
    let output = framesmith(&cwd, [OsStr::new("check"), project.as_os_str()]);
    assert_eq!(err(&output), "", "{name}");
    (out(&output), output.status.code())
}

#[test]
fn a_missing_or_wrong_version_is_reported_with_its_line_and_the_versions_read_and_exits_1() {
    let (text, code) = check_root("no-version", "<composition width=\"640\" height=\"360\" duration=\"2\"/>");
    assert_eq!(code, Some(1));
    assert_eq!(
        text,
        format!(
            "{}\nInvalid: the project is not valid (composition.xml)\n",
            report(
                "composition.xml, line 1",
                Code::VersionMissing,
                "<composition> needs the attribute `version`; the versions read are: 1"
            )
        )
    );

    let (text, code) = check_root(
        "version-2",
        "<?xml version=\"1.0\"?>\n<composition\n version=\"2\" width=\"640\" height=\"360\" duration=\"2\"/>",
    );
    assert_eq!(code, Some(1));
    assert_eq!(
        text,
        format!(
            "{}\nInvalid: the project is not valid (composition.xml)\n",
            report(
                "composition.xml, line 3",
                Code::VersionNotRead,
                "`version=\"2\"` is not a version this program reads; the versions read are: 1"
            )
        )
    );
}

#[test]
fn a_size_that_is_not_an_even_whole_number_from_2_to_8192_is_reported_with_its_line_and_the_rule() {
    for (value, name) in [
        ("641", "odd-width"),
        ("1", "width-below-2"),
        ("8194", "width-above-8192"),
        ("10.5", "width-fraction"),
        ("abc", "width-word"),
    ] {
        let root = format!("<composition\n version=\"1\"\n width=\"{value}\" height=\"360\" duration=\"2\"/>");
        let (text, code) = check_root(name, &root);
        assert_eq!(code, Some(1), "{name}");
        assert_eq!(
            text,
            format!(
                "{}\nInvalid: the project is not valid (composition.xml)\n",
                report(
                    "composition.xml, line 3",
                    Code::SizeNotAccepted,
                    &format!("`width` must be an even whole number from 2 to 8192, not {value}")
                )
            ),
            "{name}"
        );
    }
    let (text, code) = check_root(
        "odd-height",
        "<composition version=\"1\" width=\"640\"\n height=\"361\" duration=\"2\"/>",
    );
    assert_eq!(code, Some(1));
    assert!(
        text.starts_with(&format!(
            "{}\n",
            report(
                "composition.xml, line 2",
                Code::SizeNotAccepted,
                "`height` must be an even whole number from 2 to 8192, not 361"
            )
        )),
        "{text}"
    );
}

#[test]
fn a_wrong_fps_duration_or_background_is_reported_with_its_line_and_the_form_and_exits_1() {
    let cases = [
        (
            "fps-zero",
            Code::FpsNotAccepted,
            r#"fps="0" duration="2""#,
            "`fps` must be a whole number from 1 to 240, not 0",
        ),
        (
            "fps-above",
            Code::FpsNotAccepted,
            r#"fps="241" duration="2""#,
            "`fps` must be a whole number from 1 to 240, not 241",
        ),
        (
            "fps-fraction",
            Code::FpsNotAccepted,
            r#"fps="29.97" duration="2""#,
            "`fps` must be a whole number from 1 to 240, not 29.97",
        ),
        (
            "duration-zero",
            Code::DurationNotAboveZero,
            r#"duration="0""#,
            "`duration=\"0\"` must be above 0",
        ),
        (
            "duration-negative",
            Code::DurationNotATime,
            r#"duration="-5""#,
            "`duration=\"-5\"` is not a time in seconds: digits with an optional point, such as 5 or 1.25",
        ),
        (
            "duration-word",
            Code::DurationNotATime,
            r#"duration="long""#,
            "`duration=\"long\"` is not a time in seconds: digits with an optional point, such as 5 or 1.25",
        ),
        (
            "background-name",
            Code::BackgroundNotAColour,
            r##"duration="2" background="red""##,
            "`background=\"red\"` is not a colour written as #rrggbb or #rrggbbaa",
        ),
        (
            "background-two",
            Code::BackgroundNotAColour,
            r##"duration="2" background="#000000, #ffffff""##,
            "`background=\"#000000, #ffffff\"` is not a colour written as #rrggbb or #rrggbbaa",
        ),
    ];
    for (name, code_of_case, attributes, message) in cases {
        let root = format!("<composition version=\"1\" width=\"640\" height=\"360\"\n {attributes}/>");
        let (text, code) = check_root(name, &root);
        assert_eq!(code, Some(1), "{name}");
        assert_eq!(
            text,
            format!(
                "{}\nInvalid: the project is not valid (composition.xml)\n",
                report("composition.xml, line 2", code_of_case, message)
            ),
            "{name}"
        );
    }
}

#[test]
fn a_file_that_is_not_xml_is_reported_and_exits_1() {
    let (text, code) = check_root("not-xml", "<composition version=\"1\">\n</box>");
    assert_eq!(code, Some(1));
    assert!(
        text.starts_with("composition.xml, line 2: FS-0002: invalid: the file is not well-formed XML: "),
        "{text}"
    );
    assert!(
        text.lines().next().unwrap().ends_with(&format!(" {}", Code::NotXml.remedy())),
        "{text}"
    );
    assert!(text.ends_with("Invalid: the project is not valid (composition.xml)\n"), "{text}");
}

#[test]
fn the_messages_carry_no_colour_and_no_control_character() {
    let roots = [
        VALID.to_string(),
        "<composition width=\"640\" height=\"360\" duration=\"2\"/>".to_string(),
        "<composition version=\"1\" width=\"641\" height=\"360\" duration=\"2\"/>".to_string(),
        "<composition version=\"1\" width=\"640\" height=\"360\" duration=\"0\"/>".to_string(),
        "<composition version=\"1\" width=\"640\" height=\"360\" duration=\"2\" background=\"&#9;red\"/>".to_string(),
        "<html>".to_string(),
    ];
    for (index, root) in roots.iter().enumerate() {
        let (project, cwd) = project(&format!("plain-{index}"), root);
        // The colour of a terminal is asked for in the environment of `framesmith()`.
        let output = framesmith(&cwd, [OsStr::new("check"), project.as_os_str()]);
        for stream in [&output.stdout, &output.stderr] {
            assert!(!stream.contains(&0x1b), "{root}: an escape byte");
            assert!(
                stream.iter().all(|&byte| byte == b'\n' || (0x20..0x7f).contains(&byte)),
                "{root}: {stream:?}"
            );
        }
    }
}

#[test]
fn check_leaves_the_project_folder_as_it_was_and_adds_only_its_log_to_the_working_folder() {
    let (project, cwd) = project("writes-nothing", VALID);
    fs::create_dir(project.join("media")).unwrap();
    fs::write(project.join("media").join("clip.bin"), [9u8, 8, 7]).unwrap();
    fs::write(project.join("framesmith.yaml"), "name: demo\n").unwrap();
    let before_project = snapshot(&project);
    assert!(snapshot(&cwd).is_empty());

    // Valid, and invalid.
    for text in [VALID, "<composition/>"] {
        fs::write(project.join("composition.xml"), text).unwrap();
        let before_project = snapshot(&project);
        framesmith(&cwd, [OsStr::new("check"), project.as_os_str()]);
        assert_eq!(snapshot(&project), before_project);
        // The one thing written is the run log, under the folder the command is run from
        // (docs/spec/run-log.md section 1).
        let now: Vec<PathBuf> = snapshot(&cwd)
            .into_iter()
            .map(|(path, ..)| path.strip_prefix(&cwd).unwrap().to_path_buf())
            .collect();
        assert_eq!(now, [PathBuf::from("logs"), PathBuf::from("logs/framesmith.log")]);
    }
    assert_eq!(before_project.len(), 4);
}

#[test]
fn check_works_on_a_project_that_cannot_be_written_to() {
    use std::os::unix::fs::PermissionsExt;
    let (project, cwd) = project("read-only", VALID);
    fs::set_permissions(project.join("composition.xml"), fs::Permissions::from_mode(0o444)).unwrap();
    fs::set_permissions(&project, fs::Permissions::from_mode(0o555)).unwrap();
    fs::set_permissions(&cwd, fs::Permissions::from_mode(0o555)).unwrap();
    let output = framesmith(&cwd, [OsStr::new("check"), project.as_os_str()]);
    // Give the folders back before judging, so that a failure does not leave them locked.
    fs::set_permissions(&project, fs::Permissions::from_mode(0o755)).unwrap();
    fs::set_permissions(&cwd, fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(out(&output), "Ready: the project is valid, 150 frames, 1280x720, 30 fps\n");
}

#[test]
fn a_folder_without_a_composition_is_no_project_and_exits_1() {
    let base = folder("no-composition");
    let output = framesmith(&base, [OsStr::new("check"), OsStr::new("empty")]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        out(&output),
        format!("No project: {}\n", statement(Code::PathMissing, "`empty` does not exist"))
    );

    fs::create_dir(base.join("empty")).unwrap();
    let output = framesmith(&base, [OsStr::new("check"), OsStr::new("empty")]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        out(&output),
        format!(
            "No project: {}\n",
            statement(
                Code::NoComposition,
                "there is no composition.xml in `empty`, and no framesmith.yaml there names a composition"
            )
        )
    );
    assert_eq!(err(&output), "");
}

#[test]
fn a_command_line_that_cannot_be_read_is_told_on_standard_error_and_exits_1() {
    let base = folder("bad-command-lines");
    let check_usage = "usage: framesmith check <path>\n";
    let both_usages = "usage: framesmith check <path>\nusage: framesmith render <path>\n";
    let cases: [(&[&str], &str, &str); 6] = [
        (&[], "no command is given", both_usages),
        (&["play", "demo"], "`play` is not a command", both_usages),
        (&["check"], "`check` needs the path of a project", check_usage),
        (
            &["check", "--json", "demo"],
            "`check` takes no option, and --json is one",
            check_usage,
        ),
        (
            &["check", "demo", "other"],
            "`check` takes the path of one project, and other is one word too many",
            check_usage,
        ),
        (&["check", "-x"], "`check` takes no option, and -x is one", check_usage),
    ];
    for (args, message, usage) in cases {
        let output = framesmith(&base, args);
        assert_eq!(output.status.code(), Some(1), "{args:?}");
        assert_eq!(out(&output), "", "{args:?}");
        assert_eq!(
            err(&output),
            format!("framesmith: {}\n{usage}", statement(Code::CommandLineNotRead, message)),
            "{args:?}"
        );
    }
}
