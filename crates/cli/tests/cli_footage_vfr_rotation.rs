//! Sprint 5 T004 (exact VFR sampling) + T005 (container rotation).
//!
//! A variable-frame-rate clip renders the frame whose true presentation
//! interval covers each exact output instant — including across a VFR
//! gap where the covering frame starts long before the window opens —
//! with trim, rate, and loop composing on top of timestamp sampling. A
//! source with unparseable timing (no packet timestamps at all) refuses
//! before frame one naming the file, with re-encoding as the fix.
//!
//! A clip carrying container rotation renders upright like players
//! show: the oriented picture (not the stored pixels) lands on the
//! canvas with `fit` and placement applying to it, composing with
//! trim/rate/loop, while a source without rotation renders as stored.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-footage-vfr-rot-{tag}-{}-{}",
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

fn ffprobe_packets(mp4: &Path) -> Vec<f64> {
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
            &mp4.display().to_string(),
        ])
        .output()
        .expect("run host ffprobe");
    assert!(out.status.success(), "host ffprobe failed");
    let mut pts: Vec<f64> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| l.trim().parse::<f64>().expect("packet pts parses"))
        .collect();
    pts.sort_by(|a, b| a.partial_cmp(b).unwrap());
    pts
}

/// Decode every source picture in presentation order (passthrough: no
/// vsync duplication), exactly as Framesmith's own decode sees them.
fn decode_source_pass(mp4: &Path, extra: &[&str]) -> Vec<u8> {
    let path = mp4.display().to_string();
    let mut args = vec!["-v", "error"];
    args.extend(extra.iter().copied());
    args.extend([
        "-i",
        path.as_str(),
        "-fps_mode",
        "passthrough",
        "-f",
        "rawvideo",
        "-pix_fmt",
        "rgb24",
        "-",
    ]);
    let out = Command::new("ffmpeg")
        .args(["-y"])
        .args(args)
        .output()
        .expect("decode source");
    assert!(out.status.success(), "source decode failed");
    out.stdout
}

/// Decode pictures with an extra video filter (e.g. scale), optionally
/// capped to `frames` pictures (`None` decodes the whole stream).
fn decode_filtered(
    mp4: &Path,
    filter: &str,
    w: u32,
    h: u32,
    extra: &[&str],
    frames: Option<u32>,
) -> Vec<u8> {
    let dim = format!("{w}x{h}");
    let path = mp4.display().to_string();
    let count = frames.map(|n| n.to_string());
    let mut args = vec!["-v", "error"];
    args.extend(extra.iter().copied());
    args.extend(["-i", path.as_str(), "-vf", filter]);
    if let Some(n) = count.as_ref() {
        args.extend(["-frames:v", n.as_str()]);
    }
    args.extend([
        "-f",
        "rawvideo",
        "-pix_fmt",
        "rgb24",
        "-s",
        dim.as_str(),
        "-",
    ]);
    let out = Command::new("ffmpeg")
        .args(["-y"])
        .args(args)
        .output()
        .expect("decode filtered");
    assert!(out.status.success(), "filtered decode failed");
    out.stdout
}

fn frame_mad(a: &[u8], b: &[u8]) -> f64 {
    assert_eq!(a.len(), b.len());
    let sum: u64 = a
        .iter()
        .zip(b.iter())
        .map(|(&x, &y)| x.abs_diff(y) as u64)
        .sum();
    sum as f64 / a.len() as f64
}

fn write(folder: &Path, name: &str, content: &str) {
    std::fs::write(folder.join(name), content).unwrap();
}

/// A VFR source with a known one-second gap: frames 0..30 then 60..120
/// of a 30 fps testsrc, so presentation intervals are 1/30 s except
/// frame 29, which covers [0.966667, 2.0).
fn gen_vfr_gap(dir: &Path, name: &str) {
    ffmpeg(&[
        "-f",
        "lavfi",
        "-i",
        "testsrc=duration=4:size=64x48:rate=30",
        "-vf",
        "select='lt(n\\,30)+gt(n\\,59)'",
        "-fps_mode",
        "vfr",
        "-c:v",
        "libx264",
        "-pix_fmt",
        "yuv420p",
        &dir.join(name).display().to_string(),
    ]);
}

