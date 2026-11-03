//! T016 end-to-end proof: both bundled examples render from a clean
//! checkout to playable MP4s with the expected text, timing, and original
//! video sound; every invalid fixture is rejected with complete located
//! diagnostics; every later-sprint fixture is refused before the first
//! frame naming element and sprint; CPU reruns decode byte-identically;
//! and the init scaffold passes validation with no edits.
//!
//! Example/fixture folders are copied to temp dirs so the proof never
//! writes `out.mp4` or `logs/` into the checkout itself.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-proof-{}-{}-{}",
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

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
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

/// Copy an example/fixture folder to a temp dir, skipping proof output
/// (`logs/`, `out.mp4`) so reruns start clean.
fn stage(src_name: &str, kind: &str) -> (PathBuf, PathBuf) {
    let root = tmp(kind);
    let src = manifest_dir().join(src_name);
    let dst = root.join("proj");
    copy_clean(&src, &dst);
    (root, dst)
}

fn copy_clean(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).unwrap();
    for e in std::fs::read_dir(src).unwrap() {
        let e = e.unwrap();
        let name = e.file_name().into_string().unwrap();
        if name == "logs" || name == "out.mp4" {
            continue;
        }
        let target = dst.join(&name);
        if e.file_type().unwrap().is_dir() {
            copy_clean(&e.path(), &target);
        } else {
            std::fs::copy(e.path(), &target).unwrap();
        }
    }
}

fn ffmpeg(args: &[&str]) {
    let out = Command::new("ffmpeg")
        .args(["-y", "-v", "error"])
        .args(args)
        .output()
        .expect("run host ffmpeg");
    assert!(out.status.success(), "host ffmpeg failed: {args:?}");
}

fn ffprobe_streams(mp4: &Path) -> String {
    let out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "stream=codec_name,width,height,pix_fmt,sample_rate,channels",
            "-of",
            "default=noprint_wrappers=1",
        ])
        .arg(mp4)
        .output()
        .expect("run host ffprobe");
    assert!(out.status.success());
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn decode_raw(mp4: &Path, frames: usize) -> Vec<u8> {
    let out = Command::new("ffmpeg")
        .args([
            "-y",
            "-v",
            "error",
            "-i",
            &mp4.display().to_string(),
            "-frames:v",
            &frames.to_string(),
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgb24",
            "-",
        ])
        .output()
        .expect("decode mp4");
    assert!(out.status.success());
    out.stdout
}

/// Pixels far from `bg` (drawn text/video survive the yuv420p roundtrip).
fn marked_pixels(frame: &[u8], bg: (u8, u8, u8)) -> usize {
    frame
        .chunks_exact(3)
        .filter(|p| {
            (p[0] as i32 - bg.0 as i32).abs()
                + (p[1] as i32 - bg.1 as i32).abs()
                + (p[2] as i32 - bg.2 as i32)
                > 100
        })
        .count()
}

fn mean_volume_db(mp4: &Path, work: &Path) -> f64 {
    let wav = work.join("audio.wav");
    ffmpeg(&[
        "-i",
        &mp4.display().to_string(),
        "-map",
        "0:a",
        &wav.display().to_string(),
    ]);
    let out = Command::new("ffmpeg")
        .args([
            "-v",
            "info",
            "-i",
            &wav.display().to_string(),
            "-af",
            "volumedetect",
            "-f",
            "null",
            "-",
        ])
        .output()
        .expect("volumedetect");
    let text = String::from_utf8_lossy(&out.stderr);
    for line in text.lines() {
        if let Some(pos) = line.find("mean_volume:") {
            let num: String = line[pos + "mean_volume:".len()..]
                .trim()
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_string();
            if let Ok(v) = num.parse::<f64>() {
                return v;
            }
        }
    }
    panic!("no mean_volume in volumedetect output: {text}");
}

fn assert_standard_mp4(mp4: &Path, w: u32, h: u32) {
    assert!(mp4.is_file(), "render must write an MP4");
    let streams = ffprobe_streams(mp4);
    for want in [
        "codec_name=h264",
        &format!("width={w}"),
        &format!("height={h}"),
        "pix_fmt=yuv420p",
        "codec_name=aac",
        "sample_rate=48000",
        "channels=2",
    ] {
        assert!(streams.contains(want), "{streams}");
    }
}

