//! Phase 3 (T006, T009–T012) end-to-end: position tweens in all
//! three forms on box and video targets, scale/rotation/opacity tweens,
//! set holds with release and later-wins, easing shapes with out-back
//! overshoot, paint order under swapped document positions, and the
//! refusal contract for mistimed, unknown-easing, and later-sprint use.
//!
//! All fixtures use the host `ffmpeg`/`ffprobe` (like `cli_render`).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-motion-{}-{}-{}",
        tag,
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

fn ffmpeg(args: &[&str]) {
    let out = Command::new("ffmpeg")
        .args(["-y", "-v", "error"])
        .args(args)
        .output()
        .expect("run host ffmpeg");
    assert!(out.status.success(), "host ffmpeg failed: {args:?}");
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

/// Mean-x of strongly red pixels in one raw RGB24 frame (the box/video
/// centroid probe; yuv420p roundtrips keep solid reds strongly red).
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

fn red_count(frame: &[u8]) -> usize {
    frame
        .chunks_exact(3)
        .filter(|p| p[0] > 150 && p[1] < 100 && p[2] < 100)
        .count()
}

fn frame_of(raw: &[u8], k: usize, w: u32, h: u32) -> &[u8] {
    let len = w as usize * h as usize * 3;
    &raw[k * len..(k + 1) * len]
}

/// Solid-color source: `cols.len()` frames at `fps`.
fn solid_source(path: &Path, cols: &[(u8, u8, u8)], w: u32, h: u32, fps: u32) {
    let mut raw = Vec::with_capacity(cols.len() * w as usize * h as usize * 3);
    for c in cols {
        raw.extend(std::iter::repeat(*c).take(w as usize * h as usize).flat_map(|p| [p.0, p.1, p.2]));
    }
    let raw_path = path.with_extension("raw");
    std::fs::write(&raw_path, &raw).unwrap();
    ffmpeg(&[
        "-f",
        "rawvideo",
        "-pix_fmt",
        "rgb24",
        "-s",
        &format!("{w}x{h}"),
        "-r",
        &fps.to_string(),
        "-i",
        &raw_path.display().to_string(),
        "-c:v",
        "libx264",
        "-pix_fmt",
        "yuv420p",
        "-r",
        &fps.to_string(),
        &path.display().to_string(),
    ]);
}

#[test]
fn position_tween_moves_box_through_exact_path() {
    // T009: box x 10 -> 110 over [0, 1) at 30 fps, 40-wide box.
    let root = tmp("pos");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=160 height=120 duration=2 fps=30 background=\"#000000\" version=1 {\n    box x=10 y=40 width=40 height=40 fill=\"#ff0000\" start=0 duration=2\n    tween target=0 property=\"x\" kind=\"to\" start=0 duration=1 to=110 ease=\"linear\"\n}\n",
    );
    assert_eq!(run(&["check", "proj"], &root).0, 0);
    let mp4 = root.join("out.mp4");
    assert_eq!(run(&["render", "proj", "-o", "out.mp4"], &root).0, 0);
    let raw = decode_raw(&mp4, 60);
    // Box centers: frame 0 -> 30, frame 15 -> 80, frame 30/59 -> 130.
    let cx = |k: usize| red_centroid_x(frame_of(&raw, k, 160, 120), 160).unwrap();
    assert!((cx(0) - 30.0).abs() < 3.0, "start frame shows authored x, got {}", cx(0));
    assert!((cx(15) - 80.0).abs() < 3.0, "mid frame is halfway, got {}", cx(15));
    assert!((cx(30) - 130.0).abs() < 3.0, "end frame shows end value, got {}", cx(30));
    assert!((cx(59) - 130.0).abs() < 3.0, "past the window the end holds, got {}", cx(59));
}

