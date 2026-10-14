//! The run log of `framesmith check`, read the way a maintainer reads it (`docs/spec/run-log.md`).

mod common;

use std::collections::BTreeSet;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use common::*;

const GROUP: &str = "run-log";

/// A project `demo` and the folder that holds it, which is also the folder the command is run from.
fn project_here(name: &str, text: &str) -> std::path::PathBuf {
    let (project, _) = project(GROUP, name, text);
    project.parent().unwrap().to_path_buf()
}

fn check(base: &Path, env: &[(&str, &str)]) -> std::process::Output {
    framesmith(base, ["check", "demo"], env)
}

#[test]
fn a_check_logs_a_line_for_the_command_with_its_inputs_and_a_line_for_its_result_under_one_run_id() {
    let base = project_here("command-and-result", VALID);
    let output = check(&base, &[]);
    assert_eq!(output.status.code(), Some(0));

    let lines = lines(&base);
    assert_eq!(lines.len(), 2, "{}", log_text(&base));
    let (started, finished) = (&lines[0], &lines[1]);
    assert!(started.text.starts_with("command started "), "{}", started.raw);
    assert!(started.text.contains(r#"args=["check","demo"]"#), "{}", started.raw);
    assert!(finished.text.starts_with("command finished "), "{}", finished.raw);
    assert!(finished.text.contains(r#"outcome="ready" exit=0"#), "{}", finished.raw);
    assert!(!started.run_id.is_empty());
    assert_eq!(started.run_id, finished.run_id);
}

#[test]
fn an_invalid_project_logs_the_exit_code_and_how_many_problems_without_their_messages() {
    let base = project_here("invalid-result", INVALID);
    let output = check(&base, &[]);
    assert_eq!(output.status.code(), Some(1));

    let lines = lines(&base);
    assert_eq!(lines.len(), 2, "{}", log_text(&base));
    assert_eq!(lines[1].level, "WARN");
    assert!(lines[1].text.contains(r#"outcome="invalid" exit=1 problems=2"#), "{}", lines[1].raw);
    // Only the messages and fields are searched: a time or a run id can hold 641 by chance.
    let written = after_run_id(&log_text(&base));
    assert!(!written.contains("641") && !written.contains("soon"), "{written}");
}

#[test]
fn a_command_line_that_cannot_be_read_and_a_missing_folder_are_logged_with_their_codes() {
    let base = scratch(GROUP, "unreadable-and-missing");
    framesmith(&base, ["check"], &[]);
    framesmith(&base, ["check", "empty"], &[]);
    let lines = lines(&base);
    assert_eq!(lines.len(), 4, "{}", log_text(&base));
    assert!(lines[0].text.contains(r#"args=["check"]"#));
    assert!(
        lines[1].text.contains(r#"outcome="command line not read" exit=1"#),
        "{}",
        lines[1].raw
    );
    assert!(
        lines[3].text.contains(r#"outcome="no project" exit=1 code="FS-0013""#),
        "{}",
        lines[3].raw
    );
    assert_eq!(lines[2].level, "INFO");
    assert_eq!(lines[3].level, "WARN");
}

#[test]
fn a_composition_that_cannot_be_read_is_logged_as_an_error_with_its_code() {
    let base = project_here("unreadable-composition", VALID);
    let file = base.join("demo").join("composition.xml");
    fs::set_permissions(&file, fs::Permissions::from_mode(0o000)).unwrap();
    let output = check(&base, &[("FRAMESMITH_LOG", "error")]);
    fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(output.status.code(), Some(1));
    let lines = lines(&base);
    assert_eq!(lines.len(), 1, "{}", log_text(&base));
    assert_eq!(lines[0].level, "ERROR");
    assert!(
        lines[0].text.contains(r#"outcome="no project" exit=1 code="FS-0016""#),
        "{}",
        lines[0].raw
    );
}

#[test]
fn two_runs_have_different_run_ids_and_every_line_of_a_run_carries_that_runs_id() {
    let base = project_here("run-ids", INVALID);
    let mut seen = BTreeSet::new();
    let mut seen_before = 0;
    for _ in 0..25 {
        check(&base, &[("FRAMESMITH_LOG", "trace")]);
        let all = lines(&base);
        let this_run = &all[seen_before..];
        seen_before = all.len();
        // Trace shows the command, two steps of the project, two reports and the result: six lines.
        assert_eq!(this_run.len(), 6, "{this_run:#?}");
        let id = &this_run[0].run_id;
        assert_eq!(id.len(), 16);
        assert!(id.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)), "{id}");
        assert!(this_run.iter().all(|line| &line.run_id == id), "{this_run:#?}");
        assert!(seen.insert(id.clone()), "the run id {id} was used by an earlier run");
    }
    assert_eq!(seen.len(), 25);
}

/// `YYYY-MM-DDTHH:MM:SS.ffffffZ`.
fn is_time(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() == 27
        && bytes.iter().enumerate().all(|(at, &b)| match at {
            4 | 7 => b == b'-',
            10 => b == b'T',
            13 | 16 => b == b':',
            19 => b == b'.',
            26 => b == b'Z',
            _ => b.is_ascii_digit(),
        })
}

#[test]
fn every_line_has_a_time_a_level_and_the_name_of_the_part_that_wrote_it() {
    let base = project_here("line-form", INVALID);
    check(&base, &[("FRAMESMITH_LOG", "trace")]);
    // A level that is not a level adds a line from the log's own part.
    check(&base, &[("FRAMESMITH_LOG", "nonsense")]);
    let lines = lines(&base);
    assert!(lines.len() >= 8, "{}", log_text(&base));
    let mut components = BTreeSet::new();
    let mut levels = BTreeSet::new();
    for line in &lines {
        assert!(is_time(&line.time), "{}", line.raw);
        assert!(
            ["ERROR", "WARN", "INFO", "DEBUG", "TRACE"].contains(&line.level.as_str()),
            "{}",
            line.raw
        );
        assert!(
            !line.component.is_empty() && line.component.chars().all(|c| c.is_ascii_lowercase() || c == '_' || c == ':'),
            "{}",
            line.raw
        );
        assert!(line.run_id.len() == 16, "{}", line.raw);
        assert!(!line.text.is_empty(), "{}", line.raw);
        components.insert(line.component.clone());
        levels.insert(line.level.clone());
    }
    assert_eq!(
        components.into_iter().collect::<Vec<_>>(),
        ["cli", "cli::runlog", "project"],
        "the lines come from three parts"
    );
    assert_eq!(levels.len(), 4, "{levels:?}");
    // The time and the level are in the place the spec fixes: the level is padded to five.
    let info = lines.iter().find(|line| line.level == "INFO").unwrap();
    assert_eq!(&info.raw[27..34], " INFO  ", "{}", info.raw);
    // The times of one run do not go back.
    let first_run: Vec<_> = lines.iter().take(6).map(|l| l.time.clone()).collect();
    assert!(first_run.windows(2).all(|pair| pair[0] <= pair[1]), "{first_run:?}");
}

/// The messages of the lines of a run, in the order written, each cut at its first field.
fn messages(base: &Path) -> Vec<String> {
    lines(base)
        .iter()
        .map(|line| {
            let end = line
                .text
                .find(" version=")
                .or(line.text.find(" outcome="))
                .or(line.text.find(" bytes="))
                .or(line.text.find(" problems="))
                .or(line.text.find(" file="))
                .unwrap_or(line.text.len());
            line.text[..end].to_string()
        })
        .collect()
}

#[test]
fn a_quieter_level_removes_lines_below_it_and_a_more_detailed_level_adds_them() {
    let base = project_here("levels", INVALID);
    let mut counts = Vec::new();
    for level in ["error", "warn", "info", "debug", "trace"] {
        let before = lines(&base).len();
        check(&base, &[("FRAMESMITH_LOG", level)]);
        let this_run = lines(&base).len() - before;
        counts.push((level, this_run));
    }
    assert_eq!(
        counts,
        [("error", 0), ("warn", 1), ("info", 2), ("debug", 4), ("trace", 6)],
        "each level adds the lines of its own level to those of the quieter ones"
    );

    // The last run was at trace: the six lines, in order.
    let all = messages(&base);
    assert_eq!(
        &all[all.len() - 6..],
        [
            "command started",
            "composition file read",
            "root judged",
            "report",
            "report",
            "command finished"
        ]
    );
    // At warn the one line that is left is the result; at info the command comes before it.
    let warn_lines: Vec<_> = lines(&base).into_iter().filter(|line| line.level == "WARN").collect();
    assert!(warn_lines.iter().all(|line| line.text.starts_with("command finished")));
}

#[test]
fn a_valid_check_is_silent_at_warn_and_a_level_is_read_in_any_case_with_white_space_around_it() {
    let base = project_here("level-spelling", VALID);
    check(&base, &[("FRAMESMITH_LOG", "warn")]);
    assert_eq!(lines(&base).len(), 0);
    check(&base, &[("FRAMESMITH_LOG", "  DeBuG ")]);
    assert_eq!(lines(&base).len(), 4);
    check(&base, &[("FRAMESMITH_LOG", "")]);
    assert_eq!(lines(&base).len(), 6, "an empty value is the default level, info");
}

#[test]
fn a_value_that_is_not_a_level_gives_the_default_level_and_one_warning_that_does_not_hold_the_value() {
    let base = project_here("level-unreadable", VALID);
    let output = check(&base, &[("FRAMESMITH_LOG", "verbose-please")]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(err(&output), "");
    let lines = lines(&base);
    assert_eq!(lines.len(), 3, "{}", log_text(&base));
    assert_eq!(lines[1].level, "WARN");
    assert_eq!(lines[1].component, "cli::runlog");
    assert!(lines[1].text.contains("info is used"), "{}", lines[1].raw);
    assert!(!log_text(&base).contains("verbose-please"));
}

#[test]
fn the_log_is_kept_under_the_folder_the_command_is_run_from_and_not_in_a_project_folder_it_is_run_away_from() {
    let (project, cwd) = project(GROUP, "where", VALID);
    // The project is given by its full path and lies beside the folder the command is run from.
    let output = framesmith(&cwd, [std::ffi::OsStr::new("check"), project.as_os_str()], &[]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(lines(&cwd).len(), 2);
    let names: Vec<_> = fs::read_dir(&project).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert_eq!(names, ["composition.xml"]);
    let names: Vec<_> = fs::read_dir(cwd.join("logs")).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert_eq!(names, ["framesmith.log"]);
}

#[test]
fn a_run_adds_its_lines_at_the_end_and_changes_none_that_is_there() {
    let base = project_here("append", VALID);
    check(&base, &[]);
    let first = log_text(&base);
    check(&base, &[]);
    let second = log_text(&base);
    assert_eq!(first.lines().count(), 2);
    assert!(second.starts_with(&first));
    assert_eq!(second.lines().count(), 4);
}

#[test]
fn a_log_file_larger_than_5_mib_is_rotated_and_three_older_files_are_kept() {
    let base = project_here("rotation", VALID);
    let logs = base.join("logs");
    fs::create_dir(&logs).unwrap();
    let big = vec![b'x'; 5 * 1024 * 1024 + 1];
    fs::write(logs.join("framesmith.log"), &big).unwrap();
    fs::write(logs.join("framesmith.log.1"), "one").unwrap();
    fs::write(logs.join("framesmith.log.2"), "two").unwrap();
    fs::write(logs.join("framesmith.log.3"), "three").unwrap();

    check(&base, &[]);
    assert_eq!(fs::read(logs.join("framesmith.log.1")).unwrap(), big);
    assert_eq!(fs::read_to_string(logs.join("framesmith.log.2")).unwrap(), "one");
    assert_eq!(fs::read_to_string(logs.join("framesmith.log.3")).unwrap(), "two");
    assert!(!logs.join("framesmith.log.4").exists());
    let new = lines(&base);
    assert_eq!(new.len(), 2, "the run starts a new file");

    // A file of exactly 5 MiB is not rotated: the run adds to it.
    let exact = vec![b'y'; 5 * 1024 * 1024];
    fs::write(logs.join("framesmith.log"), &exact).unwrap();
    check(&base, &[]);
    let after = fs::read(logs.join("framesmith.log")).unwrap();
    assert!(after.starts_with(&exact) && after.len() > exact.len());
    assert_eq!(fs::read(logs.join("framesmith.log.1")).unwrap().len(), big.len());
}
