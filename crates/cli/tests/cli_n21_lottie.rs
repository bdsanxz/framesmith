//! Sprint 21 T001: native `lottie` clips end-to-end through the
//! `framesmith check` binary.
//!
//! Covers the user-visible half of `T001-play-lottie-natively.md`:
//! a valid project checks clean while listing the file markers as
//! named ranges, an out-of-window marker drops into a reviewable
//! note, and corrupt or misconfigured clips refuse with the file
//! (and cause or mode) named.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-n21lottie-{tag}-{}-{}",
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
    let mut cmd = Command::new(bin());
    cmd.args(args).current_dir(cwd);
    let out = cmd.output().expect("run framesmith binary");
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    (out.status.code().unwrap_or(-1), text)
}

fn write(folder: &Path, name: &str, content: &[u8]) {
    std::fs::write(folder.join(name), content).unwrap();
}

/// The shared slide fixture: a red rect sliding 10→50px over 2s at
/// 30fps with one in-window marker and one past-end marker.
fn slide_json() -> Vec<u8> {
    serde_json::json!({
        "v": "5.7.4", "fr": 30.0, "ip": 0.0, "op": 60.0, "w": 64.0, "h": 64.0,
        "nm": "slide",
        "markers": [
            {"tm": 0.0, "cm": "intro", "dr": 30.0},
            {"tm": 90.0, "cm": "past-end", "dr": 10.0}
        ],
        "layers": [{
            "ddd": 0, "ind": 1, "ty": 4, "nm": "box", "sr": 1.0,
            "ks": {"o": {"a": 0, "k": 100.0}, "r": {"a": 0, "k": 0.0},
                "p": {"a": 1, "k": [
                    {"t": 0.0, "s": [10.0, 32.0, 0.0], "e": [50.0, 32.0, 0.0]},
                    {"t": 30.0, "s": [50.0, 32.0, 0.0]}]},
                "a": {"a": 0, "k": [0.0, 0.0, 0.0]}, "s": {"a": 0, "k": [100.0, 100.0, 100.0]}},
            "shapes": [{"ty": "gr", "it": [
                {"ty": "rc", "p": {"a": 0, "k": [0.0, 0.0]}, "s": {"a": 0, "k": [20.0, 20.0]}, "r": {"a": 0, "k": 0.0}},
                {"ty": "fl", "c": {"a": 0, "k": [1.0, 0.0, 0.0, 1.0]}, "o": {"a": 0, "k": 100.0}},
                {"ty": "tr", "p": {"a": 0, "k": [0.0, 0.0]}, "a": {"a": 0, "k": [0.0, 0.0]},
                 "s": {"a": 0, "k": [100.0, 100.0]}, "r": {"a": 0, "k": 0.0}, "o": {"a": 0, "k": 100.0}}]}]}]
    })
    .to_string()
    .into_bytes()
}

fn proj(root: &Path, body: &[u8]) -> PathBuf {
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    write(&folder, "composition.kdl", body);
    folder
}

fn comp_with(lottie_line: &str) -> Vec<u8> {
    format!(
        "composition name=\"main\" width=64 height=64 duration=4 background=\"#000\" version=1 {{\n{lottie_line}\n}}\n"
    )
    .into_bytes()
}

#[test]
fn check_lists_markers_and_drop_note() {
    let root = tmp("check");
    write(&root, "slide.json", &slide_json());
    let folder = proj(
        &root,
        &comp_with("    lottie path=\"../slide.json\" start=0 width=64 height=64"),
    );
    let _ = folder;
    let (code, text) = run(&["check", "--json", "proj"], &root);
    assert_eq!(code, 0, "check --json passes:\n{text}");
    assert!(
        text.contains("\"marker_drops\"")
            && text.contains("\"past-end\"")
            && text.contains("it lies outside the animation window"),
        "the drop is machine-readable:\n{text}"
    );
    let (code, text) = run(&["check", "proj"], &root);
    assert_eq!(code, 0, "check passes:\n{text}");
    assert!(
        text.contains("marker \"intro\" on lottie \"../slide.json\"")
            && text.contains("0.000s..1.000s"),
        "the in-window marker lists as a named range:\n{text}"
    );
    assert!(
        text.contains("marker \"past-end\"") && text.contains("dropped"),
        "the out-of-window marker drops into a named note:\n{text}"
    );
}

#[test]
fn check_refuses_undecodable_and_bad_loop_naming_both() {
    let root = tmp("corrupt");
    write(&root, "slide.json", &slide_json());
    write(&root, "bad.json", b"{oops");
    let folder = proj(
        &root,
        &comp_with("    lottie path=\"../bad.json\" start=0 duration=1 width=64 height=64"),
    );
    let _ = folder;
    let (code, text) = run(&["check", "proj"], &root);
    assert_ne!(code, 0, "corrupt source refuses:\n{text}");
    assert!(
        text.contains("bad.json") && text.contains("E_BAD_LOTTIE"),
        "the refusal names the file and the code:\n{text}"
    );
    let root = tmp("loop");
    write(&root, "slide.json", &slide_json());
    let folder = proj(
        &root,
        &comp_with(
            "    lottie path=\"../slide.json\" start=0 duration=1 width=64 height=64 loop-mode=\"bounce\"",
        ),
    );
    let _ = folder;
    let (code, text) = run(&["check", "proj"], &root);
    assert_ne!(code, 0, "unknown loop mode refuses:\n{text}");
    assert!(
        text.contains("\"bounce\"") && text.contains("loop-mode"),
        "the refusal names the mode:\n{text}"
    );
}
