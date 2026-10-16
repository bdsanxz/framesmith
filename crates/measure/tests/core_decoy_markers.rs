//! The control of the core gate proves each refusal of a decoy program by the decoy's marker file, and
//! not by the system's log (`scripts/gates/core-decoys.sh`; `docs/dev/gates.md`, section 10). These tests
//! run the steps of that script on decoys of their own, in a folder of their own, and hold each verdict:
//! a decoy that writes its marker when it runs outside the profile is proven so; a decoy that writes none
//! fails the proof; a decoy that ran fails the judgement, and is named; a suite run whose system log holds
//! no record of a refusal passes, with a note. The control itself cannot run inside the profile of the
//! core suite (the system lets neither `log` nor a second `sandbox-exec` run there), so the whole control,
//! with the log's stand-in, is shown by the evidence of the ticket (`evidence/T006/run.sh`).
//!
//! The two programs the decoys stand for are put together from pieces below, so that this file names
//! neither of them in its text: the core suite refuses a core test file that does (`docs/dev/gates.md`,
//! section 10).

mod common;

use common::isolate_from_runner;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// The encoder program and the prober program that the decoys stand for, put together from pieces.
const ENCODER: &str = concat!("ff", "mpeg");
const PROBER: &str = concat!("ff", "probe");

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

/// A folder of this test's own, made new under the tests' target folder.
fn folder(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("core-decoy-markers").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("the folder is made");
    dir
}

fn text_of(path: &Path) -> &str {
    path.to_str().expect("the path is text")
}

/// What a command printed, how it ended and how long it took.
struct Run {
    code: i32,
    text: String,
    took: Duration,
}

/// Runs a command, stopping it when it has run for `limit`, so that a hang is a failure of the test.
fn run(mut command: Command, limit: Duration) -> Run {
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

/// One step of the script (`make`, `prove` or `judge`) with its arguments.
fn step(args: &[&str]) -> Run {
    let mut command = Command::new("bash");
    command.arg(root().join("scripts/gates/core-decoys.sh")).args(args);
    run(command, Duration::from_secs(60))
}

fn marker(folder: &Path, decoy: &str) -> PathBuf {
    folder.join("markers").join(decoy)
}

fn executable(path: &Path, content: &str) {
    std::fs::write(path, content).expect("the file is written");
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).expect("the file is made runnable");
}

/// A stand-in for `log show` that gives no record of a refused start, at any time.
fn never_records(folder: &Path) -> PathBuf {
    let script = folder.join("log-stand-in");
    executable(&script, "#!/bin/bash\necho \"Filtering the log data\"\n");
    script
}

/// One failing test's section of a suite run, as cargo prints it.
fn failure(name: &str, detail: &str) -> String {
    format!(
        "---- {name} stdout ----\nthread '{name}' panicked at crates/offenders/tests/offending.rs:9:5:\n\
         called `Result::unwrap()` on an `Err` value: {detail}\n\
         note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace\n\n"
    )
}

const REFUSED: &str = "Os { code: 1, kind: PermissionDenied, message: \"Operation not permitted\" }";
const NOT_FOUND: &str = "Os { code: 2, kind: NotFound, message: \"No such file or directory\" }";

