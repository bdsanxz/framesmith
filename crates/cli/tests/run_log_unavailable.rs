//! A command still runs when its log cannot be written (`docs/spec/run-log.md` section 7).

mod common;

use std::ffi::OsStr;
use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use cli::runlog::{Log, UNAVAILABLE};
use common::*;
use tracing::level_filters::LevelFilter;

const GROUP: &str = "run-log-unavailable";

/// The notice, with its line feed, as the spec words it.
const NOTICE: &str = "framesmith: the log is unavailable: logs/framesmith.log under the folder you ran from could not be written. The command ran as usual; make that place writable to keep a log.\n";

/// Ways to make the log's place unwritable. Each takes the folder the command will be run from.
/// A way to spoil the place, with what it does in words.
type Spoil = (&'static str, fn(&Path));

fn unwritable_places() -> Vec<Spoil> {
    vec![
        ("the working folder is read-only", |cwd| {
            fs::set_permissions(cwd, fs::Permissions::from_mode(0o555)).unwrap()
        }),
        ("logs is a file", |cwd| fs::write(cwd.join("logs"), "not a folder").unwrap()),
        ("logs is read-only", |cwd| {
            fs::create_dir(cwd.join("logs")).unwrap();
            fs::set_permissions(cwd.join("logs"), fs::Permissions::from_mode(0o555)).unwrap()
        }),
        ("the log file is a folder", |cwd| {
            fs::create_dir_all(cwd.join("logs").join("framesmith.log")).unwrap()
        }),
        ("the log file is read-only", |cwd| {
            fs::create_dir(cwd.join("logs")).unwrap();
            fs::write(cwd.join("logs").join("framesmith.log"), "old\n").unwrap();
            fs::set_permissions(cwd.join("logs").join("framesmith.log"), fs::Permissions::from_mode(0o444)).unwrap()
        }),
    ]
}

fn run_in(cwd: &Path, args: &[&OsStr], env: &[(&str, &str)]) -> std::process::Output {
    framesmith(cwd, args.iter().copied(), env)
}

#[test]
fn with_the_logs_place_unwritable_the_command_gives_its_normal_result_and_exit_code_and_one_line_on_standard_error() {
    for (index, (why, spoil)) in unwritable_places().into_iter().enumerate() {
        for (text, label) in [(VALID, "valid"), (INVALID, "invalid")] {
            // The normal result, from a place that can be written.
            let (folder, good_cwd) = project(GROUP, &format!("normal-{index}-{label}"), text);
            let normal = run_in(&good_cwd, &[OsStr::new("check"), folder.as_os_str()], &[]);
            assert_eq!(err(&normal), "", "{why}");

            let (folder, cwd) = project(GROUP, &format!("spoiled-{index}-{label}"), text);
            spoil(&cwd);
            let spoiled = run_in(&cwd, &[OsStr::new("check"), folder.as_os_str()], &[]);
            // Give the places back before judging, so that a failure does not leave them locked.
            common_unlock(&cwd);

            // Reports name the file from the project folder, so the two outputs are the same bytes.
            assert_eq!(spoiled.status.code(), normal.status.code(), "{why} ({label})");
            assert_eq!(spoiled.stdout, normal.stdout, "{why} ({label})");
            assert!(!spoiled.stdout.is_empty());
            assert_eq!(err(&spoiled), NOTICE, "{why} ({label})");
            assert_eq!(err(&spoiled).lines().count(), 1, "{why} ({label})");
        }
    }
}

/// Gives the working folder and its `logs` back their owner's permissions.
fn common_unlock(cwd: &Path) {
    let _ = fs::set_permissions(cwd, fs::Permissions::from_mode(0o755));
    let logs: PathBuf = cwd.join("logs");
    let _ = fs::set_permissions(&logs, fs::Permissions::from_mode(0o755));
    let _ = fs::set_permissions(logs.join("framesmith.log"), fs::Permissions::from_mode(0o644));
}

#[test]
fn the_output_of_a_check_is_the_same_bytes_with_and_without_a_log() {
    let base = scratch(GROUP, "same-bytes");
    fs::write(base.join("composition.xml"), INVALID).unwrap();
    let args = [OsStr::new("check"), OsStr::new(".")];
    let good = run_in(&base, &args, &[]);
    assert_eq!(err(&good), "");

    fs::remove_dir_all(base.join("logs")).unwrap();
    fs::write(base.join("logs"), "a file").unwrap();
    let spoiled = run_in(&base, &args, &[]);
    assert_eq!((spoiled.status.code(), &spoiled.stdout), (good.status.code(), &good.stdout));
    assert_eq!(err(&spoiled), NOTICE);
}

#[test]
fn a_command_line_that_cannot_be_read_gives_its_own_statement_and_then_the_one_line_about_the_log() {
    let base = scratch(GROUP, "unreadable-line");
    let healthy = run_in(&base, &[OsStr::new("check")], &[]);
    fs::remove_dir_all(base.join("logs")).unwrap();
    fs::write(base.join("logs"), "a file").unwrap();
    let spoiled = run_in(&base, &[OsStr::new("check")], &[]);
    assert_eq!(spoiled.status.code(), Some(1));
    assert_eq!(err(&spoiled), format!("{}{NOTICE}", err(&healthy)));
}

#[test]
fn a_log_that_is_not_asked_to_write_a_line_and_can_be_opened_prints_nothing() {
    let base = scratch(GROUP, "quiet");
    fs::write(base.join("composition.xml"), VALID).unwrap();
    let output = run_in(&base, &[OsStr::new("check"), OsStr::new(".")], &[("FRAMESMITH_LOG", "error")]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(err(&output), "");
}

/// A writer that takes `allowed` writes and then fails every write, and keeps what it took.
struct Failing {
    allowed: usize,
    taken: Arc<Mutex<Vec<u8>>>,
    calls: Arc<Mutex<usize>>,
}

impl Write for Failing {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        *self.calls.lock().unwrap() += 1;
        if self.allowed == 0 {
            return Err(io::Error::other("the disk is full"));
        }
        self.allowed -= 1;
        self.taken.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn a_write_that_fails_in_the_middle_of_a_run_is_told_once_and_the_work_goes_on() {
    let taken = Arc::new(Mutex::new(Vec::new()));
    let calls = Arc::new(Mutex::new(0));
    let writer = Failing {
        allowed: 2,
        taken: taken.clone(),
        calls: calls.clone(),
    };
    let log = Log::to_writer(Box::new(writer), LevelFilter::TRACE);
    let work_done = log.scope(|| {
        let mut done = 0;
        for step in 0..10 {
            tracing::info!(step, "a step");
            done += 1;
        }
        done
    });
    assert_eq!(work_done, 10, "the work does not stop");
    assert!(log.is_unavailable());
    assert_eq!(*calls.lock().unwrap(), 3, "after the first failure nothing more is written");
    assert_eq!(String::from_utf8(taken.lock().unwrap().clone()).unwrap().lines().count(), 2);

    let mut told = Vec::new();
    log.finish(&mut told);
    assert_eq!(String::from_utf8(told).unwrap(), NOTICE);
    assert_eq!(UNAVAILABLE, NOTICE.trim_end());
}

#[test]
fn a_log_that_wrote_every_line_says_nothing_at_the_end() {
    let taken = Arc::new(Mutex::new(Vec::new()));
    let calls = Arc::new(Mutex::new(0));
    let log = Log::to_writer(
        Box::new(Failing {
            allowed: 100,
            taken: taken.clone(),
            calls,
        }),
        LevelFilter::INFO,
    );
    log.scope(|| tracing::info!("one"));
    assert!(!log.is_unavailable());
    let mut told = Vec::new();
    log.finish(&mut told);
    assert!(told.is_empty());
    assert_eq!(String::from_utf8(taken.lock().unwrap().clone()).unwrap().lines().count(), 1);
}
