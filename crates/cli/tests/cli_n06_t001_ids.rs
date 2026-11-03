//! Sprint 6 T001 end-to-end: ids through `check` (human and `--json`).
//!
//! Valid projects pass, and every id problem reports with file, line,
//! column, code, and fix — without rendering.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-n06t001-{}-{}-{}",
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

fn write(folder: &Path, name: &str, content: &str) {
    std::fs::write(folder.join(name), content).unwrap();
}

const OPEN: &str =
    "composition name=\"main\" width=64 height=64 duration=2 background=\"#000\" version=1 {\n";

#[test]
fn valid_ids_pass_check_in_both_forms() {
    let root = tmp("valid");
    std::fs::create_dir_all(root.join("p")).unwrap();
    write(
        &root.join("p"),
        "composition.kdl",
        &format!(
            "composition name=\"main\" root=true width=64 height=64 duration=2 background=\"#000\" version=1 {{\n\
             text value=\"A\" start=0 duration=1 id=\"a\"\n\
             text value=\"B\" duration=1 id=\"b\" start-after=\"a\" label=\"follows A\"\n\
             nest nest=\"bumper\" start=0 duration=1 id=\"n1\" track=\"graphics\"\n\
             }}\n\
             composition name=\"bumper\" width=64 height=64 duration=2 background=\"#000\" version=1 {{\n\
             box x=0 y=0 width=64 height=64 fill=\"#0000ff\" start=0 duration=2\n\
             }}\n"
        ),
    );
    let (code, out) = run(&["check", "p"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("ok:"), "{out}");
    let (code, out) = run(&["check", "--json", "p"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("\"status\":\"valid\""), "{out}");
}

#[test]
fn duplicate_id_fails_naming_both_lines() {
    let root = tmp("dup");
    std::fs::create_dir_all(root.join("p")).unwrap();
    write(
        &root.join("p"),
        "composition.kdl",
        &format!(
            "{OPEN}\
             text value=\"A\" start=0 duration=1 id=\"dup\"\n\
             text value=\"B\" start=1 duration=1 id=\"dup\"\n\
             }}\n"
        ),
    );
    let (code, out) = run(&["check", "p"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_DUP_ID"), "{out}");
    // Both declaration lines are named in the single diagnostic.
    assert!(out.contains("2, 3"), "{out}");
    assert!(out.contains("fix:"), "{out}");
    let (code, json) = run(&["check", "--json", "p"], &root);
    assert_eq!(code, 1, "{json}");
    assert!(json.contains("\"code\":\"E_DUP_ID\""), "{json}");
    // Render repeats the refusal and writes no MP4.
    let (code, out) = run(&["render", "p"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(!root.join("p").join("out.mp4").exists());
}

#[test]
fn unknown_reference_fails_naming_line_and_id() {
    let root = tmp("unknown");
    std::fs::create_dir_all(root.join("p")).unwrap();
    write(
        &root.join("p"),
        "composition.kdl",
        &format!("{OPEN}text value=\"B\" duration=1 start-after=\"ghost\"\n}}\n"),
    );
    let (code, out) = run(&["check", "p"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_UNKNOWN_REF"), "{out}");
    assert!(out.contains("ghost"), "{out}");
    assert!(out.contains(":2:"), "{out}");
    let (code, json) = run(&["check", "--json", "p"], &root);
    assert_eq!(code, 1, "{json}");
    assert!(json.contains("\"code\":\"E_UNKNOWN_REF\""), "{json}");
}

#[test]
fn several_id_problems_report_together() {
    let root = tmp("together");
    std::fs::create_dir_all(root.join("p")).unwrap();
    write(
        &root.join("p"),
        "composition.kdl",
        &format!(
            "{OPEN}\
             text value=\"A\" start=0 duration=1 id=\"dup\"\n\
             text value=\"B\" start=1 duration=1 id=\"dup\"\n\
             text value=\"C\" duration=1 start-after=\"ghost\"\n\
             }}\n"
        ),
    );
    let (code, out) = run(&["check", "p"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_DUP_ID"), "{out}");
    assert!(out.contains("E_UNKNOWN_REF"), "{out}");
    assert!(out.contains("2 problem(s)"), "{out}");
}

#[test]
fn projects_without_ids_validate_exactly_as_before() {
    let root = tmp("plain");
    std::fs::create_dir_all(root.join("p")).unwrap();
    write(
        &root.join("p"),
        "composition.kdl",
        &format!("{OPEN}text value=\"Hi\" start=0 duration=2\n}}\n"),
    );
    let (code, out) = run(&["check", "p"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("ok:"), "{out}");
    let (code, out) = run(&["render", "p"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(root.join("p").join("out.mp4").is_file());
}
