//! The core gate names the programs the system refused to start from the system's log, and the
//! system writes its record of a refusal some time after it refuses the start. These tests make the
//! record arrive late, or never, on purpose, with a stand-in for the `log` command (named in
//! `FRAMESMITH_CORE_LOG_COMMAND`, which only these tests use), and require that the reading in
//! `scripts/gates/core-refused.sh` still names every refused program, or ends within the bound and
//! says so (`docs/dev/gates.md`, section 10, "Reading the refused starts"); and that the suite and
//! the control of the gate go through that reading.

mod common;

use common::isolate_from_runner;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

/// A folder of this test's own, made new.
fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("refused-starts").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("the folder is made");
    dir
}

fn executable(path: &Path, content: &str) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::write(path, content).expect("the file is written");
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).expect("the file is made runnable");
}

/// What a command printed and how long it took.
struct Run {
    code: i32,
    text: String,
    took: Duration,
}

/// Runs a command, stopping it when it has run for `limit`, so that a hang is a failure of the test.
fn run_limited(mut command: Command, limit: Duration) -> Run {
    isolate_from_runner(&mut command);
    command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let start = Instant::now();
    let child = command.spawn().expect("the command starts");
    let id = child.id();
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(child.wait_with_output());
    });
    match receiver.recv_timeout(limit) {
        Ok(output) => {
            let output = output.expect("the command ends");
            let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
            text.push_str(&String::from_utf8_lossy(&output.stderr));
            Run {
                code: output.status.code().unwrap_or(-1),
                text,
                took: start.elapsed(),
            }
        }
        Err(_) => {
            // The command this test started, by its process id.
            let _ = Command::new("kill").arg(id.to_string()).status();
            panic!("the command was still running after {limit:?}");
        }
    }
}

const PROGRAM: &str = "latecomer-0123456789abcdef";
const FIRST: &str = "/work/decoy/first-program";
const SECOND: &str = "/work/decoy/outside/bin/outside-command";

/// The output of a run of the core suite: two test programs' lines, and two tests whose start the
/// profile forbade (and, as the suite prints them, "Operation not permitted" for each).
fn run_log(refused_tests: usize) -> String {
    let mut text = format!("     Running tests/late.rs (target/debug/deps/{PROGRAM})\nrunning 2 tests\n");
    for _ in 0..refused_tests {
        text.push_str("start the program: Os { code: 1, kind: PermissionDenied, message: \"Operation not permitted\" }\n");
    }
    text.push_str("start the program: Os { code: 2, kind: NotFound, message: \"No such file or directory\" }\n");
    text
}

/// How long the record of a late refusal is left out of the answers, counted from the first answer. It
/// is shorter than the shortest wait of the script (3 seconds), as the 0.9 seconds observed are.
const LATE_BY_MS: u64 = 1500;

/// The part of a stand-in that counts its answers and tells how long ago the first one was given. The
/// state is kept in files of `folder`: `answers` (the count), `first` (the time of the first answer, in
/// milliseconds) and `hidden` (a line for each answer that left a record out). Sets `$age` in the script.
fn stand_in_head(folder: &Path) -> String {
    format!(
        "#!/bin/bash\nstate='{state}'\nnow=$(perl -MTime::HiRes=time -e 'printf \"%d\", time() * 1000')\n\
         n=$(cat \"$state/answers\" 2>/dev/null || echo 0)\nn=$((n + 1))\necho \"$n\" >\"$state/answers\"\n\
         [ -f \"$state/first\" ] || echo \"$now\" >\"$state/first\"\nage=$((now - $(cat \"$state/first\")))\n",
        state = folder.display()
    )
}