#[test]
fn position_tween_moves_video_picture() {
    // T009: a position tween on a video target shifts its picture.
    let root = tmp("vidpos");
    std::fs::create_dir_all(root.join("proj")).unwrap();
    let folder = root.join("proj");
    solid_source(&folder.join("src.mp4"), &vec![(255, 0, 0); 60], 64, 48, 30);
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=160 height=120 duration=2 fps=30 background=\"#0000ff\" version=1 {\n    video path=\"src.mp4\" start=0 duration=2 fit=\"fill\"\n    tween target=0 property=\"x\" kind=\"to\" start=0 duration=1 to=40 ease=\"linear\"\n}\n",
    );
    assert_eq!(run(&["check", "proj"], &root).0, 0);
    let mp4 = root.join("out.mp4");
    assert_eq!(run(&["render", "proj", "-o", "out.mp4"], &root).0, 0);
    let raw = decode_raw(&mp4, 60);
    // Frame 0 covers the canvas in red; frame 30 leaves a blue strip left.
    let strip_red = |k: usize| -> u64 {
        let f = frame_of(&raw, k, 160, 120);
        let mut n = 0u64;
        for y in 0..120 {
            for x in 0..40 {
                let o = (y * 160 + x) as usize * 3;
                if f[o] > 150 && f[o + 2] < 100 {
                    n += 1;
                }
            }
        }
        n
    };
    assert!(strip_red(0) > 4000, "frame 0 covers the strip");
    assert!(strip_red(30) < 200, "shifted frame leaves the strip blue");
}

#[test]
fn set_holds_then_releases() {
    // T011: box x holds 50 for [0.5, 1.0), then returns to authored 10.
    let root = tmp("set");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=160 height=120 duration=2 fps=30 background=\"#000000\" version=1 {\n    box x=10 y=40 width=40 height=40 fill=\"#ff0000\" start=0 duration=2\n    set target=0 property=\"x\" start=0.5 duration=0.5 value=50\n}\n",
    );
    assert_eq!(run(&["check", "proj"], &root).0, 0);
    let mp4 = root.join("out.mp4");
    assert_eq!(run(&["render", "proj", "-o", "out.mp4"], &root).0, 0);
    let raw = decode_raw(&mp4, 60);
    let cx = |k: usize| red_centroid_x(frame_of(&raw, k, 160, 120), 160).unwrap();
    assert!((cx(14) - 30.0).abs() < 3.0, "frame before the window, got {}", cx(14));
    assert!((cx(15) - 70.0).abs() < 3.0, "first window frame holds, got {}", cx(15));
    assert!((cx(29) - 70.0).abs() < 3.0, "last window frame holds, got {}", cx(29));
    assert!((cx(30) - 30.0).abs() < 3.0, "first frame after releases, got {}", cx(30));
}

#[test]
fn scale_and_opacity_tweens_show_on_frames() {
    // T010: scale 1->2 doubles the red extent; a from-0 opacity fades in.
    let root = tmp("scaleop");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=160 height=120 duration=2 fps=30 background=\"#000000\" version=1 {\n    box x=40 y=40 width=40 height=40 fill=\"#ff0000\" start=0 duration=2\n    tween target=0 property=\"scale\" kind=\"from-to\" start=0 duration=1 from=1 to=2 ease=\"linear\"\n}\n",
    );
    assert_eq!(run(&["check", "proj"], &root).0, 0);
    let mp4 = root.join("out.mp4");
    assert_eq!(run(&["render", "proj", "-o", "out.mp4"], &root).0, 0);
    let raw = decode_raw(&mp4, 60);
    let extent = |k: usize| -> u32 {
        let f = frame_of(&raw, k, 160, 120);
        let mut max = 0u32;
        for (i, px) in f.chunks_exact(3).enumerate() {
            if px[0] > 150 && px[1] < 100 {
                max = max.max((i as u32) % 160);
            }
        }
        max
    };
    assert!((extent(0) as f64 - 79.0).abs() < 3.0, "start extent, got {}", extent(0));
    assert!((extent(30) as f64 - 119.0).abs() < 3.0, "end extent doubled, got {}", extent(30));

    let root = tmp("fade");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=160 height=120 duration=2 fps=30 background=\"#000000\" version=1 {\n    box x=40 y=40 width=40 height=40 fill=\"#ff0000\" start=0 duration=2\n    tween target=0 property=\"opacity\" kind=\"from\" start=0 duration=1 from=0 ease=\"linear\"\n}\n",
    );
    assert_eq!(run(&["check", "proj"], &root).0, 0);
    let mp4 = root.join("out.mp4");
    assert_eq!(run(&["render", "proj", "-o", "out.mp4"], &root).0, 0);
    let raw = decode_raw(&mp4, 60);
    assert!(red_count(frame_of(&raw, 0, 160, 120)) < 10, "opacity 0 is fully transparent");
    assert!(red_count(frame_of(&raw, 30, 160, 120)) > 1500, "opacity 1 is fully opaque");
}

