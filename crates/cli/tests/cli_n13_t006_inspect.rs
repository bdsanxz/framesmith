//! Sprint 13 check, T006: inspect with commands end-to-end.
//!
//! Covers every acceptance criterion: `keyframes` lists computed
//! per-frame values matching rendered motion; `compare` writes a
//! side-by-side file plus an SSIM score against the 0.96 bar;
//! identical projects compare at SSIM 1.0 with the file still written;
//! `grade-compare` shows the graded and ungraded halves split;
//! `doctor` reports environment and project health facts; `benchmark`
//! reports timings with versions and date, labeled machine-specific;
//! and a missing project, bad flag, or unrunnable reference fails
//! naming the cause. Every command leaves project sources untouched
//! and writes no MP4.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-n13-t006-{tag}-{}-{}",
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
    let mut text = String::from(String::from_utf8_lossy(&out.stdout));
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    (out.status.code().unwrap_or(-1), text)
}

fn write(folder: &Path, name: &str, content: &str) {
    std::fs::write(folder.join(name), content).unwrap();
}

fn proj(root: &Path, name: &str, body: &str) -> PathBuf {
    let folder = root.join(name);
    std::fs::create_dir_all(&folder).unwrap();
    write(&folder, "composition.kdl", body);
    folder
}

/// A moving composition at 10 fps: the red box slides 0 to 140 px over
/// one second, so frame `k` must report `x = 14 * k`.
const MOVING: &str = "composition name=\"t\" width=160 height=120 duration=1 fps=10 background=\"#112233\" version=1 {\n    box id=\"m\" start=0 duration=1 x=0 y=40 width=20 height=20 fill=\"#ff0000\"\n    tween target=0 property=\"x\" kind=\"to\" start=0 duration=1 to=140 ease=\"linear\"\n}\n";

const STATIC_RED: &str = "composition name=\"t\" width=160 height=120 duration=1 fps=10 background=\"#112233\" version=1 {\n    box id=\"m\" start=0 duration=1 x=0 y=40 width=20 height=20 fill=\"#ff0000\"\n}\n";

const STATIC_GREEN: &str = "composition name=\"t\" width=160 height=120 duration=1 fps=10 background=\"#112233\" version=1 {\n    box id=\"m\" start=0 duration=1 x=0 y=40 width=20 height=20 fill=\"#00ff00\"\n}\n";

const GRADED: &str = "composition name=\"t\" width=160 height=120 duration=1 fps=10 background=\"#112233\" version=1 {\n grade preset=\"noir\"\n box id=\"m\" start=0 duration=1 x=0 y=40 width=20 height=20 fill=\"#ff0000\"\n}\n";

const INVALID: &str = "composition name=\"t\" width=640 height=480 duration=0 background=\"#000\" version=1 {\n    box id=\"a\" start=0 duration=0 x=100 y=100 width=100 height=100 fill=\"#ff0000\"\n}\n";

/// Decoded RGB24 pixels of one PNG file (via host FFmpeg, like the
/// `pngseq` render tests decode pictures).
fn decode_png(path: &Path, w: u32, h: u32) -> Vec<u8> {
    let out = Command::new("ffmpeg")
        .args([
            "-y",
            "-v",
            "error",
            "-i",
            &path.display().to_string(),
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgb24",
            "-",
        ])
        .output()
        .expect("decode png");
    assert!(
        out.status.success(),
        "decode failed for {}",
        path.display()
    );
    assert_eq!(out.stdout.len(), w as usize * h as usize * 3);
    out.stdout
}

/// Every `*.png` / `*.mp4` file under `dir`, recursively: inspection
/// commands must never leave pictures or movies behind in a project.
fn media_outputs(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if let Some(ext) = p.extension().and_then(|x| x.to_str()) {
                if ext == "png" || ext == "mp4" {
                    found.push(p);
                }
            }
        }
    }
    found.sort();
    found
}

/// Keyframe rows of one `keyframes` run: `(frame, time, value)` for
/// lines of the form `frame K at Ts: <clip> <prop>=<value>`.
fn keyframe_rows(out: &str) -> Vec<(u64, f64, String)> {
    let mut rows = Vec::new();
    for line in out.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("frame ") else {
            continue;
        };
        let Some((k, rest)) = rest.split_once(" at ") else {
            continue;
        };
        let Some((t, rest)) = rest.split_once("s: ") else {
            continue;
        };
        let Some((_, value)) = rest.split_once('=') else {
            continue;
        };
        rows.push((k.parse().unwrap(), t.parse().unwrap(), value.to_string()));
    }
    rows
}

