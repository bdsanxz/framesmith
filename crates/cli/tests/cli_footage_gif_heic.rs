//! Sprint 5 T007 (play GIF animation and HEIC stills).
//!
//! A `video` clip pointing at an animated GIF plays its native frame
//! timings on the composition timeline, with `trim-start`, `trim-end`,
//! `rate`, and `loop` applying exactly as for movie files. A still GIF
//! holds as a single frame. An infinitely looping GIF without an
//! explicit `duration` fails `check` asking for one. `image` accepts
//! HEIC stills like PNG/JPEG (and still GIFs), and a `video` clip
//! pointing at HEIC holds the decoded still. Pointing `image` at an
//! animated GIF is covered by the `invalid-image-animated` fixture and
//! the CLI unit test; this file proves the render side.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-footage-gif-{tag}-{}-{}",
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

fn ffprobe_packets(path: &Path) -> Vec<f64> {
    let out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "packet=pts_time",
            "-of",
            "csv=p=0",
        ])
        .arg(path)
        .output()
        .expect("run host ffprobe");
    assert!(out.status.success());
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|l| l.parse::<f64>().expect("packet pts parses"))
        .collect()
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

/// Convert one HEIC still with the macOS converter (footage scope is
/// macOS, like the host-ffmpeg fixture builders above).
fn gen_heic(dir: &Path, src_png: &Path, name: &str) {
    let out = Command::new("sips")
        .args(["-s", "format", "heic"])
        .arg(src_png)
        .args(["--out", &dir.join(name).display().to_string()])
        .output()
        .expect("run host sips (macOS footage scope)");
    assert!(out.status.success(), "host sips failed for {name:?}");
}

fn gen_solid(dir: &Path, name: &str, color: &str) {
    ffmpeg(&[
        "-f",
        "lavfi",
        "-i",
        &format!("color=c={color}:s=64x48:d=0.1:r=10"),
        "-frames:v",
        "1",
        &dir.join(name).display().to_string(),
    ]);
}

/// Four-frame animated GIF at 2 fps (red, green, blue, yellow, 0.5 s
/// each): single playthrough 2.0 s, infinitely looping (the ffmpeg
/// muxer default), so native timings sit off the 30 fps grid.
fn gen_anim(dir: &Path) {
    for (i, color) in ["red", "green", "blue", "yellow"].iter().enumerate() {
        gen_solid(dir, &format!("g{i}.png"), color);
    }
    ffmpeg(&[
        "-framerate",
        "2",
        "-i",
        &dir.join("g%d.png").display().to_string(),
        &dir.join("anim.gif").display().to_string(),
    ]);
}

/// Animated GIF with irregular native timings: frames at source 0.0,
/// 0.1, 0.5, and 1.5 s (kept input timestamps, not a renumbered grid).
fn gen_irregular(dir: &Path) {
    ffmpeg(&[
        "-f",
        "lavfi",
        "-i",
        "testsrc=duration=2:size=64x48:rate=10",
        "-vf",
        "select='eq(n\\,0)+eq(n\\,1)+eq(n\\,5)+eq(n\\,15)',setpts=PTS",
        &dir.join("irr.gif").display().to_string(),
    ]);
}

