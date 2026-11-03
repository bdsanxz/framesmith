//! Sprint 6 T010 end-to-end: `compositions`, `timeline`, and `info`
//! list the resolved project model in human and `--json` forms.
//!
//! The listings name every composition plus the root, every clip with
//! id, track, label, hidden state, and effective window, and the canvas,
//! duration, and clip counts; both forms carry the same content,
//! invalid projects refuse like `check`, and the windows match the
//! pixels `render` produces.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-n06t010-{tag}-{}-{}",
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

fn run_stdout(args: &[&str], cwd: &Path) -> (i32, String) {
    let out = Command::new(bin())
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("run framesmith binary");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    )
}

fn write(folder: &Path, name: &str, content: &str) {
    std::fs::create_dir_all(folder).unwrap();
    std::fs::write(folder.join(name), content).unwrap();
}

/// Structured two-composition project: the root carries a full-cover
/// red box window, a hidden blue box, and a relative-start follower;
/// the second composition sizes itself from a canvas preset.
fn structured() -> &'static str {
    "composition name=\"main\" width=64 height=64 duration=2 fps=30 background=\"#000000\" version=1 root=true {\n\
     \x20   box x=0 y=0 width=64 height=64 fill=\"#ff0000\" start=0.5 duration=1 id=\"red\" track=\"video\" label=\"cover\"\n\
     \x20   box x=0 y=0 width=64 height=64 fill=\"#0000ff\" start=1.5 duration=0.5 id=\"hid\" hidden=true\n\
     \x20   text value=\"Tail\" x=0 y=0 size=8 color=\"#ffffff\" duration=0.5 id=\"tail\" start-after=\"red\" label=\"follower\"\n\
     }\n\
     composition name=\"cut\" canvas=\"square\" duration=1 fps=30 background=\"#112233\" version=1 {\n\
     \x20   text value=\"Cut\" x=0 y=0 size=8 color=\"#ffffff\" start=0 duration=1 id=\"ct\" track=\"caption\" label=\"cap\"\n\
     }\n"
}

fn project(tag: &str) -> (PathBuf, PathBuf) {
    let root = tmp(tag);
    let folder = root.join("p");
    write(&folder, "composition.kdl", structured());
    (root, folder)
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

fn is_red(p: (u8, u8, u8)) -> bool {
    p.0 > 180 && p.1 < 100 && p.2 < 100
}

fn is_black(p: (u8, u8, u8)) -> bool {
    p.0 < 50 && p.1 < 50 && p.2 < 50
}

fn probe_duration(mp4: &Path) -> f64 {
    let out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "csv=p=0",
            &mp4.display().to_string(),
        ])
        .output()
        .expect("probe mp4 with host ffprobe");
    assert!(out.status.success());
    String::from_utf8_lossy(&out.stdout).trim().parse().unwrap()
}

// --- compositions ---------------------------------------------------------------

#[test]
fn compositions_lists_names_and_root_in_both_forms() {
    let (root, _) = project("comps");
    let (code, human) = run(&["compositions", "p"], &root);
    assert_eq!(code, 0, "{human}");
    assert!(human.contains("\"main\" (root, 3 clip(s))"), "{human}");
    assert!(human.contains("\"cut\" (1 clip(s))"), "{human}");
    let (code, json) = run_stdout(&["compositions", "--json", "p"], &root);
    assert_eq!(code, 0, "{json}");
    assert!(
        json.contains("\"compositions\":[{\"name\":\"main\",\"clips\":3,\"root\":true},{\"name\":\"cut\",\"clips\":1,\"root\":false}]"),
        "{json}"
    );
    assert!(json.contains("\"root\":\"main\""), "{json}");
}

// --- timeline -------------------------------------------------------------------