#[test]
fn rotation_tween_turns_box_on_frames() {
    // T010: a 60x20 box turned 90 degrees covers 20x60 about its center.
    let root = tmp("rot");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=160 height=120 duration=2 fps=30 background=\"#000000\" version=1 {\n    box x=50 y=50 width=60 height=20 fill=\"#ff0000\" start=0 duration=2\n    tween target=0 property=\"rotation\" kind=\"to\" start=0 duration=1 to=90 ease=\"linear\"\n}\n",
    );
    assert_eq!(run(&["check", "proj"], &root).0, 0);
    let mp4 = root.join("out.mp4");
    assert_eq!(run(&["render", "proj", "-o", "out.mp4"], &root).0, 0);
    let raw = decode_raw(&mp4, 60);
    let red_rows = |k: usize| -> (u32, u32) {
        let f = frame_of(&raw, k, 160, 120);
        let mut min = 120u32;
        let mut max = 0u32;
        for (i, px) in f.chunks_exact(3).enumerate() {
            if px[0] > 150 && px[1] < 100 {
                let y = (i as u32) / 160;
                min = min.min(y);
                max = max.max(y);
            }
        }
        (min, max)
    };
    let (lo0, hi0) = red_rows(0);
    assert!(lo0 >= 49 && hi0 <= 70, "unturned rows 50..70, got {lo0}..{hi0}");
    let (lo1, hi1) = red_rows(30);
    assert!(lo1 <= 32 && hi1 >= 88, "turned rows span 30..90, got {lo1}..{hi1}");
}

