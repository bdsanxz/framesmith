//! Storyboard approval gate verbs (Sprint 20, T004 / CAP-004, J-02,
//! STATE-04, REC-002): every acceptance criterion of
//! `T004-hold-the-storyboard-gate.md` through the `storyboard` CLI,
//! including refusal variants.
//!
//! The board shows stills in one page format; the film is
//! byte-identical before approval; Studio stills approval precedes
//! motion; locked frames persist for resume; no posed-still
//! slideshows are produced; missing approval reads as not approved;
//! early builds refuse pointing at the gate.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-n20-t004-{tag}-{}-{}",
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

fn run(args: &[&str]) -> (i32, String) {
    run_in(args, &std::env::temp_dir())
}

fn run_in(args: &[&str], cwd: &Path) -> (i32, String) {
    let out = Command::new(bin())
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("run framesmith binary");
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    (out.status.code().unwrap_or(-1), text)
}

/// A fresh valid project via `init` (exercises the shipped guides on
/// the way in).
fn fresh(root: &Path, name: &str) -> PathBuf {
    let (code, out) = run_in(&["init", name], root);
    assert_eq!(code, 0, "{out}");
    root.join(name)
}

fn film_bytes(folder: &Path) -> Vec<u8> {
    std::fs::read(folder.join("composition.kdl")).expect("composition reads")
}

#[test]
fn status_missing_reads_as_not_approved() {
    let root = tmp("status-missing");
    let folder = fresh(&root, "plain");
    let folder_s = folder.to_str().unwrap().to_string();
    let (code, out) = run(&["storyboard", "status", &folder_s]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("not approved"), "{out}");
    let (code, out) = run(&["storyboard", "status", "--json", &folder_s]);
    assert_eq!(code, 0, "{out}");
    let v: serde_json::Value =
        serde_json::from_str(&out).unwrap_or_else(|_| panic!("not JSON:\n{out}"));
    assert_eq!(v["presented"], false);
    assert_eq!(v["approved"], false);
    assert_eq!(v["frames"], 0);
}

#[test]
fn board_presents_one_page_of_stills_and_locks_frames() {
    let root = tmp("board");
    let folder = fresh(&root, "film");
    let folder_s = folder.to_str().unwrap().to_string();
    let before = film_bytes(&folder);
    let out_dir = root.join("board-out");
    let out_s = out_dir.to_str().unwrap().to_string();
    let (code, out) = run(&[
        "storyboard",
        "board",
        "--at",
        "0",
        "--out",
        &out_s,
        &folder_s,
    ]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("locked still(s)"), "{out}");
    assert!(out.contains("film is unchanged"), "{out}");
    // One page format: stills plus board.html, motion nowhere.
    let page = std::fs::read_to_string(out_dir.join("board.html")).expect("board page written");
    assert!(page.contains("<!DOCTYPE html>"), "one HTML page");
    assert_eq!(page.matches("<img").count(), 1, "one still shown");
    for banned in ["<video", "autoplay", "setInterval"] {
        assert!(!page.contains(banned), "posed stills never play: {banned}");
    }
    let still = std::fs::read(out_dir.join("still-0000.png")).expect("still written");
    assert_eq!(&still[..8], b"\x89PNG\r\n\x1a\n", "real PNG still");
    // The film is byte-identical before approval.
    assert_eq!(film_bytes(&folder), before, "board reads the film only");
    // The project still checks: board sidecars are ignored.
    let (code, out) = run(&["check", &folder_s]);
    assert_eq!(code, 0, "{out}");
    // The gate reads back armed with its locks.
    let (code, out) = run(&["storyboard", "status", "--json", &folder_s]);
    assert_eq!(code, 0, "{out}");
    let v: serde_json::Value =
        serde_json::from_str(&out).unwrap_or_else(|_| panic!("not JSON:\n{out}"));
    assert_eq!(v["presented"], true);
    assert_eq!(v["approved"], false);
    assert_eq!(v["frames"], 1);
    assert_eq!(v["locked-frames"].as_array().unwrap().len(), 1);
}

#[test]
fn default_board_holds_four_evenly_spaced_stills() {
    let root = tmp("board-default");
    let folder = fresh(&root, "film");
    let folder_s = folder.to_str().unwrap().to_string();
    let (code, out) = run(&["storyboard", "board", &folder_s]);
    assert_eq!(code, 0, "{out}");
    let page = std::fs::read_to_string(folder.join("storyboard").join("board.html"))
        .expect("default board page written");
    assert_eq!(page.matches("<img").count(), 4, "four evenly spaced stills");
    let (code, out) = run(&["storyboard", "status", "--json", &folder_s]);
    assert_eq!(code, 0, "{out}");
    let v: serde_json::Value =
        serde_json::from_str(&out).unwrap_or_else(|_| panic!("not JSON:\n{out}"));
    assert_eq!(v["frames"], 4);
}