#[test]
fn timeline_shows_every_row_and_matches_json() {
    let (root, _) = project("timeline");
    let (code, human) = run(&["timeline", "p"], &root);
    assert_eq!(code, 0, "{human}");
    assert!(human.contains("timeline of composition \"main\""), "{human}");
    assert!(
        human.contains("[0] box id=\"red\" track=video label=\"cover\" visible window 0.500s..1.500s."),
        "{human}"
    );
    assert!(
        human.contains("[1] box id=\"hid\" track=graphics label=- hidden window 1.500s..2.000s."),
        "{human}"
    );
    assert!(
        human.contains("[2] text id=\"tail\" track=graphics label=\"follower\" visible window 1.500s..2.000s."),
        "{human}"
    );
    let (code, json) = run_stdout(&["timeline", "--json", "p"], &root);
    assert_eq!(code, 0, "{json}");
    assert!(json.contains("\"composition\":\"main\",\"root\":\"main\""), "{json}");
    assert!(
        json.contains("{\"index\":0,\"kind\":\"box\",\"id\":\"red\",\"track\":\"video\",\"label\":\"cover\",\"hidden\":false,\"start\":0.5,\"end\":1.5}"),
        "{json}"
    );
    assert!(
        json.contains("{\"index\":1,\"kind\":\"box\",\"id\":\"hid\",\"track\":\"graphics\",\"label\":null,\"hidden\":true,\"start\":1.5,\"end\":2}"),
        "{json}"
    );
    assert!(
        json.contains("{\"index\":2,\"kind\":\"text\",\"id\":\"tail\",\"track\":\"graphics\",\"label\":\"follower\",\"hidden\":false,\"start\":1.5,\"end\":2}"),
        "{json}"
    );
}

#[test]
fn timeline_names_other_compositions_positionally_or_by_flag() {
    let (root, _) = project("tlnamed");
    // The second composition reports its own canvas, duration, and rows.
    let (code, human) = run(&["timeline", "p", "cut"], &root);
    assert_eq!(code, 0, "{human}");
    assert!(human.contains("timeline of composition \"cut\""), "{human}");
    assert!(
        human.contains("[0] text id=\"ct\" track=caption label=\"cap\" visible window 0.000s..1.000s."),
        "{human}"
    );
    let (code, flagged) = run(&["timeline", "--composition", "cut", "p"], &root);
    assert_eq!(code, 0, "{flagged}");
    assert_eq!(flagged, human, "flag and positional name the same listing");
    // Composition `id` answers too.
    let (code, json) = run_stdout(&["timeline", "--json", "p", "cut"], &root);
    assert_eq!(code, 0, "{json}");
    assert!(json.contains("\"composition\":\"cut\""), "{json}");
    assert!(json.contains("\"preset\":\"square\""), "{json}");
}

// --- info -----------------------------------------------------------------------

#[test]
fn info_summarizes_canvas_duration_and_counts_in_both_forms() {
    let (root, _) = project("info");
    let (code, human) = run(&["info", "p"], &root);
    assert_eq!(code, 0, "{human}");
    assert!(human.contains("info for composition \"main\""), "{human}");
    assert!(human.contains("canvas: 64x64."), "{human}");
    assert!(human.contains("duration: 2.000s (60 frames at 30fps)."), "{human}");
    assert!(
        human.contains("clips: 3 total (text=1, video=0, box=2, image=0, audio=0, nest=0; hidden=1)."),
        "{human}"
    );
    let (code, json) = run_stdout(&["info", "--json", "p"], &root);
    assert_eq!(code, 0, "{json}");
    assert!(json.contains("\"composition\":\"main\""), "{json}");
    assert!(json.contains("\"canvas\":{\"width\":64,\"height\":64,\"preset\":null}"), "{json}");
    assert!(json.contains("\"duration\":2,\"fps\":\"30\",\"frames\":60"), "{json}");
    assert!(
        json.contains("\"clips\":{\"total\":3,\"by_kind\":{\"text\":1,\"video\":0,\"box\":2,\"image\":0,\"audio\":0,\"nest\":0},\"hidden\":1}"),
        "{json}"
    );
    // A preset-sized composition reports the preset and its dimensions.
    let (code, human) = run(&["info", "p", "cut"], &root);
    assert_eq!(code, 0, "{human}");
    assert!(human.contains("canvas: 1080x1080 (preset \"square\")."), "{human}");
    assert!(human.contains("duration: 1.000s (30 frames at 30fps)."), "{human}");
}

// --- refusal parity --------------------------------------------------------------

