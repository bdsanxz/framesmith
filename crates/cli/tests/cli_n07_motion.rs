//! Sprint 7 motion (T001 + T007) end-to-end: keyframe chains render
//! per-segment easings at frame-exact values, stop-to-key-to-linear
//! fallback shows, `hold` and `steps` step on frames, every new curve
//! renders exact ends, refusals name the stop or curve with no MP4, and
//! existing tween/set motion re-renders byte-identically.
//!
//! All fixtures use the host `ffmpeg` (like `cli_motion`).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-n07-{tag}-{}-{}",
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
const BOX: &str = "    box x=10 y=40 width=40 height=40 fill=\"#ff0000\" start=0 duration=2\n";

#[test]
fn keyframes_render_per_segment_easings() {
    // T001: stops 10/60/110 at 0s/0.5s/1s; segment one runs linear,
    // segment two runs in-quad. Box centers sit 20 right of x.
    let root = tmp("keys");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    write(
        &folder,
        "composition.kdl",
        &format!(
            "{HEAD}{BOX}    key target=0 property=\"x\" {{\n        frame at=0 value=10\n        frame at=0.5 value=60 ease=\"in-quad\"\n        frame at=1 value=110 ease=\"bounce\"\n    }}\n}}\n"
        ),
    );
    assert_eq!(run(&["check", "proj"], &root).0, 0);
    let mp4 = root.join("out.mp4");
    assert_eq!(run(&["render", "proj", "-o", "out.mp4"], &root).0, 0);
    let raw = decode_raw(&mp4, 60);
    let cx = |k: usize| red_centroid_x(frame_of(&raw, k, 160, 120), 160).unwrap();
    assert!(
        (cx(0) - 30.0).abs() < 3.0,
        "first stop shows, got {}",
        cx(0)
    );
    assert!(
        (cx(7) - 53.3).abs() < 3.0,
        "linear first segment, got {}",
        cx(7)
    );
    assert!(
        (cx(15) - 80.0).abs() < 3.0,
        "middle stop lands, got {}",
        cx(15)
    );
    // Second segment progress 7/15 under in-quad: 60 + 50 * (7/15)^2.
    let seg2 = 60.0 + 50.0 * (7.0f64 / 15.0).powi(2) + 20.0;
    assert!(
        (cx(22) - seg2).abs() < 3.0,
        "in-quad second segment, got {}",
        cx(22)
    );
    assert!(
        (cx(30) - 130.0).abs() < 3.0,
        "last stop lands, got {}",
        cx(30)
    );
    assert!((cx(59) - 130.0).abs() < 3.0, "end holds, got {}", cx(59));
}

#[test]
fn key_ease_falls_back_from_stop_to_key_to_linear() {
    // T001: the segment ease is the stop's, else the key's, else linear.
    // Stops 10 -> 110 over [0, 1); frame 15 samples progress 1/2.
    let root = tmp("fallback");
    for (tag, key_ease, stop_ease, center) in [
        ("stop", "ease=\"out-quad\"", "ease=\"in-quad\"", 55.0),
        ("key", "ease=\"out-quad\"", "", 105.0),
        ("linear", "", "", 80.0),
    ] {
        let folder = root.join(tag);
        std::fs::create_dir_all(&folder).unwrap();
        let key_attr = if key_ease.is_empty() {
            String::new()
        } else {
            format!(" {key_ease}")
        };
        write(
            &folder,
            "composition.kdl",
            &format!(
                "{HEAD}{BOX}    key target=0 property=\"x\"{key_attr} {{\n        frame at=0 value=10 {stop_ease}\n        frame at=1 value=110\n    }}\n}}\n"
            ),
        );
        assert_eq!(run(&["check", "."], &folder).0, 0, "{tag} must validate");
        assert_eq!(run(&["render", ".", "-o", "out.mp4"], &folder).0, 0);
        let raw = decode_raw(&folder.join("out.mp4"), 60);
        let cx = red_centroid_x(frame_of(&raw, 15, 160, 120), 160).unwrap();
        assert!((cx - center).abs() < 3.0, "{tag}: midpoint, got {cx}");
    }
}

