//! Sprint 7 motion (T002 + T003) end-to-end: split typewriter reveals
//! and count numbers render frame-varying text, marks drive
//! `start-after` windows, every invalid variant refuses with its code
//! and no MP4, and still motion re-renders byte-identically.
//!
//! All fixtures use the host `ffmpeg` (like `cli_n07_motion`).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-n07-split-marks-{tag}-{}-{}",
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

fn frame_of<'a>(raw: &'a [u8], k: usize, w: u32, h: u32) -> &'a [u8] {
    let len = w as usize * h as usize * 3;
    &raw[k * len..(k + 1) * len]
}

/// Ink pixels of one raw RGB24 frame: any channel clearly off black.
fn ink_pixels(frame: &[u8]) -> u64 {
    frame
        .chunks_exact(3)
        .filter(|px| px[0] > 20 || px[1] > 20 || px[2] > 20)
        .count() as u64
}

/// Mean-x of strongly red pixels in one raw RGB24 frame.
fn red_centroid_x(frame: &[u8], w: u32) -> Option<f64> {
    let (mut sx, mut n) = (0u64, 0u64);
    for (i, px) in frame.chunks_exact(3).enumerate() {
        if px[0] > 150 && px[1] < 100 && px[2] < 100 {
            sx += (i as u32 % w) as u64;
            n += 1;
        }
    }
    if n == 0 {
        None
    } else {
        Some(sx as f64 / n as f64)
    }
}

const HEAD: &str =
    "composition name=\"t\" width=160 height=120 duration=2 fps=30 background=\"#000000\" version=1 {\n";

#[test]
fn typewriter_reveal_grows_and_rerenders_identically() {
    // T002: a six-piece chars typewriter over [0, 2) shows nothing on
    // the first frame and strictly more ink by the last frame.
    let root = tmp("typewriter");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    write(
        &folder,
        "composition.kdl",
        &format!(
            "{HEAD}    text value=\"abcdef\" x=10 y=40 size=32 color=\"#ffffff\" start=0 duration=2 split=\"chars\" typewriter=true\n}}\n"
        ),
    );
    assert_eq!(run(&["check", "proj"], &root).0, 0);
    assert_eq!(run(&["render", "proj", "-o", "a.mp4"], &root).0, 0);
    assert_eq!(run(&["render", "proj", "-o", "b.mp4"], &root).0, 0);
    let a = std::fs::read(root.join("a.mp4")).unwrap();
    let b = std::fs::read(root.join("b.mp4")).unwrap();
    assert_eq!(a, b, "typewriter re-renders byte-identically");
    let raw = decode_raw(&root.join("a.mp4"), 60);
    let ink = |k: usize| ink_pixels(frame_of(&raw, k, 160, 120));
    assert_eq!(ink(0), 0, "progress 0 shows no pieces");
    assert!(ink(15) > 0, "pieces reveal by the quarter mark");
    assert!(
        ink(59) > ink(15),
        "the last frame shows more ink than the quarter mark ({} vs {})",
        ink(59),
        ink(15)
    );
}

#[test]
fn count_clips_render_changing_numbers_identically() {
    // T002: a 0-to-20 count with no decimals changes its ink between
    // the first and middle frames and re-renders byte-identically.
    let root = tmp("count");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    write(
        &folder,
        "composition.kdl",
        &format!(
            "{HEAD}    text value=\"ignored\" x=10 y=40 size=32 color=\"#ffffff\" start=0 duration=2 count-from=0 count-to=20\n}}\n"
        ),
    );
    assert_eq!(run(&["check", "proj"], &root).0, 0);
    assert_eq!(run(&["render", "proj", "-o", "a.mp4"], &root).0, 0);
    assert_eq!(run(&["render", "proj", "-o", "b.mp4"], &root).0, 0);
    let a = std::fs::read(root.join("a.mp4")).unwrap();
    let b = std::fs::read(root.join("b.mp4")).unwrap();
    assert_eq!(a, b, "counts re-render byte-identically");
    let raw = decode_raw(&root.join("a.mp4"), 60);
    let (f0, f30) = (frame_of(&raw, 0, 160, 120), frame_of(&raw, 30, 160, 120));
    let changed: u64 = f0
        .chunks_exact(3)
        .zip(f30.chunks_exact(3))
        .filter(|(a, b)| {
            (a[0] as i16 - b[0] as i16).abs()
                + (a[1] as i16 - b[1] as i16).abs()
                + (a[2] as i16 - b[2] as i16).abs()
                > 60
        })
        .count() as u64;
    assert!(
        changed > 20,
        "\"0\" and \"10\" must draw different ink, changed {changed} pixels"
    );
}

