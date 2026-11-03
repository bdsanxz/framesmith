//! Sprint 6 T008 end-to-end: `render --start/--end` sub-range MP4s.
//!
//! A 4 s composition (red `[0, 2)`, blue `[2, 4)`, tone in `[1, 3)`)
//! renders whole and as the `[1, 3)` standalone range: the range holds
//! its own 60 frames with picture and sound aligned to the full
//! render, re-renders byte-identically, and every empty, backward, or
//! out-of-range window refuses naming the accepted window with no
//! partial MP4. A named composition renders its own ranged content.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-n06t008-{tag}-{}-{}",
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
    assert!(out.status.success(), "host ffmpeg failed");
}

fn write(folder: &Path, name: &str, content: &str) {
    std::fs::create_dir_all(folder).unwrap();
    std::fs::write(folder.join(name), content).unwrap();
}

fn gen_tone(path: &Path, seconds: u32) {
    ffmpeg(&[
        "-f",
        "lavfi",
        "-i",
        &format!("sine=frequency=440:duration={seconds}"),
        "-ar",
        "48000",
        "-ac",
        "2",
        &path.display().to_string(),
    ]);
}

fn decode_video(mp4: &Path, w: u32, h: u32) -> Vec<u8> {
    let out = Command::new("ffmpeg")
        .args([
            "-y",
            "-v",
            "error",
            "-i",
            &mp4.display().to_string(),
            "-map",
            "0:v",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgb24",
            "-s",
            &format!("{w}x{h}"),
            "-",
        ])
        .output()
        .expect("decode mp4 with host ffmpeg");
    assert!(out.status.success());
    out.stdout
}

fn decode_audio(mp4: &Path) -> Vec<i16> {
    let out = Command::new("ffmpeg")
        .args([
            "-y",
            "-v",
            "error",
            "-i",
            &mp4.display().to_string(),
            "-map",
            "0:a",
            "-f",
            "s16le",
            "-ac",
            "2",
            "-ar",
            "48000",
            "-",
        ])
        .output()
        .expect("decode audio with host ffmpeg");
    assert!(out.status.success());
    let bytes = out.stdout;
    assert!(bytes.len() % 2 == 0);
    bytes
        .chunks_exact(2)
        .map(|c| i16::from_le_bytes([c[0], c[1]]))
        .collect()
}

fn center(frames: &[u8], frame: usize, w: u32, h: u32) -> (u8, u8, u8) {
    let stride = (w * h * 3) as usize;
    let at = frame * stride + ((h / 2 * w + w / 2) * 3) as usize;
    (frames[at], frames[at + 1], frames[at + 2])
}

fn is_red(p: (u8, u8, u8)) -> bool {
    p.0 > 180 && p.1 < 100 && p.2 < 100
}

fn is_blue(p: (u8, u8, u8)) -> bool {
    p.2 > 180 && p.0 < 100 && p.1 < 100
}

fn rms(samples: &[i16]) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum: f64 = samples.iter().map(|&x| (x as f64) * (x as f64)).sum();
    (sum / samples.len() as f64).sqrt()
}

fn ffprobe_duration(mp4: &Path) -> f64 {
    let out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "csv=p=0",
        ])
        .arg(mp4)
        .output()
        .expect("run host ffprobe");
    assert!(out.status.success());
    String::from_utf8_lossy(&out.stdout).trim().parse().unwrap()
}

// Red in [0, 2), blue in [2, 4), tone in [1, 3): the range [1, 3)
// straddles both the picture cut and the sound edges.
const COMP: &str = "composition name=\"main\" width=64 height=64 duration=4 fps=30 background=\"#000000\" version=1 {\n\
     box x=0 y=0 width=64 height=64 fill=\"#ff0000\" start=0 duration=2\n\
     box x=0 y=0 width=64 height=64 fill=\"#0000ff\" start=2 duration=2\n\
     audio path=\"tone.wav\" start=1 duration=2\n\
     }\n";

fn project(root: &Path) -> PathBuf {
    let p = root.join("p");
    write(&p, "composition.kdl", COMP);
    gen_tone(&p.join("tone.wav"), 2);
    p
}

// --- alignment -----------------------------------------------------------------

#[test]
fn sub_range_aligns_picture_and_sound_with_the_full_render() {
    let root = tmp("align");
    project(&root);
    let (code, out) = run(&["render", "p", "-o", "full.mp4"], &root);
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(
        &[
            "render",
            "p",
            "-o",
            "range.mp4",
            "--start",
            "1",
            "--end",
            "3",
        ],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains("[range 1.000s..3.000s]"),
        "success names the range: {out}"
    );

    // Picture: 60 frames of its own; the first half shows comp red, the
    // second half comp blue — the cut lands exactly mid-range.
    let full = decode_video(&root.join("full.mp4"), 64, 64);
    assert_eq!(
        full.len(),
        120 * 64 * 64 * 3,
        "full render holds 120 frames"
    );
    let ranged = decode_video(&root.join("range.mp4"), 64, 64);
    assert_eq!(
        ranged.len(),
        60 * 64 * 64 * 3,
        "range holds its own 60 frames"
    );
    assert!(
        is_red(center(&ranged, 0, 64, 64)),
        "range frame 0 is comp t=1 red"
    );
    assert!(
        is_red(center(&ranged, 29, 64, 64)),
        "range frame 29 is comp t=1.967 red"
    );
    assert!(
        is_blue(center(&ranged, 30, 64, 64)),
        "range frame 30 is comp t=2 blue"
    );
    assert!(
        is_blue(center(&ranged, 59, 64, 64)),
        "range frame 59 is comp t=2.967 blue"
    );

    // Sound: the tone fills the whole range (it spans comp [1, 3)),
    // while the full render starts with a silent second.
    let full_mix = decode_audio(&root.join("full.mp4"));
    assert!(
        rms(&full_mix[..48000 * 2]) < 50.0,
        "full render starts silent"
    );
    assert!(
        rms(&full_mix[48000 * 2..144000 * 2]) > 200.0,
        "full render tones in [1, 3)"
    );
    let range_mix = decode_audio(&root.join("range.mp4"));
    assert!(rms(&range_mix) > 200.0, "range mix tones throughout");
    assert!(
        (ffprobe_duration(&root.join("range.mp4")) - 2.0).abs() < 0.15,
        "range media lasts 2 s"
    );
}