fn gen_base(dir: &Path, name: &str, seconds: f64) {
    ffmpeg(&[
        "-f",
        "lavfi",
        "-i",
        &format!("testsrc=duration={seconds}:size=64x48:rate=30"),
        "-c:v",
        "libx264",
        "-pix_fmt",
        "yuv420p",
        &dir.join(name).display().to_string(),
    ]);
}

/// Stamp a 90-degree display-matrix rotation onto an MP4 (stored pixels
/// untouched): the file then plays portrait while storing landscape.
fn stamp_rotation_90(mp4: &Path) {
    let mut bytes = std::fs::read(mp4).expect("read mp4");
    let pos = bytes
        .windows(4)
        .position(|w| w == b"tkhd")
        .expect("tkhd atom present");
    let moff = pos + 44;
    let one: i32 = 65536;
    let matrix: [i32; 9] = [0, -one, 0, one, 0, 0, 0, 0, 1 << 30];
    for (i, v) in matrix.iter().enumerate() {
        bytes[moff + i * 4..moff + i * 4 + 4].copy_from_slice(&v.to_be_bytes());
    }
    std::fs::write(mp4, bytes).expect("write rotated mp4");
    let out = Command::new("ffmpeg")
        .args(["-hide_banner", "-i", &mp4.display().to_string()])
        .output()
        .expect("probe rotated mp4");
    let text = String::from_utf8_lossy(&out.stderr);
    assert!(
        text.contains("rotation of 90"),
        "rotation matrix took: {text}"
    );
}

/// Last index whose presentation timestamp covers `t`.
fn covering(pts: &[f64], t: f64) -> usize {
    let mut idx = 0;
    for (i, &p) in pts.iter().enumerate() {
        if p <= t + 1e-6 {
            idx = i;
        } else {
            break;
        }
    }
    idx
}

/// A VFR clip renders the timestamp-correct frame across a gap: the
/// window opens at 1.5 s inside frame 29's [0.966667, 2.0) interval, so
/// output frame 0 (and every instant before 2.0) shows frame 29 — never
/// frame 30 early — and later frames track their covering frames.
#[test]
fn vfr_gap_renders_the_covering_frame() {
    let folder = tmp("gap");
    gen_vfr_gap(&folder, "vfr.mp4");
    let pts = ffprobe_packets(&folder.join("vfr.mp4"));
    assert_eq!(pts.len(), 90, "fixture has 90 source frames");
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=64 height=48 duration=2 fps=30 background=\"#000000\" version=1 {\n    video path=\"vfr.mp4\" start=0 duration=2 trim-start=1.5\n}\n",
    );
    let (code, out) = run(&["check", "."], &folder);
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(&["render", ".", "-o", "out.mp4"], &folder);
    assert_eq!(code, 0, "{out}");
    let (w, h) = (64u32, 48u32);
    let rendered = decode_filtered(&folder.join("out.mp4"), "null", w, h, &[], None);
    let source = decode_source_pass(&folder.join("vfr.mp4"), &[]);
    let fs = (w * h * 3) as usize;
    assert_eq!(rendered.len(), 60 * fs, "2 s at 30 fps");
    assert_eq!(source.len(), 90 * fs, "all 90 source frames, no vsync dups");
    for k in [0usize, 3, 7, 14, 15, 16, 29, 44, 59] {
        let src_t = 1.5 + k as f64 / 30.0;
        let want_idx = covering(&pts, src_t);
        let got = &rendered[k * fs..(k + 1) * fs];
        let want = &source[want_idx * fs..(want_idx + 1) * fs];
        let good = frame_mad(got, want);
        println!("frame {k}: src_t={src_t:.4} covering source frame {want_idx}, mad={good:.2}");
        assert!(
            good < 15.0,
            "frame {k} must show covering source frame {want_idx}, mad={good:.2}"
        );
    }
    // The gap opening shows the covering frame, not the next one: the
    // 1.5 s instant belongs to source frame 29 (pts 0.966667).
    assert_eq!(covering(&pts, 1.5), 29);
    let first = &rendered[0..fs];
    let right = frame_mad(first, &source[29 * fs..30 * fs]);
    let wrong = frame_mad(first, &source[30 * fs..31 * fs]);
    println!("first frame: covering={right:.2} next={wrong:.2}");
    assert!(
        right < 15.0,
        "first frame must be the covering frame, mad={right:.2}"
    );
    assert!(
        right < wrong * 0.5,
        "first frame must not slip to the next frame ({right:.2} vs {wrong:.2})"
    );
}

