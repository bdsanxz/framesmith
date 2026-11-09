//! Sprint 13 check, T005: snapshot PNGs end-to-end.
//!
//! Covers every acceptance criterion: `--at` times land PNGs named
//! for their times; `--every` lands PNGs across the composition; a
//! `--from`/`--to` range lands exactly the frames inside;
//! between-frame times snap to exact frames and report the frame used;
//! each PNG decodes pixel-identical to the same full-render (`pngseq`)
//! frame; no MP4 is ever written; and a bad time, an empty range, or
//! an unwritable directory fails naming the flag with no PNG written.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-n13-t005-{tag}-{}-{}",
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

fn proj(root: &Path, body: &str) -> PathBuf {
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    write(&folder, "composition.kdl", body);
    folder
}

/// A moving composition at 10 fps: the red box slides 0 to 140 px over
/// one second, so every frame differs and each snapshot must land on
/// its exact frame (a wrong frame decodes to different pixels).
const MOVING: &str = "composition name=\"t\" width=160 height=120 duration=1 fps=10 background=\"#112233\" version=1 {\n    box id=\"m\" start=0 duration=1 x=0 y=40 width=20 height=20 fill=\"#ff0000\"\n    tween target=0 property=\"x\" kind=\"to\" start=0 duration=1 to=140 ease=\"linear\"\n}\n";

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
    assert!(out.status.success(), "decode failed for {}", path.display());
    assert_eq!(out.stdout.len(), w as usize * h as usize * 3);
    out.stdout
}

/// Sorted PNG file names directly inside `dir` (empty when missing).
fn png_names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(|e| {
                let name = e.ok()?.file_name().to_string_lossy().into_owned();
                name.ends_with(".png").then_some(name)
            })
            .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

/// No MP4 may ever appear inside a snapshot output directory.
fn assert_no_mp4(dir: &Path) {
    let mut mp4s = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) == Some("mp4") {
                mp4s.push(p);
            }
        }
    }
    assert!(mp4s.is_empty(), "snapshot wrote an MP4: {mp4s:?}");
}