#[test]
fn example_text_on_color_renders_with_expected_text() {
    let (root, folder) = stage("examples/text-on-color", "color");
    let name = folder.file_name().unwrap().to_string_lossy().into_owned();
    let (code, out) = run(&["check", &name], &root);
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(&["render", &name], &root);
    assert_eq!(code, 0, "{out}");
    let mp4 = folder.join("out.mp4");
    assert_standard_mp4(&mp4, 640, 360);
    // Both text lines are drawn over the background color.
    let raw = decode_raw(&mp4, 1);
    assert!(
        marked_pixels(&raw, (0x1a, 0x1a, 0x2e)) > 100,
        "text must be drawn over the background"
    );
}

#[test]
fn example_text_on_video_renders_with_text_timing_and_sound() {
    let (root, folder) = stage("examples/text-on-video", "video");
    let name = folder.file_name().unwrap().to_string_lossy().into_owned();
    let (code, out) = run(&["check", &name], &root);
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(&["render", &name], &root);
    assert_eq!(code, 0, "{out}");
    let mp4 = folder.join("out.mp4");
    assert_standard_mp4(&mp4, 640, 360);
    // The full 3 s / 90-frame timeline is present.
    let raw = decode_raw(&mp4, 90);
    assert_eq!(raw.len(), 90 * 640 * 360 * 3, "90 frames of 640x360 rgb24");
    // Text is drawn over the video picture (far from black bars).
    let first = &raw[..640 * 360 * 3];
    assert!(marked_pixels(first, (0, 0, 0)) > 100, "text over video");
    // The source tone passes through audibly.
    let db = mean_volume_db(&mp4, &root);
    assert!(db > -50.0, "original sound must be audible, got {db} dB");
}

#[test]
fn sprint2_still_elements_validate_and_render() {
    // T001/T002: boxes, images, written paint order, and text spacing
    // validate with no diagnostics and stay render candidates; the
    // render path builds one description per frame time (box and image
    // pixels land in T004/T005, text draws now).
    let (root, folder) = stage("fixtures/sprint2-still", "still");
    let name = folder.file_name().unwrap().to_string_lossy().into_owned();
    let (code, out) = run(&["check", &name], &root);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("ok:"), "{out}");
    let (code, out) = run(&["check", "--json", &name], &root);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("\"status\":\"valid\""), "{out}");
    let (code, out) = run(&["render", &name], &root);
    assert_eq!(code, 0, "{out}");
    let mp4 = folder.join("out.mp4");
    assert_standard_mp4(&mp4, 320, 240);
    let raw = decode_raw(&mp4, 1);
    assert!(
        marked_pixels(&raw, (0x1a, 0x1a, 0x2e)) > 100,
        "text must be drawn over the background"
    );
}

#[test]
fn invalid_fixtures_rejected_with_located_diagnostics() {
    for (dir, code) in [
        ("fixtures/invalid-unknown-key", "E_UNKNOWN_KEY"),
        ("fixtures/invalid-timing", "E_BAD_TIMING"),
        ("fixtures/invalid-missing-video", "E_MISSING_FILE"),
        ("fixtures/invalid-missing-font", "E_MISSING_FILE"),
        ("fixtures/invalid-font-format", "E_BAD_FONT"),
        ("fixtures/invalid-text-attr", "E_BAD_TIMING"),
        ("fixtures/invalid-video-fit", "E_BAD_FIT"),
        ("fixtures/invalid-box-size", "E_BAD_BOX"),
        ("fixtures/invalid-image-missing", "E_MISSING_FILE"),
        ("fixtures/invalid-image-corrupt", "E_BAD_IMAGE"),
        ("fixtures/invalid-order", "E_BAD_ORDER"),
        ("fixtures/invalid-spacing", "E_BAD_SPACING"),
    ] {
        let (root, folder) = stage(dir, "invalid");
        let name = folder.file_name().unwrap().to_string_lossy().into_owned();
        // Human form: exit 1 with file, line, column, code, fix.
        let (exit, out) = run(&["check", &name], &root);
        assert_eq!(exit, 1, "{dir}: {out}");
        assert!(out.contains(&format!("error[{code}]")), "{dir}: {out}");
        assert!(out.contains("composition.kdl:"), "{dir}: {out}");
        assert!(out.contains("(fix:"), "{dir}: {out}");
        // JSON form reports the same problem set for agents.
        let (exit, out) = run(&["check", "--json", &name], &root);
        assert_eq!(exit, 1, "{dir}: {out}");
        assert!(out.contains(&format!("\"code\":\"{code}\"")), "{dir}: {out}");
        assert!(out.contains("\"line\":"), "{dir}: {out}");
        assert!(out.contains("\"column\":"), "{dir}: {out}");
        // Render refuses and writes no MP4.
        let (exit, out) = run(&["render", &name], &root);
        assert_eq!(exit, 1, "{dir}: {out}");
        assert!(!folder.join("out.mp4").exists(), "{dir}");
        assert!(!folder.join("out.mp4.framesmith-part").exists(), "{dir}");
    }
}

