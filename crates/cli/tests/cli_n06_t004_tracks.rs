//! Sprint 6 T004 end-to-end: tracks and labels through `check` and `render`.
//!
//! Declared tracks validate, unknown names fail singly, and track or
//! label declarations change no rendered pixel or sample.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-n06t004-{}-{}-{}",
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
    "composition name=\"t\" width=96 height=64 duration=1 fps=30 background=\"#000000\" version=1 {\n";

#[test]
fn declared_tracks_and_labels_pass_check() {
    let root = tmp("valid");
    std::fs::create_dir_all(root.join("p")).unwrap();
    write(
        &root.join("p"),
        "composition.kdl",
        &format!(
            "{OPEN}\
             text value=\"Hi\" start=0 duration=1 track=\"caption\" label=\"Editor note\"\n\
             text value=\"Yo\" start=0 duration=1 track=\"graphics\"\n\
             }}\n"
        ),
    );
    let (code, out) = run(&["check", "p"], &root);
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(&["check", "--json", "p"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("\"status\":\"valid\""), "{out}");
}

#[test]
fn unknown_track_fails_naming_accepted_set() {
    let root = tmp("unknown");
    std::fs::create_dir_all(root.join("p")).unwrap();
    write(
        &root.join("p"),
        "composition.kdl",
        &format!("{OPEN}text value=\"Hi\" start=0 duration=1 track=\"titles\"\n}}\n"),
    );
    let (code, out) = run(&["check", "p"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_BAD_TRACK"), "{out}");
    for name in ["video", "audio", "caption", "graphics"] {
        assert!(out.contains(name), "{out}");
    }
    assert!(out.contains("1 problem(s)"), "{out}");
    let (code, json) = run(&["check", "--json", "p"], &root);
    assert_eq!(code, 1, "{json}");
    assert!(json.contains("\"code\":\"E_BAD_TRACK\""), "{json}");
}

#[test]
fn tracks_and_labels_change_no_rendered_byte() {
    let root = tmp("bytes");
    for proj in ["plain", "tagged"] {
        std::fs::create_dir_all(root.join(proj)).unwrap();
    }
    write(
        &root.join("plain"),
        "composition.kdl",
        &format!(
            "{OPEN}text value=\"Hi\" start=0 duration=1 x=4 y=4 size=16 color=\"#ffffff\"\n}}\n"
        ),
    );
    write(
        &root.join("tagged"),
        "composition.kdl",
        &format!(
            "{OPEN}text value=\"Hi\" start=0 duration=1 x=4 y=4 size=16 color=\"#ffffff\" id=\"hi\" track=\"caption\" label=\"Editor note\"\n}}\n"
        ),
    );
    for proj in ["plain", "tagged"] {
        let (code, out) = run(&["check", proj], &root);
        assert_eq!(code, 0, "{out}");
        let (code, out) = run(&["render", proj], &root);
        assert_eq!(code, 0, "{out}");
    }
    let a = std::fs::read(root.join("plain").join("out.mp4")).unwrap();
    let b = std::fs::read(root.join("tagged").join("out.mp4")).unwrap();
    assert!(!a.is_empty() && !b.is_empty());
    assert_eq!(
        a, b,
        "track/label declarations must not change a rendered byte"
    );
}
