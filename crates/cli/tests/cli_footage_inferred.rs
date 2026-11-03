//! Sprint 5 T009 (omit duration and render the inferred length).
//!
//! A `video` clip without `duration` renders exactly
//! `(trim-end - trim-start) / rate` composition seconds, using the
//! probed source end for an omitted `trim-end`. `check` (including
//! `--json`) shows that inferred length for review before rendering.
//! Looping clips and infinitely looping GIFs without `duration` are
//! refused until `duration` is written (covered by the loop-hold test
//! and the GIF test); explicit `duration` clips render exactly as
//! before.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-footage-inferred-{tag}-{}-{}",
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
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse()
        .expect("duration parses")
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

fn write(folder: &Path, name: &str, content: &str) {
    std::fs::write(folder.join(name), content).unwrap();
}

fn comp(duration: f64, clip_line: &str) -> String {
    format!(
        "composition name=\"t\" width=64 height=48 duration={duration} fps=30 background=\"#000000\" version=1 {{\n    {clip_line}\n}}\n"
    )
}

/// An omitted `duration` renders exactly the inferred length for
/// plain, trimmed, and re-timed clips — and the length is reviewable
/// in `check` human and `--json` output before rendering.
#[test]
fn omitted_duration_renders_inferred_length() {
    for (tag, clip, comp_len, inferred) in [
        ("plain", "video path=\"clip.mp4\" start=0", 4.0, 4.0),
        (
            "trim",
            "video path=\"clip.mp4\" start=0 trim-start=1 trim-end=3",
            2.0,
            2.0,
        ),
        ("rate", "video path=\"clip.mp4\" start=0 rate=2", 2.0, 2.0),
    ] {
        let folder = tmp(tag);
        gen_clip(&folder, 4.0);
        let src = probe_duration(&folder.join("clip.mp4"));
        assert!((src - 4.0).abs() < 0.05, "fixture is about 4s, got {src}");
        write(&folder, "composition.kdl", &comp(comp_len, clip));
        let (code, out) = run(&["check", "."], &folder);
        assert_eq!(code, 0, "{tag}: {out}");
        assert!(
            out.contains(&format!("clip duration {inferred:.3}s (inferred)")),
            "{tag}: {out}"
        );
        assert!(
            out.contains(&format!("effective length {inferred:.3}s")),
            "{tag}: {out}"
        );
        let (code, out) = run(&["check", "--json", "."], &folder);
        assert_eq!(code, 0, "{tag}: {out}");
        assert!(out.contains("\"status\":\"valid\""), "{tag}: {out}");
        assert!(out.contains("\"inferred\":true"), "{tag}: {out}");
        assert!(
            out.contains(&format!("\"duration\":{inferred}")),
            "{tag}: {out}"
        );
        assert!(
            out.contains(&format!("\"effective_duration\":{inferred}")),
            "{tag}: {out}"
        );
        let (code, out) = run(&["render", ".", "-o", "out.mp4"], &folder);
        assert_eq!(code, 0, "{tag}: {out}");
        assert!(
            out.contains("(inferred)"),
            "{tag}: render announces the inference:\n{out}"
        );
        let rendered = probe_duration(&folder.join("out.mp4"));
        assert!(
            (rendered - inferred).abs() < 0.15,
            "{tag}: renders exactly the inferred length, got {rendered}"
        );
    }
}

/// An explicit `duration` renders exactly as before: inference changes
/// nothing when the length is written, and the output carries no
/// inference marking.
#[test]
fn explicit_duration_renders_unchanged() {
    let folder = tmp("explicit");
    gen_clip(&folder, 4.0);
    write(
        &folder,
        "composition.kdl",
        &comp(1.0, "video path=\"clip.mp4\" start=0 duration=1"),
    );
    let (code, out) = run(&["check", "."], &folder);
    assert_eq!(code, 0, "{out}");
    assert!(!out.contains("(inferred)"), "{out}");
    assert!(out.contains("clip duration 1.000s"), "{out}");
    let (code, out) = run(&["check", "--json", "."], &folder);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("\"inferred\":false"), "{out}");
    assert!(out.contains("\"duration\":1"), "{out}");
    let (code, out) = run(&["render", ".", "-o", "out.mp4"], &folder);
    assert_eq!(code, 0, "{out}");
    let rendered = probe_duration(&folder.join("out.mp4"));
    assert!(
        (rendered - 1.0).abs() < 0.15,
        "explicit length renders, got {rendered}"
    );
}

