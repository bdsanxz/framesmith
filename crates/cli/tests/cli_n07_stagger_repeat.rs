//! Sprint 7 motion (T004 + T009) end-to-end: repeated motion replays
//! values across passes (yoyo reverses, gaps hold), key chains
//! replay, stagger overflow clamps with a check-output report,
//! refusals name the bound with no MP4, and unrepeated/unstaggered
//! motion re-renders byte-identically with pinned values.
//!
//! All fixtures use the host `ffmpeg` (like `cli_motion`).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-n07-stagger-repeat-{tag}-{}-{}",
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

fn frame_of(raw: &[u8], k: usize, w: u32, h: u32) -> &[u8] {
    let len = w as usize * h as usize * 3;
    &raw[k * len..(k + 1) * len]
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
const HEAD4: &str =
    "composition name=\"t\" width=160 height=120 duration=4 fps=30 background=\"#000000\" version=1 {\n";
const BOX: &str = "    box x=10 y=40 width=40 height=40 fill=\"#ff0000\" start=0 duration=2\n";
const BOX4: &str = "    box x=10 y=40 width=40 height=40 fill=\"#ff0000\" start=0 duration=4\n";

#[test]
fn repeat_replays_motion_across_passes() {
    // T004: from-to 10 -> 110 over [0, 1), three passes back to back.
    // Box centers sit 20 right of x: passes restart at 30 and land
    // the last pass on 130.
    let root = tmp("passes");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    write(
        &folder,
        "composition.kdl",
        &format!(
            "{HEAD4}{BOX4}    tween target=0 property=\"x\" kind=\"from-to\" from=10 to=110 start=0 duration=1 repeat=3\n}}\n"
        ),
    );
    assert_eq!(run(&["check", "proj"], &root).0, 0);
    let mp4 = root.join("out.mp4");
    assert_eq!(run(&["render", "proj", "-o", "out.mp4"], &root).0, 0);
    let raw = decode_raw(&mp4, 120);
    let cx = |k: usize| red_centroid_x(frame_of(&raw, k, 160, 120), 160).unwrap();
    assert!((cx(0) - 30.0).abs() < 3.0, "pass one starts, got {}", cx(0));
    assert!(
        (cx(15) - 80.0).abs() < 3.0,
        "pass one midpoints, got {}",
        cx(15)
    );
    assert!(
        (cx(30) - 30.0).abs() < 3.0,
        "pass two restarts, got {}",
        cx(30)
    );
    assert!(
        (cx(45) - 80.0).abs() < 3.0,
        "pass two midpoints, got {}",
        cx(45)
    );
    assert!(
        (cx(60) - 30.0).abs() < 3.0,
        "pass three restarts, got {}",
        cx(60)
    );
    assert!(
        (cx(75) - 80.0).abs() < 3.0,
        "pass three midpoints, got {}",
        cx(75)
    );
    assert!(
        (cx(90) - 130.0).abs() < 3.0,
        "pass three ends, got {}",
        cx(90)
    );
    assert!(
        (cx(119) - 130.0).abs() < 3.0,
        "past the passes the end holds, got {}",
        cx(119)
    );
    // Re-renders are byte-identical.
    let again = root.join("again.mp4");
    assert_eq!(run(&["render", "proj", "-o", "again.mp4"], &root).0, 0);
    assert_eq!(
        std::fs::read(&mp4).unwrap(),
        std::fs::read(&again).unwrap(),
        "repeated motion re-renders byte-identically"
    );
}

#[test]
fn yoyo_reverses_and_gap_holds_end_values() {
    // T004: linear yoyo over two passes mirrors the first pass; a
    // 0.5s gap holds 130 between passes.
    let root = tmp("yoyo-gap");
    let folder = root.join("yoyo");
    std::fs::create_dir_all(&folder).unwrap();
    write(
        &folder,
        "composition.kdl",
        &format!(
            "{HEAD}{BOX}    tween target=0 property=\"x\" kind=\"from-to\" from=10 to=110 start=0 duration=1 repeat=2 yoyo=true\n}}\n"
        ),
    );
    assert_eq!(run(&["check", "."], &folder).0, 0);
    assert_eq!(run(&["render", ".", "-o", "out.mp4"], &folder).0, 0);
    let raw = decode_raw(&folder.join("out.mp4"), 60);
    let cx = |k: usize| red_centroid_x(frame_of(&raw, k, 160, 120), 160).unwrap();
    assert!(
        (cx(15) - 80.0).abs() < 3.0,
        "forward pass midpoints, got {}",
        cx(15)
    );
    assert!(
        (cx(30) - 130.0).abs() < 3.0,
        "turnaround lands, got {}",
        cx(30)
    );
    assert!(
        (cx(45) - 80.0).abs() < 3.0,
        "reversed pass mirrors, got {}",
        cx(45)
    );
    assert!(
        (cx(59) - 33.0).abs() < 4.0,
        "reversed pass returns, got {}",
        cx(59)
    );

    let folder = root.join("gap");
    std::fs::create_dir_all(&folder).unwrap();
    write(
        &folder,
        "composition.kdl",
        &format!(
            "{HEAD}{BOX}    tween target=0 property=\"x\" kind=\"from-to\" from=10 to=110 start=0 duration=1 repeat=2 repeat-delay=0.5\n}}\n"
        ),
    );
    assert_eq!(run(&["check", "."], &folder).0, 0);
    assert_eq!(run(&["render", ".", "-o", "out.mp4"], &folder).0, 0);
    let raw = decode_raw(&folder.join("out.mp4"), 60);
    let cx = |k: usize| red_centroid_x(frame_of(&raw, k, 160, 120), 160).unwrap();
    assert!(
        (cx(30) - 130.0).abs() < 3.0,
        "first pass ends, got {}",
        cx(30)
    );
    assert!(
        (cx(37) - 130.0).abs() < 3.0,
        "gap holds the end, got {}",
        cx(37)
    );
    assert!(
        (cx(45) - 30.0).abs() < 3.0,
        "second pass restarts, got {}",
        cx(45)
    );
}

#[test]
fn repeat_key_replays_the_stop_chain_on_frames() {
    // T004: key stops 10 -> 110 over [0, 1) repeated twice; the chain
    // replays whole, then holds its end.
    let root = tmp("key-repeat");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    write(
        &folder,
        "composition.kdl",
        &format!(
            "{HEAD}{BOX}    key target=0 property=\"x\" repeat=2 {{\n        frame at=0 value=10\n        frame at=1 value=110\n    }}\n}}\n"
        ),
    );
    assert_eq!(run(&["check", "proj"], &root).0, 0);
    assert_eq!(run(&["render", "proj", "-o", "out.mp4"], &root).0, 0);
    let raw = decode_raw(&root.join("out.mp4"), 60);
    let cx = |k: usize| red_centroid_x(frame_of(&raw, k, 160, 120), 160).unwrap();
    assert!(
        (cx(15) - 80.0).abs() < 3.0,
        "first chain midpoints, got {}",
        cx(15)
    );
    assert!(
        (cx(30) - 30.0).abs() < 3.0,
        "second chain restarts, got {}",
        cx(30)
    );
    assert!(
        (cx(45) - 80.0).abs() < 3.0,
        "second chain midpoints, got {}",
        cx(45)
    );
    // Frame 59 runs 29/30 through the second chain: 30 + 290/3.
    let end = 30.0 + 290.0 / 3.0;
    assert!(
        (cx(59) - end).abs() < 3.0,
        "chain end nears, got {}",
        cx(59)
    );
}

#[test]
fn stagger_overflow_clamps_and_reports_in_check() {
    // T009: four split pieces, motion 0..2, stagger 0.5 from the
    // start: the last piece runs to 3.5s, 1.5s past the 2s window.
    // Check stays green and reports the cut with the numbers.
    let root = tmp("clamp");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    write(
        &folder,
        "composition.kdl",
        &format!(
            "{HEAD}    text value=\"ABCD\" start=0 duration=2 split=\"chars\"\n    tween target=0 property=\"x\" kind=\"to\" start=0 duration=2 to=10 stagger=0.5\n}}\n"
        ),
    );
    let (status, out) = run(&["check", "proj"], &root);
    assert_eq!(status, 0, "clamping stays a valid verdict: {out}");
    assert!(
        out.contains("note: tween target=0"),
        "clamp names the directive: {out}"
    );
    assert!(out.contains("4 pieces"), "clamp counts the pieces: {out}");
    assert!(
        out.contains("overflow 1.500s"),
        "clamp measures the cut: {out}"
    );
    assert!(
        out.contains("clamped to the clip window"),
        "clamp states the cut: {out}"
    );
    let (jstatus, json) = run(&["check", "--json", "proj"], &root);
    assert_eq!(jstatus, 0, "json check stays green: {json}");
    assert!(
        json.contains("\"stagger_clamps\":[{"),
        "json carries the clamp: {json}"
    );
    assert!(
        json.contains("\"overflow\":1.5"),
        "json measures the cut: {json}"
    );
}

#[test]
fn stagger_and_repeat_refusals_fail_before_frame_one() {
    // T004 + T009: every invalid variant fails check and render with
    // its bound, and render writes no MP4.
    let root = tmp("refuse");
    let cases = [
        ("endless", "    tween target=0 property=\"x\" kind=\"to\" start=0 duration=1 to=10 repeat=0\n"),
        ("over-limit", "    tween target=0 property=\"x\" kind=\"to\" start=0 duration=1 to=10 repeat=1001\n"),
        (
            "unbounded",
            "    tween target=0 property=\"x\" kind=\"to\" start=0 duration=1 to=10 repeat=\"infinite\"\n",
        ),
        ("neg-stagger", "    tween target=0 property=\"x\" kind=\"to\" start=0 duration=1 to=10 stagger=-0.5\n"),
        (
            "bad-origin",
            "    tween target=0 property=\"x\" kind=\"to\" start=0 duration=1 to=10 stagger=0.5 stagger-from=\"middle\"\n",
        ),
        (
            "neg-gap",
            "    key target=0 property=\"x\" repeat-delay=-1 {\n        frame at=0 value=10\n        frame at=1 value=110\n    }\n",
        ),
    ];
    for (tag, motion) in cases {
        let folder = root.join(tag);
        std::fs::create_dir_all(&folder).unwrap();
        write(
            &folder,
            "composition.kdl",
            &format!("{HEAD}{BOX}{motion}}}\n"),
        );
        let (status, out) = run(&["check", "."], &folder);
        assert_eq!(status, 1, "{tag}: check must exit 1, got {status}: {out}");
        assert!(
            out.contains("E_BAD_MOTION"),
            "{tag}: diagnostics must name E_BAD_MOTION: {out}"
        );
        let (rstatus, _) = run(&["render", ".", "-o", "out.mp4"], &folder);
        assert_eq!(rstatus, 1, "{tag}: render must exit 1");
        assert!(!folder.join("out.mp4").exists(), "{tag}: no MP4 on refusal");
    }
}

#[test]
fn default_repeat_and_stagger_render_byte_identically() {
    // Regression: explicit defaults (repeat 1, stagger 0, yoyo
    // false, no gap) render exactly like the bare tween — the
    // pre-Sprint-7 pixels pinned, byte-identical bytes.
    let root = tmp("defaults");
    let bare = root.join("bare");
    let full = root.join("full");
    std::fs::create_dir_all(&bare).unwrap();
    std::fs::create_dir_all(&full).unwrap();
    write(
        &bare,
        "composition.kdl",
        &format!(
            "{HEAD}{BOX}    tween target=0 property=\"x\" kind=\"to\" start=0 duration=1 to=110 ease=\"in-quad\"\n}}\n"
        ),
    );
    write(
        &full,
        "composition.kdl",
        &format!(
            "{HEAD}{BOX}    tween target=0 property=\"x\" kind=\"to\" start=0 duration=1 to=110 ease=\"in-quad\" stagger=0 stagger-from=\"start\" repeat=1 yoyo=false repeat-delay=0\n}}\n"
        ),
    );
    assert_eq!(run(&["check", "."], &bare).0, 0);
    assert_eq!(run(&["check", "."], &full).0, 0);
    assert_eq!(run(&["render", ".", "-o", "out.mp4"], &bare).0, 0);
    assert_eq!(run(&["render", ".", "-o", "out.mp4"], &full).0, 0);
    assert_eq!(
        std::fs::read(bare.join("out.mp4")).unwrap(),
        std::fs::read(full.join("out.mp4")).unwrap(),
        "default repeat/stagger render byte-identically to the bare tween"
    );
    let raw = decode_raw(&full.join("out.mp4"), 60);
    let cx = |k: usize| red_centroid_x(frame_of(&raw, k, 160, 120), 160).unwrap();
    assert!(
        (cx(15) - 55.0).abs() < 3.0,
        "in-quad midpoint pins, got {}",
        cx(15)
    );
    assert!(
        (cx(30) - 130.0).abs() < 3.0,
        "tween end pins, got {}",
        cx(30)
    );
}