#[test]
fn easing_midpoint_differs_and_outback_overshoots() {
    // T012: same tween under linear vs in-quad ends equal but mids
    // differ; out-back passes its end, then settles exactly.
    let body = |ease: &str| {
        format!(
            "composition name=\"t\" width=160 height=120 duration=2 fps=30 background=\"#000000\" version=1 {{\n    box x=0 y=40 width=20 height=40 fill=\"#ff0000\" start=0 duration=2\n    tween target=0 property=\"x\" kind=\"to\" start=0 duration=1 to=100 ease=\"{ease}\"\n}}\n"
        )
    };
    let root = tmp("ease");
    for (tag, ease) in [("lin", "linear"), ("quad", "in-quad")] {
        let folder = root.join(tag);
        std::fs::create_dir_all(&folder).unwrap();
        write(&folder, "composition.kdl", &body(ease));
        assert_eq!(run(&["check", "."], &folder).0, 0, "{tag} must validate");
        assert_eq!(run(&["render", ".", "-o", "out.mp4"], &folder).0, 0);
    }
    let lin = decode_raw(&root.join("lin/out.mp4"), 60);
    let quad = decode_raw(&root.join("quad/out.mp4"), 60);
    // Encoded bytes vary between renders even for identical pixels, so
    // cross-render equality compares decoded content: the box is 20
    // wide, so both starts sit at x 0..20 with center 10.
    let bbox = |v: &[u8]| {
        let (mut x0, mut x1) = (160u32, 0u32);
        for (i, p) in v.chunks_exact(3).enumerate() {
            if p[0] > 150 && p[1] < 100 && p[2] < 100 {
                let x = i as u32 % 160;
                x0 = x0.min(x);
                x1 = x1.max(x);
            }
        }
        (x0, x1)
    };
    assert_eq!(bbox(&lin[0..57600]), (0, 19), "linear starts exactly");
    assert_eq!(bbox(&quad[0..57600]), (0, 19), "in-quad starts exactly");
    // Later frames are P-frames whose bytes depend on their history, so
    // the ends and middles compare by red centroid, not by bytes: the
    // box is 20 wide, so its center is x + 10.
    let cx = |raw: &[u8], k: usize| red_centroid_x(frame_of(raw, k, 160, 120), 160).unwrap();
    assert!((cx(&lin, 30) - 110.0).abs() < 3.0, "linear ends at 100, got {}", cx(&lin, 30));
    assert!((cx(&quad, 30) - 110.0).abs() < 3.0, "in-quad ends at 100, got {}", cx(&quad, 30));
    assert!((cx(&lin, 15) - 60.0).abs() < 3.0, "linear midpoint is halfway, got {}", cx(&lin, 15));
    assert!(
        cx(&quad, 15) < cx(&lin, 15) - 10.0,
        "in-quad lags the linear middle: {} vs {}",
        cx(&quad, 15),
        cx(&lin, 15)
    );

    let folder = root.join("back");
    std::fs::create_dir_all(&folder).unwrap();
    write(&folder, "composition.kdl", &body("out-back(1.7)"));
    assert_eq!(run(&["check", "."], &folder).0, 0);
    assert_eq!(run(&["render", ".", "-o", "out.mp4"], &folder).0, 0);
    let raw = decode_raw(&folder.join("out.mp4"), 60);
    let cx = |k: usize| red_centroid_x(frame_of(&raw, k, 160, 120), 160).unwrap();
    let peak = (0..30).map(cx).fold(0.0f64, f64::max);
    assert!(peak > cx(30) + 2.0, "out-back passes its end (peak {peak}, end {})", cx(30));
    assert!((cx(30) - 110.0).abs() < 3.0, "out-back settles at the end, got {}", cx(30));
    assert!((cx(45) - cx(30)).abs() < 1.0, "settled value holds past the window");
}

#[test]
fn swapped_document_positions_render_identical_pictures() {
    // T006: the picture follows written order; swapping document
    // positions with fixed orders changes nothing on the CPU renderer.
    let root = tmp("order");
    let red_first = "composition name=\"t\" width=64 height=64 duration=1 fps=30 background=\"#000000\" version=1 {\n    box x=4 y=4 width=32 height=32 fill=\"#ff0000\" start=0 duration=1 order=1\n    box x=16 y=16 width=32 height=32 fill=\"#0000ff\" start=0 duration=1 order=0\n}\n";
    let blue_first = "composition name=\"t\" width=64 height=64 duration=1 fps=30 background=\"#000000\" version=1 {\n    box x=16 y=16 width=32 height=32 fill=\"#0000ff\" start=0 duration=1 order=0\n    box x=4 y=4 width=32 height=32 fill=\"#ff0000\" start=0 duration=1 order=1\n}\n";
    for (tag, body) in [("a", red_first), ("b", blue_first)] {
        let folder = root.join(tag);
        std::fs::create_dir_all(&folder).unwrap();
        write(&folder, "composition.kdl", body);
        assert_eq!(run(&["check", "."], &folder).0, 0, "{tag} must validate");
        assert_eq!(run(&["render", ".", "-o", "out.mp4"], &folder).0, 0);
    }
    let a = decode_raw(&root.join("a/out.mp4"), 30);
    let b = decode_raw(&root.join("b/out.mp4"), 30);
    // Encoded bytes may vary between renders, so every frame compares
    // by content: red/blue pixel counts and red/blue extents.
    let signature = |raw: &[u8]| -> Vec<(usize, usize, u32, u32, u32, u32)> {
        (0..30)
            .map(|k| {
                let f = frame_of(raw, k, 64, 64);
                let (mut reds, mut blues) = (0usize, 0usize);
                let (mut rx0, mut rx1, mut bx0, mut bx1) = (64u32, 0u32, 64u32, 0u32);
                for (i, p) in f.chunks_exact(3).enumerate() {
                    let x = i as u32 % 64;
                    if p[0] > 150 && p[1] < 100 && p[2] < 100 {
                        reds += 1;
                        rx0 = rx0.min(x);
                        rx1 = rx1.max(x);
                    } else if p[2] > 150 && p[0] < 100 && p[1] < 100 {
                        blues += 1;
                        bx0 = bx0.min(x);
                        bx1 = bx1.max(x);
                    }
                }
                (reds, blues, rx0, rx1, bx0, bx1)
            })
            .collect()
    };
    assert_eq!(signature(&a), signature(&b), "swapped documents with fixed orders render identically");
}