/// Finite animated GIF (red 0.5 s, blue 0.5 s, plays once): no
/// NETSCAPE loop extension, so it has a finite native length.
fn gen_finite(dir: &Path) {
    ffmpeg(&[
        "-f",
        "lavfi",
        "-i",
        "color=c=red:s=64x48:d=0.5:r=30",
        "-f",
        "lavfi",
        "-i",
        "color=c=blue:s=64x48:d=0.5:r=30",
        "-filter_complex",
        "[0:v][1:v]concat=n=2:v=1:a=0",
        "-loop",
        "-1",
        &dir.join("finite.gif").display().to_string(),
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

fn ref_rgb(png: &Path, w: u32, h: u32) -> Vec<u8> {
    let out = Command::new("ffmpeg")
        .args([
            "-y",
            "-v",
            "error",
            "-i",
            &png.display().to_string(),
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgb24",
            "-",
        ])
        .output()
        .expect("decode reference png");
    assert!(out.status.success());
    assert_eq!(out.stdout.len(), (w * h * 3) as usize);
    out.stdout
}

/// Mean absolute byte difference between two frames.
fn frame_mad(a: &[u8], b: &[u8]) -> f64 {
    assert_eq!(a.len(), b.len());
    let sum: u64 = a
        .iter()
        .zip(b.iter())
        .map(|(&x, &y)| x.abs_diff(y) as u64)
        .sum();
    sum as f64 / a.len() as f64
}

/// An animated GIF renders with its native frame timings mapped onto
/// composition time: each output frame shows the GIF frame whose
/// interval covers that instant.
#[test]
fn animated_gif_maps_native_timings() {
    let folder = tmp("native");
    gen_anim(&folder);
    let src = probe_duration(&folder.join("anim.gif"));
    assert!((src - 2.0).abs() < 0.05, "fixture is about 2s, got {src}");
    write(
        &folder,
        "composition.kdl",
        &comp(2.0, "video path=\"anim.gif\" start=0 duration=2"),
    );
    let (code, out) = run(&["check", "."], &folder);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("0.000s..2.000s"), "{out}");
    let (code, out) = run(&["render", ".", "-o", "out.mp4"], &folder);
    assert_eq!(code, 0, "{out}");
    // 2 fps GIF on a 30 fps timeline: t=0.23 red, 0.73 green,
    // 1.23 blue, 1.73 yellow.
    for (frame, color) in [(7, 0), (22, 1), (37, 2), (52, 3)] {
        let got = output_frame(&folder.join("out.mp4"), frame, 64, 48);
        let want = ref_rgb(&folder.join(format!("g{color}.png")), 64, 48);
        let mad = frame_mad(&got, &want);
        assert!(
            mad < 15.0,
            "frame {frame} shows color {color}, mad={mad:.2}"
        );
    }
}

/// Irregular native timings select the covering frame: with source
/// frames at 0.0/0.1/0.5/1.5 s, t=0.3 shows the 0.1 s frame and t=0.7
/// shows the 0.5 s frame — no slip to the next frame, no hold of an
/// earlier one past its interval.
#[test]
fn irregular_gif_selects_covering_frame() {
    let folder = tmp("irregular");
    gen_irregular(&folder);
    let pts = ffprobe_packets(&folder.join("irr.gif"));
    assert_eq!(pts.len(), 4, "four native timestamps, got {pts:?}");
    for (got, want) in pts.iter().zip([0.0, 0.1, 0.5, 1.5]) {
        assert!((got - want).abs() < 1e-3, "native timings hold: {pts:?}");
    }
    write(
        &folder,
        "composition.kdl",
        &comp(2.0, "video path=\"irr.gif\" start=0 duration=2"),
    );
    let (code, out) = run(&["check", "."], &folder);
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(&["render", ".", "-o", "out.mp4"], &folder);
    assert_eq!(code, 0, "{out}");
    // Reference pictures straight from the GIF decoder (no seek: the
    // GIF demuxer drops pre-seek pictures, so decode all and index).
    ffmpeg(&[
        "-i",
        &folder.join("irr.gif").display().to_string(),
        &folder.join("refs_%d.png").display().to_string(),
    ]);
    // t=9/30=0.3 covers source 0.1 (2nd frame); t=21/30=0.7 covers 0.5.
    for (frame, ref_idx) in [(9, 2), (21, 3)] {
        let got = output_frame(&folder.join("out.mp4"), frame, 64, 48);
        let want = ref_rgb(&folder.join(format!("refs_{ref_idx}.png")), 64, 48);
        let mad = frame_mad(&got, &want);
        assert!(
            mad < 15.0,
            "frame {frame} covers source frame {ref_idx}, mad={mad:.2}"
        );
    }
}

/// Trim, rate, and loop apply to GIF sources exactly as for movies: a
/// 0.5..1.5 s window (green then blue) at rate 2 loops every
/// composition half-second.
#[test]
fn gif_trim_rate_and_loop_apply() {
    let folder = tmp("window");
    gen_anim(&folder);
    write(
        &folder,
        "composition.kdl",
        &comp(
            2.0,
            "video path=\"anim.gif\" start=0 duration=2 trim-start=0.5 trim-end=1.5 rate=2 loop=true",
        ),
    );
    let (code, out) = run(&["check", "."], &folder);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("0.500s..1.500s"), "{out}");
    assert!(out.contains("loop on"), "{out}");
    let (code, out) = run(&["render", ".", "-o", "out.mp4"], &folder);
    assert_eq!(code, 0, "{out}");
    // t=0.1 -> src 0.7 (green); t=0.4 -> src 1.3 (blue); the loop
    // point at t=0.5 wraps back to green by t=0.6 and t=1.1.
    for (frame, color) in [(3, 1), (12, 2), (18, 1), (33, 1)] {
        let got = output_frame(&folder.join("out.mp4"), frame, 64, 48);
        let want = ref_rgb(&folder.join(format!("g{color}.png")), 64, 48);
        let mad = frame_mad(&got, &want);
        assert!(
            mad < 15.0,
            "frame {frame} shows color {color}, mad={mad:.2}"
        );
    }
}

/// A finite animated GIF (plays once) without `duration` infers its
/// single playthrough: red then blue over exactly 1.0 s.
#[test]
fn finite_gif_without_duration_infers_single_play() {
    let folder = tmp("finite");
    gen_finite(&folder);
    let src = probe_duration(&folder.join("finite.gif"));
    assert!(
        (src - 1.0).abs() < 0.05,
        "fixture plays once over ~1s, got {src}"
    );
    write(
        &folder,
        "composition.kdl",
        &comp(1.0, "video path=\"finite.gif\" start=0"),
    );
    let (code, out) = run(&["check", "."], &folder);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("clip duration 1.000s (inferred)"), "{out}");
    let (code, out) = run(&["check", "--json", "."], &folder);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("\"inferred\":true"), "{out}");
    let (code, out) = run(&["render", ".", "-o", "out.mp4"], &folder);
    assert_eq!(code, 0, "{out}");
    let mp4 = probe_duration(&folder.join("out.mp4"));
    assert!(
        (mp4 - 1.0).abs() < 0.15,
        "renders the inferred length, got {mp4}"
    );
    gen_solid(&folder, "red.png", "red");
    gen_solid(&folder, "blue.png", "blue");
    let early = output_frame(&folder.join("out.mp4"), 7, 64, 48);
    let mad = frame_mad(&early, &ref_rgb(&folder.join("red.png"), 64, 48));
    assert!(mad < 15.0, "first half is red, mad={mad:.2}");
    let late = output_frame(&folder.join("out.mp4"), 22, 64, 48);
    let mad = frame_mad(&late, &ref_rgb(&folder.join("blue.png"), 64, 48));
    assert!(mad < 15.0, "second half is blue, mad={mad:.2}");
}

