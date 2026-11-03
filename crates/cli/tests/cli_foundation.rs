//! Foundation end-to-end: init → check → render through the real binary.
//! Covers T001 (scaffold renders), T002 (refusals), T003/T004/T005
//! (verdicts) and T008 (FFmpeg finding is reported by render).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-{}-{}-{}",
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

#[test]
fn init_check_render_golden_path() {
    let root = tmp("golden");
    let (code, out) = run(&["init", "hello"], &root);
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(&["check", "hello"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("ok:") || out.contains("valid"), "{out}");
    let (code, out) = run(&["render", "hello"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains("ffmpeg:"),
        "render must report the FFmpeg finding: {out}"
    );
    let mp4 = root.join("hello").join("out.mp4");
    assert!(mp4.is_file(), "render must write an MP4: {out}");
    assert!(std::fs::metadata(&mp4).unwrap().len() > 0);
}

#[test]
fn render_writes_custom_output() {
    let root = tmp("custom");
    assert_eq!(run(&["init", "p"], &root).0, 0);
    let target = root.join("movie.mp4");
    let target_arg = target.to_string_lossy().into_owned();
    let (code, out) = run(&["render", "p", "-o", &target_arg], &root);
    assert_eq!(code, 0, "{out}");
    assert!(target.is_file());
    assert!(std::fs::metadata(&target).unwrap().len() > 0);
}

#[test]
fn init_refusal_leaves_existing_content() {
    let root = tmp("refuse");
    let target = root.join("proj");
    std::fs::create_dir_all(&target).unwrap();
    write(&target, "notes.txt", "do not touch");
    let before = std::fs::read(target.join("notes.txt")).unwrap();
    let (code, out) = run(&["init", "proj"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("not empty"), "{out}");
    assert_eq!(std::fs::read(target.join("notes.txt")).unwrap(), before);
}

#[test]
fn invalid_folder_fails_check_and_render() {
    let root = tmp("invalid");
    std::fs::create_dir_all(root.join("bad")).unwrap();
    write(
        &root.join("bad"),
        "composition.kdl",
        "composition name=\"t\" width=0 height=64 duration=1 background=\"#000\" version=1 {\n}\n",
    );
    let (code, out) = run(&["check", "bad"], &root);
    assert_eq!(code, 1, "{out}");
    let (code, out) = run(&["render", "bad", "-o", "x.mp4"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(!root.join("bad").join("x.mp4").exists());
}

#[test]
fn unsupported_folder_exits_2_and_writes_no_mp4() {
    let root = tmp("unsupported");
    std::fs::create_dir_all(root.join("later")).unwrap();
    write(
        &root.join("later"),
        "composition.kdl",
        "composition name=\"t\" width=320 height=240 duration=1 background=\"#000\" version=1 {\n    automation lane=\"volume\" start=0 duration=1\n}\n",
    );
    let (code, out) = run(&["check", "later"], &root);
    assert_eq!(code, 2, "{out}");
    assert!(out.contains("Sprint 10"), "{out}");
    let (code, _) = run(&["render", "later"], &root);
    assert_eq!(code, 2);
    assert!(!root.join("later").join("out.mp4").exists());
}

#[test]
fn missing_folder_is_a_located_diagnostic() {
    let root = tmp("missing");
    let (code, out) = run(&["check", "ghost"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_MISSING_DIR"), "{out}");
}