/// The output of a run of the control's suite in its workspace, as cargo prints it. The decoy started by
/// its path fails with the refusal, unless `encoder_ran`, when it ran and passed.
fn suite_output(encoder_ran: bool) -> String {
    let mut text = String::from("     Running tests/offending.rs (target/debug/deps/offending-0123456789abcdef)\nrunning 7 tests\n");
    text.push_str(&format!("test starts_{}_by_name ... FAILED\n", ENCODER));
    text.push_str(&format!("test starts_{}_by_name ... FAILED\n", PROBER));
    text.push_str(&format!(
        "test starts_{}_by_its_path ... {}\n",
        ENCODER,
        if encoder_ran { "ok" } else { "FAILED" }
    ));
    text.push_str("test starts_a_command_installed_outside_the_target_and_the_system ... FAILED\n");
    text.push_str(&format!(
        "test starts_a_system_tool ... ok\ntest takes_{}_not_being_found_for_success ... ok\ntest starts_nothing ... ok\n\n",
        ENCODER
    ));
    text.push_str("failures:\n\n");
    text.push_str(&failure(&format!("starts_{}_by_name", ENCODER), NOT_FOUND));
    text.push_str(&failure(&format!("starts_{}_by_name", PROBER), NOT_FOUND));
    if !encoder_ran {
        text.push_str(&failure(&format!("starts_{}_by_its_path", ENCODER), REFUSED));
    }
    text.push_str(&failure("starts_a_command_installed_outside_the_target_and_the_system", REFUSED));
    let failed = if encoder_ran { 3 } else { 4 };
    text.push_str("failures:\n");
    text.push_str(&format!(
        "\ntest result: FAILED. {} passed; {failed} failed; 0 ignored; 0 measured; 0 filtered out\n",
        7 - failed
    ));
    text
}

fn problems(text: &str) -> Vec<&str> {
    text.lines().filter(|line| line.starts_with("core self-test problem:")).collect()
}

#[test]
fn a_decoy_refused_by_the_system_is_proven_by_its_marker_when_the_log_holds_no_record() {
    let dir = folder("refused-without-a-record");
    let made = step(&["make", text_of(&dir)]);
    assert_eq!(made.code, 0, "{}", made.text);
    let proved = step(&["prove", text_of(&dir)]);
    assert_eq!(proved.code, 0, "{}", proved.text);
    assert!(proved.text.contains(&format!(
        "proved, outside the profile, that decoy/{} writes its marker file when it runs",
        ENCODER
    )));
    assert!(proved
        .text
        .contains("proved, outside the profile, that decoy/outside/bin/outside-command writes its marker file when it runs"));
    assert!(!marker(&dir, ENCODER).exists(), "the proof left its marker behind");
    assert!(
        !marker(&dir, "outside_bin_outside-command").exists(),
        "the proof left its marker behind"
    );

    // The suite's output, read by the reader with a stand-in that gives no record: the reader waits for its bound.
    let output = dir.join("core-selftest-suite.log");
    std::fs::write(&output, suite_output(false)).expect("the output of the run is written");
    let mut reader = Command::new("bash");
    reader
        .arg(root().join("scripts/gates/core-refused.sh"))
        .args(["--since", "2026-10-16 08:00:00", "--run-log"])
        .arg(&output)
        .env("FRAMESMITH_CORE_LOG_COMMAND", never_records(&dir));
    let read = run(reader, Duration::from_secs(40));
    assert_eq!(read.code, 0, "{}", read.text);
    assert!(
        read.text
            .contains("core problem: after waiting 20 seconds the system log names 0 program(s)"),
        "{}",
        read.text
    );
    assert!(
        read.took >= Duration::from_secs(20),
        "the reader gave up after {:?}, before the bound",
        read.took
    );
    std::fs::OpenOptions::new()
        .append(true)
        .open(&output)
        .expect("the output of the run is opened")
        .write_all(read.text.as_bytes())
        .expect("the reader's lines are added to the output");

    let judged = step(&["judge", text_of(&dir), text_of(&output)]);
    assert_eq!(judged.code, 0, "{}", judged.text);
    assert!(
        judged.text.contains(&format!(
            "refused, as it must: decoy/{} (test starts_{}_by_its_path failed by its name",
            ENCODER, ENCODER
        )),
        "{}",
        judged.text
    );
    assert!(
        judged.text.contains("refused, as it must: decoy/outside/bin/outside-command (test starts_a_command_installed_outside_the_target_and_the_system failed by its name"),
        "{}",
        judged.text
    );
    assert!(
        judged
            .text
            .contains("core self-test note: the system log named no decoy program as refused in this run"),
        "{}",
        judged.text
    );
    assert!(problems(&judged.text).is_empty(), "{}", judged.text);
    assert!(
        !judged.text.lines().any(|line| line.starts_with("core problem:")),
        "a line of the suite's own reading reached the control's output: {}",
        judged.text
    );
}

