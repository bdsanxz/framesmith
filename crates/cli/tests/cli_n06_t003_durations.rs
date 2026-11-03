//! Sprint 6 T003: the final instant and omitted-duration inference,
//! end to end.
//!
//! A clip ending exactly at the composition end shows on the final
//! rendered frame; omitted audio durations render the source length
//! at the declared rate; omitted still durations render until the
//! composition end; omitted composition durations end at the latest
//! finite clip end; every inferred length echoes in `check` (human
//! and `--json`) before rendering; lengths below 1/60s and unbounded
//! clips (loop, open nest) fail before frame one; and Sprint 5 video
//! inference renders exactly as before (written vs inferred lengths
//! are byte-identical).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-n06t003-{tag}-{}-{}",
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

/// One raw RGB frame `n` of `mp4` as bytes.
fn raw_frame(mp4: &Path, n: u32, w: u32, h: u32) -> Vec<u8> {
    let out = Command::new("ffmpeg")
        .args(["-y", "-v", "error", "-i"])
        .arg(mp4)
        .args([
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
        .expect("extract frame");
    assert!(out.status.success(), "frame {n} extracts");
    assert_eq!(out.stdout.len(), (w * h * 3) as usize, "frame {n} is whole");
    out.stdout
}

fn center_pixel(frame: &[u8], w: u32, h: u32) -> (u8, u8, u8) {
    let at = ((h / 2 * w + w / 2) * 3) as usize;
    (frame[at], frame[at + 1], frame[at + 2])
}

fn gen_clip(dir: &Path, seconds: f64) {
    ffmpeg(&[
        "-f",
        "lavfi",
        "-i",
        &format!("testsrc=duration={seconds}:size=64x48:rate=30"),
        "-c:v",
        "libx264",
        "-pix_fmt",
        "yuv420p",
        &dir.join("clip.mp4").display().to_string(),
    ]);
}

fn gen_tone(dir: &Path, name: &str, seconds: f64) {
    ffmpeg(&[
        "-f",
        "lavfi",
        "-i",
        &format!("sine=frequency=440:duration={seconds}:sample_rate=48000"),
        "-c:a",
        "pcm_s16le",
        &dir.join(name).display().to_string(),
    ]);
}

fn write(folder: &Path, name: &str, content: &str) {
    std::fs::write(folder.join(name), content).unwrap();
}

/// A clip ending exactly at the composition end is visible on the
/// final rendered frame — both a full-length box and a box that only
/// covers the second half.
#[test]
fn boundary_clip_shows_on_final_frame() {
    for (tag, clip) in [
        ("full", "box x=0 y=0 width=64 height=48 fill=\"#ff0000\" start=0 duration=2"),
        ("half", "box x=0 y=0 width=64 height=48 fill=\"#ff0000\" start=1 duration=1"),
    ] {
        let folder = tmp(&format!("final-{tag}"));
        write(
            &folder,
            "composition.kdl",
            &format!(
                "composition name=\"t\" width=64 height=48 duration=2 fps=30 background=\"#000000\" version=1 {{\n    {clip}\n}}\n"
            ),
        );
        let (code, out) = run(&["check", "."], &folder);
        assert_eq!(code, 0, "{tag}: {out}");
        let (code, out) = run(&["render", ".", "-o", "out.mp4"], &folder);
        assert_eq!(code, 0, "{tag}: {out}");
        let frame = raw_frame(&folder.join("out.mp4"), 59, 64, 48);
        let (r, g, b) = center_pixel(&frame, 64, 48);
        assert!(
            r > 200 && g < 100 && b < 100,
            "{tag}: final frame is the red box, got ({r},{g},{b})"
        );
    }
}

/// An omitted still duration renders until the composition end, and
/// the inferred length echoes in `check` before rendering.
#[test]
fn omitted_still_renders_until_composition_end() {
    let folder = tmp("still");
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=64 height=48 duration=2 fps=30 background=\"#000000\" version=1 {\n    box x=0 y=0 width=64 height=48 fill=\"#ff0000\" start=0\n}\n",
    );
    let (code, out) = run(&["check", "."], &folder);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("inferred box"), "{out}");
    assert!(out.contains("clip duration 2.000s (inferred until composition end 2.000s)"), "{out}");
    let (code, out) = run(&["check", "--json", "."], &folder);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("\"kind\":\"box\""), "{out}");
    assert!(out.contains("\"duration\":2"), "{out}");
    let (code, out) = run(&["render", ".", "-o", "out.mp4"], &folder);
    assert_eq!(code, 0, "{out}");
    let frame = raw_frame(&folder.join("out.mp4"), 59, 64, 48);
    let (r, g, b) = center_pixel(&frame, 64, 48);
    assert!(r > 200 && g < 100 && b < 100, "still holds to the end, got ({r},{g},{b})");
}

