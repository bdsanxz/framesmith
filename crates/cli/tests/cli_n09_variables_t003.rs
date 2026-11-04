//! Sprint 9 T003 end-to-end: `--variables` on `check` and `render`
//! through the built binary — override scoping, visible warnings,
//! shape refusals, overridden validation, and byte-identical reruns.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-n09t003-{}-{}-{}",
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

const PROJ: &str = "composition name=\"t\" width=64 height=64 duration=1 background=\"#000\" version=1 {\nvariable id=\"title\" type=\"string\" default=\"News\"\nvariable id=\"sub\" type=\"string\" default=\"keep\"\ntext value=\"var(--title) | var(--sub)\" start=0 duration=1\n}\n";

#[test]
fn check_accepts_variables_and_stays_green_on_defaults() {
    let root = tmp("check-ok");
    std::fs::create_dir_all(root.join("proj")).unwrap();
    write(&root.join("proj"), "composition.kdl", PROJ);
    let (code, out) = run(
        &["check", "--variables", r##"{"title": "Sale"}"##, "proj"],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("ok:"), "{out}");
}

#[test]
fn check_warns_visibly_on_unknown_keys_without_blocking() {
    let root = tmp("check-warn");
    std::fs::create_dir_all(root.join("proj")).unwrap();
    write(&root.join("proj"), "composition.kdl", PROJ);
    let (code, out) = run(
        &["check", "--variables", r##"{"ghost": "x"}"##, "proj"],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("warning"), "{out}");
    assert!(out.contains("ghost"), "{out}");
    assert!(out.contains("ok:"), "{out}");

    let (code, json) = run(
        &[
            "check",
            "--json",
            "--variables",
            r##"{"ghost": "x"}"##,
            "proj",
        ],
        &root,
    );
    assert_eq!(code, 0, "{json}");
    assert!(json.contains("\"variable_warnings\""), "{json}");
    assert!(json.contains("\"ghost\""), "{json}");
    assert!(json.contains("\"status\":\"valid\""), "{json}");
}

#[test]
fn check_refuses_misshapen_payloads_naming_the_flag() {
    let root = tmp("check-shape");
    std::fs::create_dir_all(root.join("proj")).unwrap();
    write(&root.join("proj"), "composition.kdl", PROJ);
    for payload in ["[1, 2]", r##"{"title": {"a": 1}}"##, "{bad json"] {
        let (code, out) = run(&["check", "--variables", payload, "proj"], &root);
        assert_eq!(code, 1, "{payload}: {out}");
        assert!(out.contains("--variables"), "{payload}: {out}");
    }
}

#[test]
fn check_validates_the_overridden_composition() {
    let root = tmp("check-typed");
    std::fs::create_dir_all(root.join("proj")).unwrap();
    write(&root.join("proj"), "composition.kdl", PROJ);
    // A mistyped override for a bound variable refuses naming it.
    let bad = "composition name=\"t\" width=64 height=64 duration=1 background=\"#000\" version=1 {\nvariable id=\"n\" type=\"number\" default=1\ntext value=\"Hi\" start=0 duration=1 x=\"var(--n)\"\n}\n";
    write(&root.join("proj"), "composition.kdl", bad);
    let (code, out) = run(
        &["check", "--variables", r##"{"n": "abc"}"##, "proj"],
        &root,
    );
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("--n"), "{out}");
    assert!(out.contains("--variables"), "{out}");
    // The same override with a good value stays green.
    let (code, out) = run(&["check", "--variables", r##"{"n": 2.5}"##, "proj"], &root);
    assert_eq!(code, 0, "{out}");
}

#[test]
fn render_overrides_are_per_render_deterministic_and_disk_clean() {
    let root = tmp("render-det");
    std::fs::create_dir_all(root.join("proj")).unwrap();
    write(&root.join("proj"), "composition.kdl", PROJ);
    let before = std::fs::read(root.join("proj").join("composition.kdl")).unwrap();

    let (code, out) = run(
        &[
            "render",
            "proj",
            "-o",
            "a.mp4",
            "--variables",
            r##"{"title": "Sale"}"##,
        ],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(
        &[
            "render",
            "proj",
            "-o",
            "b.mp4",
            "--variables",
            r##"{"title": "Sale"}"##,
        ],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(
        &[
            "render",
            "proj",
            "-o",
            "c.mp4",
            "--variables",
            r##"{"title": "Other"}"##,
        ],
        &root,
    );
    assert_eq!(code, 0, "{out}");

    let a = std::fs::read(root.join("a.mp4")).unwrap();
    let b = std::fs::read(root.join("b.mp4")).unwrap();
    let c = std::fs::read(root.join("c.mp4")).unwrap();
    assert!(!a.is_empty() && !c.is_empty());
    assert_eq!(a, b, "same overrides must re-render byte-identically");
    assert_ne!(a, c, "different overrides must render different outputs");

    let after = std::fs::read(root.join("proj").join("composition.kdl")).unwrap();
    assert_eq!(
        before, after,
        "overridden renders must leave the project on disk unchanged"
    );
}

#[test]
fn render_warns_on_unknown_keys_and_refuses_misshapen_payloads() {
    let root = tmp("render-warn-shape");
    std::fs::create_dir_all(root.join("proj")).unwrap();
    write(&root.join("proj"), "composition.kdl", PROJ);

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
    assert!(out.contains("ghost"), "{out}");
    assert!(root.join("warn.mp4").exists());

    let (code, out) = run(
        &["render", "proj", "-o", "bad.mp4", "--variables", "[1]"],
        &root,
    );
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("--variables"), "{out}");
    assert!(!root.join("bad.mp4").exists());
}

#[test]
fn render_supplies_default_less_variables_from_overrides() {
    let root = tmp("render-late");
    std::fs::create_dir_all(root.join("proj")).unwrap();
    write(
        &root.join("proj"),
        "composition.kdl",
        "composition name=\"t\" width=64 height=64 duration=1 background=\"#000\" version=1 {\nvariable id=\"late\" type=\"string\"\ntext value=\"Hi var(--late)\" start=0 duration=1\n}\n",
    );
    // Without the value the render refuses naming the variable.
    let (code, out) = run(&["render", "proj", "-o", "nolate.mp4"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("--late"), "{out}");
    // With the override it renders.
    let (code, out) = run(
        &[
            "render",
            "proj",
            "-o",
            "late.mp4",
            "--variables",
            r##"{"late": "there"}"##,
        ],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    assert!(root.join("late.mp4").exists());
}