/// A stand-in for `log show`: it answers in the compact form with the lines the system writes for a
/// refused start. The first record is there from the first answer; the second one only from
/// `second_after_ms` after the first answer on, and never when that is `None`. A record of a program
/// that is not of this run is always there. `readable` false makes it end with an error, and then it
/// keeps the time of each answer, in milliseconds since 1970, in the file `answered`. `first_sleep`
/// makes the first answer of a readable stand-in come only after that many seconds.
fn stand_in_script(folder: &Path, second_after_ms: Option<u64>, readable: bool, first_sleep: Option<&str>) -> PathBuf {
    let script = folder.join("log-stand-in");
    let mut body = stand_in_head(folder);
    if readable {
        if let Some(seconds) = first_sleep {
            body.push_str(&format!("if [ \"$n\" -eq 1 ]; then sleep {seconds}; fi\n"));
        }
        body.push_str(&format!(
            "echo \"Filtering the log data using \\\"eventMessage CONTAINS ...\\\"\"\n\
             echo \"2026-10-14 07:15:35.191 E  kernel[0:29c9bd6] (Sandbox) Sandbox: {PROGRAM}(25029) deny(1) process-exec* {FIRST}\"\n\
             echo \"2026-10-14 07:15:35.200 E  kernel[0:29c9bd7] (Sandbox) Sandbox: another-worktree-0000(31) deny(1) process-exec* /elsewhere/other-program\"\n"
        ));
        if let Some(after) = second_after_ms {
            body.push_str(&format!(
                "if [ \"$age\" -ge {after} ]; then echo \"2026-10-14 07:15:36.102 E  sandboxd[395:1c0b] (Sandbox) Sandbox: {PROGRAM}(25030) deny(1) process-exec* {SECOND}\"; else echo hidden >>\"$state/hidden\"; fi\n"
            ));
        }
    } else {
        body.push_str("echo \"log: cannot open the log archive\" >&2\n");
        body.push_str("perl -MTime::HiRes=time -e 'printf \"%d\\n\", time() * 1000' >>\"$state/answered\"\n");
        body.push_str("exit 1\n");
    }
    executable(&script, &body);
    script
}

fn log_stand_in(folder: &Path, second_after_ms: Option<u64>, readable: bool) -> PathBuf {
    stand_in_script(folder, second_after_ms, readable, None)
}

fn count_lines(folder: &Path, file: &str) -> usize {
    std::fs::read_to_string(folder.join(file))
        .map(|text| text.lines().count())
        .unwrap_or(0)
}

fn answers(folder: &Path) -> usize {
    std::fs::read_to_string(folder.join("answers"))
        .ok()
        .and_then(|text| text.trim().parse().ok())
        .unwrap_or(0)
}

/// The times, in milliseconds since 1970, at which an unreadable stand-in gave its answers, in order.
fn answer_times(folder: &Path) -> Vec<u128> {
    std::fs::read_to_string(folder.join("answered"))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| line.trim().parse().ok())
        .collect()
}

fn unix_millis_now() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("the clock is after 1970")
        .as_millis()
}

/// Runs the reading with the stand-in against the output of a run that had `refused_tests` forbidden starts.
fn read_with(folder: &Path, stand_in: &Path, refused_tests: usize, limit: u64) -> Run {
    let log = folder.join("core-suite.log");
    std::fs::write(&log, run_log(refused_tests)).expect("the output of the run is written");
    let mut command = Command::new("bash");
    command
        .arg(root().join("scripts/gates/core-refused.sh"))
        .args(["--since", "2026-10-14 07:15:30", "--run-log"])
        .arg(&log)
        .env("FRAMESMITH_CORE_LOG_COMMAND", stand_in);
    run_limited(command, Duration::from_secs(limit))
}

fn named(path: &str) -> String {
    format!("core problem: the system refused to start {path}")
}

#[test]
fn a_refusal_the_system_records_late_is_still_named() {
    let folder = scratch("recorded-late");
    // The record of the second start is left out of the answers of the first 1.5 seconds, as when the system is slow.
    let stand_in = log_stand_in(&folder, Some(LATE_BY_MS), true);
    let run = read_with(&folder, &stand_in, 2, 40);
    assert_eq!(run.code, 0, "{}", run.text);
    assert!(run.text.contains(&named(FIRST)), "{}", run.text);
    assert!(run.text.contains(&named(SECOND)), "{}", run.text);
    assert!(
        count_lines(&folder, "hidden") >= 1,
        "no answer left the record out, so nothing arrived late"
    );
    assert!(
        answers(&folder) > count_lines(&folder, "hidden"),
        "the log was read {} times, and every answer left the record out",
        answers(&folder)
    );
    assert!(!run.text.contains("may be missing"), "{}", run.text);
    assert!(run.took < Duration::from_secs(15), "took {:?}", run.took);
}

