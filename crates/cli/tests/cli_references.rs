//! T002 proof: both reference compositions exist in Framesmith KDL
//! form and each renders in its own tool (the Framesmith side) with
//! exit 0 to a standard MP4.
//!
//! Reference 1 is a 10 s 1080p30 title card (text, image, audio,
//! entrance + exit animation). Reference 2 is an animated lower-third
//! over video with sound. The HyperFrames sides stay OPEN: no
//! HyperFrames tool exists on this machine, so nothing about them is
//! asserted here (see `evidences/proof-n04-T002/REPORT.md`).
//!
//! Folders are staged to a temp dir so the proof never writes
//! `out.mp4` or `logs/` into the checkout itself.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-refs-proof-{}-{}-{}",
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

/// Recursively stage an example folder (including `media/`) while
/// skipping render outputs.
fn stage_example(name: &str) -> (PathBuf, PathBuf) {
    let root = tmp(name);
    let src = manifest_dir().join("examples").join(name);
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

fn ffprobe_streams(mp4: &Path) -> String {
    let out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "stream=codec_name,width,height,pix_fmt,sample_rate,channels,duration",
            "-of",
            "default=noprint_wrappers=1",
        ])
        .arg(mp4)
        .output()
        .expect("run host ffprobe");
    assert!(out.status.success());
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Fully decode every video and audio frame through software decoders;
/// any corrupt packet fails this step.
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

fn prove_reference(name: &str, width: u32, height: u32) {
    let (root, _folder) = stage_example(name);
    let (code, out) = run(&["check", "proj"], &root);
    assert_eq!(code, 0, "{name} check: {out}");
    assert!(out.contains("ok:"), "{name} check: {out}");

    let (code, out) = run(&["render", "proj"], &root);
    assert_eq!(code, 0, "{name} render: {out}");

    let mp4 = root.join("proj").join("out.mp4");
    assert!(mp4.is_file(), "{name}: render must write an MP4");
    let streams = ffprobe_streams(&mp4);
    for want in [
        "codec_name=h264",
        &format!("width={width}"),
        &format!("height={height}"),
        "pix_fmt=yuv420p",
        "codec_name=aac",
        "sample_rate=48000",
        "channels=2",
    ] {
        assert!(streams.contains(want), "{name}: {streams}");
    }
    decode_all(&mp4);

    // The first frame must be full-size RGB after decode.
    let raw = Command::new("ffmpeg")
        .args([
            "-y",
            "-v",
            "error",
            "-i",
            &mp4.display().to_string(),
            "-frames:v",
            "1",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgb24",
            "-",
        ])
        .output()
        .expect("decode first frame");
    assert!(raw.status.success());
    assert_eq!(raw.stdout.len(), width as usize * height as usize * 3);
}

#[test]
fn reference_1_title_card_renders_in_own_tool() {
    prove_reference("reference-1-title-card", 1920, 1080);
}

#[test]
fn reference_2_lower_third_renders_in_own_tool() {
    prove_reference("reference-2-lower-third", 1920, 1080);
}