/// A still GIF holds its single frame for the whole clip window.
#[test]
fn still_gif_holds_for_clip_window() {
    let folder = tmp("still");
    gen_solid(&folder, "red.png", "red");
    ffmpeg(&[
        "-i",
        &folder.join("red.png").display().to_string(),
        &folder.join("still.gif").display().to_string(),
    ]);
    write(
        &folder,
        "composition.kdl",
        &comp(1.0, "video path=\"still.gif\" start=0 duration=1"),
    );
    let (code, out) = run(&["check", "."], &folder);
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(&["render", ".", "-o", "out.mp4"], &folder);
    assert_eq!(code, 0, "{out}");
    for frame in [0, 15, 29] {
        let got = output_frame(&folder.join("out.mp4"), frame, 64, 48);
        let want = ref_rgb(&folder.join("red.png"), 64, 48);
        let mad = frame_mad(&got, &want);
        assert!(mad < 15.0, "frame {frame} holds the still, mad={mad:.2}");
    }
}

/// HEIC stills render as a full still in `image` and as a held frame
/// in `video`, matching the PNG reference picture.
#[test]
fn heic_renders_in_image_and_as_held_video() {
    let folder = tmp("heic");
    gen_solid(&folder, "red.png", "red");
    gen_heic(&folder, &folder.join("red.png"), "red.heic");
    write(
        &folder,
        "composition.kdl",
        &comp(1.0, "image path=\"red.heic\" start=0 duration=1"),
    );
    let (code, out) = run(&["check", "."], &folder);
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(&["render", ".", "-o", "img.mp4"], &folder);
    assert_eq!(code, 0, "{out}");
    let want = ref_rgb(&folder.join("red.png"), 64, 48);
    let got = output_frame(&folder.join("img.mp4"), 15, 64, 48);
    let mad = frame_mad(&got, &want);
    assert!(mad < 15.0, "image shows the HEIC still, mad={mad:.2}");
    write(
        &folder,
        "composition.kdl",
        &comp(1.0, "video path=\"red.heic\" start=0 duration=1"),
    );
    let (code, out) = run(&["check", "."], &folder);
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(&["render", ".", "-o", "vid.mp4"], &folder);
    assert_eq!(code, 0, "{out}");
    for frame in [0, 15, 29] {
        let got = output_frame(&folder.join("vid.mp4"), frame, 64, 48);
        let mad = frame_mad(&got, &want);
        assert!(
            mad < 15.0,
            "video holds the HEIC frame {frame}, mad={mad:.2}"
        );
    }
}

/// An infinitely looping GIF without `duration` fails `check` (and
/// `render`) asking for an explicit `duration`; nothing is written.
#[test]
fn infinite_gif_without_duration_fails_asking_for_duration() {
    let folder = tmp("unbounded");
    gen_anim(&folder);
    write(
        &folder,
        "composition.kdl",
        &comp(2.0, "video path=\"anim.gif\" start=0"),
    );
    let (code, out) = run(&["check", "."], &folder);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_BAD_FOOTAGE"), "{out}");
    assert!(out.contains("anim.gif"), "{out}");
    assert!(out.contains("duration"), "{out}");
    let (code, out) = run(&["check", "--json", "."], &folder);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("\"code\":\"E_BAD_FOOTAGE\""), "{out}");
    assert!(!folder.join("out.mp4").exists(), "check writes no MP4");
    let (code, out) = run(&["render", ".", "-o", "out.mp4"], &folder);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("duration"), "{out}");
    assert!(
        !folder.join("out.mp4").exists(),
        "no MP4 survives the refusal"
    );
    assert!(
        !folder.join("out.mp4.framesmith-part").exists(),
        "no partial survives"
    );
}
