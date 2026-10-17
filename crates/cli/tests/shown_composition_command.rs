//! `framesmith check` and `framesmith render` on a composition that shows other files: a chain of sixteen is
//! valid, and a loop, a chain of seventeen, a frame rate that differs, a missing file, an unreadable file and
//! problems in a shown file are each reported with the file and line where they are, in one run, and the exit
//! code (`docs/spec/notation.md` section 10; `docs/spec/check-command.md`; `docs/spec/render-command.md`).

mod common;

use common::{framesmith, out, scratch};
use project::Code;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

const ROOT: &str = r#"<composition version="1" width="96" height="54" fps="30" duration="2">"#;

/// A composition text: `root` on line 1, then each of `body` on the lines after it, and the closing tag.
fn file<S: AsRef<str>>(root: &str, body: &[S]) -> String {
    let mut text = format!("{root}\n");
    for line in body {
        text.push_str(line.as_ref());
        text.push('\n');
    }
    text.push_str("</composition>\n");
    text
}

/// A project folder `demo` holding `files` (relative paths and their texts), next to an empty folder `cwd`.
/// Both are new. Returns `(project, cwd)`.
fn folder<P: AsRef<str>, T: AsRef<str>>(group: &str, name: &str, files: &[(P, T)]) -> (PathBuf, PathBuf) {
    let base = scratch(group, name);
    let project = base.join("demo");
    let cwd = base.join("cwd");
    fs::create_dir_all(&project).unwrap();
    fs::create_dir_all(&cwd).unwrap();
    for (path, text) in files {
        let target = project.join(path.as_ref());
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(target, text.as_ref()).unwrap();
    }
    (project, cwd)
}