/// Trim, rate, and loop compose with VFR sampling: a doubled window
/// looping over a gap maps every output instant through the wrapped
/// source time to its covering frame.
#[test]
fn vfr_composes_with_trim_rate_and_loop() {
    let folder = tmp("vfr-loop");
    gen_vfr_gap(&folder, "vfr.mp4");
    let pts = ffprobe_packets(&folder.join("vfr.mp4"));
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=64 height=48 duration=2 fps=30 background=\"#000000\" version=1 {\n    video path=\"vfr.mp4\" start=0 duration=2 trim-start=0.5 trim-end=2.5 rate=2 loop=true\n}\n",
    );
    let (code, out) = run(&["check", "."], &folder);
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(&["render", ".", "-o", "out.mp4"], &folder);
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains("loop"),
        "render summary announces loop clips: {out}"
    );
    let (w, h) = (64u32, 48u32);
    let rendered = decode_filtered(&folder.join("out.mp4"), "null", w, h, &[], None);
    let source = decode_source_pass(&folder.join("vfr.mp4"), &[]);
    let fs = (w * h * 3) as usize;
    assert_eq!(rendered.len(), 60 * fs);
    // Window [0.5, 2.5) at rate 2 repeats every composition second.
    for k in [0usize, 5, 14, 29, 30, 45, 59] {
        let src_t = 0.5 + ((k as f64 / 30.0) * 2.0 % 2.0);
        let want_idx = covering(&pts, src_t);
        let got = &rendered[k * fs..(k + 1) * fs];
        let want = &source[want_idx * fs..(want_idx + 1) * fs];
        let good = frame_mad(got, want);
        println!("frame {k}: wrapped src_t={src_t:.4} covering {want_idx}, mad={good:.2}");
        assert!(
            good < 15.0,
            "looped frame {k} must show covering frame {want_idx}, mad={good:.2}"
        );
    }
    // The loop point repeats the window opening exactly.
    let reopen = frame_mad(&rendered[30 * fs..31 * fs], &rendered[0..fs]);
    println!("loop-point self mad: {reopen:.2}");
    assert!(
        reopen < 15.0,
        "the loop point reopens the window, mad={reopen:.2}"
    );
}

/// A source whose timing is unparseable (a raw Annex B stream carries
/// no packet timestamps at all) refuses before frame one naming the
/// file, with re-encoding as the fix — never guessed timing.
#[test]
fn unparseable_timing_refuses_naming_the_file() {
    let folder = tmp("no-timing");
    gen_base(&folder, "base.mp4", 2.0);
    ffmpeg(&[
        "-i",
        &folder.join("base.mp4").display().to_string(),
        "-c:v",
        "copy",
        "-bsf:v",
        "h264_mp4toannexb",
        &folder.join("raw.h264").display().to_string(),
    ]);
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=64 height=48 duration=1 fps=30 background=\"#000000\" version=1 {\n    video path=\"raw.h264\" start=0 duration=1\n}\n",
    );
    let (code, out) = run(&["render", ".", "-o", "out.mp4"], &folder);
    assert_eq!(code, 1, "unparseable timing refuses: {out}");
    assert!(out.contains("raw.h264"), "refusal names the file: {out}");
    assert!(
        out.contains("re-encode"),
        "refusal points at re-encoding: {out}"
    );
    assert!(
        !folder.join("out.mp4").exists(),
        "no partial MP4 is left behind"
    );
    assert!(
        !folder.join("out.mp4.framesmith-part").exists(),
        "no staged partial is left behind"
    );
}