#[test]
fn a_decoy_that_ran_fails_the_control_and_is_named() {
    let dir = folder("decoy-ran");
    let made = step(&["make", text_of(&dir)]);
    assert_eq!(made.code, 0, "{}", made.text);
    let proved = step(&["prove", text_of(&dir)]);
    assert_eq!(proved.code, 0, "{}", proved.text);

    // The decoy that is started by its path runs: it writes its marker, and its test passes.
    let ran = Command::new(dir.join("decoy").join(ENCODER));
    let started = run(ran, Duration::from_secs(20));
    assert_eq!(started.code, 0, "{}", started.text);
    assert!(marker(&dir, ENCODER).exists(), "the decoy ran but wrote no marker");

    let output = dir.join("core-selftest-suite.log");
    std::fs::write(&output, suite_output(true)).expect("the output of the run is written");
    let judged = step(&["judge", text_of(&dir), text_of(&output)]);
    assert_eq!(judged.code, 1, "{}", judged.text);
    let ran_line = format!("decoy/{} ran", ENCODER);
    let named: Vec<&str> = problems(&judged.text).into_iter().filter(|line| line.contains(&ran_line)).collect();
    assert_eq!(named.len(), 1, "the decoy that ran is not named by its path: {}", judged.text);
    assert!(
        judged.text.contains("refused, as it must: decoy/outside/bin/outside-command"),
        "the other decoy was still refused: {}",
        judged.text
    );
}

#[test]
fn a_decoy_that_writes_no_marker_fails_the_proof_outside_the_profile() {
    let dir = folder("decoy-silent");
    let made = step(&["make", text_of(&dir)]);
    assert_eq!(made.code, 0, "{}", made.text);
    // The decoy that is started by its path is made to write nothing when it runs.
    executable(&dir.join("decoy").join(ENCODER), "#!/bin/sh\nexit 0\n");

    let proved = step(&["prove", text_of(&dir)]);
    assert_eq!(proved.code, 1, "{}", proved.text);
    assert!(
        proved.text.contains(&format!(
            "core self-test problem: decoy/{} wrote no marker file when it ran outside the profile",
            ENCODER
        )),
        "{}",
        proved.text
    );
    assert!(
        proved
            .text
            .contains("proved, outside the profile, that decoy/outside/bin/outside-command writes its marker file when it runs"),
        "the other decoy was not proven: {}",
        proved.text
    );
}

#[test]
fn the_decoys_the_control_starts_are_the_ones_the_judge_checks() {
    let control = std::fs::read_to_string(root().join("scripts/gates/core-selftest.sh")).expect("the control is read");
    let decoys = std::fs::read_to_string(root().join("scripts/gates/core-decoys.sh")).expect("the script is read");
    let by_its_path = format!("starts_{}_by_its_path", ENCODER);
    for name in [by_its_path.as_str(), "starts_a_command_installed_outside_the_target_and_the_system"] {
        assert!(
            control.contains(&format!("fn {name}()")),
            "the control's workspace has no test {name}"
        );
        assert!(decoys.contains(name), "the judge does not check the test {name}");
    }
    let encoder_decoy = format!("decoy/{}", ENCODER);
    for path in [encoder_decoy.as_str(), "decoy/outside/bin/outside-command"] {
        assert!(control.contains(path), "the control's workspace does not start {path}");
    }
    for step in ["make", "prove", "judge"] {
        assert!(
            control.contains(&format!("\"$DECOYS_SH\" {step}")),
            "the control does not run the decoys' {step} step"
        );
    }
}