#[test]
fn corrupt_video_refuses_at_render_naming_the_file() {
    let (root, folder) = stage("fixtures/invalid-corrupt-video", "corrupt");
    let name = folder.file_name().unwrap().to_string_lossy().into_owned();
    // Existence/readability validate; decodability refuses at render.
    let (check_exit, _) = run(&["check", &name], &root);
    assert_eq!(check_exit, 0);
    let (exit, out) = run(&["render", &name], &root);
    assert_eq!(exit, 1, "{out}");
    assert!(out.contains("clip.mp4"), "refusal names the file: {out}");
    assert!(!folder.join("out.mp4").exists());
}

#[test]
fn later_sprint_fixtures_refused_before_the_first_frame() {
    // Tweens, sets, and plain audio tracks validate since Sprints 2
    // and 3 (T003/T007); audio effects and later image formats still
    // wait for their sprints.
    for (dir, element, sprint) in [
        ("fixtures/later-audio", "audio", "Sprint 10"),
        ("fixtures/invalid-image-format", "image", "Sprint 2"),
    ] {
        let (root, folder) = stage(dir, "later");
        let name = folder.file_name().unwrap().to_string_lossy().into_owned();
        let (exit, out) = run(&["check", &name], &root);
        assert_eq!(exit, 2, "{dir}: {out}");
        assert!(out.contains(element), "{dir}: {out}");
        assert!(out.contains(sprint), "{dir}: {out}");
        let (exit, out) = run(&["render", &name], &root);
        assert_eq!(exit, 2, "{dir}: {out}");
        assert!(out.contains("before the first frame"), "{dir}: {out}");
        assert!(!folder.join("out.mp4").exists(), "{dir}");
        assert!(!folder.join("out.mp4.framesmith-part").exists(), "{dir}");
    }
}

#[test]
fn example_reruns_decode_byte_identical() {
    let (root, _) = stage("examples/text-on-color", "determinism");
    assert_eq!(run(&["render", "proj", "-o", "a.mp4"], &root).0, 0);
    assert_eq!(run(&["render", "proj", "-o", "b.mp4"], &root).0, 0);
    let a = decode_raw(&root.join("a.mp4"), 10);
    let b = decode_raw(&root.join("b.mp4"), 10);
    assert_eq!(a.len(), b.len());
    assert_eq!(a, b, "CPU reruns must decode byte-identically");
}

#[test]
fn init_scaffold_passes_check_with_no_edits() {
    let root = tmp("initscaffold");
    let (code, out) = run(&["init", "demo"], &root);
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(&["check", "demo"], &root);
    assert_eq!(code, 0, "init scaffold must validate untouched: {out}");
}

#[test]
fn run_log_covers_check_to_render_on_examples() {
    let (root, folder) = stage("examples/text-on-video", "logcover");
    assert_eq!(run(&["check", "proj"], &root).0, 0);
    let (code, _) = run(&["render", "proj"], &root);
    assert_eq!(code, 0);
    let log = std::fs::read_to_string(folder.join("logs").join("framesmith.log"))
        .expect("run log must exist");
    assert!(!log.is_empty());
    for line in log.lines() {
        for field in ["ts=", "level=", "component=", "run="] {
            assert!(line.contains(field), "line lacks {field}: {line}");
        }
    }
    assert!(log.contains("component=check"), "{log}");
    assert!(log.contains("component=bootstrap"), "{log}");
    assert!(log.contains("component=render"), "{log}");
}