/// A 90-degree-rotated source renders upright: on a portrait canvas the
/// oriented picture fills the frame exactly like a player display, and
/// nothing like the sideways stored pixels.
#[test]
fn rotated_source_renders_upright() {
    let folder = tmp("rot-upright");
    gen_base(&folder, "rot.mp4", 2.0);
    stamp_rotation_90(&folder.join("rot.mp4"));
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=48 height=64 duration=1 fps=30 background=\"#000000\" version=1 {\n    video path=\"rot.mp4\" start=0 duration=1\n}\n",
    );
    let (code, out) = run(&["check", "."], &folder);
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(&["render", ".", "-o", "out.mp4"], &folder);
    assert_eq!(code, 0, "{out}");
    let (w, h) = (48u32, 64u32);
    let rendered = decode_filtered(&folder.join("out.mp4"), "null", w, h, &[], None);
    let fs = (w * h * 3) as usize;
    assert_eq!(rendered.len(), 30 * fs, "1 s at 30 fps");
    // Player-visible reference: autorotate the stored pixels, then fit
    // (contain is exact here: oriented 48x64 on a 48x64 canvas).
    let upright = decode_filtered(
        &folder.join("rot.mp4"),
        "scale=48:64",
        w,
        h,
        &["-autorotate"],
        Some(1),
    );
    let sideways = decode_filtered(
        &folder.join("rot.mp4"),
        "scale=48:64",
        w,
        h,
        &["-noautorotate"],
        Some(1),
    );
    let first = &rendered[0..fs];
    let good = frame_mad(first, &upright[0..fs]);
    let bad = frame_mad(first, &sideways[0..fs]);
    println!("upright={good:.2} sideways={bad:.2}");
    assert!(
        good < 15.0,
        "rendered frame must match the player-visible picture, mad={good:.2}"
    );
    assert!(
        bad > good * 3.0 + 10.0,
        "rendered frame must not be the sideways stored picture ({good:.2} vs {bad:.2})"
    );
}

/// `fit` and placement apply to the oriented picture: a portrait source
/// contained on a wider canvas letterboxes with the picture centered.
#[test]
fn rotated_fit_applies_to_the_oriented_picture() {
    let folder = tmp("rot-fit");
    gen_base(&folder, "rot.mp4", 1.0);
    stamp_rotation_90(&folder.join("rot.mp4"));
    // Canvas 96x64, oriented picture 48x64: contain fits height (64),
    // centers the 48-wide picture with black bars left and right.
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=96 height=64 duration=1 fps=30 background=\"#000000\" version=1 {\n    video path=\"rot.mp4\" start=0 duration=1\n}\n",
    );
    let (code, out) = run(&["render", ".", "-o", "out.mp4"], &folder);
    assert_eq!(code, 0, "{out}");
    let (w, h) = (96u32, 64u32);
    let rendered = decode_filtered(&folder.join("out.mp4"), "null", w, h, &[], None);
    let fs = (w * h * 3) as usize;
    let frame = &rendered[0..fs];
    // Letterbox bars: far-left and far-right columns stay background.
    let column = |x: u32| -> f64 {
        let mut sum = 0u64;
        for y in 0..h {
            let o = (y * w + x) as usize * 3;
            sum += frame[o] as u64 + frame[o + 1] as u64 + frame[o + 2] as u64;
        }
        sum as f64 / h as f64
    };
    let (left, middle, right) = (column(2), column(48), column(93));
    println!("columns: left={left:.1} middle={middle:.1} right={right:.1}");
    assert!(left < 8.0, "left bar stays background, mean={left:.1}");
    assert!(right < 8.0, "right bar stays background, mean={right:.1}");
    assert!(
        middle > 20.0,
        "the centered oriented picture paints, mean={middle:.1}"
    );
    // The centered picture matches the player-visible reference.
    let upright = decode_filtered(
        &folder.join("rot.mp4"),
        "scale=48:64",
        48,
        64,
        &["-autorotate"],
        Some(1),
    );
    let mut expect = vec![0u8; fs];
    for y in 0..64 {
        for x in 0..48u32 {
            let s = (y * 48 + x) as usize * 3;
            let d = (y * 96 + (x + 24)) as usize * 3;
            expect[d..d + 3].copy_from_slice(&upright[s..s + 3]);
        }
    }
    let fit_mad = frame_mad(frame, &expect);
    println!("fit mad: {fit_mad:.2}");
    assert!(
        fit_mad < 15.0,
        "contained picture matches the oriented reference, mad={fit_mad:.2}"
    );
}