#[test]
fn refusals_recorded_at_once_are_named_after_the_shortest_wait_and_no_longer() {
    let folder = scratch("recorded-at-once");
    let stand_in = log_stand_in(&folder, Some(0), true);
    let run = read_with(&folder, &stand_in, 2, 40);
    assert_eq!(run.code, 0, "{}", run.text);
    assert!(
        run.text.contains(&named(FIRST)) && run.text.contains(&named(SECOND)),
        "{}",
        run.text
    );
    assert!(
        run.took >= Duration::from_secs(3),
        "stopped after {:?}, before the shortest wait",
        run.took
    );
    assert!(run.took < Duration::from_secs(10), "took {:?}", run.took);
    assert!(!run.text.contains("may be missing"), "{}", run.text);
    // The log belongs to the whole machine: the stand-in also answers with a refusal of a program of another run.
    assert!(!run.text.contains("/elsewhere/other-program"), "{}", run.text);
}

#[test]
fn a_record_that_never_arrives_ends_the_wait_at_the_bound_and_says_so() {
    let folder = scratch("never-recorded");
    let stand_in = log_stand_in(&folder, None, true);
    let run = read_with(&folder, &stand_in, 2, 60);
    assert_eq!(run.code, 0, "{}", run.text);
    assert!(
        run.text.contains(&named(FIRST)),
        "the program that was recorded is named: {}",
        run.text
    );
    assert!(!run.text.contains(&named(SECOND)), "{}", run.text);
    assert!(
        run.text.contains(
            "core problem: after waiting 20 seconds the system log names 1 program(s) the system refused to start, \
             and the output of the tests shows 2 start(s) refused (\"Operation not permitted\"); a refused program may be missing from this list"
        ),
        "{}",
        run.text
    );
    assert!(
        run.took >= Duration::from_secs(20),
        "stopped after {:?}, before the bound",
        run.took
    );
    assert!(run.took < Duration::from_secs(30), "took {:?}, after the bound", run.took);
}

#[test]
fn no_reading_is_begun_after_the_bound() {
    let folder = scratch("first-answer-past-the-pause");
    // The first answer comes 19.2 seconds after the start: it ends before the bound, and the pause after it ends past the bound.
    let stand_in = stand_in_script(&folder, None, true, Some("19.2"));
    let run = read_with(&folder, &stand_in, 2, 40);
    assert_eq!(run.code, 0, "{}", run.text);
    assert_eq!(answers(&folder), 1, "a reading was begun after the bound: {}", run.text);
    assert!(run.text.contains(&named(FIRST)), "{}", run.text);
    assert!(
        run.text.contains(
            "core problem: after waiting 20 seconds the system log names 1 program(s) the system refused to start, \
             and the output of the tests shows 2 start(s) refused (\"Operation not permitted\"); a refused program may be missing from this list"
        ),
        "{}",
        run.text
    );
    assert!(
        run.took >= Duration::from_secs(20),
        "stopped after {:?}, before the bound",
        run.took
    );
    assert!(run.took < Duration::from_secs(30), "took {:?}, after the bound", run.took);
}

#[test]
fn a_log_that_cannot_be_read_ends_the_wait_and_says_so() {
    let folder = scratch("unreadable");
    let stand_in = log_stand_in(&folder, None, false);
    let run = read_with(&folder, &stand_in, 2, 40);
    let ended = unix_millis_now();
    assert_eq!(run.code, 0, "{}", run.text);
    assert!(
        run.text
            .contains("core problem: the system log could not be read, so the programs the system refused to start are not named"),
        "{}",
        run.text
    );
    assert!(!run.text.contains("refused to start /"), "{}", run.text);
    assert_eq!(answers(&folder), 2, "two readings in a row end the wait");
    // Measured from the answer of the second reading, not from the start of the run: a busy machine can
    // make each start of `log` take seconds, and a wait that ran on to its bound would be about 20 seconds
    // from that answer.
    let second = answer_times(&folder)
        .get(1)
        .copied()
        .expect("the time of the second answer is kept");
    let after_second = ended.saturating_sub(second);
    assert!(
        after_second < 10_000,
        "{after_second} ms from the second answer to the end of the wait"
    );
}