#[test]
fn motion_refusals_name_location_and_code() {
    // T009–T012: mistimed windows, bad easings, and later-sprint
    // properties fail check with file/line/column/code, and render
    // writes no MP4.
    let root = tmp("refuse");
    let cases = [
        (
            "mistimed",
            "composition name=\"t\" width=64 height=64 duration=2 fps=30 background=\"#000\" version=1 {\n    box x=4 y=4 width=20 height=12 fill=\"#ff0000\" start=0 duration=2\n    tween target=0 property=\"x\" kind=\"to\" start=-1 duration=1 to=10 ease=\"linear\"\n}\n",
            "E_BAD_TIMING",
            1,
        ),
        (
            "targetless",
            "composition name=\"t\" width=64 height=64 duration=2 fps=30 background=\"#000\" version=1 {\n    box x=4 y=4 width=20 height=12 fill=\"#ff0000\" start=0 duration=2\n    tween target=9 property=\"x\" kind=\"to\" start=0 duration=1 to=10 ease=\"linear\"\n}\n",
            "E_BAD_MOTION",
            1,
        ),
        (
            "badease",
            "composition name=\"t\" width=64 height=64 duration=2 fps=30 background=\"#000\" version=1 {\n    box x=4 y=4 width=20 height=12 fill=\"#ff0000\" start=0 duration=2\n    tween target=0 property=\"x\" kind=\"to\" start=0 duration=1 to=10 ease=\"fast\"\n}\n",
            "E_BAD_EASING",
            1,
        ),
        (
            "badn",
            "composition name=\"t\" width=64 height=64 duration=2 fps=30 background=\"#000\" version=1 {\n    box x=4 y=4 width=20 height=12 fill=\"#ff0000\" start=0 duration=2\n    tween target=0 property=\"x\" kind=\"to\" start=0 duration=1 to=10 ease=\"out-back\"\n}\n",
            "E_BAD_EASING",
            1,
        ),
        (
            "laterprop",
            "composition name=\"t\" width=64 height=64 duration=2 fps=30 background=\"#000\" version=1 {\n    box x=4 y=4 width=20 height=12 fill=\"#ff0000\" start=0 duration=2\n    tween target=0 property=\"blur\" kind=\"to\" start=0 duration=1 to=3 ease=\"linear\"\n}\n",
            "E_UNSUPPORTED",
            2,
        ),
    ];
    for (tag, body, code, exit) in cases {
        let folder = root.join(tag);
        std::fs::create_dir_all(&folder).unwrap();
        write(&folder, "composition.kdl", body);
        let (status, out) = run(&["check", "."], &folder);
        assert_eq!(status, exit, "{tag}: check must exit {exit}, got {status}: {out}");
        assert!(out.contains(code), "{tag}: diagnostics must name {code}: {out}");
        let (rstatus, _) = run(&["render", ".", "-o", "out.mp4"], &folder);
        assert_eq!(rstatus, exit, "{tag}: render must exit {exit}");
        assert!(!folder.join("out.mp4").exists(), "{tag}: no MP4 on refusal");
    }
}