/// Rotation composes with trim, rate, and loop: a re-timed looping
/// portrait clip maps every output instant to its oriented window frame.
#[test]
fn rotated_composes_with_trim_rate_and_loop() {
    let folder = tmp("rot-loop");
    gen_base(&folder, "rot.mp4", 4.0);
    stamp_rotation_90(&folder.join("rot.mp4"));
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=48 height=64 duration=2 fps=30 background=\"#000000\" version=1 {\n    video path=\"rot.mp4\" start=0 duration=2 trim-start=1 trim-end=3 rate=2 loop=true\n}\n",
    );
    let (code, out) = run(&["check", "."], &folder);
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(&["render", ".", "-o", "out.mp4"], &folder);
    assert_eq!(code, 0, "{out}");
    let (w, h) = (48u32, 64u32);
    let rendered = decode_filtered(&folder.join("out.mp4"), "null", w, h, &[], None);
    // Oriented reference timeline: every autorotated stored frame scaled
    // to the canvas (contain is exact here).
    let upright = decode_filtered(
        &folder.join("rot.mp4"),
        "scale=48:64",
        w,
        h,
        &["-autorotate"],
        None,
    );
    let fs = (w * h * 3) as usize;
    assert_eq!(upright.len(), 120 * fs, "4 s oriented reference at 30 fps");
    // Window [1, 3) at rate 2 repeats every composition second.
    for k in [0usize, 7, 15, 29, 30, 44, 59] {
        let src_idx = 30 + ((k as f64 / 30.0 * 2.0 % 2.0) * 30.0).round() as usize;
        let got = &rendered[k * fs..(k + 1) * fs];
        let want = &upright[src_idx * fs..(src_idx + 1) * fs];
        let good = frame_mad(got, want);
        println!("frame {k}: oriented source frame {src_idx}, mad={good:.2}");
        assert!(
            good < 15.0,
            "frame {k} must show oriented source frame {src_idx}, mad={good:.2}"
        );
    }
}

/// A source without rotation metadata renders exactly as stored: the
/// Sprint 1 picture path is unchanged by the orientation work.
#[test]
fn unrotated_source_renders_as_stored() {
    let folder = tmp("stored");
    gen_base(&folder, "clip.mp4", 2.0);
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=64 height=48 duration=1 fps=30 background=\"#000000\" version=1 {\n    video path=\"clip.mp4\" start=0 duration=1 trim-start=0.5\n}\n",
    );
    let (code, out) = run(&["render", ".", "-o", "out.mp4"], &folder);
    assert_eq!(code, 0, "{out}");
    let (w, h) = (64u32, 48u32);
    let rendered = decode_filtered(&folder.join("out.mp4"), "null", w, h, &[], None);
    let source = decode_source_pass(&folder.join("clip.mp4"), &[]);
    let fs = (w * h * 3) as usize;
    for k in [0usize, 7, 29] {
        let got = &rendered[k * fs..(k + 1) * fs];
        let want = &source[(15 + k) * fs..(15 + k + 1) * fs];
        let good = frame_mad(got, want);
        println!("frame {k}: stored source frame {}, mad={good:.2}", 15 + k);
        assert!(
            good < 15.0,
            "frame {k} must match the stored picture, mad={good:.2}"
        );
    }
}
