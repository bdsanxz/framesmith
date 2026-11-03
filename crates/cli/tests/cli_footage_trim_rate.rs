//! Sprint 5 T001 (declare trim and rate for a video clip).
//!
//! A `video` clip with valid `trim-start`, `trim-end`, and `rate`
//! passes `check` with the effective source window and effective
//! rendered length echoed in human and `--json` form. Backward,
//! empty, and past-end windows plus out-of-range rates fail `check`
//! (exit 1) with located `E_BAD_FOOTAGE` diagnostics, while a
//! past-end `trim-end` clamps and is reported with a green verdict.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-footage-{tag}-{}-{}",
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

fn ffmpeg(args: &[&str]) {
    let out = Command::new("ffmpeg")
        .args(["-y", "-v", "error"])
        .args(args)
        .output()
        .expect("run host ffmpeg");
    assert!(out.status.success(), "host ffmpeg failed: {args:?}");
}

fn gen_clip(dir: &Path, name: &str, seconds: f64) {
    ffmpeg(&[
        "-f",
        "lavfi",
        "-i",
        &format!("testsrc=duration={seconds}:size=64x48:rate=30"),
        "-c:v",
        "libx264",
        "-pix_fmt",
        "yuv420p",
        &dir.join(name).display().to_string(),
    ]);
}

fn probe_duration(path: &Path) -> f64 {
    let out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
        ])
        .arg(path)
        .output()
        .expect("run host ffprobe");
    assert!(out.status.success());
    String::from_utf8_lossy(&out.stdout).trim().parse().expect("duration parses")
}

fn write(folder: &Path, name: &str, content: &str) {
    std::fs::write(folder.join(name), content).unwrap();
}

fn comp(clip_line: &str) -> String {
    format!(
        "composition name=\"t\" width=64 height=48 duration=4 background=\"#000000\" fps=30 version=1 {{\n    {clip_line}\n}}\n"
    )
}

/// A valid trimmed and re-timed clip passes `check` (exit 0) and the
/// human and `--json` output echo the same effective source window
/// and effective rendered length.
#[test]
fn valid_trim_and_rate_echo_window_in_both_forms() {
    let folder = tmp("valid");
    gen_clip(&folder, "clip.mp4", 4.0);
    let src = probe_duration(&folder.join("clip.mp4"));
    assert!((src - 4.0).abs() < 0.05, "fixture is about 4s, got {src}");
    write(
        &folder,
        "composition.kdl",
        &comp("video path=\"clip.mp4\" start=0 duration=2 trim-start=1 trim-end=3 rate=2"),
    );
    let (code, out) = run(&["check", "."], &folder);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("1.000s..3.000s"), "{out}");
    assert!(out.contains("effective length 1.000s"), "{out}");
    assert!(out.contains("rate 2x"), "{out}");
    let (code, out) = run(&["check", "--json", "."], &folder);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("\"status\":\"valid\""), "{out}");
    assert!(out.contains("\"trim_start\":1"), "{out}");
    assert!(out.contains("\"trim_end\":3"), "{out}");
    assert!(out.contains("\"rate\":2"), "{out}");
    assert!(out.contains("\"effective_start\":1"), "{out}");
    assert!(out.contains("\"effective_end\":3"), "{out}");
    assert!(out.contains("\"effective_duration\":1"), "{out}");
    assert!(out.contains("\"clamped\":false"), "{out}");
}

/// Each invalid footage shape fails `check` (exit 1) with a located
/// `E_BAD_FOOTAGE` diagnostic naming the file and the accepted rule.
#[test]
fn invalid_windows_and_rates_fail_check() {
    let folder = tmp("invalid");
    gen_clip(&folder, "clip.mp4", 4.0);
    let src = probe_duration(&folder.join("clip.mp4"));
    let at_end = format!("video path=\"clip.mp4\" start=0 duration=2 trim-start={src}");
    let cases = [
        "video path=\"clip.mp4\" start=0 duration=2 trim-start=3 trim-end=1",
        "video path=\"clip.mp4\" start=0 duration=2 trim-start=2 trim-end=2",
        "video path=\"clip.mp4\" start=0 duration=2 trim-start=9",
        at_end.as_str(),
        "video path=\"clip.mp4\" start=0 duration=2 rate=0.05",
        "video path=\"clip.mp4\" start=0 duration=2 rate=20",
        "video path=\"clip.mp4\" start=0 duration=2 rate=0",
        "video path=\"clip.mp4\" start=0 duration=2 rate=-1",
    ];
    for line in cases {
        write(&folder, "composition.kdl", &comp(line));
        let (code, out) = run(&["check", "."], &folder);
        assert_eq!(code, 1, "{line}: {out}");
        assert!(out.contains("E_BAD_FOOTAGE"), "{line}: {out}");
        assert!(out.contains("composition.kdl"), "{line}: {out}");
        let (code, out) = run(&["check", "--json", "."], &folder);
        assert_eq!(code, 1, "{line}: {out}");
        assert!(out.contains("\"status\":\"invalid\""), "{line}: {out}");
        assert!(out.contains("\"code\":\"E_BAD_FOOTAGE\""), "{line}: {out}");
    }
    // Rates name the accepted range; windows name the window values.
    write(
        &folder,
        "composition.kdl",
        &comp("video path=\"clip.mp4\" start=0 duration=2 rate=20"),
    );
    let (_, out) = run(&["check", "."], &folder);
    assert!(out.contains("0.1 to 10"), "{out}");
}

/// A `trim-end` past the source end stays green (exit 0): the window
/// clamps to the true end and both forms report the clamp.
#[test]
fn past_end_trim_end_clamps_and_reports() {
    let folder = tmp("clamp");
    gen_clip(&folder, "clip.mp4", 4.0);
    let src = probe_duration(&folder.join("clip.mp4"));
    write(
        &folder,
        "composition.kdl",
        &comp("video path=\"clip.mp4\" start=0 duration=2 trim-end=9"),
    );
    let (code, out) = run(&["check", "."], &folder);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("clamped to"), "{out}");
    assert!(out.contains(&format!("{src:.3}s")), "{out}");
    let (code, out) = run(&["check", "--json", "."], &folder);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("\"clamped\":true"), "{out}");
    assert!(out.contains(&format!("\"effective_end\":{src}")), "{out}");
    assert!(out.contains(&format!("\"effective_duration\":{src}")), "{out}");
}

/// Several footage problems in one composition are all reported
/// together; exit codes stay 0 valid, 1 invalid.
#[test]
fn footage_problems_report_together_with_stable_exits() {
    let folder = tmp("multi");
    gen_clip(&folder, "clip.mp4", 4.0);
    write(
        &folder,
        "composition.kdl",
        &comp("video path=\"clip.mp4\" start=0 duration=2 trim-start=3 trim-end=1 rate=20\n    video path=\"clip.mp4\" start=2 duration=2 trim-start=9"),
    );
    let (code, out) = run(&["check", "."], &folder);
    assert_eq!(code, 1, "{out}");
    assert_eq!(out.matches("E_BAD_FOOTAGE").count(), 3, "{out}");
    let (code, out) = run(&["check", "--json", "."], &folder);
    assert_eq!(code, 1, "{out}");
    assert_eq!(out.matches("\"code\":\"E_BAD_FOOTAGE\"").count(), 3, "{out}");
}
