//! T006/T007 end-to-end: `check --json`, the `errors` catalog, usage
//! errors, and render repeating the check diagnostics.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-json-{}-{}-{}",
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
    run_env(args, cwd, &[])
}

fn run_env(args: &[&str], cwd: &Path, extra_env: &[(&str, &str)]) -> (i32, String) {
    let mut cmd = Command::new(bin());
    cmd.args(args).current_dir(cwd);
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("run framesmith binary");
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    (out.status.code().unwrap_or(-1), text)
}

fn write(folder: &Path, name: &str, content: &str) {
    std::fs::write(folder.join(name), content).unwrap();
}

const VALID: &str = "composition name=\"t\" width=640 height=480 duration=2 fps=30 background=\"#112233\" version=1 {\n    text value=\"Hi\" start=0 duration=2\n}\n";

const MULTI: &str = "composition name=\"t\" width=64 height=64 duration=4 bogus=1 background=\"#000\" version=1 {\n    text value=\"Hi\" duration=0\n    video path=\"media/gone.mp4\" start=0 duration=1\n}\n";

fn error_codes(text: &str) -> Vec<String> {
    // Human form: error[CODE]; JSON form: "code":"CODE".
    let mut codes = Vec::new();
    for line in text.lines() {
        let mut rest = line;
        while let Some(i) = rest.find("error[") {
            rest = &rest[i + 6..];
            if let Some(end) = rest.find(']') {
                codes.push(rest[..end].to_string());
                rest = &rest[end + 1..];
            } else {
                break;
            }
        }
    }
    let mut rest = text;
    while let Some(i) = rest.find("\"code\":\"") {
        rest = &rest[i + 8..];
        if let Some(end) = rest.find('"') {
            codes.push(rest[..end].to_string());
            rest = &rest[end + 1..];
        } else {
            break;
        }
    }
    codes.sort();
    codes
}

#[test]
fn json_valid_matches_human_valid() {
    let root = tmp("valid");
    std::fs::create_dir_all(root.join("ok")).unwrap();
    write(&root.join("ok"), "composition.kdl", VALID);
    let (code, human) = run(&["check", "ok"], &root);
    assert_eq!(code, 0, "{human}");
    let (code, json) = run(&["check", "--json", "ok"], &root);
    assert_eq!(code, 0, "{json}");
    assert!(json.contains("\"status\":\"valid\""), "{json}");
    assert!(json.contains("\"diagnostics\":[]"), "{json}");
    assert!(human.contains("ok:"), "{human}");
}

#[test]
fn json_reports_same_problems_as_human() {
    let root = tmp("multi");
    std::fs::create_dir_all(root.join("bad")).unwrap();
    write(&root.join("bad"), "composition.kdl", MULTI);
    let (code_h, human) = run(&["check", "bad"], &root);
    assert_eq!(code_h, 1, "{human}");
    let (code_j, json) = run(&["check", "--json", "bad"], &root);
    assert_eq!(code_j, 1, "{json}");
    let mut h = error_codes(&human);
    h.retain(|c| !c.is_empty());
    let mut j: Vec<String> = Vec::new();
    // JSON-only pass over the json output.
    let mut rest = json.as_str();
    while let Some(i) = rest.find("\"code\":\"") {
        rest = &rest[i + 8..];
        if let Some(end) = rest.find('"') {
            j.push(rest[..end].to_string());
            rest = &rest[end + 1..];
        } else {
            break;
        }
    }
    j.sort();
    assert!(h.contains(&"E_UNKNOWN_KEY".to_string()), "{human}");
    assert!(h.contains(&"E_BAD_TIMING".to_string()), "{human}");
    assert!(h.contains(&"E_MISSING_FILE".to_string()), "{human}");
    assert_eq!(h, j, "human {human} vs json {json}");
    // Every JSON item carries file, line and column.
    assert!(json.contains("\"file\":"), "{json}");
    assert!(json.contains("\"line\":"), "{json}");
    assert!(json.contains("\"column\":"), "{json}");
}

#[test]
fn json_unsupported_matches_human() {
    let root = tmp("unsup");
    std::fs::create_dir_all(root.join("later")).unwrap();
    write(
        &root.join("later"),
        "composition.kdl",
        "composition name=\"t\" width=320 height=240 duration=1 background=\"#000\" version=1 {\n    automation lane=\"volume\" start=0 duration=1\n}\n",
    );
    let (code, human) = run(&["check", "later"], &root);
    assert_eq!(code, 2, "{human}");
    let (code, json) = run(&["check", "--json", "later"], &root);
    assert_eq!(code, 2, "{json}");
    assert!(json.contains("\"status\":\"unsupported\""), "{json}");
    assert!(json.contains("E_UNSUPPORTED"), "{json}");
    assert!(json.contains("Sprint 10"), "{json}");
}

