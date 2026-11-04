//! Sprint 11 transitions (T006) end-to-end: a `transition` element
//! bridges two clips over borrowed handle frames, the mid-bridge
//! frame blends per kind, the timeline decodes to exactly its
//! untransitioned frame count, and every refusal variant fails
//! `check` naming the element and the fix.
//!
//! The MP4 leg proves the validate-then-render seam: pixel assertions
//! stay tolerant of H.264 (flat areas survive CRF 23 within a few
//! LSB), while cut-vs-blend verdicts compare whole decoded frames,
//! so they prove the bridge, not the codec.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-transition-e2e-{tag}-{}-{}",
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
    let root = tmp("bridge");
    let proj = root.join("proj");
    std::fs::create_dir_all(&proj).unwrap();
    std::fs::write(proj.join("composition.kdl"), body).unwrap();
    (root, proj)
}

fn check_and_render(body: &str) -> (PathBuf, PathBuf) {
    let (root, proj) = stage(body);
    let (code, out) = run(&["check", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(&["render", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    let mp4 = proj.join("out.mp4");
    assert!(mp4.is_file(), "render must write an MP4");
    (root, proj)
}

/// Output frame `n` as raw RGB24.
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
    assert_eq!(out.stdout.len(), (w * h * 3) as usize, "frame {n} decodes");
    out.stdout
}

/// Whole-video decode as raw RGB24.
fn decode_all(mp4: &Path) -> Vec<u8> {
    let out = Command::new("ffmpeg")
        .args([
            "-y",
            "-v",
            "error",
            "-i",
            &mp4.display().to_string(),
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgb24",
            "-",
        ])
        .output()
        .expect("decode whole video");
    assert!(out.status.success());
    out.stdout
}

/// Mean absolute byte difference between two frames.
fn frame_mad(a: &[u8], b: &[u8]) -> f64 {
    assert_eq!(a.len(), b.len());
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (*x as f64 - *y as f64).abs())
        .sum::<f64>()
        / a.len() as f64
}

const FADE: &str = "composition name=\"main\" root=true width=64 height=64 duration=2 background=\"#000\" version=1 {\n box x=0 y=0 width=64 height=64 fill=\"#ff0000\" start=0 duration=1 id=\"a\"\n box x=0 y=0 width=64 height=64 fill=\"#0000ff\" start=1 duration=1 id=\"b\"\n transition from=\"a\" to=\"b\" duration=1 kind=\"fade\"\n}\n";

#[test]
fn fade_bridge_blends_midway_and_hands_off_at_the_seams() {
    let (_root, proj) = check_and_render(FADE);
    let mp4 = proj.join("out.mp4");
    // First frame: full outgoing red.
    let first = output_frame(&mp4, 0, 64, 64);
    let o = (32 * 64 + 32) * 3;
    assert!(
        first[o] >= 200 && first[o + 1] <= 64 && first[o + 2] <= 64,
        "first frame must be red, got {},{},{}",
        first[o],
        first[o + 1],
        first[o + 2]
    );
    // Mid-bridge (t=1.5): half-mixed purple, not a cut to either side.
    let mid = output_frame(&mp4, 45, 64, 64);
    assert!(
        (100..=170).contains(&mid[o]) && mid[o + 1] <= 64 && (100..=170).contains(&mid[o + 2]),
        "mid-bridge frame must half-mix red and blue, got {},{},{}",
        mid[o],
        mid[o + 1],
        mid[o + 2]
    );
    // Tail (t=59/30): the incoming blue has nearly taken over.
    let last = output_frame(&mp4, 59, 64, 64);
    assert!(
        last[o] <= 64 && last[o + 1] <= 64 && last[o + 2] >= 200,
        "tail frame must lean blue, got {},{},{}",
        last[o],
        last[o + 1],
        last[o + 2]
    );
}

#[test]
fn transitioned_timeline_decodes_to_the_same_frame_count() {
    // The bridge borrows handles without moving the timeline: with
    // and without the transition, the same 60 frames decode.
    let (_root, proj) = check_and_render(FADE);
    let bridged = decode_all(&proj.join("out.mp4"));
    assert_eq!(bridged.len(), 60 * 64 * 64 * 3);
    let plain = FADE.replace(
        "\n transition from=\"a\" to=\"b\" duration=1 kind=\"fade\"",
        "",
    );
    let (_root, proj) = check_and_render(&plain);
    let unbridged = decode_all(&proj.join("out.mp4"));
    assert_eq!(unbridged.len(), bridged.len());
}

#[test]
fn every_kind_bridges_midway_instead_of_cutting() {
    // One fixture per kind: the mid-bridge frame differs from both
    // the first and the last frame of the same video, so every kind
    // blends or moves rather than cutting.
    for kind in [
        "fade",
        "dissolve",
        "wipe-left",
        "wipe-right",
        "wipe-up",
        "wipe-down",
        "slide",
        "push",
        "cover",
        "circle-open",
        "iris",
        "dip-black",
        "dip-white",
        "blur-dissolve",
        "css-fade-up",
        "css-fade-down",
        "css-zoom-in",
        "css-zoom-out",
        "css-slide-left",
        "css-slide-right",
    ] {
        let body = FADE.replace("kind=\"fade\"", &format!("kind=\"{kind}\""));
        let (_root, proj) = check_and_render(&body);
        let mp4 = proj.join("out.mp4");
        let first = output_frame(&mp4, 0, 64, 64);
        let mid = output_frame(&mp4, 45, 64, 64);
        let last = output_frame(&mp4, 59, 64, 64);
        assert!(
            frame_mad(&mid, &first) > 2.0,
            "{kind} mid-bridge frame must move past the first frame"
        );
        assert!(
            frame_mad(&mid, &last) > 2.0,
            "{kind} mid-bridge frame must not jump to the last frame"
        );
    }
}

#[test]
fn check_refuses_unknown_clip_ids() {
    let bad = FADE.replace("from=\"a\"", "from=\"ghost\"");
    let (root, _proj) = stage(&bad);
    let (code, out) = run(&["check", "proj"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_UNKNOWN_REF"), "{out}");
    assert!(out.contains("transition"), "{out}");
    assert!(out.contains("\"ghost\""), "{out}");

    let bad = FADE.replace("to=\"b\"", "to=\"ghost\"");
    let (root, _proj) = stage(&bad);
    let (code, out) = run(&["check", "proj"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_UNKNOWN_REF"), "{out}");
    assert!(out.contains("`to`"), "{out}");
}

#[test]
fn check_refuses_overlapping_bridges_on_one_boundary() {
    let bad = FADE.replace(
        " transition from=\"a\" to=\"b\" duration=1 kind=\"fade\"",
        " transition from=\"a\" to=\"b\" duration=1 kind=\"fade\"\n transition from=\"a\" to=\"b\" duration=0.5 kind=\"dissolve\"",
    );
    let (root, _proj) = stage(&bad);
    let (code, out) = run(&["check", "proj"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_BAD_TRANSITION"), "{out}");
    assert!(out.contains("overlaps"), "{out}");
    assert!(out.contains("fade") && out.contains("dissolve"), "{out}");
}

#[test]
fn check_refuses_short_handles_with_the_frame_shortfall() {
    let bad = FADE.replace("duration=1 kind", "duration=1.5 kind");
    let (root, _proj) = stage(&bad);
    let (code, out) = run(&["check", "proj"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_BAD_TRANSITION"), "{out}");
    assert!(out.contains("transition"), "{out}");
    assert!(out.contains("frame"), "{out}");
}

#[test]
fn check_refuses_zero_durations_and_unknown_kinds() {
    let bad = FADE.replace("duration=1 kind", "duration=0 kind");
    let (root, _proj) = stage(&bad);
    let (code, out) = run(&["check", "proj"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_BAD_TRANSITION"), "{out}");
    assert!(out.contains("positive"), "{out}");

    let bad = FADE.replace("kind=\"fade\"", "kind=\"morph\"");
    let (root, _proj) = stage(&bad);
    let (code, out) = run(&["check", "proj"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_BAD_TRANSITION"), "{out}");
    assert!(out.contains("morph"), "{out}");
    assert!(out.contains("wipe-left"), "{out}");
}