/// The files `f2.xml` to `f{last}.xml` of a chain: each shows the next, and the last one is a box.
/// The root shows `f2.xml`.
fn chain(last: u32) -> Vec<(String, String)> {
    let mut files = vec![(
        "composition.xml".to_string(),
        file(ROOT, &[r#"<composition src="f2.xml" left="0" top="0"/>"#]),
    )];
    for number in 2..last {
        let shows = format!(r#"<composition src="f{}.xml" left="0" top="0"/>"#, number + 1);
        files.push((format!("f{number}.xml"), file(ROOT, &[shows])));
    }
    files.push((format!("f{last}.xml"), with_a_box()));
    files
}

/// A composition with one box, for a file that is only ever shown.
fn with_a_box() -> String {
    file(ROOT, &[r##"<box left="0" top="0" width="4" height="4" background="#ff0000"/>"##])
}

fn check(project: &Path, cwd: &Path) -> std::process::Output {
    framesmith(cwd, [OsStr::new("check"), project.as_os_str()], &[])
}

#[test]
fn a_chain_of_sixteen_files_checks_with_exit_0() {
    let (project, cwd) = folder("shown-command", "sixteen", &chain(16));
    let output = check(&project, &cwd);
    assert_eq!(output.status.code(), Some(0), "{}", out(&output));
    assert_eq!(out(&output), "Ready: the project is valid, 60 frames, 96x54, 30 fps\n");
}

#[test]
fn a_chain_of_seventeen_files_is_reported_at_the_element_in_the_sixteenth_file_that_names_the_seventeenth() {
    let (project, cwd) = folder("shown-command", "seventeen", &chain(17));
    let output = check(&project, &cwd);
    assert_eq!(output.status.code(), Some(1), "{}", out(&output));
    let printed = out(&output);
    let lines: Vec<&str> = printed.lines().collect();
    assert_eq!(lines.len(), 2, "{printed}");
    let start = "f16.xml, line 2: FS-0045: invalid: `f17.xml` would be the 17th composition of its chain, and a chain holds at most 16, the root counted. ";
    assert!(lines[0].starts_with(start), "{}", lines[0]);
    assert!(lines[0].ends_with(Code::ShownTooDeep.remedy()), "{}", lines[0]);
    assert_eq!(lines[1], "Invalid: the project is not valid (f16.xml)");
}

#[test]
fn a_loop_is_reported_with_the_files_of_the_loop_and_renders_nothing_with_exit_1() {
    let files = [
        ("composition.xml", file(ROOT, &[r#"<composition src="b.xml" left="0" top="0"/>"#])),
        (
            "b.xml",
            file(
                ROOT,
                &[
                    r#"<box left="0" top="0" width="4" height="4"/>"#,
                    r#"<composition src="composition.xml" left="0" top="0"/>"#,
                ],
            ),
        ),
    ];
    let (project, cwd) = folder("shown-command", "loop", &files);
    let output = check(&project, &cwd);
    assert_eq!(output.status.code(), Some(1), "{}", out(&output));
    let printed = out(&output);
    let report = printed.lines().next().unwrap_or_default();
    assert!(report.starts_with("b.xml, line 3: FS-0044: invalid: "), "{report}");
    assert!(
        report.contains("the loop is `composition.xml`, `b.xml`, `composition.xml`."),
        "{report}"
    );
    assert!(report.ends_with(Code::ShownLoop.remedy()), "{report}");

    let video = cwd.join("loop.mp4");
    let rendered = framesmith(
        &cwd,
        [OsStr::new("render"), project.as_os_str(), OsStr::new("--output"), video.as_os_str()],
        &[],
    );
    assert_eq!(rendered.status.code(), Some(1), "{}", out(&rendered));
    assert!(!video.exists(), "a refused render writes no file");
}

#[test]
fn a_shown_file_with_another_frame_rate_is_reported_with_both_rates_and_the_element_line() {
    let root_24 = r#"<composition version="1" width="96" height="54" fps="24" duration="2">"#;
    let files = [
        (
            "composition.xml",
            file(ROOT, &[r#"<composition src="inner.xml" left="0" top="0"/>"#]),
        ),
        (
            "inner.xml",
            r#"<composition version="1" width="32" height="24" fps="24" duration="2">
<box left="0" top="0" width="4" height="4"/>
</composition>
"#
            .to_string(),
        ),
    ];
    let (project, cwd) = folder("shown-command", "fps-rate", &files);
    let output = check(&project, &cwd);
    assert_eq!(output.status.code(), Some(1), "{}", out(&output));
    let printed = out(&output);
    let report = printed.lines().next().unwrap_or_default();
    assert!(
        report.starts_with("composition.xml, line 2: FS-0046: invalid: `inner.xml` has 24 frames a second and the composition that shows it has 30; they must be the same. "),
        "{report}"
    );

    let files = [
        (
            "composition.xml",
            file(root_24, &[r#"<composition src="inner.xml" left="0" top="0"/>"#]),
        ),
        ("inner.xml", with_a_box()),
    ];
    let (project, cwd) = folder("shown-command", "fps-default", &files);
    let output = check(&project, &cwd);
    assert_eq!(output.status.code(), Some(1), "{}", out(&output));
    let report = out(&output).lines().next().unwrap_or_default().to_string();
    assert!(
        report.starts_with(
            "composition.xml, line 2: FS-0046: invalid: `inner.xml` has 30 frames a second and the composition that shows it has 24; "
        ),
        "a file that writes no frame rate is at 30: {report}"
    );
}

#[test]
fn a_missing_shown_file_is_reported_with_the_element_line_and_the_path_looked_for_and_exits_1() {
    let files = [(
        "composition.xml",
        file(ROOT, &[r#"<composition src="scenes/gone.xml" left="0" top="0"/>"#]),
    )];
    let (project, cwd) = folder("shown-command", "missing", &files);
    let output = check(&project, &cwd);
    assert_eq!(output.status.code(), Some(1), "{}", out(&output));
    let printed = out(&output);
    let report = printed.lines().next().unwrap_or_default();
    assert!(
        report.starts_with(
            "composition.xml, line 2: FS-0047: invalid: the composition `scenes/gone.xml` shown here is not in the project folder. "
        ),
        "{report}"
    );
    assert!(report.ends_with(Code::ShownMissing.remedy()), "{report}");
}

#[cfg(unix)]
#[test]
fn a_shown_file_that_cannot_be_read_is_reported_as_a_failure_with_the_element_line_and_exits_1() {
    use std::os::unix::fs::PermissionsExt;
    let files = [
        (
            "composition.xml",
            file(ROOT, &[r#"<composition src="inner.xml" left="0" top="0"/>"#]),
        ),
        ("inner.xml", with_a_box()),
    ];
    let (project, cwd) = folder("shown-command", "unreadable", &files);
    fs::set_permissions(project.join("inner.xml"), fs::Permissions::from_mode(0o000)).unwrap();
    let output = check(&project, &cwd);
    assert_eq!(output.status.code(), Some(1), "{}", out(&output));
    let report = out(&output).lines().next().unwrap_or_default().to_string();
    assert!(
        report.starts_with("composition.xml, line 2: FS-0048: failure: the composition `inner.xml` shown here could not be read: "),
        "{report}"
    );
    assert!(report.ends_with(Code::ShownUnreadable.remedy()), "{report}");
}

#[test]
fn a_wrong_value_in_a_shown_file_is_reported_with_its_name_and_line_with_the_outer_problems_in_one_run() {
    let outer = r#"<composition version="1" width="641" height="54" fps="30" duration="2">"#;
    let files = [
        (
            "composition.xml",
            file(outer, &[r#"<composition src="inner.xml" left="0" top="0"/>"#]),
        ),
        (
            "inner.xml",
            r#"<composition version="1" width="33" height="24" fps="30" duration="2">
<box left="0" top="0" width="4" height="4"/>
</composition>
"#
            .to_string(),
        ),
    ];
    let (project, cwd) = folder("shown-command", "one-run", &files);
    let output = check(&project, &cwd);
    assert_eq!(output.status.code(), Some(1), "{}", out(&output));
    let printed = out(&output);
    let lines: Vec<&str> = printed.lines().collect();
    assert_eq!(lines.len(), 3, "{printed}");
    assert!(
        lines[0].starts_with("composition.xml, line 1: FS-0007: invalid: `width` must be an even whole number from 2 to 8192, not 641. "),
        "{}",
        lines[0]
    );
    assert!(
        lines[1].starts_with("inner.xml, line 1: FS-0007: invalid: `width` must be an even whole number from 2 to 8192, not 33. "),
        "{}",
        lines[1]
    );
    assert_eq!(lines[2], "Invalid: the project is not valid (composition.xml, inner.xml)");
}
