//! T015 end-to-end: every validation and render run appends its notable
//! events to `logs/framesmith.log`, each line carrying a timestamp,
//! level, component, and run id. An unwritable log location is stated on
//! the terminal while the run still follows its own verdict.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-log-{}-{}-{}",
        tag,
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_framesmith"))
}

fn run(args: &[&str], cwd: &Path) -> (i32, String) {
    let out = Command::new(bin())
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("run framesmith binary");
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    (out.status.code().unwrap_or(-1), text)
}

/// Stdout only (stderr excluded) so `--json` purity can be asserted.
fn run_stdout(args: &[&str], cwd: &Path) -> (i32, String, String) {
    let out = Command::new(bin())
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("run framesmith binary");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn write(folder: &Path, name: &str, content: &str) {
    std::fs::write(folder.join(name), content).unwrap();
}

const VALID: &str = "composition name=\"t\" width=64 height=48 duration=1 fps=30 background=\"#112233\" version=1 {\n    text value=\"Hi\" start=0 duration=1\n}\n";

fn scaffold(root: &Path, name: &str) -> PathBuf {
    let folder = root.join(name);
    std::fs::create_dir_all(&folder).unwrap();
    write(&folder, "composition.kdl", VALID);
    folder
}

fn log_text(folder: &Path) -> String {
    std::fs::read_to_string(folder.join("logs").join("framesmith.log"))
        .expect("run log must exist")
}

fn assert_four_fields(log: &str) {
    assert!(!log.is_empty(), "run log must gain entries");
    for line in log.lines() {
        for field in ["ts=", "level=", "component=", "run="] {
            assert!(line.contains(field), "line lacks {field}: {line}");
        }
    }
}

#[test]
fn check_appends_four_field_lines() {
    let root = tmp("check");
    let folder = scaffold(&root, "proj");
    let (code, _) = run(&["check", "proj"], &root);
    assert_eq!(code, 0);
    let log = log_text(&folder);
    assert_four_fields(&log);
    assert!(log.contains("check start"), "{log}");
    assert!(log.contains("check valid"), "{log}");
}

#[test]
fn check_json_keeps_stdout_parseable_and_still_logs() {
    let root = tmp("json");
    let folder = scaffold(&root, "proj");
    let (code, stdout, _) = run_stdout(&["check", "--json", "proj"], &root);
    assert_eq!(code, 0);
    // Stdout stays one machine-readable line for agents.
    let first = stdout.lines().next().unwrap_or("");
    assert!(first.starts_with("{\"tool\":\"framesmith\""), "{stdout}");
    assert!(first.contains("\"status\":\"valid\""), "{stdout}");
    assert_four_fields(&log_text(&folder));
}

#[test]
fn invalid_check_and_render_refusal_still_log() {
    let root = tmp("invalid");
    let folder = scaffold(&root, "bad");
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=0 height=64 duration=1 background=\"#000\" version=1 {\n}\n",
    );
    let (code, _) = run(&["check", "bad"], &root);
    assert_eq!(code, 1);
    assert_four_fields(&log_text(&folder));

    let (code, out) = run(&["render", "bad"], &root);
    assert_eq!(code, 1, "{out}");
    let log = log_text(&folder);
    assert_four_fields(&log);
    assert!(log.contains("render validation: invalid"), "{log}");
    assert!(log.contains("render refused"), "{log}");
    assert!(!folder.join("out.mp4").exists());
}

#[test]
fn unsupported_check_and_render_refusal_still_log() {
    let root = tmp("unsupported");
    let folder = scaffold(&root, "later");
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=64 height=64 duration=1 background=\"#000\" version=1 {\n    automation lane=\"volume\" start=0 duration=1\n}\n",
    );
    let (code, _) = run(&["check", "later"], &root);
    assert_eq!(code, 2);
    let (code, out) = run(&["render", "later"], &root);
    assert_eq!(code, 2, "{out}");
    let log = log_text(&folder);
    assert_four_fields(&log);
    assert!(log.contains("render refused before the first frame"), "{log}");
    assert!(!folder.join("out.mp4").exists());
}

#[test]
fn unwritable_check_log_states_gap_while_verdict_stands() {
    let root = tmp("checknowrite");
    let folder = scaffold(&root, "proj");
    // A regular file where the logs/ directory should be.
    write(&folder, "logs", "blocker");
    let (code, out) = run(&["check", "proj"], &root);
    assert_eq!(code, 0, "verdict stands without a log: {out}");
    assert!(out.contains("run log unavailable"), "{out}");
    assert!(out.contains("logs"), "{out}");
    assert!(out.contains("ok:"), "verdict still reported: {out}");
}

#[test]
fn unwritable_render_log_states_gap_while_verdict_stands() {
    let root = tmp("rendernowrite");
    let folder = scaffold(&root, "proj");
    write(&folder, "logs", "blocker");
    let (code, out) = run(&["render", "proj"], &root);
    assert_eq!(code, 0, "verdict stands without a log: {out}");
    assert!(out.contains("run log unavailable"), "{out}");
    assert!(folder.join("out.mp4").is_file(), "MP4 still written: {out}");
}

#[test]
fn missing_folder_states_missing_log_without_creating_it() {
    let root = tmp("ghost");
    let (code, out) = run(&["check", "ghost"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("run log unavailable"), "{out}");
    assert!(
        !root.join("ghost").exists(),
        "a log attempt must not create the queried folder"
    );
}

#[test]
fn each_run_logs_under_its_own_run_id() {
    let root = tmp("runids");
    let folder = scaffold(&root, "proj");
    assert_eq!(run(&["check", "proj"], &root).0, 0);
    assert_eq!(run(&["render", "proj"], &root).0, 0);
    let log = log_text(&folder);
    assert_four_fields(&log);
    let mut runs = std::collections::BTreeSet::new();
    for line in log.lines() {
        let pos = line.find("run=").unwrap() + 4;
        let end = line[pos..].find(' ').map(|i| pos + i).unwrap_or(line.len());
        runs.insert(line[pos..end].to_string());
    }
    assert!(runs.len() >= 2, "check and render need distinct run ids: {log}");
    assert!(log.contains("component=check"), "{log}");
    assert!(log.contains("component=bootstrap"), "{log}");
    assert!(log.contains("component=render"), "{log}");
}