#[test]
fn hold_and_steps_step_on_frames() {
    // T007: hold shows the prior value to the last frame, then jumps;
    // steps(4) holds each quarter until its end.
    let root = tmp("steps");
    let folder = root.join("hold");
    std::fs::create_dir_all(&folder).unwrap();
    write(
        &folder,
        "composition.kdl",
        &format!(
            "{HEAD}{BOX}    key target=0 property=\"x\" {{\n        frame at=0 value=10 ease=\"hold\"\n        frame at=1 value=110\n    }}\n}}\n"
        ),
    );
    assert_eq!(run(&["check", "."], &folder).0, 0);
    assert_eq!(run(&["render", ".", "-o", "out.mp4"], &folder).0, 0);
    let raw = decode_raw(&folder.join("out.mp4"), 60);
    let cx = |k: usize| red_centroid_x(frame_of(&raw, k, 160, 120), 160).unwrap();
    assert!(
        (cx(15) - 30.0).abs() < 3.0,
        "hold keeps the start, got {}",
        cx(15)
    );
    assert!(
        (cx(29) - 30.0).abs() < 3.0,
        "hold keeps to the last frame, got {}",
        cx(29)
    );
    assert!(
        (cx(30) - 130.0).abs() < 3.0,
        "the next stop jumps, got {}",
        cx(30)
    );

    let folder = root.join("steps");
    std::fs::create_dir_all(&folder).unwrap();
    write(
        &folder,
        "composition.kdl",
        &format!(
            "{HEAD}{BOX}    tween target=0 property=\"x\" kind=\"to\" start=0 duration=1 to=110 ease=\"steps(4)\"\n}}\n"
        ),
    );
    assert_eq!(run(&["check", "."], &folder).0, 0);
    assert_eq!(run(&["render", ".", "-o", "out.mp4"], &folder).0, 0);
    let raw = decode_raw(&folder.join("out.mp4"), 60);
    let cx = |k: usize| red_centroid_x(frame_of(&raw, k, 160, 120), 160).unwrap();
    assert!(
        (cx(7) - 30.0).abs() < 3.0,
        "first step holds, got {}",
        cx(7)
    );
    assert!(
        (cx(8) - 55.0).abs() < 3.0,
        "second step jumps, got {}",
        cx(8)
    );
    assert!(
        (cx(30) - 130.0).abs() < 3.0,
        "steps end exactly, got {}",
        cx(30)
    );
}

#[test]
fn every_new_curve_renders_exact_ends() {
    // T007: each new curve starts and ends exactly on frames.
    let root = tmp("curves");
    for (tag, ease) in [
        ("bezier", "cubic-bezier(0.25,0.1,0.25,1)"),
        ("spring", "spring"),
        ("spring2", "spring(100,5)"),
        ("bounce", "bounce"),
        ("elastic", "elastic"),
        ("wiggle", "wiggle(2)"),
    ] {
        let folder = root.join(tag);
        std::fs::create_dir_all(&folder).unwrap();
        write(
            &folder,
            "composition.kdl",
            &format!(
                "{HEAD}{BOX}    tween target=0 property=\"x\" kind=\"to\" start=0 duration=1 to=110 ease=\"{ease}\"\n}}\n"
            ),
        );
        assert_eq!(run(&["check", "."], &folder).0, 0, "{tag} must validate");
        assert_eq!(run(&["render", ".", "-o", "out.mp4"], &folder).0, 0);
        let raw = decode_raw(&folder.join("out.mp4"), 60);
        let cx = |k: usize| red_centroid_x(frame_of(&raw, k, 160, 120), 160).unwrap();
        assert!(
            (cx(0) - 30.0).abs() < 3.0,
            "{tag} starts exactly, got {}",
            cx(0)
        );
        assert!(
            (cx(30) - 130.0).abs() < 3.0,
            "{tag} ends exactly, got {}",
            cx(30)
        );
    }
}