#[test]
fn a_run_whose_output_shows_no_refused_start_ends_the_wait_after_the_shortest_wait() {
    let folder = scratch("nothing-refused");
    let stand_in = log_stand_in(&folder, None, true);
    // The stand-in still answers with the one record; the output of this run shows no refusal, so the
    // wait ends as soon as two readings agree after the shortest wait.
    let run = read_with(&folder, &stand_in, 0, 40);
    assert_eq!(run.code, 0, "{}", run.text);
    assert!(run.text.contains(&named(FIRST)), "{}", run.text);
    assert!(!run.text.contains("may be missing"), "{}", run.text);
    assert!(
        run.took >= Duration::from_secs(3) && run.took < Duration::from_secs(10),
        "took {:?}",
        run.took
    );
}

#[test]
fn the_document_states_the_numbers_the_script_waits_by() {
    let text = std::fs::read_to_string(root().join("docs/dev/gates.md")).expect("the document is read");
    let paragraph = text
        .split("**Reading the refused starts.**")
        .nth(1)
        .expect("the document has the paragraph on reading the refused starts")
        .split("**Placement.**")
        .next()
        .unwrap();
    for stated in ["**3 seconds**", "**20 seconds**", "30 seconds", "FRAMESMITH_CORE_LOG_COMMAND"] {
        assert!(paragraph.contains(stated), "the paragraph does not state {stated}");
    }
    let script = std::fs::read_to_string(root().join("scripts/gates/core-refused.sh")).expect("the script is read");
    for set in ["SETTLE_MS=3000", "BOUND_MS=20000", "READ_LIMIT=30"] {
        assert!(script.contains(set), "the script does not set {set}");
    }
}

// --- the wiring of the suite and of the control -----------------------------------------------------
//
// The suite and the control cannot be run by a test under the profile of the core gate: the system
// lets neither `log` ("Cannot run while sandboxed") nor a second `sandbox-exec` run inside it. Their
// reading of the refused starts is therefore proved by the tests above, against a stand-in, and by the
// evidence of the ticket, which runs the control with a stand-in outside the profile; these tests hold
// the suite and the control to the reader.

fn text(relative: &str) -> String {
    std::fs::read_to_string(root().join(relative)).unwrap_or_else(|why| panic!("{relative}: {why}"))
}

#[test]
fn the_suite_reads_the_refused_starts_through_the_reader_after_each_run_that_failed() {
    let suite = text("scripts/gates/core-suite.sh");
    assert!(
        suite.contains("bash \"$REPO/scripts/gates/core-refused.sh\" --since \"$STARTED\" --run-log \"$1\""),
        "the suite does not call the reader with the start of the run and the output of the run"
    );
    assert!(
        !suite.contains("log show"),
        "the suite reads the system log itself, once, and not through the reader"
    );
    for run in ["core-suite", "core-reference"] {
        let call = format!("refused_starts \"$LOGS/{run}.log\"");
        let at = suite
            .find(&call)
            .unwrap_or_else(|| panic!("the suite does not read the refused starts of {run}"));
        let before = &suite[..at];
        let guard = before.rfind("if [ \"$RUN_CODE\" -ne 0 ]; then").expect("the call is guarded");
        assert!(
            !before[guard..].contains("fi\n"),
            "the reading of {run} is not inside the branch of a run that failed"
        );
    }
}

#[test]
fn the_control_proves_each_refusal_by_its_marker_and_reads_the_log_only_as_a_note() {
    let control = text("scripts/gates/core-selftest.sh");
    for step in ["make", "prove", "judge"] {
        assert!(
            control.contains(&format!("\"$DECOYS_SH\" {step}")),
            "the control does not run the decoys' {step} step"
        );
    }
    assert!(control.contains("scripts/gates/core-suite.sh\" --hosted --root \"$SELF\""));
    assert!(
        !control.contains("grep -q \"^core problem: the system refused to start"),
        "the control still requires the system log to name the decoys"
    );
}