#[test]
fn board_every_interval_locks_a_still_per_step() {
    let root = tmp("board-every");
    let folder = fresh(&root, "film");
    let folder_s = folder.to_str().unwrap().to_string();
    let out_dir = root.join("every-out");
    let out_s = out_dir.to_str().unwrap().to_string();
    let (code, out) = run(&[
        "storyboard",
        "board",
        "--every",
        "1",
        "--out",
        &out_s,
        &folder_s,
    ]);
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(&["storyboard", "status", "--json", &folder_s]);
    assert_eq!(code, 0, "{out}");
    let v: serde_json::Value =
        serde_json::from_str(&out).unwrap_or_else(|_| panic!("not JSON:\n{out}"));
    let frames = v["frames"].as_u64().unwrap();
    assert!(frames >= 2, "interval stills lock: {out}");
}

#[test]
fn early_render_refuses_pointing_at_the_gate() {
    let root = tmp("early-render");
    let folder = fresh(&root, "film");
    let folder_s = folder.to_str().unwrap().to_string();
    let before = film_bytes(&folder);
    let (code, out) = run(&["storyboard", "board", "--at", "0", &folder_s]);
    assert_eq!(code, 0, "{out}");
    let mp4 = root.join("early.mp4");
    let mp4_s = mp4.to_str().unwrap().to_string();
    let (code, out) = run(&["render", &folder_s, "-o", &mp4_s]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("storyboard"), "names the gate: {out}");
    assert!(out.contains("approve"), "points at approval: {out}");
    assert!(!mp4.exists(), "no MP4 was written");
    assert_eq!(film_bytes(&folder), before, "refused build changes nothing");
}

#[test]
fn approve_lands_the_gate_then_builds_proceed() {
    let root = tmp("approve-render");
    let folder = fresh(&root, "film");
    let folder_s = folder.to_str().unwrap().to_string();
    let (code, out) = run(&["storyboard", "board", "--at", "0", &folder_s]);
    assert_eq!(code, 0, "{out}");
    // Approve through the Studio-shaped receipt.
    let (code, out) = run(&["storyboard", "approve", "--json", &folder_s]);
    assert_eq!(code, 0, "{out}");
    let v: serde_json::Value =
        serde_json::from_str(&out).unwrap_or_else(|_| panic!("not JSON:\n{out}"));
    assert_eq!(v["staged"], true);
    assert_eq!(v["tool"], "approve-stills");
    assert_eq!(v["approved"], true);
    assert!(v["turn"].as_u64().unwrap() >= 1, "history turn recorded");
    let (code, out) = run(&["storyboard", "status", &folder_s]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("approved"), "{out}");
    // The approval is a shared history turn the maker can undo.
    let (code, out) = run(&["history", "list", "--json", &folder_s]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("storyboard"), "approval turn recorded: {out}");
    // With the gate landed, the build proceeds to a real MP4.
    let mp4 = root.join("approved.mp4");
    let mp4_s = mp4.to_str().unwrap().to_string();
    let (code, out) = run(&["render", &folder_s, "-o", &mp4_s]);
    assert_eq!(code, 0, "{out}");
    assert!(mp4.is_file(), "approved build renders");
}

#[test]
fn approve_without_a_board_refuses() {
    let root = tmp("approve-empty");
    let folder = fresh(&root, "film");
    let folder_s = folder.to_str().unwrap().to_string();
    let (code, out) = run(&["storyboard", "approve", &folder_s]);
    assert_eq!(code, 1, "{out}");
    assert!(
        out.contains("present a board first"),
        "names the fix: {out}"
    );
}

#[test]
fn storyboard_usage_mistakes_exit_two() {
    let root = tmp("usage");
    let folder = fresh(&root, "film");
    let folder_s = folder.to_str().unwrap().to_string();
    let (code, _) = run(&["storyboard", "dance", &folder_s]);
    assert_eq!(code, 2, "unknown verb is usage");
    let (code, _) = run(&["storyboard", "board"]);
    assert_eq!(code, 2, "missing folder is usage");
    let (code, _) = run(&["storyboard", "board", "--at", "0", "--at", "1", &folder_s]);
    assert_eq!(code, 2, "duplicate flags are usage");
    let (code, _) = run(&["storyboard", "board", "--bogus", &folder_s]);
    assert_eq!(code, 2, "unknown flags are usage");
    let (code, _) = run(&["storyboard", "status"]);
    assert_eq!(code, 2, "status needs its folder");
}