#[test]
fn motion_refusals_name_stop_or_curve_and_write_no_mp4() {
    // T001 + T007: every invalid motion variant fails check and render
    // with its code, and render writes no MP4.
    let root = tmp("refuse");
    let cases = [
        (
            "unordered",
            "    key target=0 property=\"x\" {\n        frame at=0 value=10\n        frame at=2 value=60\n        frame at=1 value=110\n    }\n",
            "E_BAD_MOTION",
        ),
        (
            "duplicate",
            "    key target=0 property=\"x\" {\n        frame at=0 value=10\n        frame at=1 value=60\n        frame at=1 value=110\n    }\n",
            "E_BAD_MOTION",
        ),
        (
            "badprop",
            "    key target=0 property=\"zebra\" {\n        frame at=0 value=10\n        frame at=1 value=60\n    }\n",
            "E_BAD_MOTION",
        ),
        (
            "badshape",
            "    key target=0 property=\"x\" {\n        frame at=0 value=\"abc\"\n        frame at=1 value=60\n    }\n",
            "E_BAD_MOTION",
        ),
        (
            "onestop",
            "    key target=0 property=\"x\" {\n        frame at=0 value=10\n    }\n",
            "E_BAD_MOTION",
        ),
        (
            "badease",
            "    tween target=0 property=\"x\" kind=\"to\" start=0 duration=1 to=10 ease=\"fast\"\n",
            "E_BAD_EASING",
        ),
        (
            "badsteps",
            "    tween target=0 property=\"x\" kind=\"to\" start=0 duration=1 to=10 ease=\"steps(0)\"\n",
            "E_BAD_EASING",
        ),
        (
            "badbezier",
            "    tween target=0 property=\"x\" kind=\"to\" start=0 duration=1 to=10 ease=\"cubic-bezier(2,0,0,1)\"\n",
            "E_BAD_EASING",
        ),
        (
            "badspring",
            "    key target=0 property=\"x\" {\n        frame at=0 value=10 ease=\"spring(0,5)\"\n        frame at=1 value=60\n    }\n",
            "E_BAD_EASING",
        ),
    ];
    for (tag, motion, code) in cases {
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
            out.contains(code),
            "{tag}: diagnostics must name {code}: {out}"
        );
        let (rstatus, _) = run(&["render", ".", "-o", "out.mp4"], &folder);
        assert_eq!(rstatus, 1, "{tag}: render must exit 1");
        assert!(!folder.join("out.mp4").exists(), "{tag}: no MP4 on refusal");
    }
}

#[test]
fn existing_tween_and_set_motion_rerenders_byte_identically() {
    // Regression: pre-Sprint-7 motion renders exactly as before —
    // byte-identical re-renders with pinned decoded values.
    let root = tmp("regression");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    write(
        &folder,
        "composition.kdl",
        &format!(
            "{HEAD}{BOX}    tween target=0 property=\"x\" kind=\"to\" start=0 duration=1 to=110 ease=\"in-quad\"\n    tween target=0 property=\"y\" kind=\"from-to\" start=0 duration=1 from=40 to=80 ease=\"out-back(1.7)\"\n    set target=0 property=\"x\" start=1.5 duration=0.25 value=50\n}}\n"
        ),
    );
    assert_eq!(run(&["check", "."], &folder).0, 0);
    assert_eq!(run(&["render", ".", "-o", "a.mp4"], &folder).0, 0);
    assert_eq!(run(&["render", ".", "-o", "b.mp4"], &folder).0, 0);
    let a = std::fs::read(folder.join("a.mp4")).unwrap();
    let b = std::fs::read(folder.join("b.mp4")).unwrap();
    assert_eq!(a, b, "unchanged motion re-renders byte-identically");
    // Pinned decoded values: in-quad midpoint x = 10 + 100/4 = 35
    // (center 55); the set holds x = 50 (center 70) at frame 45.
    let raw = decode_raw(&folder.join("a.mp4"), 60);
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
    assert!((cx(45) - 70.0).abs() < 3.0, "set hold pins, got {}", cx(45));
}