#[test]
fn unknown_flags_are_reported_with_usage() {
    let root = tmp("flags");
    assert_eq!(run(&["init", "p"], &root).0, 0);
    let (code, out) = run(&["check", "--bogus", "p"], &root);
    assert_eq!(code, 2, "{out}");
    assert!(out.contains("usage"), "{out}");
    let (code, out) = run(&["check", "--json"], &root);
    assert_eq!(code, 2, "{out}");
    assert!(out.contains("usage"), "{out}");
    let (code, out) = run(&["check", "p", "extra"], &root);
    assert_eq!(code, 2, "{out}");
    assert!(out.contains("usage"), "{out}");
    let (code, out) = run(&["check", "--json", "--json", "p"], &root);
    assert_eq!(code, 2, "{out}");
}

#[test]
fn errors_catalog_lists_every_reported_code() {
    let root = tmp("catalog");
    let (code, out) = run(&["errors"], &root);
    assert_eq!(code, 0, "{out}");
    for expected in [
        "E_PARSE",
        "E_UNKNOWN_KEY",
        "E_MISSING_FILE",
        "E_BAD_TIMING",
        "E_UNSUPPORTED",
        "E_NO_FFMPEG",
        "E_DOWNLOAD_FAILED",
    ] {
        assert!(out.contains(expected), "catalog is missing {expected}:\n{out}");
    }
    // Each entry carries meaning, example and fix.
    assert!(out.contains("meaning:"), "{out}");
    assert!(out.contains("example:"), "{out}");
    assert!(out.contains("fix:"), "{out}");

    let (code, out) = run(&["errors", "E_PARSE"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("E_PARSE"), "{out}");
    assert!(out.contains("meaning:"), "{out}");
    assert!(out.contains("fix:"), "{out}");

    let (code, out) = run(&["errors", "--json", "E_NO_FFMPEG"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("\"code\":\"E_NO_FFMPEG\""), "{out}");

    let (code, out) = run(&["errors", "E_BOGUS"], &root);
    assert_eq!(code, 2, "{out}");
    assert!(out.contains("unknown error code"), "{out}");

    let (code, out) = run(&["errors", "--bogus"], &root);
    assert_eq!(code, 2, "{out}");
    assert!(out.contains("usage"), "{out}");
}

#[test]
fn failed_render_repeats_check_diagnostics() {
    // T006: the actor sees the same diagnostics from both commands.
    let root = tmp("repeat");
    std::fs::create_dir_all(root.join("bad")).unwrap();
    write(&root.join("bad"), "composition.kdl", MULTI);
    let (_, check_out) = run(&["check", "bad"], &root);
    let (_, render_out) = run(&["render", "bad"], &root);
    let check_codes = error_codes(&check_out);
    assert!(!check_codes.is_empty());
    let render_codes = error_codes(&render_out);
    assert_eq!(check_codes, render_codes, "check vs render:\n{check_out}\n{render_out}");
}

#[test]
fn check_needs_no_ffmpeg_and_writes_nothing() {
    // T006/T010: validation works with FFmpeg absent and writes no MP4.
    // T015: the only write check may make is the append-only run log.
    let root = tmp("noffmpeg");
    std::fs::create_dir_all(root.join("ok")).unwrap();
    write(&root.join("ok"), "composition.kdl", VALID);
    let before = std::fs::read(root.join("ok").join("composition.kdl")).unwrap();
    let (code, out) = run_env(
        &["check", "ok"],
        &root,
        &[("PATH", "/nonexistent-framesmith-empty")],
    );
    assert_eq!(code, 0, "{out}");
    assert_eq!(
        std::fs::read(root.join("ok").join("composition.kdl")).unwrap(),
        before,
        "check must not touch the composition"
    );
    assert!(
        !root.join("ok").join("out.mp4").exists(),
        "check must not write an MP4"
    );
    for entry in std::fs::read_dir(root.join("ok")).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        assert!(
            name == "composition.kdl" || name == "logs",
            "check wrote an unexpected entry: {name}"
        );
    }
}