/// An omitted `duration` with no knowable source end (an unprobable
/// source, or a still with no declared `trim-end`) fails asking for an
/// explicit `duration` — never a guessed length.
#[test]
fn unknown_end_without_duration_fails_asking_for_duration() {
    for tag in ["unprobable", "still"] {
        let folder = tmp(tag);
        if tag == "unprobable" {
            std::fs::write(folder.join("clip.mp4"), b"fake").unwrap();
        } else {
            // Minimal HEIC head: a still has no timeline, so its end is
            // unknown. (A real sips HEIC is exercised in the GIF/HEIC
            // test; the head bytes are enough for the check-time
            // refusal, which never decodes.)
            let mut heic = vec![0u8; 4];
            heic.extend_from_slice(b"ftypheic");
            heic.extend_from_slice(&[0, 0, 0, 0, b'm', b'i', b'f', b'1']);
            std::fs::write(folder.join("still.heic"), &heic).unwrap();
        }
        let clip = if tag == "unprobable" {
            "video path=\"clip.mp4\" start=0"
        } else {
            "video path=\"still.heic\" start=0"
        };
        write(&folder, "composition.kdl", &comp(2.0, clip));
        let (code, out) = run(&["check", "."], &folder);
        assert_eq!(code, 1, "{tag}: {out}");
        assert!(out.contains("E_BAD_FOOTAGE"), "{tag}: {out}");
        assert!(out.contains("duration"), "{tag}: {out}");
        let (code, out) = run(&["check", "--json", "."], &folder);
        assert_eq!(code, 1, "{tag}: {out}");
        assert!(out.contains("\"code\":\"E_BAD_FOOTAGE\""), "{tag}: {out}");
        assert!(
            !folder.join("out.mp4").exists(),
            "{tag}: check writes no MP4"
        );
        let (code, out) = run(&["render", ".", "-o", "out.mp4"], &folder);
        assert_eq!(code, 1, "{tag}: {out}");
        assert!(
            !folder.join("out.mp4").exists(),
            "{tag}: no MP4 survives the refusal"
        );
        assert!(
            !folder.join("out.mp4.framesmith-part").exists(),
            "{tag}: no partial survives"
        );
    }
}

/// An inferred-length render is deterministic: rendering twice yields
/// byte-identical MP4s.
#[test]
fn inferred_render_reruns_byte_identically() {
    let folder = tmp("rerun");
    gen_clip(&folder, 4.0);
    write(
        &folder,
        "composition.kdl",
        &comp(
            1.0,
            "video path=\"clip.mp4\" start=0 trim-start=1 trim-end=3 rate=2",
        ),
    );
    let (code, out) = run(&["check", "."], &folder);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("clip duration 1.000s (inferred)"), "{out}");
    let (code, out) = run(&["render", ".", "-o", "out.mp4"], &folder);
    assert_eq!(code, 0, "{out}");
    let first = std::fs::read(folder.join("out.mp4")).unwrap();
    let (code, out) = run(&["render", ".", "-o", "out.mp4"], &folder);
    assert_eq!(code, 0, "{out}");
    let second = std::fs::read(folder.join("out.mp4")).unwrap();
    assert_eq!(first, second, "inferred render re-renders byte-identically");
    let rendered = probe_duration(&folder.join("out.mp4"));
    assert!(
        (rendered - 1.0).abs() < 0.15,
        "window 2s at rate 2 infers 1s, got {rendered}"
    );
}
