//! Sprint 11 effects (T004) end-to-end: an effected composition
//! checks green and renders to a standard MP4 whose decoded pixels
//! carry the look, while a bad preset, amount, or parameter fails
//! `check` naming the preset and the fix.
//!
//! The MP4 leg proves the validate-then-render seam: like the other
//! end-to-end tests, rendering and stream facts use the host FFmpeg.
//! Pixel assertions stay tolerant of H.264 (flat areas survive CRF 23
//! within a few LSB), so they prove the look, not the codec.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-effect-e2e-{tag}-{}-{}",
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

fn stage(body: &str) -> (PathBuf, PathBuf) {
    let root = tmp("effect");
    let proj = root.join("proj");
    std::fs::create_dir_all(&proj).unwrap();
    std::fs::write(proj.join("composition.kdl"), body).unwrap();
    (root, proj)
}

fn decode_first_frame(mp4: &Path, w: u32, h: u32) -> Vec<u8> {
    let out = Command::new("ffmpeg")
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
        .expect("decode mp4");
    assert!(out.status.success());
    assert_eq!(out.stdout.len(), (w * h * 3) as usize);
    out.stdout
}

const GOOD: &str = "composition name=\"main\" root=true width=64 height=64 duration=1 background=\"#000\" version=1 {\n box x=8 y=8 width=48 height=48 fill=\"#c86432\" start=0 duration=1 {\n effect preset=\"grayscale\"\n }\n}\n";

#[test]
fn effect_checks_and_renders_the_look() {
    let (root, proj) = stage(GOOD);
    let (code, out) = run(&["check", "proj"], &root);
    assert_eq!(code, 0, "{out}");

    let (code, out) = run(&["render", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    let mp4 = proj.join("out.mp4");
    assert!(mp4.is_file(), "render must write an MP4");

    // The box decodes gray, near the authored luma, far from the source hue.
    let raw = decode_first_frame(&mp4, 64, 64);
    let o = (32 * 64 + 32) * 3;
    let (r, g, b) = (raw[o] as i32, raw[o + 1] as i32, raw[o + 2] as i32);
    assert!(
        (r - g).abs() <= 16 && (g - b).abs() <= 16,
        "grayscale box must decode gray, got {r},{g},{b}"
    );
    assert!(
        (100..=140).contains(&((r + g + b) / 3)),
        "grayscale luma must sit near the authored 117, got {r},{g},{b}"
    );
}

#[test]
fn effect_zero_amount_renders_the_source_color() {
    // `amount=0` passes through: the box decodes in its authored color.
    let body = GOOD.replace(
        "effect preset=\"grayscale\"",
        "effect preset=\"grayscale\" amount=0",
    );
    let (root, proj) = stage(&body);
    let (code, out) = run(&["check", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(&["render", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    let raw = decode_first_frame(&proj.join("out.mp4"), 64, 64);
    let o = (32 * 64 + 32) * 3;
    let (r, g, b) = (raw[o] as i32, raw[o + 1] as i32, raw[o + 2] as i32);
    assert!(
        (r - 200).abs() <= 16 && (g - 100).abs() <= 16 && (b - 50).abs() <= 16,
        "zero-amount box must decode in its authored color, got {r},{g},{b}"
    );
}

#[test]
fn effect_stack_checks_green_in_both_orders() {
    for (first, second) in [("invert", "grayscale"), ("grayscale", "invert")] {
        let body = format!(
            "composition name=\"main\" root=true width=64 height=64 duration=1 background=\"#000\" version=1 {{\n box x=8 y=8 width=48 height=48 fill=\"#c86432\" start=0 duration=1 {{\n effect preset=\"{first}\"\n effect preset=\"{second}\"\n }}\n}}\n"
        );
        let (root, _proj) = stage(&body);
        let (code, out) = run(&["check", "proj"], &root);
        assert_eq!(code, 0, "{out}");
    }
}

#[test]
fn effect_check_refuses_unknown_preset() {
    let bad = GOOD.replace("effect preset=\"grayscale\"", "effect preset=\"vintage\"");
    let (root, _proj) = stage(&bad);
    let (code, out) = run(&["check", "proj"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_BAD_EFFECT"), "{out}");
    for name in ["grayscale", "chromatic-aberration", "hue-rotate"] {
        assert!(out.contains(name), "check names {name}: {out}");
    }
}

#[test]
fn effect_check_refuses_bad_amount_and_parameter() {
    let bad_amount = GOOD.replace(
        "effect preset=\"grayscale\"",
        "effect preset=\"blur\" amount=2",
    );
    let (root, _proj) = stage(&bad_amount);
    let (code, out) = run(&["check", "proj"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_BAD_EFFECT"), "{out}");
    assert!(out.contains("0 to 1"), "{out}");

    let bad_param = GOOD.replace(
        "effect preset=\"grayscale\"",
        "effect preset=\"brightness\" brightness=\"hot\"",
    );
    let (root, _proj) = stage(&bad_param);
    let (code, out) = run(&["check", "proj"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_BAD_EFFECT"), "{out}");
    assert!(out.contains("`brightness`"), "{out}");
}
