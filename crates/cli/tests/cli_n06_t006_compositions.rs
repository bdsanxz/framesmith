//! Sprint 6 T006 end-to-end: several compositions with one root.
//!
//! `check` validates every block and names the root; `render` produces
//! the root by default and the `--composition` target on request; an
//! ambiguous root or an unknown name refuses with no MP4 written; a
//! single-composition project renders exactly as before.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-n06t006-{tag}-{}-{}",
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
    std::fs::create_dir_all(folder).unwrap();
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
        .expect("decode mp4 with host ffmpeg");
    assert!(out.status.success());
    out.stdout
}

fn center(frames: &[u8], frame: usize, w: u32, h: u32) -> (u8, u8, u8) {
    let stride = (w * h * 3) as usize;
    let at = frame * stride + ((h / 2 * w + w / 2) * 3) as usize;
    (frames[at], frames[at + 1], frames[at + 2])
}

const RED_MAIN: &str = "composition name=\"main\" width=64 height=64 duration=2 fps=2 background=\"#000000\" version=1 {\n\
     box x=0 y=0 width=64 height=64 fill=\"#ff0000\" start=0 duration=2\n\
     }\n";
const BLUE_CUTDOWN_ROOT: &str = "composition name=\"cutdown\" root=true width=64 height=64 duration=2 fps=2 background=\"#000000\" version=1 {\n\
     box x=0 y=0 width=64 height=64 fill=\"#0000ff\" start=0 duration=2\n\
     }\n";

fn two_comp_project() -> String {
    format!("{RED_MAIN}{BLUE_CUTDOWN_ROOT}")
}

