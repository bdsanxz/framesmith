//! T010 proof: the footage reference composition renders in its own tool.
//!
//! `examples/reference-footage` exercises every sprint footage behavior
//! in one 6 s 720p composition — VFR source with trim-start/trim-end at
//! half rate, rotated source trimmed and looped, finite GIF sticker
//! with an inferred duration — and must check green (with the inferred
//! length echoed), render to a standard MP4, show the GIF color on the
//! timeline, and re-render byte-identically at any worker count. The
//! HyperFrames twin (`examples/reference-footage-hf`) and the picture
//! plus speed comparison live in `evidences/proof-n05-T010/` (one
//! command: `compare.sh`); they need npx/network, so they stay out of
//! this test.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-footage-ref-{tag}-{}-{}",
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
    run_env(args, cwd, &[])
}

fn run_env(args: &[&str], cwd: &Path, env: &[(&str, &str)]) -> (i32, String) {
    let out = Command::new(bin())
        .args(args)
        .envs(env.iter().copied())
        .current_dir(cwd)
        .output()
        .expect("run framesmith binary");
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    (out.status.code().unwrap_or(-1), text)
}

/// Recursively stage the reference folder while skipping render outputs.
fn stage_reference() -> (PathBuf, PathBuf) {
    let root = tmp("stage");
    let src = manifest_dir().join("examples").join("reference-footage");
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
        let from = e.path();
        let to = dst.join(&name);
        if from.is_dir() {
            copy_clean(&from, &to);
        } else {
            std::fs::copy(&from, &to).unwrap();
        }
    }
}

fn ffprobe(entries: &str, mp4: &Path) -> String {
    let out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            entries,
            "-of",
            "default=noprint_wrappers=1",
        ])
        .arg(mp4)
        .output()
        .expect("run host ffprobe");
    assert!(out.status.success());
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Fully decode every video and audio frame through software decoders.
fn decode_all(mp4: &Path) {
    let out = Command::new("ffmpeg")
        .args([
            "-y",
            "-v",
            "error",
            "-i",
            &mp4.display().to_string(),
            "-f",
            "null",
            "-",
        ])
        .output()
        .expect("decode mp4");
    assert!(out.status.success(), "full software decode failed");
}

/// One output frame as raw RGB24.
fn output_frame(mp4: &Path, n: u64, w: u32, h: u32) -> Vec<u8> {
    let out = Command::new("ffmpeg")
        .args([
            "-y",
            "-v",
            "error",
            "-i",
            &mp4.display().to_string(),
            "-vf",
            &format!("select='eq(n\\,{n})'"),
            "-frames:v",
            "1",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgb24",
            "-",
        ])
        .output()
        .expect("extract output frame");
    assert!(out.status.success());
    assert_eq!(out.stdout.len(), (w * h * 3) as usize);
    out.stdout
}

/// The reference checks green and reviews every footage window,
/// including the GIF inferred length.
#[test]
fn footage_reference_checks_green_with_inferred_duration() {
    let (root, _folder) = stage_reference();
    let (code, out) = run(&["check", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("1.000s..4.000s"), "{out}");
    assert!(out.contains("rate 0.5x"), "{out}");
    assert!(out.contains("loop on"), "{out}");
    assert!(out.contains("2.000s (inferred)"), "{out}");
    assert!(out.contains("ok:"), "{out}");
}

/// The reference renders to a standard 720p MP4 whose timeline shows
/// the GIF sticker mid-composition and the looped portrait after it.
#[test]
fn footage_reference_renders_standard_mp4_with_footage_content() {
    let (root, _folder) = stage_reference();
    let (code, out) = run(&["render", "proj", "-o", "out.mp4"], &root);
    assert_eq!(code, 0, "{out}");
    let mp4 = root.join("out.mp4");
    assert!(mp4.is_file());
    let streams = ffprobe(
        "stream=codec_name,width,height,pix_fmt,sample_rate,channels",
        &mp4,
    );
    for want in [
        "codec_name=h264",
        "width=1280",
        "height=720",
        "pix_fmt=yuv420p",
        "codec_name=aac",
        "sample_rate=48000",
        "channels=2",
    ] {
        assert!(streams.contains(want), "{streams}");
    }
    let dur = ffprobe("format=duration", &mp4);
    let secs: f64 = dur
        .lines()
        .find_map(|l| l.strip_prefix("duration=")?.parse().ok())
        .expect("duration parses");
    assert!(
        (secs - 6.0).abs() < 0.05,
        "composition renders 6 s, got {secs}"
    );
    decode_all(&mp4);

    // Frame 60 (t=2 s) sits inside the GIF window [1,3): the centered
    // 960x720 sticker paints the canvas middle solid blue.
    let (w, h) = (1280u32, 720u32);
    let gif_frame = output_frame(&mp4, 60, w, h);
    let at = |d: &[u8], x: u32, y: u32| -> (u8, u8, u8) {
        let i = ((y * w + x) * 3) as usize;
        (d[i], d[i + 1], d[i + 2])
    };
    let (r, g, b) = at(&gif_frame, 640, 360);
    assert!(
        b > 150 && r < 150 && g < 150,
        "GIF sticker shows blue at t=2 s, got ({r},{g},{b})"
    );

    // Frame 150 (t=5 s) is past the GIF window: the looped portrait
    // pillarbox is back over the slowed background, so the middle
    // pixel differs from the sticker blue.
    let later = output_frame(&mp4, 150, w, h);
    assert_ne!(
        at(&later, 640, 360),
        (r, g, b),
        "post-GIF frame differs from the sticker frame"
    );
}

/// The reference re-renders byte-identically (cache replay) and does
/// not depend on worker count (fresh decodes at 1 vs 8 workers).
#[test]
fn footage_reference_rerenders_byte_identically_at_any_worker_count() {
    let (root, _folder) = stage_reference();
    let (code, out) = run(&["render", "proj", "-o", "r1.mp4"], &root);
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(&["render", "proj", "-o", "r2.mp4"], &root);
    assert_eq!(code, 0, "{out}");
    let (code, out) = run_env(
        &["render", "proj", "-o", "w1.mp4"],
        &root,
        &[("FRAMESMITH_WORKERS", "1")],
    );
    assert_eq!(code, 0, "{out}");
    std::fs::remove_dir_all(
        root.join("proj")
            .join("logs")
            .join("framesmith-footage-cache"),
    )
    .ok();
    let (code, out) = run_env(
        &["render", "proj", "-o", "w8.mp4"],
        &root,
        &[("FRAMESMITH_WORKERS", "8")],
    );
    assert_eq!(code, 0, "{out}");
    let (r1, r2, w1, w8) = (
        std::fs::read(root.join("r1.mp4")).unwrap(),
        std::fs::read(root.join("r2.mp4")).unwrap(),
        std::fs::read(root.join("w1.mp4")).unwrap(),
        std::fs::read(root.join("w8.mp4")).unwrap(),
    );
    assert_eq!(r1, r2, "cache replay re-renders identical bytes");
    assert_eq!(r1, w1, "1 worker renders identical bytes");
    assert_eq!(r1, w8, "8 workers render identical bytes");
}
