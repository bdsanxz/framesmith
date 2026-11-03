//! Sprint 7 motion (T008) end-to-end: path travel places clips at
//! eased arc-length distances, `orient` turns them to the tangent,
//! morphs interpolate, refusals name the command with no MP4, and
//! pathless motion re-renders byte-identically with pinned values.
//!
//! All fixtures use the host `ffmpeg` (like `cli_motion`).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-n07-paths-{tag}-{}-{}",
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

/// Pixels strongly red: the box fill `#ff0000` unshaped by anything else.
fn red_pixels(frame: &[u8]) -> Vec<(u32, u32)> {
    let mut out = Vec::new();
    for (i, px) in frame.chunks_exact(3).enumerate() {
        if px[0] > 150 && px[1] < 100 && px[2] < 100 {
            out.push(((i % 160) as u32, (i / 160) as u32));
        }
    }
    out
}

fn red_centroid_x(frame: &[u8]) -> Option<f64> {
    let px = red_pixels(frame);
    if px.is_empty() {
        return None;
    }
    Some(px.iter().map(|(x, _)| *x as f64).sum::<f64>() / px.len() as f64)
}

fn red_bbox(frame: &[u8]) -> Option<(u32, u32, u32, u32)> {
    let px = red_pixels(frame);
    if px.is_empty() {
        return None;
    }
    Some((
        px.iter().map(|(x, _)| *x).min().unwrap(),
        px.iter().map(|(x, _)| *x).max().unwrap(),
        px.iter().map(|(_, y)| *y).min().unwrap(),
        px.iter().map(|(_, y)| *y).max().unwrap(),
    ))
}

const HEAD: &str =
    "composition name=\"t\" width=160 height=120 duration=2 fps=30 background=\"#000000\" version=1 {\n";
const BOX: &str = "    box x=10 y=40 width=40 height=40 fill=\"#ff0000\" start=0 duration=2\n";

#[test]
fn travel_places_the_clip_at_eased_arc_length() {
    // T008: travel "M10,40 L110,40" over [0, 2) linear. The box top-left
    // rides the path, so its center (20 right of x) probes 30/80/~128.
    let root = tmp("travel");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    write(
        &folder,
        "composition.kdl",
        &format!("{HEAD}{BOX}    tween target=0 path=\"M10,40 L110,40\" start=0 duration=2 ease=\"linear\"\n}}\n"),
    );
    assert_eq!(run(&["check", "proj"], &root).0, 0);
    assert_eq!(run(&["render", "proj", "-o", "out.mp4"], &root).0, 0);
    let raw = decode_raw(&root.join("out.mp4"), 60);
    let cx = |k: usize| red_centroid_x(frame_of(&raw, k, 160, 120)).unwrap();
    assert!((cx(0) - 30.0).abs() < 3.0, "travel starts, got {}", cx(0));
    assert!(
        (cx(30) - 80.0).abs() < 3.0,
        "travel midpoints, got {}",
        cx(30)
    );
    assert!((cx(59) - 128.3).abs() < 3.5, "travel ends, got {}", cx(59));
    // Re-renders are byte-identical.
    assert_eq!(run(&["render", "proj", "-o", "again.mp4"], &root).0, 0);
    assert_eq!(
        std::fs::read(root.join("out.mp4")).unwrap(),
        std::fs::read(root.join("again.mp4")).unwrap(),
        "path travel re-renders byte-identically"
    );
}

#[test]
fn orient_turns_the_clip_to_the_tangent() {
    // T008: a tall box (20x60) traveling due south with `orient`
    // rotates 90 degrees about its center: the pixels span 60 wide
    // and 20 tall. Without `orient` the box keeps its upright span.
    let root = tmp("orient");
    let mk = |orient: &str| {
        format!(
            "{HEAD}    box x=70 y=10 width=20 height=60 fill=\"#ff0000\" start=0 duration=2\n    tween target=0 path=\"M70,10 L70,70\" start=0 duration=2 {orient}\n}}\n"
        )
    };
    let on = root.join("on");
    let off = root.join("off");
    std::fs::create_dir_all(&on).unwrap();
    std::fs::create_dir_all(&off).unwrap();
    write(&on, "composition.kdl", &mk("orient=true"));
    write(&off, "composition.kdl", &mk(""));
    assert_eq!(run(&["check", "."], &on).0, 0);
    assert_eq!(run(&["check", "."], &off).0, 0);
    assert_eq!(run(&["render", ".", "-o", "out.mp4"], &on).0, 0);
    assert_eq!(run(&["render", ".", "-o", "out.mp4"], &off).0, 0);
    // Frame 30 (t=1s): the top-left sits at (70, 40), center (80, 70).
    let oriented = decode_raw(&on.join("out.mp4"), 60);
    let (x0, x1, y0, y1) = red_bbox(frame_of(&oriented, 30, 160, 120)).unwrap();
    let (w, h) = (x1 - x0 + 1, y1 - y0 + 1);
    assert!((w as f64 - 60.0).abs() < 5.0, "orient spans wide, got {w}");
    assert!((h as f64 - 20.0).abs() < 5.0, "orient spans flat, got {h}");
    let flat = decode_raw(&off.join("out.mp4"), 60);
    let (x0, x1, y0, y1) = red_bbox(frame_of(&flat, 30, 160, 120)).unwrap();
    let (w, h) = (x1 - x0 + 1, y1 - y0 + 1);
    assert!((w as f64 - 20.0).abs() < 5.0, "flat stays narrow, got {w}");
    assert!((h as f64 - 60.0).abs() < 5.0, "flat stays tall, got {h}");
}

