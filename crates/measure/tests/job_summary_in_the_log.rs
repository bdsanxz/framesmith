//! What `scripts/gates.sh` prints at its very end on a hosted runner: the job summary file as it then
//! is, between two marker lines, so that the log of the job holds what the summary page shows
//! (`docs/dev/gates.md`, section 3). The runner's variables are set to files of the test's own; no
//! real runner is involved.

mod common;

use common::scratch::Scratch;
use common::Run;
use std::path::Path;

const BEGIN: &str = "==== framesmith gates: job summary, begin ====";

/// A test of the scratch workspace that does what a program started by a gate can do on a runner:
/// it writes lines of its own to the job summary file, among them lines a runner would take for
/// commands if they were printed as they are.
const WRITES_TO_THE_SUMMARY: &str = r#"
use std::io::Write;

#[test]
fn a_program_the_gate_started_writes_to_the_job_summary() {
    let path = std::env::var("GITHUB_STEP_SUMMARY").expect("the gate gives its summary file");
    let mut file = std::fs::OpenOptions::new().append(true).open(path).expect("the file is opened");
    file.write_all(b"- FAIL static analysis\n::error title=forged::a command\n   ::warning::an indented command\n##[error]an old form of a command\ncarriage return then a command\r::error::after a carriage return\n")
        .expect("the file is written");
}
"#;