#[test]
fn invalid_projects_refuse_like_check_on_every_command() {
    let root = tmp("invalid");
    let folder = root.join("p");
    write(
        &folder,
        "composition.kdl",
        "composition name=\"a\" width=64 height=64 duration=1 fps=30 background=\"#000\" version=1 {\n\
         \x20   text value=\"A\" x=0 y=0 size=8 color=\"#fff\" start=0 duration=1 id=\"dup\"\n\
         \x20   text value=\"B\" x=0 y=0 size=8 color=\"#fff\" start=0 duration=1 id=\"dup\"\n\
         }\n",
    );
    let (check_code, check) = run(&["check", "p"], &root);
    assert_eq!(check_code, 1, "{check}");
    assert!(check.contains("E_DUP_ID"), "{check}");
    for args in [
        vec!["compositions", "p"],
        vec!["timeline", "p"],
        vec!["info", "p"],
    ] {
        let (code, out) = run(&args, &root);
        assert_eq!(code, 1, "{args:?}: {out}");
        assert!(out.contains("E_DUP_ID"), "{args:?}: {out}");
        assert!(!out.contains("compositions in"), "{args:?}: {out}");
        assert!(!out.contains("timeline of"), "{args:?}: {out}");
        assert!(!out.contains("info for"), "{args:?}: {out}");
    }
    for args in [
        vec!["compositions", "--json", "p"],
        vec!["timeline", "--json", "p"],
        vec!["info", "--json", "p"],
    ] {
        let (code, out) = run_stdout(&args, &root);
        assert_eq!(code, 1, "{args:?}: {out}");
        assert!(out.contains("E_DUP_ID"), "{args:?}: {out}");
        assert!(out.contains("\"status\":\"invalid\",\"exit\":1"), "{args:?}: {out}");
        assert!(!out.contains("\"clips\":["), "{args:?}: {out}");
    }
}

#[test]
fn unknown_composition_names_refuse_without_listing() {
    let (root, _) = project("unknown");
    let (code, out) = run(&["timeline", "p", "ghost"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("unknown composition \"ghost\""), "{out}");
    assert!(out.contains("\"cut\", \"main\""), "{out}");
    assert!(!out.contains("timeline of"), "{out}");
    let (code, out) = run_stdout(&["info", "--json", "p", "ghost"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("unknown composition"), "{out}");
    assert!(out.contains("\"status\":\"invalid\",\"exit\":1"), "{out}");
}

#[test]
fn listing_usage_errors_exit_2() {
    let (root, _) = project("usage");
    for args in [
        vec!["timeline"],
        vec!["info"],
        vec!["compositions"],
        vec!["timeline", "p", "cut", "extra"],
        vec!["timeline", "--composition", "main", "p", "cut"],
        vec!["compositions", "p", "cut"],
        vec!["timeline", "--bogus", "p"],
    ] {
        let (code, out) = run(&args, &root);
        assert_eq!(code, 2, "{args:?}: {out}");
        assert!(out.contains("usage:"), "{args:?}: {out}");
    }
}

// --- agreement with render -------------------------------------------------------

#[test]
fn listings_match_what_render_produces() {
    let (root, _folder) = project("agree");
    // The info duration is the MP4 duration: render the default target
    // (the root) and probe the output.
    let (code, out) = run(&["render", "p", "-o", "root.mp4"], &root);
    assert_eq!(code, 0, "{out}");
    let mp4 = root.join("root.mp4");
    let probed = probe_duration(&mp4);
    assert!((probed - 2.0).abs() < 0.05, "probed duration {probed}");
    // The timeline windows are the rendered pixels: the red box covers
    // exactly [0.5, 1.5), the hidden blue box never shows.
    let frames = decode_video(&mp4, 64, 64);
    assert_eq!(frames.len(), 60 * 64 * 64 * 3, "60 frames at 30fps");
    assert!(is_black(center(&frames, 7, 64, 64)), "t=0.25 is background");
    assert!(is_red(center(&frames, 30, 64, 64)), "t=1.0 is red");
    assert!(!is_red(center(&frames, 45, 64, 64)), "t=1.5 is past the red end");
    assert!(is_black(center(&frames, 52, 64, 64)), "t=1.75 hides blue, shows background");
    // Re-rendering is byte-identical, so the listing stays truthful.
    let (code, out) = run(&["render", "p", "-o", "root2.mp4"], &root);
    assert_eq!(code, 0, "{out}");
    let a = std::fs::read(root.join("root.mp4")).unwrap();
    let b = std::fs::read(root.join("root2.mp4")).unwrap();
    assert_eq!(a, b, "re-render is byte-identical");
}