#[test]
fn snapshot_at_times_land_pngs_named_for_their_times() {
    let root = tmp("at");
    proj(&root, MOVING);
    let (code, out) = run(
        &["snapshot", "--at", "0,0.5,0.9", "--out", "snaps", "proj"],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    assert_eq!(
        png_names(&root.join("snaps")),
        vec!["snap-0.5s.png", "snap-0.9s.png", "snap-0s.png"],
        "{out}"
    );
    assert!(
        out.contains("snapshot: wrote snap-0s.png (frame 0 at 0s)"),
        "{out}"
    );
    assert!(
        out.contains("snapshot: wrote snap-0.5s.png (frame 5 at 0.5s)"),
        "{out}"
    );
    assert!(
        out.contains("snapshot: wrote snap-0.9s.png (frame 9 at 0.9s)"),
        "{out}"
    );
    assert_no_mp4(&root.join("snaps"));
}

#[test]
fn snapshot_every_lands_pngs_across_the_composition() {
    let root = tmp("every");
    proj(&root, MOVING);
    let (code, out) = run(
        &["snapshot", "--every", "0.4", "--out", "snaps", "proj"],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    assert_eq!(
        png_names(&root.join("snaps")),
        vec!["snap-0.4s.png", "snap-0.8s.png", "snap-0s.png"],
        "{out}"
    );
    assert!(out.contains("frame 4 at 0.4s"), "{out}");
    assert!(out.contains("frame 8 at 0.8s"), "{out}");
    assert_no_mp4(&root.join("snaps"));
}

#[test]
fn snapshot_range_lands_exactly_the_frames_inside() {
    let root = tmp("range");
    proj(&root, MOVING);
    let (code, out) = run(
        &[
            "snapshot", "--from", "0.2", "--to", "0.5", "--out", "snaps", "proj",
        ],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    assert_eq!(
        png_names(&root.join("snaps")),
        vec!["snap-0.2s.png", "snap-0.3s.png", "snap-0.4s.png"],
        "{out}"
    );
    assert!(out.contains("frame 2 at 0.2s"), "{out}");
    assert!(out.contains("frame 4 at 0.4s"), "{out}");
    assert_no_mp4(&root.join("snaps"));
}

#[test]
fn snapshot_between_frame_time_snaps_and_reports_the_frame_used() {
    let root = tmp("snap");
    proj(&root, MOVING);
    let (code, out) = run(
        &["snapshot", "--at", "0.33", "--out", "snaps", "proj"],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    assert_eq!(
        png_names(&root.join("snaps")),
        vec!["snap-0.3s.png"],
        "{out}"
    );
    assert!(
        out.contains("time 0.33s is between frames; using frame 3 at 0.3s"),
        "{out}"
    );
    assert_no_mp4(&root.join("snaps"));
}

#[test]
fn snapshot_pngs_are_pixel_identical_to_full_render_frames() {
    let root = tmp("pixel");
    proj(&root, MOVING);
    let (code, out) = run(
        &["render", "proj", "--format", "pngseq", "-o", "seq"],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(
        &["snapshot", "--at", "0,0.5,0.9", "--out", "snaps", "proj"],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    // The full render really moves: neighboring frames differ, so an
    // off-by-one snapshot could not pass the comparisons below.
    let seq: Vec<Vec<u8>> = (0..10)
        .map(|i| decode_png(&root.join(format!("seq/frame-{i:04}.png")), 160, 120))
        .collect();
    assert_ne!(
        seq[0], seq[5],
        "seeded motion must move between frames 0 and 5"
    );
    assert_ne!(
        seq[5], seq[9],
        "seeded motion must move between frames 5 and 9"
    );
    for (snap, frame) in [
        ("snap-0s.png", 0),
        ("snap-0.5s.png", 5),
        ("snap-0.9s.png", 9),
    ] {
        let got = decode_png(&root.join("snaps").join(snap), 160, 120);
        assert_eq!(
            got, seq[frame],
            "{snap} must decode pixel-identical to full-render frame {frame}"
        );
    }
    assert_no_mp4(&root.join("snaps"));
}

#[test]
fn snapshot_range_pngs_match_full_render_frames() {
    let root = tmp("pixel-range");
    proj(&root, MOVING);
    let (code, out) = run(
        &["render", "proj", "--format", "pngseq", "-o", "seq"],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(
        &[
            "snapshot", "--from", "0.2", "--to", "0.5", "--out", "snaps", "proj",
        ],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    for (snap, frame) in [
        ("snap-0.2s.png", 2),
        ("snap-0.3s.png", 3),
        ("snap-0.4s.png", 4),
    ] {
        let got = decode_png(&root.join("snaps").join(snap), 160, 120);
        let want = decode_png(&root.join(format!("seq/frame-{frame:04}.png")), 160, 120);
        assert_eq!(got, want, "{snap} must match full-render frame {frame}");
    }
    assert_no_mp4(&root.join("snaps"));
}

#[test]
fn snapshot_json_reports_frames_and_files() {
    let root = tmp("json");
    proj(&root, MOVING);
    let (code, out) = run(
        &[
            "snapshot", "--json", "--at", "0,0.5", "--out", "snaps", "proj",
        ],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("\"command\":\"snapshot\""), "{out}");
    assert!(out.contains("\"status\":\"ok\""), "{out}");
    assert!(out.contains("\"frame\":0"), "{out}");
    assert!(out.contains("\"frame\":5"), "{out}");
    assert!(out.contains("\"file\":\"snap-0s.png\""), "{out}");
    assert!(out.contains("\"file\":\"snap-0.5s.png\""), "{out}");
    assert_eq!(
        png_names(&root.join("snaps")),
        vec!["snap-0.5s.png", "snap-0s.png"],
        "{out}"
    );
}

#[test]
fn snapshot_bad_time_refuses_naming_the_flag_with_nothing_written() {
    let root = tmp("bad-time");
    proj(&root, MOVING);
    let (code, out) = run(
        &["snapshot", "--at", "abc", "--out", "snaps", "proj"],
        &root,
    );
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("--at"), "{out}");
    assert!(out.contains("no PNG was written"), "{out}");
    assert!(
        !root.join("snaps").exists(),
        "bad time must write nothing:\n{out}"
    );
}

#[test]
fn snapshot_outside_time_refuses_naming_the_flag_with_nothing_written() {
    let root = tmp("outside");
    proj(&root, MOVING);
    let (code, out) = run(
        &["snapshot", "--at", "2.5", "--out", "snaps", "proj"],
        &root,
    );
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("--at 2.5s is outside"), "{out}");
    assert!(out.contains("no PNG was written"), "{out}");
    assert!(
        !root.join("snaps").exists(),
        "outside time must write nothing:\n{out}"
    );
}

#[test]
fn snapshot_empty_and_backward_ranges_refuse_with_nothing_written() {
    let root = tmp("empty");
    proj(&root, MOVING);
    let (code, out) = run(
        &[
            "snapshot", "--from", "0.5", "--to", "0.5", "--out", "s1", "proj",
        ],
        &root,
    );
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("--from/--to"), "{out}");
    assert!(out.contains("empty range"), "{out}");
    assert!(
        !root.join("s1").exists(),
        "empty range must write nothing:\n{out}"
    );
    let (code, out) = run(
        &[
            "snapshot", "--from", "0.8", "--to", "0.2", "--out", "s2", "proj",
        ],
        &root,
    );
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("--from/--to"), "{out}");
    assert!(out.contains("runs backward"), "{out}");
    assert!(
        !root.join("s2").exists(),
        "backward range must write nothing:\n{out}"
    );
}

#[test]
fn snapshot_bad_every_refuses_naming_the_flag_with_nothing_written() {
    let root = tmp("bad-every");
    proj(&root, MOVING);
    for every in ["0", "-1", "soon"] {
        let (code, out) = run(
            &["snapshot", "--every", every, "--out", "snaps", "proj"],
            &root,
        );
        assert_eq!(code, 1, "--every {every}:\n{out}");
        assert!(out.contains("--every"), "{out}");
        assert!(out.contains("no PNG was written"), "{out}");
    }
    assert!(
        png_names(&root.join("snaps")).is_empty(),
        "bad --every must write no PNG"
    );
}

#[test]
fn snapshot_unwritable_directory_refuses_naming_the_flag_with_nothing_written() {
    let root = tmp("unwritable");
    proj(&root, MOVING);
    // A regular file where the output directory should hang: no user
    // (not even root) can create files beneath it, so the refusal is
    // permission-independent.
    write(&root, "blocker", "in the way");
    let (code, out) = run(
        &["snapshot", "--at", "0", "--out", "blocker/sub", "proj"],
        &root,
    );
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("--out"), "{out}");
    assert!(out.contains("not writable"), "{out}");
    assert!(out.contains("no PNG was written"), "{out}");
    assert!(
        !root.join("blocker/sub").exists(),
        "unwritable dir must stay empty:\n{out}"
    );
}

#[test]
fn snapshot_invalid_and_missing_projects_refuse_with_nothing_written() {
    let root = tmp("invalid");
    proj(&root, INVALID);
    let (code, out) = run(&["snapshot", "--at", "0", "--out", "snaps", "proj"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("is invalid"), "{out}");
    assert!(out.contains("No PNG was written"), "{out}");
    assert!(
        !root.join("snaps").exists(),
        "invalid project must write nothing:\n{out}"
    );
    let (code, out) = run(
        &["snapshot", "--at", "0", "--out", "elsewhere", "nope"],
        &root,
    );
    assert_ne!(code, 0, "{out}");
    assert!(out.contains("No PNG was written"), "{out}");
    assert!(
        !root.join("elsewhere").exists(),
        "missing project must write nothing:\n{out}"
    );
}

#[test]
fn snapshot_shape_errors_are_usage_errors() {
    let root = tmp("usage");
    proj(&root, MOVING);
    let (code, out) = run(&["snapshot", "--out", "snaps", "proj"], &root);
    assert_eq!(code, 2, "{out}");
    assert!(out.contains("need one of --at"), "{out}");
    let (code, out) = run(
        &[
            "snapshot", "--at", "0", "--every", "0.5", "--out", "snaps", "proj",
        ],
        &root,
    );
    assert_eq!(code, 2, "{out}");
    assert!(out.contains("exactly one of --at"), "{out}");
    let (code, out) = run(
        &["snapshot", "--from", "0", "--out", "snaps", "proj"],
        &root,
    );
    assert_eq!(code, 2, "{out}");
    assert!(out.contains("--from together with --to"), "{out}");
    assert!(
        !root.join("snaps").exists(),
        "usage errors must write nothing:\n{out}"
    );
}
