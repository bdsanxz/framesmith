//! Sprint 6 T002 end-to-end: relative starts through `check` and `render`.
//!
//! Exact, offset, and negative-overlap followers land on their resolved
//! windows in the MP4 (checked per frame at boundaries); cycles,
//! ambiguity, and before-zero starts refuse with file, line, column,
//! code, and fix, and `render` repeats the refusal writing no MP4.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-n06t002-{tag}-{}-{}",
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
        .expect("decode mp4 video");
    assert!(out.status.success());
    out.stdout
}

/// Center pixel of one frame (solid fills stay solid at the center even
/// after lossy encode; the tolerance absorbs codec drift).
fn center(frames: &[u8], frame: usize, w: u32, h: u32) -> (u8, u8, u8) {
    let stride = (w * h * 3) as usize;
    let base = frame * stride + ((h as usize / 2 * w as usize + w as usize / 2) * 3);
    (frames[base], frames[base + 1], frames[base + 2])
}

fn assert_near_red(px: (u8, u8, u8), what: &str) {
    assert!(
        px.0 > 180 && px.1 < 100 && px.2 < 100,
        "{what}: expected red, found {px:?}"
    );
}

fn assert_near_blue(px: (u8, u8, u8), what: &str) {
    assert!(
        px.2 > 180 && px.0 < 100 && px.1 < 100,
        "{what}: expected blue, found {px:?}"
    );
}

const BOX_A: &str = "box id=\"a\" start=0 duration=1 x=0 y=0 width=64 height=64 fill=\"#ff0000\"\n";
const BOX_B: &str =
    "box duration=1 start-after=\"a\" x=0 y=0 width=64 height=64 fill=\"#0000ff\"\n";

fn comp_2s(body: &str) -> String {
    format!(
        "composition name=\"main\" width=64 height=64 duration=2 fps=30 background=\"#000000\" version=1 {{\n{body}}}\n"
    )
}

#[test]
fn exact_follower_renders_on_resolved_window() {
    let root = tmp("exact");
    std::fs::create_dir_all(root.join("p")).unwrap();
    write(
        &root.join("p"),
        "composition.kdl",
        &comp_2s(&format!("{BOX_A}{BOX_B}")),
    );
    let (code, out) = run(&["check", "p"], &root);
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(&["render", "p"], &root);
    assert_eq!(code, 0, "{out}");
    let mp4 = root.join("p").join("out.mp4");
    assert!(mp4.is_file());
    // 60 frames at fps=30: frame 29 is instant 29/30 (red tail of A),
    // frame 30 is instant 1.0, the first instant of B's [1, 2) window.
    let frames = decode_video(&mp4, 64, 64);
    assert_eq!(frames.len(), 60 * 64 * 64 * 3);
    assert_near_red(center(&frames, 0, 64, 64), "frame 0");
    assert_near_red(center(&frames, 29, 64, 64), "frame 29");
    assert_near_blue(center(&frames, 30, 64, 64), "frame 30");
    assert_near_blue(center(&frames, 59, 64, 64), "frame 59");
}

#[test]
fn relative_render_is_byte_identical_on_rerun() {
    let root = tmp("rerun");
    std::fs::create_dir_all(root.join("p")).unwrap();
    write(
        &root.join("p"),
        "composition.kdl",
        &comp_2s(&format!("{BOX_A}{BOX_B}")),
    );
    let (code, out) = run(&["render", "p"], &root);
    assert_eq!(code, 0, "{out}");
    let first = std::fs::read(root.join("p").join("out.mp4")).unwrap();
    let (code, out) = run(&["render", "p"], &root);
    assert_eq!(code, 0, "{out}");
    let second = std::fs::read(root.join("p").join("out.mp4")).unwrap();
    assert_eq!(first, second);
}

#[test]
fn negative_overlap_renders_overlapping_content() {
    // B follows A back one full second: at fps=1 the t=1 frame shows B
    // on top while A still runs underneath.
    let root = tmp("overlap");
    std::fs::create_dir_all(root.join("p")).unwrap();
    write(
        &root.join("p"),
        "composition.kdl",
        "composition name=\"main\" width=64 height=64 duration=3 fps=1 background=\"#000000\" version=1 {\n\
         box id=\"a\" start=0 duration=2 x=0 y=0 width=64 height=64 fill=\"#ff0000\"\n\
         box duration=2 start-after=\"a\" start-offset=-1 x=0 y=0 width=64 height=64 fill=\"#0000ff\"\n\
         }\n",
    );
    let (code, out) = run(&["check", "p"], &root);
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(&["render", "p"], &root);
    assert_eq!(code, 0, "{out}");
    let frames = decode_video(&root.join("p").join("out.mp4"), 64, 64);
    assert_eq!(frames.len(), 3 * 64 * 64 * 3);
    assert_near_red(center(&frames, 0, 64, 64), "frame t=0");
    assert_near_blue(center(&frames, 1, 64, 64), "frame t=1");
    assert_near_blue(center(&frames, 2, 64, 64), "frame t=2");
}