#[test]
fn mark_followers_start_at_the_marked_instant() {
    // T003: box B follows mark "beat" at 1s while box A owns [0, 1).
    // Frame 15 shows A at x=10 (center 30); frame 30 shows B at
    // x=100 (center 120) after A expired.
    let root = tmp("marks");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    write(
        &folder,
        "composition.kdl",
        &format!(
            "{HEAD}    box id=\"a\" x=10 y=40 width=40 height=40 fill=\"#ff0000\" start=0 duration=1\n    mark name=\"beat\" at=1\n    box start-after=\"beat\" duration=1 x=100 y=40 width=40 height=40 fill=\"#ff0000\"\n}}\n"
        ),
    );
    assert_eq!(run(&["check", "proj"], &root).0, 0);
    assert_eq!(run(&["render", "proj", "-o", "out.mp4"], &root).0, 0);
    let raw = decode_raw(&root.join("out.mp4"), 60);
    let cx = |k: usize| red_centroid_x(frame_of(&raw, k, 160, 120), 160).unwrap();
    assert!(
        (cx(15) - 30.0).abs() < 3.0,
        "A shows before the mark, got {}",
        cx(15)
    );
    assert!(
        (cx(30) - 120.0).abs() < 3.0,
        "B starts at the mark, got {}",
        cx(30)
    );
}

#[test]
fn split_and_mark_refusals_name_the_code_and_write_no_mp4() {
    // T002 + T003: every invalid split/mark variant fails check and
    // render with its code, and render writes no MP4.
    let root = tmp("refuse");
    let cases = [
        (
            "split-on-box",
            "    box x=10 y=40 width=40 height=40 fill=\"#ff0000\" start=0 duration=2 split=\"chars\"\n",
            "E_BAD_MOTION",
        ),
        (
            "contradictory",
            "    text value=\"Hi\" start=0 duration=2 split=\"chars\" count-from=0 count-to=9\n",
            "E_BAD_MOTION",
        ),
        (
            "typewriter-without-split",
            "    text value=\"Hi\" start=0 duration=2 typewriter=true\n",
            "E_BAD_MOTION",
        ),
        (
            "unknown-mark",
            "    box x=10 y=40 width=40 height=40 fill=\"#ff0000\" start-after=\"ghost\" duration=1\n",
            "E_UNKNOWN_REF",
        ),
        (
            "outside-mark",
            "    mark name=\"late\" at=9\n    box x=10 y=40 width=40 height=40 fill=\"#ff0000\" start=0 duration=2\n",
            "E_BAD_TIMING",
        ),
    ];
    for (tag, body, code) in cases {
        let folder = root.join(tag);
        std::fs::create_dir_all(&folder).unwrap();
        write(&folder, "composition.kdl", &format!("{HEAD}{body}}}\n"));
        let (status, out) = run(&["check", "."], &folder);
        assert_eq!(status, 1, "{tag}: check must exit 1, got {status}: {out}");
        assert!(
            out.contains(code),
            "{tag}: diagnostics must name {code}: {out}"
        );
        let (rstatus, _) = run(&["render", ".", "-o", "out.mp4"], &folder);
        assert_eq!(rstatus, 1, "{tag}: render must refuse");
        assert!(!folder.join("out.mp4").exists(), "{tag}: no MP4 on refusal");
    }
}

#[test]
fn still_text_beside_tweens_rerenders_byte_identically() {
    // Regression: the frame-varying text lookup misses still text, so
    // a static text clip beside tweens renders exactly as before —
    // byte-identical re-renders with pinned tween values.
    let root = tmp("regression");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    write(
        &folder,
        "composition.kdl",
        &format!(
            "{HEAD}    text value=\"Hi\" x=10 y=10 size=24 color=\"#ffffff\" start=0 duration=2\n    box x=10 y=40 width=40 height=40 fill=\"#ff0000\" start=0 duration=2\n    tween target=1 property=\"x\" kind=\"to\" start=0 duration=1 to=110 ease=\"in-quad\"\n    set target=1 property=\"x\" start=1.5 duration=0.25 value=50\n}}\n"
        ),
    );
    assert_eq!(run(&["check", "."], &folder).0, 0);
    assert_eq!(run(&["render", ".", "-o", "a.mp4"], &folder).0, 0);
    assert_eq!(run(&["render", ".", "-o", "b.mp4"], &folder).0, 0);
    let a = std::fs::read(folder.join("a.mp4")).unwrap();
    let b = std::fs::read(folder.join("b.mp4")).unwrap();
    assert_eq!(a, b, "still text beside tweens re-renders byte-identically");
    // Pinned decoded values: in-quad midpoint x = 10 + 100/4 = 35
    // (center 55); the set holds x = 50 (center 70) at frame 45.
    let raw = decode_raw(&folder.join("a.mp4"), 60);
    let cx = |k: usize| red_centroid_x(frame_of(&raw, k, 160, 120), 160).unwrap();
    assert!(
        (cx(15) - 55.0).abs() < 3.0,
        "in-quad midpoint pins, got {}",
        cx(15)
    );
    assert!((cx(45) - 70.0).abs() < 3.0, "set hold pins, got {}", cx(45));
}