fn on_a_runner(summary: &Path) -> [(&'static str, &str); 2] {
    [
        ("GITHUB_ACTIONS", "true"),
        ("GITHUB_STEP_SUMMARY", summary.to_str().expect("a path of text")),
    ]
}

/// The lines of the output from the marker that begins the printed part to the end, as the log of a
/// job holds them.
fn printed_part(run: &Run) -> Vec<&str> {
    let lines: Vec<&str> = run.text.lines().collect();
    let begin = lines
        .iter()
        .position(|line| *line == BEGIN)
        .unwrap_or_else(|| panic!("the output has no begin marker: {}", run.text));
    lines[begin..].to_vec()
}

fn the_part_is_the_end_of_the_run(run: &Run) {
    let part = printed_part(run);
    assert_eq!(part.iter().filter(|line| **line == BEGIN).count(), 1, "{}", run.text);
    let last = part.last().expect("a last line");
    assert!(
        last.starts_with("==== framesmith gates: job summary, end (") && last.ends_with(") ===="),
        "the last line of the output is not the end marker: {}",
        run.text
    );
    assert!(
        !run.text
            .lines()
            .take_while(|line| *line != BEGIN)
            .any(|line| line.contains("job summary, end")),
        "an end marker comes before the begin marker: {}",
        run.text
    );
}

/// What the printed part holds between its markers, with the prefix of each line taken off.
fn shown(run: &Run) -> Vec<String> {
    let part = printed_part(run);
    part[1..part.len() - 1]
        .iter()
        .map(|line| {
            line.strip_prefix("| ")
                .unwrap_or_else(|| panic!("a printed line does not start with \"| \": {line:?}"))
                .to_string()
        })
        .collect()
}

#[test]
fn a_hosted_run_prints_the_summary_it_wrote_between_two_markers_as_its_last_lines() {
    let scratch = Scratch::new("summary-printed");
    let summary = scratch.dir.join("summary.md");
    std::fs::write(&summary, "### written before the gate ran\n").expect("the file is written");

    let run = scratch.gates(&["--hosted", "--gate", "format"], &on_a_runner(&summary));
    assert_eq!(run.code, 0, "{}", run.text);
    the_part_is_the_end_of_the_run(&run);
    let before = run.text.find("HOSTED GATE PASSED").expect("the run says it passed");
    assert!(
        before < run.text.find(BEGIN).unwrap(),
        "the part comes before the end of the run: {}",
        run.text
    );

    let written = std::fs::read_to_string(&summary).expect("the summary is read");
    assert_eq!(written, "### written before the gate ran\n- PASS formatting\n");
    assert_eq!(shown(&run), written.lines().collect::<Vec<_>>(), "{}", run.text);
    assert!(
        run.text.trim_end().ends_with("job summary, end (2 line(s)) ===="),
        "the end marker does not count the lines: {}",
        run.text
    );
}

#[test]
fn a_false_line_written_by_a_program_the_gate_started_is_shown_in_the_printed_part() {
    let scratch = Scratch::new("summary-false-line");
    scratch.write("crates/user/tests/writes_to_the_summary.rs", WRITES_TO_THE_SUMMARY);
    let summary = scratch.dir.join("summary.md");
    std::fs::write(&summary, "").expect("the file is made, as the runner makes it");

    let run = scratch.gates(&["--hosted", "--gate", "test"], &on_a_runner(&summary));
    assert_eq!(
        run.code, 0,
        "the gate passed, and the false line is what the part must show: {}",
        run.text
    );
    assert!(run.text.contains("PASS tests"), "{}", run.text);
    the_part_is_the_end_of_the_run(&run);
    let shown = shown(&run);
    assert!(
        shown.iter().any(|line| line == "- FAIL static analysis"),
        "the false line is not in the printed part: {shown:?}"
    );
    assert!(shown.iter().any(|line| line == "- PASS tests"), "{shown:?}");
    // The false line came first, the gate's own line after it.
    let at = |wanted: &str| shown.iter().position(|line| line == wanted).expect("a line");
    assert!(at("- FAIL static analysis") < at("- PASS tests"), "{shown:?}");
}

#[test]
fn no_line_of_the_printed_part_can_be_taken_for_a_command_of_the_runner() {
    let scratch = Scratch::new("summary-commands");
    scratch.write("crates/user/tests/writes_to_the_summary.rs", WRITES_TO_THE_SUMMARY);
    let summary = scratch.dir.join("summary.md");
    std::fs::write(&summary, "").expect("the file is made, as the runner makes it");

    let run = scratch.gates(&["--hosted", "--gate", "test"], &on_a_runner(&summary));
    assert_eq!(run.code, 0, "{}", run.text);

    // The control: the file does hold what a runner would take for commands once its lines are cut
    // as the runner cuts them (at a line feed and at a carriage return, after leading spaces).
    let written = std::fs::read_to_string(&summary).expect("the summary is read");
    let is_a_command = |line: &str| line.trim_start().starts_with("::") || line.trim_start().starts_with("##[");
    let commands_in_the_file = written.split(['\n', '\r']).filter(|line| is_a_command(line)).count();
    assert_eq!(commands_in_the_file, 4, "the control has lost its forged lines: {written:?}");

    // The printed part has none, cut the same way, and every line of it begins with the prefix.
    let part = printed_part(&run).join("\n");
    let commands_in_the_part = part.split(['\n', '\r']).filter(|line| is_a_command(line)).count();
    assert_eq!(commands_in_the_part, 0, "{part}");
    let shown = shown(&run);
    assert!(
        shown.iter().any(|line| line == "::error title=forged::a command")
            && shown.iter().any(|line| line == "::error::after a carriage return"),
        "the forged lines are shown, as text: {shown:?}"
    );
}

#[test]
fn a_failing_hosted_gate_prints_its_summary_after_its_failure() {
    let scratch = Scratch::new("summary-failing");
    scratch.write(
        "crates/user/tests/plain.rs",
        "#[test]\nfn a_test_that_breaks() {\n    assert_eq!(1 + 1, 3);\n}\n",
    );
    let summary = scratch.dir.join("summary.md");
    std::fs::write(&summary, "").expect("the file is made, as the runner makes it");

    let run = scratch.gates(&["--hosted", "--gate", "test"], &on_a_runner(&summary));
    assert_eq!(run.code, 1, "{}", run.text);
    the_part_is_the_end_of_the_run(&run);
    assert!(
        run.text.find("GATES FAILED").unwrap() < run.text.find(BEGIN).unwrap(),
        "{}",
        run.text
    );
    let shown = shown(&run);
    assert!(shown.iter().any(|line| line == "- FAIL tests"), "{shown:?}");
    assert!(
        shown
            .iter()
            .any(|line| line.starts_with("    test a_test_that_breaks failed (crates/user/tests/plain.rs:")),
        "{shown:?}"
    );
}

#[test]
fn a_run_that_is_not_on_a_runner_prints_nothing_of_the_kind() {
    let scratch = Scratch::new("summary-local");
    let summary = scratch.dir.join("summary.md");

    // A local run: no GITHUB_ACTIONS, though the summary variable is set. Nothing is written to the
    // file either (the gate writes to it only on a runner), and nothing is printed.
    let local = scratch.gates(
        &["--hosted", "--gate", "format"],
        &[("GITHUB_STEP_SUMMARY", summary.to_str().unwrap())],
    );
    assert_eq!(local.code, 0, "{}", local.text);
    assert!(!local.text.contains("job summary"), "{}", local.text);
    assert!(!summary.exists(), "a local run wrote to the summary file");
    assert!(local.text.trim_end().ends_with("HOSTED GATE PASSED"), "{}", local.text);

    // The marker of the runner and no summary file: there is nothing to print.
    let no_file = scratch.gates(&["--hosted", "--gate", "format"], &[("GITHUB_ACTIONS", "true")]);
    assert_eq!(no_file.code, 0, "{}", no_file.text);
    assert!(!no_file.text.contains("job summary"), "{}", no_file.text);
    assert!(no_file.text.trim_end().ends_with("HOSTED GATE PASSED"), "{}", no_file.text);

    // A run that is not a pass, narrowed or whole, prints nothing either.
    let narrowed = scratch.gates(&["--gate", "format"], &[("GITHUB_STEP_SUMMARY", summary.to_str().unwrap())]);
    assert_eq!(narrowed.code, 3, "{}", narrowed.text);
    assert!(!narrowed.text.contains("job summary"), "{}", narrowed.text);
}

#[test]
fn a_summary_file_that_cannot_be_read_is_said_so_in_the_part() {
    let scratch = Scratch::new("summary-unreadable");
    // A folder in the place of the file: nothing can be appended to it and nothing can be read.
    let summary = scratch.dir.join("summary-is-a-folder");
    std::fs::create_dir_all(&summary).expect("the folder is made");

    let run = scratch.gates(&["--hosted", "--gate", "format"], &on_a_runner(&summary));
    assert_eq!(run.code, 0, "{}", run.text);
    // (The script's own complaint that it cannot append to a folder is on its standard error, which
    // the helper puts after the standard output: the part is the first two lines from the marker.)
    let part = printed_part(&run);
    assert_eq!(
        part[..2],
        [BEGIN, "==== framesmith gates: job summary, end (the file cannot be read) ===="],
        "{}",
        run.text
    );
}