#[test]
fn offset_chain_renders_at_resolved_time() {
    // A [0,1), B at 1.5, C follows B: fps=2 over 4s, so the frame at
    // instant 2.5 (index 5) is the first blue... here greens mark C.
    let root = tmp("chain");
    std::fs::create_dir_all(root.join("p")).unwrap();
    write(
        &root.join("p"),
        "composition.kdl",
        "composition name=\"main\" width=64 height=64 duration=4 fps=2 background=\"#000000\" version=1 {\n\
         box id=\"a\" start=0 duration=1 x=0 y=0 width=64 height=64 fill=\"#ff0000\"\n\
         box id=\"b\" duration=0.5 start-after=\"a\" start-offset=0.5 x=0 y=0 width=64 height=64 fill=\"#0000ff\"\n\
         box duration=1 start-after=\"b\" x=0 y=0 width=64 height=64 fill=\"#00ff00\"\n\
         }\n",
    );
    let (code, out) = run(&["check", "p"], &root);
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(&["render", "p"], &root);
    assert_eq!(code, 0, "{out}");
    let frames = decode_video(&root.join("p").join("out.mp4"), 64, 64);
    assert_eq!(frames.len(), 8 * 64 * 64 * 3);
    // B covers [1.5, 2.0): frame 3 (t=1.5) is blue, frame 4 (t=2.0) green.
    assert_near_blue(center(&frames, 3, 64, 64), "frame t=1.5");
    let g = center(&frames, 4, 64, 64);
    assert!(
        g.1 > 180 && g.0 < 100 && g.2 < 100,
        "frame t=2.0 green, found {g:?}"
    );
}

#[test]
fn cycle_refuses_naming_the_chain() {
    let root = tmp("cycle");
    std::fs::create_dir_all(root.join("p")).unwrap();
    write(
        &root.join("p"),
        "composition.kdl",
        "composition name=\"main\" width=64 height=64 duration=2 background=\"#000\" version=1 {\n\
         text value=\"A\" duration=1 id=\"a\" start-after=\"b\"\n\
         text value=\"B\" duration=1 id=\"b\" start-after=\"a\"\n\
         }\n",
    );
    let (code, out) = run(&["check", "p"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_BAD_TIMING"), "{out}");
    assert!(out.contains("\"a\"") && out.contains("\"b\""), "{out}");
    assert!(out.contains("cycl"), "{out}");
    assert!(out.contains("fix:"), "{out}");
    let (code, out) = run(&["render", "p"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(!root.join("p").join("out.mp4").exists());
}

#[test]
fn ambiguous_start_refuses() {
    let root = tmp("ambiguous");
    std::fs::create_dir_all(root.join("p")).unwrap();
    write(
        &root.join("p"),
        "composition.kdl",
        "composition name=\"main\" width=64 height=64 duration=2 background=\"#000\" version=1 {\n\
         text value=\"A\" start=0 duration=1 id=\"a\"\n\
         text value=\"B\" start=1 duration=1 start-after=\"a\"\n\
         }\n",
    );
    let (code, out) = run(&["check", "p"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_BAD_TIMING"), "{out}");
    assert!(out.contains(":3:"), "{out}");
    let (code, json) = run(&["check", "--json", "p"], &root);
    assert_eq!(code, 1, "{json}");
    assert!(json.contains("\"code\":\"E_BAD_TIMING\""), "{json}");
}

#[test]
fn before_zero_start_refuses() {
    let root = tmp("neg");
    std::fs::create_dir_all(root.join("p")).unwrap();
    write(
        &root.join("p"),
        "composition.kdl",
        "composition name=\"main\" width=64 height=64 duration=2 background=\"#000\" version=1 {\n\
         text value=\"A\" start=0 duration=1 id=\"a\"\n\
         text value=\"B\" duration=1 start-after=\"a\" start-offset=-2\n\
         }\n",
    );
    let (code, out) = run(&["check", "p"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_BAD_TIMING"), "{out}");
    assert!(out.contains("-1"), "{out}");
    let (code, out) = run(&["render", "p"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(!root.join("p").join("out.mp4").exists());
}