/// An omitted audio duration renders the source length at the
/// declared rate, and the echoed length matches the rendered length.
#[test]
fn omitted_audio_renders_source_length_at_rate() {
    for (tag, rate, expect) in [("unity", "1", 3.0), ("double", "2", 1.5)] {
        let folder = tmp(&format!("audio-{tag}"));
        gen_tone(&folder, "s.wav", 3.0);
        write(
            &folder,
            "composition.kdl",
            &format!(
                "composition name=\"t\" width=64 height=48 duration={expect} fps=30 background=\"#000000\" version=1 {{\n    audio path=\"s.wav\" start=0 rate={rate}\n}}\n"
            ),
        );
        let (code, out) = run(&["check", "."], &folder);
        assert_eq!(code, 0, "{tag}: {out}");
        assert!(
            out.contains(&format!("clip duration {expect:.3}s (inferred from source window")),
            "{tag}: {out}"
        );
        assert!(out.contains(&format!("rate {rate}x")), "{tag}: {out}");
        let (code, out) = run(&["render", ".", "-o", "out.mp4"], &folder);
        assert_eq!(code, 0, "{tag}: {out}");
        let rendered = probe_duration(&folder.join("out.mp4"));
        assert!((rendered - expect).abs() < 0.2, "{tag}: got {rendered}");
    }
}

/// An omitted composition duration ends at the latest finite clip
/// end, relative chains included, and echoes for review.
#[test]
fn omitted_composition_ends_at_latest_finite_clip_end() {
    let folder = tmp("comp");
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=64 height=48 fps=30 background=\"#000000\" version=1 {\n    text value=\"A\" start=0 duration=1 id=\"a\"\n    box x=0 y=0 width=8 height=8 fill=\"#ff0000\" id=\"b\" start-after=\"a\" duration=1.5\n}\n",
    );
    let (code, out) = run(&["check", "."], &folder);
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains("inferred composition \"t\" (1:1): duration 2.500s (inferred until the latest finite clip end)"),
        "{out}"
    );
    let (code, out) = run(&["render", ".", "-o", "out.mp4"], &folder);
    assert_eq!(code, 0, "{out}");
    let rendered = probe_duration(&folder.join("out.mp4"));
    assert!((rendered - 2.5).abs() < 0.2, "composition ends at 2.5, got {rendered}");
}

/// A written or resolved length below 1/60s fails naming the value
/// and the bound, before frame one, with no MP4 left behind.
#[test]
fn tiny_length_fails_naming_value_and_bound() {
    let folder = tmp("tiny");
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=64 height=48 duration=2 fps=30 background=\"#000000\" version=1 {\n    text value=\"Hi\" start=0 duration=0.01\n}\n",
    );
    let (code, out) = run(&["check", "."], &folder);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_BAD_TIMING"), "{out}");
    assert!(out.contains("0.01"), "{out}");
    assert!(out.contains("1/60"), "{out}");
    let (code, out) = run(&["render", ".", "-o", "out.mp4"], &folder);
    assert_eq!(code, 1, "{out}");
    assert!(!folder.join("out.mp4").exists(), "no MP4 survives the refusal");
    assert!(!folder.join("out.mp4.framesmith-part").exists(), "no partial survives");
}

/// Unbounded clips — a looping video and an open nest without
/// `duration` — fail until the length is written.
#[test]
fn unbounded_clips_fail_until_duration_is_written() {
    let folder = tmp("loop");
    gen_clip(&folder, 2.0);
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=64 height=48 duration=2 fps=30 background=\"#000000\" version=1 {\n    video path=\"clip.mp4\" start=0 loop=true\n}\n",
    );
    let (code, out) = run(&["check", "."], &folder);
    assert_eq!(code, 1, "loop: {out}");
    assert!(out.contains("E_BAD_FOOTAGE"), "loop: {out}");
    assert!(out.contains("duration"), "loop: {out}");

    let folder = tmp("nest");
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=64 height=48 duration=2 fps=30 background=\"#000000\" version=1 {\n    nest nest=\"t\" start=0\n}\n",
    );
    let (code, out) = run(&["check", "."], &folder);
    assert_eq!(code, 1, "nest: {out}");
    assert!(out.contains("E_BAD_TIMING"), "nest: {out}");
    assert!(out.contains("unbounded"), "nest: {out}");
    let (code, out) = run(&["render", ".", "-o", "out.mp4"], &folder);
    assert_eq!(code, 1, "nest render: {out}");
    assert!(!folder.join("out.mp4").exists(), "no MP4 survives the refusal");
}

/// Sprint 5 video inference renders exactly as before: a written
/// length equal to the inferred one produces a byte-identical MP4.
#[test]
fn video_inference_regression_renders_byte_identically() {
    let mk = |tag: &str, clip: &str| {
        let folder = tmp(tag);
        gen_clip(&folder, 4.0);
        write(
            &folder,
            "composition.kdl",
            &format!(
                "composition name=\"t\" width=64 height=48 duration=2 fps=30 background=\"#000000\" version=1 {{\n    {clip}\n}}\n"
            ),
        );
        folder
    };
    // Same source window [1, 3): once with the length written, once
    // inferred — the renders must be byte-identical.
    let written = mk(
        "vid-written",
        "video path=\"clip.mp4\" start=0 duration=2 trim-start=1 trim-end=3",
    );
    let inferred = mk("vid-inferred", "video path=\"clip.mp4\" start=0 trim-start=1 trim-end=3");
    let (code, out) = run(&["check", "."], &inferred);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("clip duration 2.000s (inferred)"), "{out}");
    let (code, out) = run(&["render", ".", "-o", "out.mp4"], &written);
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(&["render", ".", "-o", "out.mp4"], &inferred);
    assert_eq!(code, 0, "{out}");
    let a = std::fs::read(written.join("out.mp4")).unwrap();
    let b = std::fs::read(inferred.join("out.mp4")).unwrap();
    assert_eq!(a, b, "written and inferred lengths render byte-identically");
}