#[test]
fn morph_moves_through_the_interpolated_path() {
    // T008: "M10,40 L110,40" morphing to "M10,80 L110,80" over [0, 2).
    // Halfway the clip rides y=60 (box top), center row 80.
    let root = tmp("morph");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    write(
        &folder,
        "composition.kdl",
        &format!("{HEAD}{BOX}    tween target=0 path=\"M10,40 L110,40\" path-to=\"M10,80 L110,80\" start=0 duration=2\n}}\n"),
    );
    assert_eq!(run(&["check", "proj"], &root).0, 0);
    assert_eq!(run(&["render", "proj", "-o", "out.mp4"], &root).0, 0);
    let raw = decode_raw(&root.join("out.mp4"), 60);
    let row = |k: usize| {
        let px = red_pixels(frame_of(&raw, k, 160, 120));
        px.iter().map(|(_, y)| *y as f64).sum::<f64>() / px.len() as f64
    };
    assert!(
        (row(30) - 80.0).abs() < 3.0,
        "morph midpoints, got {}",
        row(30)
    );
    assert!(
        (row(59) - 100.0).abs() < 4.0,
        "morph lands low, got {}",
        row(59)
    );
    assert_eq!(run(&["render", "proj", "-o", "again.mp4"], &root).0, 0);
    assert_eq!(
        std::fs::read(root.join("out.mp4")).unwrap(),
        std::fs::read(root.join("again.mp4")).unwrap(),
        "morphs re-render byte-identically"
    );
}

#[test]
fn path_refusals_name_the_command_with_no_mp4() {
    // T008: unparseable paths and cross-structure morphs fail check
    // and render naming the command; render writes no MP4.
    let root = tmp("refuse");
    let cases = [
        (
            "bad-command",
            "    tween target=0 path=\"M10,40 H90\" start=0 duration=1\n",
            "`H`",
        ),
        (
            "cross-structure",
            "    tween target=0 path=\"M10,40 L110,40\" path-to=\"M10,40 C60,0 90,80 110,40\" start=0 duration=1\n",
            "`C`",
        ),
        (
            "key-cross-structure",
            "    key target=0 {\n        frame at=0 path=\"M10,40 L110,40\"\n        frame at=1 path=\"M10,40 Q80,0 110,40\"\n    }\n",
            "stop 2",
        ),
        (
            "path-to-alone",
            "    tween target=0 path-to=\"M10,40 L110,40\" start=0 duration=1\n",
            "`path-to`",
        ),
        (
            "orient-alone",
            "    tween target=0 property=\"x\" kind=\"to\" to=10 orient=true start=0 duration=1\n",
            "`orient`",
        ),
        (
            "property-with-path",
            "    tween target=0 property=\"x\" path=\"M10,40 L110,40\" start=0 duration=1\n",
            "`property`",
        ),
    ];
    for (tag, motion, named) in cases {
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
        assert!(
            out.contains(named),
            "{tag}: diagnostics must name {named}: {out}"
        );
        let (rstatus, _) = run(&["render", ".", "-o", "out.mp4"], &folder);
        assert_eq!(rstatus, 1, "{tag}: render must exit 1");
        assert!(!folder.join("out.mp4").exists(), "{tag}: no MP4 on refusal");
    }
}

#[test]
fn pathless_motion_renders_byte_identically_with_pinned_values() {
    // T008 regression: existing tween/set and easing behavior renders
    // exactly as before — pinned pixels plus byte-identical bytes.
    let root = tmp("regression");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    write(
        &folder,
        "composition.kdl",
        &format!("{HEAD}{BOX}    tween target=0 property=\"x\" kind=\"from-to\" from=10 to=110 start=0 duration=1 ease=\"in-quad\"\n    set target=0 property=\"y\" start=1 duration=1 value=60\n}}\n"),
    );
    assert_eq!(run(&["check", "."], &folder).0, 0);
    assert_eq!(run(&["render", ".", "-o", "out.mp4"], &folder).0, 0);
    let raw = decode_raw(&folder.join("out.mp4"), 60);
    let cx = |k: usize| red_centroid_x(frame_of(&raw, k, 160, 120)).unwrap();
    // In-quad eases 10 -> 110: frame 15 sits at 10 + 100/4 = 35
    // (center 55), frame 30 lands on 110 (center 130).
    assert!(
        (cx(15) - 55.0).abs() < 3.0,
        "eased midpoint pins, got {}",
        cx(15)
    );
    assert!(
        (cx(30) - 130.0).abs() < 3.0,
        "tween end pins, got {}",
        cx(30)
    );
    assert_eq!(run(&["render", ".", "-o", "again.mp4"], &folder).0, 0);
    assert_eq!(
        std::fs::read(folder.join("out.mp4")).unwrap(),
        std::fs::read(folder.join("again.mp4")).unwrap(),
        "pathless motion re-renders byte-identically"
    );
}
