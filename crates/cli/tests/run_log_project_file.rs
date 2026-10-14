//! What the run log holds of a project file: the steps taken, the result of the run, and nothing of
//! the file's content (`docs/spec/run-log.md` sections 4 and 6).

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use common::*;

const GROUP: &str = "run-log-project-file";

/// A project `demo` with a `framesmith.yaml` and a valid composition, in a folder that is also the
/// folder the command is run from.
fn project_here(name: &str, yaml: &str) -> PathBuf {
    let (project, _) = project(GROUP, name, VALID);
    fs::write(project.join("framesmith.yaml"), yaml).unwrap();
    project.parent().unwrap().to_path_buf()
}

fn check(base: &Path, level: &str) -> std::process::Output {
    framesmith(base, ["check", "demo"], &[("FRAMESMITH_LOG", level)])
}

/// The message of each line: what comes before its first field.
fn messages(base: &Path) -> Vec<String> {
    lines(base)
        .iter()
        .map(|line| {
            let text = &line.text;
            let cut = [" version=", " bytes=", " problems=", " file=", " outcome="]
                .iter()
                .filter_map(|marker| text.find(marker))
                .min()
                .unwrap_or(text.len());
            text[..cut].to_string()
        })
        .collect()
}

#[test]
fn a_project_file_is_read_and_judged_before_the_composition_and_the_steps_are_logged_in_order() {
    let base = project_here("steps-in-order", "name: demo\n");
    let output = check(&base, "debug");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        messages(&base),
        [
            "command started",
            "project file read",
            "project file judged",
            "composition file read",
            "root judged",
            "command finished"
        ]
    );
    let lines = lines(&base);
    assert!(lines[2].text.contains("problems=0"), "{}", lines[2].raw);
    assert_eq!(lines[1].component, "project");
    assert_eq!(lines[2].component, "project");
    assert!(lines.iter().all(|line| line.run_id == lines[0].run_id));
}

#[test]
fn a_project_with_no_project_file_logs_no_step_for_it() {
    let (project, _) = project(GROUP, "no-project-file", VALID);
    let base = project.parent().unwrap().to_path_buf();
    check(&base, "debug");
    assert_eq!(
        messages(&base),
        ["command started", "composition file read", "root judged", "command finished"]
    );
}

#[test]
fn an_invalid_project_file_logs_its_reports_by_code_and_line_and_the_count_of_problems() {
    let base = project_here("invalid-project-file", "name: demo\nspeed: fast\nrender:\n  codec: vp9\n");
    let output = check(&base, "trace");
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        messages(&base),
        [
            "command started",
            "project file read",
            "project file judged",
            "composition file read",
            "root judged",
            "report",
            "report",
            "command finished"
        ]
    );
    let lines = lines(&base);
    assert!(lines[2].text.contains("problems=2"), "{}", lines[2].raw);
    assert!(
        lines[5].text.contains(r#"file="framesmith.yaml" code="FS-0022" line=2"#),
        "{}",
        lines[5].raw
    );
    assert!(
        lines[6].text.contains(r#"file="framesmith.yaml" code="FS-0024" line=4"#),
        "{}",
        lines[6].raw
    );
    assert_eq!(lines[7].level, "WARN");
    assert!(lines[7].text.contains(r#"outcome="invalid" exit=1 problems=2"#), "{}", lines[7].raw);
}

#[test]
fn a_file_that_is_not_yaml_stops_the_log_before_the_composition_is_read() {
    let base = project_here("not-yaml", "name: a\nname: b\n");
    check(&base, "debug");
    assert_eq!(
        messages(&base),
        ["command started", "project file read", "project file judged", "command finished"]
    );
    assert!(lines(&base)[2].text.contains("problems=1"));
}

#[test]
fn a_project_that_is_only_not_supported_ends_with_exit_2_and_its_own_outcome() {
    let base = project_here("unsupported", "timeline: []\n");
    let output = check(&base, "info");
    assert_eq!(output.status.code(), Some(2));
    let lines = lines(&base);
    assert_eq!(lines.len(), 2, "{}", log_text(&base));
    assert_eq!(lines[1].level, "WARN");
    assert!(
        lines[1].text.contains(r#"outcome="unsupported" exit=2 problems=1"#),
        "{}",
        lines[1].raw
    );
}

#[test]
fn no_composition_after_reports_of_the_project_file_logs_the_code_and_how_many_reports_came_first() {
    let base = project_here("reports-then-no-project", "composition: nothing.xml\nspeed: fast\n");
    let output = check(&base, "info");
    assert_eq!(output.status.code(), Some(1));
    let lines = lines(&base);
    assert_eq!(lines.len(), 2, "{}", log_text(&base));
    assert_eq!(lines[1].level, "WARN");
    assert!(
        lines[1].text.contains(r#"outcome="no project" exit=1 code="FS-0025" problems=1"#),
        "{}",
        lines[1].raw
    );
}

#[test]
fn a_project_file_that_cannot_be_read_is_logged_as_an_error_with_its_code() {
    let base = project_here("project-file-unreadable", "");
    let file = base.join("demo").join("framesmith.yaml");
    fs::remove_file(&file).unwrap();
    fs::create_dir(&file).unwrap();
    let output = check(&base, "error");
    assert_eq!(output.status.code(), Some(1));
    let lines = lines(&base);
    assert_eq!(lines.len(), 1, "{}", log_text(&base));
    assert_eq!(lines[0].level, "ERROR");
    assert!(
        lines[0].text.contains(r#"outcome="no project" exit=1 code="FS-0020""#),
        "{}",
        lines[0].raw
    );
    assert!(!lines[0].text.contains("problems="), "{}", lines[0].raw);
}

#[test]
fn the_log_of_a_run_holds_no_content_of_the_project_file() {
    let yaml =
        "name: needle-name\ncomposition: needle-path.xml\nneedle-key: needle-value\nrender:\n  codec: needle-codec\n  needle-sub: x\n";
    let base = project_here("no-content", yaml);
    let mut printed = String::new();
    for level in ["trace", "debug", "info"] {
        printed.push_str(&out(&check(&base, level)));
    }
    // The control: the command itself shows the words, so they are in play.
    for needle in ["needle-key", "needle-codec", "needle-sub", "needle-path.xml"] {
        assert!(printed.contains(needle), "{needle} is not printed:\n{printed}");
    }
    let log = log_text(&base);
    assert!(log.lines().count() >= 3 * 2, "{log}");
    let written = after_run_id(&log);
    for needle in [
        "needle-name",
        "needle-path",
        "needle-key",
        "needle-value",
        "needle-codec",
        "needle-sub",
    ] {
        assert!(!written.contains(needle), "{needle} is in the log:\n{log}");
    }
}
