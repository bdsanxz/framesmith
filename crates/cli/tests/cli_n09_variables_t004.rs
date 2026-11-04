//! Sprint 9 T004 end-to-end: `--strict-variables` on `check` and
//! `render` through the built binary — default-mode warnings that never
//! block, strict-mode refusals naming the key or variable and the fix,
//! and a strict run with everything supplied rendering clean.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-n09t004-{}-{}-{}",
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

// Both declarations bound: strict-clean by default.
const CLEAN: &str = "composition name=\"t\" width=64 height=64 duration=1 background=\"#000\" version=1 {\nvariable id=\"title\" type=\"string\" default=\"News\"\nvariable id=\"sub\" type=\"string\" default=\"keep\"\ntext value=\"var(--title) | var(--sub)\" start=0 duration=1\n}\n";

// `stale` is declared but never bound.
const STALE: &str = "composition name=\"t\" width=64 height=64 duration=1 background=\"#000\" version=1 {\nvariable id=\"title\" type=\"string\" default=\"News\"\nvariable id=\"stale\" type=\"string\" default=\"old\"\ntext value=\"var(--title)\" start=0 duration=1\n}\n";

// `late` is default-less, bound only through an inline fallback.
const LATE: &str = "composition name=\"t\" width=64 height=64 duration=1 background=\"#000\" version=1 {\nvariable id=\"late\" type=\"string\"\ntext value=\"Hi var(--late, friend)\" start=0 duration=1\n}\n";

fn proj(root: &Path, body: &str) {
    std::fs::create_dir_all(root.join("proj")).unwrap();
    write(&root.join("proj"), "composition.kdl", body);
}

#[test]
fn check_default_warns_on_stale_key_and_unused_declaration_without_blocking() {
    let root = tmp("check-default");
    proj(&root, STALE);
    let (code, out) = run(
        &["check", "--variables", r##"{"ghost": "x"}"##, "proj"],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("warning"), "{out}");
    assert!(out.contains("--ghost"), "{out}");
    assert!(out.contains("--stale"), "{out}");
    assert!(out.contains("ok:"), "{out}");
}

#[test]
fn check_strict_fails_unknown_key_naming_key_and_fix() {
    let root = tmp("check-strict-key");
    proj(&root, CLEAN);
    let (code, out) = run(
        &[
            "check",
            "--strict-variables",
            "--variables",
            r##"{"ghost": "x"}"##,
            "proj",
        ],
        &root,
    );
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("--ghost"), "{out}");
    assert!(out.contains("remove the key"), "{out}");
    assert!(out.contains("--strict-variables"), "{out}");
}

#[test]
fn check_strict_fails_unused_declaration_naming_variable_and_fix() {
    let root = tmp("check-strict-unused");
    proj(&root, STALE);
    let (code, out) = run(&["check", "--strict-variables", "proj"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("--stale"), "{out}");
    assert!(out.contains("bind the variable"), "{out}");
    assert!(!out.contains("warning:"), "{out}");
}

#[test]
fn check_strict_fails_default_less_variable_without_value() {
    let root = tmp("check-strict-late");
    proj(&root, LATE);
    let (code, out) = run(&["check", "--strict-variables", "proj"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("--late"), "{out}");
    assert!(out.contains("supply the value"), "{out}");
    // The same project stays green in default mode (fallback resolves).
    let (code, out) = run(&["check", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("ok:"), "{out}");
}

#[test]
fn check_strict_json_reports_invalid_with_named_diagnostic() {
    let root = tmp("check-strict-json");
    proj(&root, STALE);
    let (code, json) = run(&["check", "--json", "--strict-variables", "proj"], &root);
    assert_eq!(code, 1, "{json}");
    assert!(json.contains("\"status\":\"invalid\""), "{json}");
    assert!(json.contains("--stale"), "{json}");
}

#[test]
fn check_strict_clean_project_validates_without_warnings() {
    let root = tmp("check-strict-clean");
    proj(&root, CLEAN);
    let (code, out) = run(
        &[
            "check",
            "--strict-variables",
            "--variables",
            r##"{"title": "Sale"}"##,
            "proj",
        ],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    assert!(!out.contains("warning:"), "{out}");
    assert!(out.contains("ok:"), "{out}");
}

#[test]
fn render_strict_clean_renders_and_strict_stale_refuses() {
    let root = tmp("render-strict");
    proj(&root, CLEAN);

    let (code, out) = run(
        &[
            "render",
            "proj",
            "-o",
            "clean.mp4",
            "--strict-variables",
            "--variables",
            r##"{"title": "Sale"}"##,
        ],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    assert!(!out.contains("warning:"), "{out}");
    assert!(root.join("clean.mp4").exists());

    // The same render with a stale key refuses before frame one.
    let (code, out) = run(
        &[
            "render",
            "proj",
            "-o",
            "stale.mp4",
            "--strict-variables",
            "--variables",
            r##"{"ghost": "x"}"##,
        ],
        &root,
    );
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("--ghost"), "{out}");
    assert!(!root.join("stale.mp4").exists());

    // And an unused declaration refuses even when every key is known.
    write(&root.join("proj"), "composition.kdl", STALE);
    let (code, out) = run(
        &["render", "proj", "-o", "unused.mp4", "--strict-variables"],
        &root,
    );
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("--stale"), "{out}");
    assert!(!root.join("unused.mp4").exists());
}

#[test]
fn render_default_warns_on_unused_declaration_and_still_renders() {
    let root = tmp("render-default");
    proj(&root, STALE);
    let (code, out) = run(
        &[
            "render",
            "proj",
            "-o",
            "warn.mp4",
            "--variables",
            r##"{"ghost": "x"}"##,
        ],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("warning"), "{out}");
    assert!(out.contains("--stale"), "{out}");
    assert!(root.join("warn.mp4").exists());
}
