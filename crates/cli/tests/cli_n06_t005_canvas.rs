//! Sprint 6 T005 end-to-end: canvas presets through `check` and `render`.
//!
//! Each preset (plus alias) renders at its documented dimensions,
//! explicit dimensions override the preset byte-identically, unknown
//! presets fail, and preset-less projects render exactly as before.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-n06t005-{}-{}-{}",
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

fn ffprobe_dims(mp4: &Path) -> (u32, u32) {
    let out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height",
            "-of",
            "csv=p=0",
        ])
        .arg(mp4)
        .output()
        .expect("run host ffprobe");
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let (w, h) = text.split_once(',').expect("width,height");
    (w.parse().unwrap(), h.parse().unwrap())
}

fn comp(canvas: &str, extra: &str, fps: u32) -> String {
    format!(
        "composition name=\"c\" {canvas} {extra} duration=1 fps={fps} background=\"#112233\" version=1 {{\n\
         text value=\"Hi\" start=0 duration=1 x=10 y=20 size=24 color=\"#ffffff\"\n\
         }}\n"
    )
}

#[test]
fn each_preset_renders_at_documented_dimensions() {
    // fps=1 keeps the 4k/uhd renders to a single frame; small presets
    // render one frame too, so every case measures canvas size only.
    let cases = [
        ("canvas=\"1080p\"", 1920, 1080),
        ("canvas=\"fullhd\"", 1920, 1080),
        ("canvas=\"720p\"", 1280, 720),
        ("canvas=\"square\"", 1080, 1080),
        ("canvas=\"portrait\"", 1080, 1920),
        ("canvas=\"4k\"", 3840, 2160),
        ("canvas=\"uhd\"", 3840, 2160),
    ];
    for (i, (canvas, w, h)) in cases.iter().enumerate() {
        let root = tmp(&format!("preset-{i}"));
        let proj = format!("p{i}");
        std::fs::create_dir_all(root.join(&proj)).unwrap();
        write(&root.join(&proj), "composition.kdl", &comp(canvas, "", 1));
        let (code, out) = run(&["check", &proj], &root);
        assert_eq!(code, 0, "{canvas}: {out}");
        let (code, out) = run(&["render", &proj], &root);
        assert_eq!(code, 0, "{canvas}: {out}");
        let mp4 = root.join(&proj).join("out.mp4");
        assert!(mp4.is_file(), "{canvas}: {out}");
        assert_eq!(ffprobe_dims(&mp4), (*w, *h), "{canvas}");
    }
}

#[test]
fn explicit_dims_override_the_preset_byte_identically() {
    let root = tmp("override");
    for proj in ["sized", "preset"] {
        std::fs::create_dir_all(root.join(proj)).unwrap();
    }
    write(
        &root.join("sized"),
        "composition.kdl",
        &comp("", "width=1280 height=720", 30),
    );
    write(
        &root.join("preset"),
        "composition.kdl",
        &comp("canvas=\"1080p\"", "width=1280 height=720", 30),
    );
    for proj in ["sized", "preset"] {
        let (code, out) = run(&["check", proj], &root);
        assert_eq!(code, 0, "{out}");
        let (code, out) = run(&["render", proj], &root);
        assert_eq!(code, 0, "{out}");
    }
    let a = std::fs::read(root.join("sized").join("out.mp4")).unwrap();
    let b = std::fs::read(root.join("preset").join("out.mp4")).unwrap();
    assert!(!a.is_empty() && !b.is_empty());
    assert_eq!(a, b, "explicit dims over a preset must match explicit dims alone");

    // A partial override resizes only that dimension.
    std::fs::create_dir_all(root.join("partial")).unwrap();
    write(
        &root.join("partial"),
        "composition.kdl",
        &comp("canvas=\"square\"", "width=640", 1),
    );
    let (code, out) = run(&["render", "partial"], &root);
    assert_eq!(code, 0, "{out}");
    assert_eq!(
        ffprobe_dims(&root.join("partial").join("out.mp4")),
        (640, 1080)
    );
}

#[test]
fn unknown_preset_fails_listing_accepted_names() {
    let root = tmp("unknown");
    std::fs::create_dir_all(root.join("p")).unwrap();
    write(
        &root.join("p"),
        "composition.kdl",
        &comp("canvas=\"wide\"", "width=64 height=64", 30),
    );
    let (code, out) = run(&["check", "p"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_BAD_CANVAS"), "{out}");
    for name in ["1080p", "fullhd", "720p", "square", "portrait", "4k", "uhd"] {
        assert!(out.contains(name), "{out}");
    }
    let (code, out) = run(&["render", "p"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(!root.join("p").join("out.mp4").exists());
}

#[test]
fn single_composition_projects_render_byte_identically() {
    // Regression guard for the preservation boundary: one composition
    // with explicit dimensions renders deterministically, and a second
    // render of the same folder matches the first byte for byte.
    let root = tmp("regression");
    std::fs::create_dir_all(root.join("p")).unwrap();
    write(
        &root.join("p"),
        "composition.kdl",
        "composition name=\"hello\" width=160 height=120 duration=1 fps=30 background=\"#1a1a2e\" version=1 {\n\
         text value=\"Hello\" start=0 duration=1 x=10 y=20 size=24 color=\"#ffffff\"\n\
         }\n",
    );
    let (code, out) = run(&["check", "p"], &root);
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(&["render", "p", "-o", "first.mp4"], &root);
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(&["render", "p", "-o", "second.mp4"], &root);
    assert_eq!(code, 0, "{out}");
    let a = std::fs::read(root.join("first.mp4")).unwrap();
    let b = std::fs::read(root.join("second.mp4")).unwrap();
    assert!(!a.is_empty() && !b.is_empty());
    assert_eq!(a, b, "re-renders must be byte-identical");
}