#[test]
fn keyframes_lists_computed_values_per_frame() {
    let root = tmp("kf");
    proj(&root, "proj", MOVING);
    let (code, out) = run(&["keyframes", "--clip", "m", "--property", "x", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("keyframes for clip"), "{out}");
    let rows = keyframe_rows(&out);
    assert_eq!(rows.len(), 10, "{out}");
    for (k, t, v) in &rows {
        assert!((*t - *k as f64 / 10.0).abs() < 1e-9, "{out}");
        let got: f64 = v.parse().expect("numeric value");
        assert!((got - *k as f64 * 14.0).abs() < 1e-6, "{out}");
    }
    assert!(out.contains("frame 5 at 0.5s: m x=70"), "{out}");
    assert!(out.contains("frame 9 at 0.9s: m x=126"), "{out}");
}

#[test]
fn keyframes_json_and_alias_and_fill() {
    let root = tmp("kfjson");
    proj(&root, "proj", MOVING);
    let (code, out) = run(
        &["keyframes", "--json", "--clip", "m", "--property", "x", "proj"],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    let doc = out.lines().rfind(|l| l.contains("\"command\":\"keyframes\"")).unwrap();
    assert!(doc.contains("\"clip\":\"m\""), "{doc}");
    assert!(doc.contains("\"property\":\"x\""), "{doc}");
    assert!(doc.contains("\"frame\":5,\"time\":0.5,\"value\":70"), "{doc}");
    // A paint property reports hex, and an aliased spelling resolves.
    let (code, out) = run(&["keyframes", "--clip", "0", "--property", "fill", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("m fill=#ff0000"), "{out}");
    let (code, out) = run(
        &["keyframes", "--clip", "m", "--property", "rotate-x", "proj"],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("m rotation-x=0"), "{out}");
}

#[test]
fn keyframes_match_rendered_motion() {
    // The computed frame-5 value (x=70) must land where the rendered
    // frame-5 picture puts the red box: centroid near x=80 (box spans
    // 70..90 at y=40..60).
    let root = tmp("kfrender");
    proj(&root, "proj", MOVING);
    let (code, out) = run(&["snapshot", "--at", "0.5", "--out", "snaps", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    let rgb = decode_png(&root.join("snaps").join("snap-0.5s.png"), 160, 120);
    let mut sx = 0u64;
    let mut n = 0u64;
    for y in 0..120 {
        for x in 0..160 {
            let o = (y * 160 + x) * 3;
            if rgb[o] > 200 && rgb[o + 1] < 80 && rgb[o + 2] < 80 {
                sx += x as u64;
                n += 1;
            }
        }
    }
    assert!(n > 100, "red box missing from frame 5");
    let cx = sx as f64 / n as f64;
    assert!((cx - 80.0).abs() <= 3.0, "red centroid x={cx}, want ~80");
}

#[test]
fn keyframes_refusals_name_the_cause() {
    let root = tmp("kfref");
    proj(&root, "proj", MOVING);
    proj(&root, "bad", INVALID);
    let (code, out) = run(&["keyframes", "--clip", "m", "--property", "skew", "proj"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("--property"), "{out}");
    assert!(out.contains("nothing was listed"), "{out}");
    let (code, out) = run(&["keyframes", "--clip", "ghost", "--property", "x", "proj"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("--clip"), "{out}");
    let (code, out) = run(&["keyframes", "--property", "x", "proj"], &root);
    assert_eq!(code, 2, "{out}");
    assert!(out.contains("--clip"), "{out}");
    let (code, out) = run(&["keyframes", "--clip", "m", "proj"], &root);
    assert_eq!(code, 2, "{out}");
    assert!(out.contains("--property"), "{out}");
    let (code, out) = run(&["keyframes", "--clip", "m", "--property", "bogus", "proj"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("--property"), "{out}");
    let (code, out) = run(&["keyframes", "--clip", "m", "--property", "x", "bad"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("invalid"), "{out}");
    let (code, out) = run(&["keyframes", "--clip", "m", "--property", "x", "missing"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("missing"), "{out}");
    let (code, out) = run(&["keyframes", "--wat", "1", "--clip", "m", "--property", "x", "proj"], &root);
    assert_eq!(code, 2, "{out}");
    assert!(out.contains("--wat"), "{out}");
}

#[test]
fn compare_identical_scores_one_and_writes_file() {
    let root = tmp("cmp1");
    proj(&root, "pa", STATIC_RED);
    proj(&root, "pb", STATIC_RED);
    let (code, out) = run(&["compare", "-o", "cmp.png", "pa", "pb"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("ssim 1 against bar 0.96: pass"), "{out}");
    assert!(out.contains("left pa, right pb"), "{out}");
    let rgb = decode_png(&root.join("cmp.png"), 320, 120);
    assert_eq!(rgb.len(), 320 * 120 * 3);
    // The halves of identical inputs decode byte-equal.
    let mut left = Vec::with_capacity(160 * 120 * 3);
    let mut right = Vec::with_capacity(160 * 120 * 3);
    for y in 0..120 {
        left.extend_from_slice(&rgb[y * 960..y * 960 + 480]);
        right.extend_from_slice(&rgb[y * 960 + 480..y * 960 + 960]);
    }
    assert_eq!(left, right);
    let (code, out) = run(&["compare", "--json", "-o", "cmpj.png", "pa", "pb"], &root);
    assert_eq!(code, 0, "{out}");
    let doc = out.lines().rfind(|l| l.contains("\"command\":\"compare\"")).unwrap();
    assert!(doc.contains("\"ssim\":1"), "{doc}");
    assert!(doc.contains("\"bar\":0.96"), "{doc}");
    assert!(doc.contains("\"pass\":true"), "{doc}");
}

#[test]
fn compare_different_scores_below_bar() {
    let root = tmp("cmp2");
    proj(&root, "pa", STATIC_RED);
    proj(&root, "pb", STATIC_GREEN);
    let (code, out) = run(&["compare", "--at", "0", "-o", "cmp.png", "pa", "pb"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("against bar 0.96: below bar"), "{out}");
    assert!(root.join("cmp.png").is_file());
    assert!(media_outputs(&root.join("pa")).is_empty());
    assert!(media_outputs(&root.join("pb")).is_empty());
    let score: f64 = out
        .lines()
        .find(|l| l.contains("ssim "))
        .and_then(|l| l.split("ssim ").nth(1))
        .and_then(|l| l.split_whitespace().next())
        .unwrap()
        .parse()
        .unwrap();
    assert!(score < 0.96, "score={score}");
}

#[test]
fn compare_refusals_name_the_cause_and_write_nothing() {
    let root = tmp("cmpref");
    proj(&root, "pa", STATIC_RED);
    proj(&root, "pb", STATIC_RED);
    proj(&root, "wide", &STATIC_RED.replace("width=160 height=120", "width=320 height=120"));
    proj(&root, "bad", INVALID);
    let (code, out) = run(&["compare", "-o", "cmp.png", "pa", "missing"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("missing"), "{out}");
    assert!(!root.join("cmp.png").exists());
    let (code, out) = run(&["compare", "-o", "cmp2.png", "pa", "wide"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("160x120"), "{out}");
    assert!(out.contains("320x120"), "{out}");
    assert!(!root.join("cmp2.png").exists());
    let (code, out) = run(&["compare", "pa", "pb"], &root);
    assert_eq!(code, 2, "{out}");
    assert!(out.contains("-o"), "{out}");
    let (code, out) = run(&["compare", "--at", "soon", "-o", "cmp3.png", "pa", "pb"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("--at"), "{out}");
    assert!(!root.join("cmp3.png").exists());
    let (code, out) = run(&["compare", "--at", "5", "-o", "cmp4.png", "pa", "pb"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("--at"), "{out}");
    assert!(!root.join("cmp4.png").exists());
    let (code, out) = run(&["compare", "-o", "cmp5.png", "pa", "bad"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("invalid"), "{out}");
    assert!(!root.join("cmp5.png").exists());
    let (code, out) = run(&["compare", "-o", "cmp6.png", "pa"], &root);
    assert_eq!(code, 2, "{out}");
}

#[test]
fn grade_compare_splits_halves() {
    let root = tmp("gc");
    proj(&root, "gp", GRADED);
    let (code, out) = run(&["grade-compare", "-o", "gc.png", "gp"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("left ungraded"), "{out}");
    assert!(out.contains("right graded"), "{out}");
    let rgb = decode_png(&root.join("gc.png"), 320, 120);
    let mut left = Vec::with_capacity(160 * 120 * 3);
    let mut right = Vec::with_capacity(160 * 120 * 3);
    for y in 0..120 {
        left.extend_from_slice(&rgb[y * 960..y * 960 + 480]);
        right.extend_from_slice(&rgb[y * 960 + 480..y * 960 + 960]);
    }
    assert_ne!(left, right, "graded and ungraded halves must differ");
    let (code, out) = run(&["grade-compare", "--json", "-o", "gcj.png", "gp"], &root);
    assert_eq!(code, 0, "{out}");
    let doc = out.lines().rfind(|l| l.contains("\"command\":\"grade-compare\"")).unwrap();
    assert!(doc.contains("\"status\":\"ok\""), "{doc}");
    assert!(doc.contains("\"file\""), "{doc}");
}

#[test]
fn grade_compare_refusals_name_the_cause() {
    let root = tmp("gcref");
    proj(&root, "gp", GRADED);
    proj(&root, "plain", STATIC_RED);
    proj(&root, "bad", INVALID);
    let (code, out) = run(&["grade-compare", "-o", "gc.png", "plain"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("no composition or lane grade"), "{out}");
    assert!(!root.join("gc.png").exists());
    let (code, out) = run(&["grade-compare", "-o", "gc2.png", "missing"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("missing"), "{out}");
    assert!(!root.join("gc2.png").exists());
    let (code, out) = run(&["grade-compare", "-o", "gc3.png", "bad"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("invalid"), "{out}");
    assert!(!root.join("gc3.png").exists());
    let (code, out) = run(&["grade-compare", "gp"], &root);
    assert_eq!(code, 2, "{out}");
    assert!(out.contains("-o"), "{out}");
    let (code, out) = run(&["grade-compare", "--at", "9", "-o", "gc4.png", "gp"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("--at"), "{out}");
    assert!(!root.join("gc4.png").exists());
}

#[test]
fn doctor_reports_machine_and_project() {
    let root = tmp("doc");
    proj(&root, "proj", STATIC_RED);
    let (code, out) = run(&["doctor"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("doctor: framesmith "), "{out}");
    assert!(out.contains("doctor: ffmpeg: "), "{out}");
    assert!(out.contains("doctor: gpu renderer "), "{out}");
    assert!(out.contains("doctor: text shaping "), "{out}");
    assert!(out.contains("doctor: host "), "{out}");
    let (code, out) = run(&["doctor", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("is valid"), "{out}");
    assert!(out.contains("composition \"t\""), "{out}");
    assert!(out.contains("160x120"), "{out}");
    let (code, out) = run(&["doctor", "--json", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    let doc = out.lines().rfind(|l| l.contains("\"command\":\"doctor\"")).unwrap();
    assert!(doc.contains("\"status\":\"valid\""), "{doc}");
    assert!(doc.contains("\"framesmith\":"), "{doc}");
    assert!(doc.contains("\"compositions\":"), "{doc}");
}

#[test]
fn doctor_refusals_name_the_cause() {
    let root = tmp("docref");
    proj(&root, "proj", STATIC_RED);
    proj(&root, "bad", INVALID);
    let (code, out) = run(&["doctor", "missing"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("missing"), "{out}");
    let (code, out) = run(&["doctor", "bad"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("is invalid"), "{out}");
    let (code, out) = run(&["doctor", "proj", "extra"], &root);
    assert_eq!(code, 2, "{out}");
}

#[test]
fn benchmark_reports_timings_versions_and_date() {
    let root = tmp("bench");
    let (code, out) = run(&["benchmark", "--out", "bench-out"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("benchmark: framesmith "), "{out}");
    assert!(out.contains("benchmark: reference \"still\""), "{out}");
    assert!(out.contains("benchmark: reference \"slide\""), "{out}");
    assert!(out.contains("(deterministic yes)"), "{out}");
    assert!(out.contains("machine-specific"), "{out}");
    assert!(out.contains("date 20"), "{out}");
    let outs: Vec<String> = std::fs::read_dir(root.join("bench-out"))
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert!(outs.contains(&"benchmark-still.png".to_string()), "{outs:?}");
    assert!(outs.contains(&"benchmark-slide.png".to_string()), "{outs:?}");
    assert!(!outs.iter().any(|n| n.ends_with(".mp4")), "{outs:?}");
    let (code, out) = run(&["benchmark", "--json"], &root);
    assert_eq!(code, 0, "{out}");
    let doc = out.lines().rfind(|l| l.contains("\"command\":\"benchmark\"")).unwrap();
    assert!(doc.contains("\"machine_specific\":true"), "{doc}");
    assert!(doc.contains("\"references\":"), "{doc}");
    assert!(doc.contains("\"date\":"), "{doc}");
    let (code, out) = run(&["benchmark", "--bogus"], &root);
    assert_eq!(code, 2, "{out}");
}

#[test]
fn inspect_commands_leave_projects_read_only() {
    let root = tmp("ro");
    let pa = proj(&root, "pa", STATIC_RED);
    let pb = proj(&root, "pb", STATIC_RED);
    let gp = proj(&root, "gp", GRADED);
    let before: Vec<(PathBuf, Vec<u8>)> = [&pa, &pb, &gp]
        .iter()
        .map(|d| (d.join("composition.kdl"), std::fs::read(d.join("composition.kdl")).unwrap()))
        .collect();
    let (code, out) = run(&["keyframes", "--clip", "m", "--property", "x", "pa"], &root);
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(&["compare", "-o", "cmp.png", "pa", "pb"], &root);
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(&["grade-compare", "-o", "gc.png", "gp"], &root);
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(&["doctor", "pa"], &root);
    assert_eq!(code, 0, "{out}");
    for (path, bytes) in &before {
        assert_eq!(&std::fs::read(path).unwrap(), bytes, "{path:?} changed");
    }
    for d in [&pa, &pb, &gp] {
        assert!(media_outputs(d).is_empty(), "{d:?} gained outputs");
    }
    assert!(root.join("cmp.png").is_file());
    assert!(root.join("gc.png").is_file());
}