#[test]
fn check_validates_both_blocks_and_names_the_root() {
    let root = tmp("check");
    write(&root.join("p"), "composition.kdl", &two_comp_project());
    let (code, out) = run(&["check", "p"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("ok:"), "{out}");
    assert!(out.contains("compositions:"), "{out}");
    assert!(out.contains("\"main\""), "{out}");
    assert!(out.contains("\"cutdown\" (root)"), "{out}");
}

#[test]
fn check_json_lists_compositions_and_root() {
    let root = tmp("check-json");
    write(&root.join("p"), "composition.kdl", &two_comp_project());
    let (code, json) = run(&["check", "--json", "p"], &root);
    assert_eq!(code, 0, "{json}");
    assert!(
        json.contains("\"compositions\":[\"main\",\"cutdown\"]"),
        "{json}"
    );
    assert!(json.contains("\"root\":\"cutdown\""), "{json}");
}

#[test]
fn default_render_produces_the_root_output() {
    let root = tmp("default");
    write(&root.join("p"), "composition.kdl", &two_comp_project());
    let (code, out) = run(&["render", "p"], &root);
    assert_eq!(code, 0, "{out}");
    let mp4 = root.join("p").join("out.mp4");
    assert!(mp4.is_file());
    let frames = decode_video(&mp4, 64, 64);
    assert_eq!(frames.len(), 4 * 64 * 64 * 3);
    let (r, g, b) = center(&frames, 0, 64, 64);
    assert!(
        b > 180 && r < 100 && g < 100,
        "root frame is blue: r={r} g={g} b={b}"
    );
}

#[test]
fn composition_flag_renders_the_named_output() {
    let root = tmp("named");
    write(&root.join("p"), "composition.kdl", &two_comp_project());
    let (code, out) = run(
        &["render", "p", "-o", "p/main.mp4", "--composition", "main"],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    let main_mp4 = root.join("p").join("main.mp4");
    assert!(main_mp4.is_file());
    let frames = decode_video(&main_mp4, 64, 64);
    let (r, g, b) = center(&frames, 0, 64, 64);
    assert!(
        r > 180 && g < 100 && b < 100,
        "named frame is red: r={r} g={g} b={b}"
    );

    let (code, out) = run(
        &["render", "p", "-o", "p/cut.mp4", "--composition", "cutdown"],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    let frames = decode_video(&root.join("p").join("cut.mp4"), 64, 64);
    let (r, g, b) = center(&frames, 0, 64, 64);
    assert!(
        b > 180 && r < 100 && g < 100,
        "root frame is blue: r={r} g={g} b={b}"
    );
}

#[test]
fn composition_flag_also_accepts_the_composition_id() {
    let root = tmp("by-id");
    write(
        &root.join("p"),
        "composition.kdl",
        &format!(
            "{RED_MAIN}composition name=\"cutdown\" id=\"cut\" root=true width=64 height=64 duration=2 fps=2 background=\"#000000\" version=1 {{\n\
             box x=0 y=0 width=64 height=64 fill=\"#0000ff\" start=0 duration=2\n\
             }}\n"
        ),
    );
    let (code, out) = run(
        &["render", "p", "-o", "p/cut.mp4", "--composition", "cut"],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    let frames = decode_video(&root.join("p").join("cut.mp4"), 64, 64);
    let (r, g, b) = center(&frames, 0, 64, 64);
    assert!(
        b > 180 && r < 100 && g < 100,
        "id-selected frame is blue: r={r} g={g} b={b}"
    );
}

#[test]
fn unknown_composition_refuses_without_output() {
    let root = tmp("unknown");
    write(&root.join("p"), "composition.kdl", &two_comp_project());
    let (code, out) = run(
        &["render", "p", "-o", "p/ghost.mp4", "--composition", "ghost"],
        &root,
    );
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("unknown composition"), "{out}");
    assert!(out.contains("\"ghost\""), "{out}");
    assert!(out.contains("no MP4"), "{out}");
    assert!(!root.join("p").join("ghost.mp4").exists());
    assert!(!root.join("p").join("out.mp4").exists());
}

#[test]
fn composition_flag_needs_a_value() {
    let root = tmp("novalue");
    write(&root.join("p"), "composition.kdl", &two_comp_project());
    let (code, out) = run(&["render", "p", "--composition"], &root);
    assert_eq!(code, 2, "{out}");
    assert!(!root.join("p").join("out.mp4").exists());
}

#[test]
fn no_root_mark_refuses_check_and_render() {
    let root = tmp("noroot");
    write(
        &root.join("p"),
        "composition.kdl",
        "composition name=\"main\" width=64 height=64 duration=2 fps=2 background=\"#000000\" version=1 {\n\
         box x=0 y=0 width=64 height=64 fill=\"#ff0000\" start=0 duration=2\n\
         }\n\
         composition name=\"cutdown\" width=64 height=64 duration=2 fps=2 background=\"#000000\" version=1 {\n\
         box x=0 y=0 width=64 height=64 fill=\"#0000ff\" start=0 duration=2\n\
         }\n",
    );
    let (code, out) = run(&["check", "p"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_AMBIGUOUS_ROOT"), "{out}");
    let (code, json) = run(&["check", "--json", "p"], &root);
    assert_eq!(code, 1, "{json}");
    assert!(json.contains("\"code\":\"E_AMBIGUOUS_ROOT\""), "{json}");
    let (code, out) = run(&["render", "p", "--composition", "main"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_AMBIGUOUS_ROOT"), "{out}");
    assert!(!root.join("p").join("out.mp4").exists());
}

#[test]
fn two_root_marks_refuse_render_without_output() {
    let root = tmp("tworoots");
    write(
        &root.join("p"),
        "composition.kdl",
        "composition name=\"main\" root=true width=64 height=64 duration=2 fps=2 background=\"#000000\" version=1 {\n\
         box x=0 y=0 width=64 height=64 fill=\"#ff0000\" start=0 duration=2\n\
         }\n\
         composition name=\"cutdown\" root=true width=64 height=64 duration=2 fps=2 background=\"#000000\" version=1 {\n\
         box x=0 y=0 width=64 height=64 fill=\"#0000ff\" start=0 duration=2\n\
         }\n",
    );
    let (code, out) = run(&["render", "p"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_AMBIGUOUS_ROOT"), "{out}");
    assert!(!root.join("p").join("out.mp4").exists());
}

#[test]
fn single_composition_renders_unchanged_and_byte_identical() {
    let root = tmp("single");
    write(&root.join("p"), "composition.kdl", RED_MAIN);
    let (code, out) = run(&["check", "p"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("ok:"), "{out}");
    assert!(!out.contains("compositions:"), "{out}");
    let (code, out) = run(&["render", "p"], &root);
    assert_eq!(code, 0, "{out}");
    let first = std::fs::read(root.join("p").join("out.mp4")).unwrap();
    let (code, out) = run(&["render", "p"], &root);
    assert_eq!(code, 0, "{out}");
    let second = std::fs::read(root.join("p").join("out.mp4")).unwrap();
    assert_eq!(first, second);
    let frames = decode_video(&root.join("p").join("out.mp4"), 64, 64);
    let (r, g, b) = center(&frames, 0, 64, 64);
    assert!(
        r > 180 && g < 100 && b < 100,
        "single-comp frame is red: r={r} g={g} b={b}"
    );
}
