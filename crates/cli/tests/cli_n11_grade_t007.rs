//! Sprint 11 motion blur (T007) end-to-end: a composition
//! `motion-blur` element smears moving content along its motion scaled
//! by the shutter angle, still content under `auto` matches blur off,
//! the timeline keeps its frame count, and every refusal variant fails
//! `check` naming `motion-blur` and the accepted shape.
//!
//! The MP4 leg proves the validate-then-render seam: pixel assertions
//! stay tolerant of H.264 (whole-frame means, not exact bytes), while
//! still-vs-blurred verdicts compare decoded frames, so they prove
//! the blur, not the codec.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-blur-e2e-{tag}-{}-{}",
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
    let root = tmp("blur");
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

const MOVING: &str = "composition name=\"main\" root=true width=160 height=120 duration=2 background=\"#000\" version=1 {\n box x=10 y=40 width=40 height=40 fill=\"#ff0000\" start=0 duration=2\n tween target=0 property=\"x\" kind=\"to\" start=0 duration=1 to=110 ease=\"linear\"\n motion-blur shutter=180 samples=8 tier=\"high\"\n}\n";

#[test]
fn shutter_blur_smears_motion_and_keeps_frame_count() {
    // The blurred timeline decodes to exactly its 60 frames, and the
    // mid-motion frame visibly smears past the sharp render of the
    // same composition without `motion-blur`.
    let (_root, proj) = check_and_render(MOVING);
    let blurred_all = decode_all(&proj.join("out.mp4"));
    assert_eq!(blurred_all.len(), 60 * 160 * 120 * 3);

    let plain = MOVING.replace("\n motion-blur shutter=180 samples=8 tier=\"high\"", "");
    let (_root, proj) = check_and_render(&plain);
    let sharp_all = decode_all(&proj.join("out.mp4"));
    assert_eq!(sharp_all.len(), blurred_all.len());

    // Frame 15 (t=0.5s, box mid-travel): the smear moves whole-frame
    // means well past codec noise.
    let w = 160 * 120 * 3;
    let (sharp_mid, blurred_mid) = (&sharp_all[15 * w..16 * w], &blurred_all[15 * w..16 * w]);
    assert!(
        frame_mad(sharp_mid, blurred_mid) > 0.25,
        "mid-motion frame must smear, mad={}",
        frame_mad(sharp_mid, blurred_mid)
    );
    // A background corner far from the box stays black in both.
    let o = (110 * 160 + 150) * 3;
    assert!(sharp_mid[o] <= 24 && blurred_mid[o] <= 24);
}

#[test]
fn still_auto_matches_blur_off() {
    // No tween: nothing moves, so `auto` renders the sharp frame and
    // decodes within codec noise of blur off.
    let still = MOVING.replace(
        "\n tween target=0 property=\"x\" kind=\"to\" start=0 duration=1 to=110 ease=\"linear\"",
        "",
    );
    let auto = still.replace("tier=\"high\"", "tier=\"auto\"");
    let plain = still.replace("\n motion-blur shutter=180 samples=8 tier=\"high\"", "");
    let (_root, proj) = check_and_render(&auto);
    let auto_first = output_frame(&proj.join("out.mp4"), 0, 160, 120);
    let (_root, proj) = check_and_render(&plain);
    let sharp_first = output_frame(&proj.join("out.mp4"), 0, 160, 120);
    assert!(
        frame_mad(&auto_first, &sharp_first) < 0.5,
        "still auto must match blur off, mad={}",
        frame_mad(&auto_first, &sharp_first)
    );
}

#[test]
fn check_refuses_shutter_samples_and_tier() {
    let bad = MOVING.replace("shutter=180", "shutter=400");
    let (root, _proj) = stage(&bad);
    let (code, out) = run(&["check", "proj"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_BAD_BLUR"), "{out}");
    assert!(
        out.contains("motion-blur") && out.contains("shutter"),
        "{out}"
    );
    assert!(out.contains("0 to 360"), "{out}");

    let bad = MOVING.replace("samples=8", "samples=64");
    let (root, _proj) = stage(&bad);
    let (code, out) = run(&["check", "proj"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_BAD_BLUR"), "{out}");
    assert!(
        out.contains("motion-blur") && out.contains("samples"),
        "{out}"
    );
    assert!(out.contains("1 to 32"), "{out}");

    let bad = MOVING.replace("tier=\"high\"", "tier=\"ultra\"");
    let (root, _proj) = stage(&bad);
    let (code, out) = run(&["check", "proj"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_BAD_BLUR"), "{out}");
    assert!(
        out.contains("motion-blur") && out.contains("ultra"),
        "{out}"
    );
    assert!(out.contains("auto"), "{out}");
}

#[test]
fn check_refuses_duplicate_motion_blur() {
    let bad = MOVING.replace(
        "\n motion-blur shutter=180 samples=8 tier=\"high\"",
        "\n motion-blur shutter=180 samples=8 tier=\"high\"\n motion-blur shutter=90 samples=4 tier=\"low\"",
    );
    let (root, _proj) = stage(&bad);
    let (code, out) = run(&["check", "proj"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_BAD_BLUR"), "{out}");
    assert!(out.contains("motion-blur"), "{out}");
    assert!(out.contains("at most one"), "{out}");
}