#[test]
fn sub_range_rerenders_byte_identically() {
    let root = tmp("rerun");
    project(&root);
    let (code, out) = run(
        &["render", "p", "-o", "a.mp4", "--start", "1", "--end", "3"],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(
        &["render", "p", "-o", "b.mp4", "--start", "1", "--end", "3"],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    let (a, b) = (
        std::fs::read(root.join("a.mp4")).unwrap(),
        std::fs::read(root.join("b.mp4")).unwrap(),
    );
    assert!(
        !a.is_empty() && a == b,
        "same range re-renders byte-identically"
    );
}

// --- refusals ------------------------------------------------------------------

fn refused(root: &Path, extra: &[&str], needle: &str, out_name: &str) {
    project(root);
    let mut args = vec!["render", "p", "-o", out_name];
    args.extend(extra);
    let (code, text) = run(&args, root);
    assert_eq!(code, 1, "refusal exits 1: {text}");
    assert!(text.contains(needle), "refusal names the cause: {text}");
    assert!(
        text.contains("accepted window is 0 <= start < end <= 4.000s"),
        "refusal names the accepted window: {text}"
    );
    assert!(
        !root.join(out_name).exists(),
        "no partial MP4 remains after refusal"
    );
    assert!(
        !root.join("p").join("out.mp4").exists(),
        "no default MP4 is written either"
    );
}

#[test]
fn empty_window_refuses() {
    let root = tmp("empty");
    refused(&root, &["--start", "2", "--end", "2"], "empty", "empty.mp4");
}

#[test]
fn backward_window_refuses() {
    let root = tmp("backward");
    refused(
        &root,
        &["--start", "3", "--end", "1"],
        "backward",
        "backward.mp4",
    );
}

#[test]
fn out_of_range_windows_refuse() {
    let root = tmp("oor-end");
    refused(
        &root,
        &["--end", "9"],
        "outside the composition",
        "oor1.mp4",
    );
    let root = tmp("oor-start");
    refused(
        &root,
        &["--start", "-1"],
        "outside the composition",
        "oor2.mp4",
    );
    let root = tmp("oor-both");
    refused(
        &root,
        &["--start", "5", "--end", "9"],
        "outside the composition",
        "oor3.mp4",
    );
}

#[test]
fn non_numeric_bound_refuses_with_exit_1_and_no_mp4() {
    let root = tmp("nan");
    project(&root);
    let (code, text) = run(&["render", "p", "-o", "nan.mp4", "--start", "abc"], &root);
    assert_eq!(code, 1, "{text}");
    assert!(text.contains("not a number of seconds"), "{text}");
    assert!(!root.join("nan.mp4").exists());
}

#[test]
fn flag_misuse_exits_2() {
    let root = tmp("misuse");
    project(&root);
    let (code, _) = run(&["render", "p", "-o", "x.mp4", "--start"], &root);
    assert_eq!(code, 2, "missing --start value exits 2");
    let (code, _) = run(
        &["render", "p", "-o", "x.mp4", "--start", "1", "--start", "2"],
        &root,
    );
    assert_eq!(code, 2, "duplicate --start exits 2");
    assert!(!root.join("x.mp4").exists());
}

// --- named compositions ----------------------------------------------------------

#[test]
fn named_composition_sub_range_renders_its_own_content() {
    let root = tmp("named");
    let p = root.join("p");
    write(
        &p,
        "composition.kdl",
        "composition name=\"main\" root=true width=64 height=64 duration=4 fps=30 background=\"#000000\" version=1 {\n\
         box x=0 y=0 width=64 height=64 fill=\"#ff0000\" start=0 duration=4\n\
         }\n\
         composition name=\"cut\" width=64 height=64 duration=2 fps=30 background=\"#000000\" version=1 {\n\
         box x=0 y=0 width=64 height=64 fill=\"#00ff00\" start=0 duration=2\n\
         }\n",
    );
    let (code, out) = run(
        &[
            "render",
            "p",
            "-o",
            "cut.mp4",
            "--composition",
            "cut",
            "--start",
            "0.5",
            "--end",
            "1.5",
        ],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    let frames = decode_video(&root.join("cut.mp4"), 64, 64);
    assert_eq!(
        frames.len(),
        30 * 64 * 64 * 3,
        "range holds its own 30 frames"
    );
    let (r, g, b) = center(&frames, 0, 64, 64);
    assert!(
        g > 180 && r < 100 && b < 100,
        "named range shows the cut content"
    );

    // The refusal names the selected composition's own window, not the root's.
    let (code, text) = run(
        &[
            "render",
            "p",
            "-o",
            "bad.mp4",
            "--composition",
            "cut",
            "--end",
            "9",
        ],
        &root,
    );
    assert_eq!(code, 1, "{text}");
    assert!(
        text.contains("accepted window is 0 <= start < end <= 2.000s"),
        "{text}"
    );
    assert!(text.contains("\"cut\""), "{text}");
    assert!(!root.join("bad.mp4").exists());
}
