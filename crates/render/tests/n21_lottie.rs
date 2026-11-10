//! Sprint 21 T001: native `lottie` clips through prepare plus the CPU
//! blitter — the same frames `render` writes.
//!
//! Covers the render half of `T001-play-lottie-natively.md`: every seek
//! renders the time-correct frame, loop wraps land on the base instant
//! (deterministic across loop boundaries), reruns are byte-identical,
//! and an undecodable file refuses at preparation — before the first
//! frame — naming the file.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-render-n21lottie-{}-{}-{}",
        tag,
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The shared slide fixture: a red rect sliding 10→50px over 2s at
/// 30fps (see `crates/lottie/src/lib.rs`).
fn slide_json() -> Vec<u8> {
    serde_json::json!({
        "v": "5.7.4", "fr": 30.0, "ip": 0.0, "op": 60.0, "w": 64.0, "h": 64.0,
        "nm": "slide",
        "markers": [{"tm": 0.0, "cm": "intro", "dr": 30.0}],
        "layers": [{
            "ddd": 0, "ind": 1, "ty": 4, "nm": "box", "sr": 1.0,
            "ks": {
                "o": {"a": 0, "k": 100.0},
                "r": {"a": 0, "k": 0.0},
                "p": {"a": 1, "k": [
                    {"t": 0.0, "s": [10.0, 32.0, 0.0], "e": [50.0, 32.0, 0.0]},
                    {"t": 30.0, "s": [50.0, 32.0, 0.0]}
                ]},
                "a": {"a": 0, "k": [0.0, 0.0, 0.0]},
                "s": {"a": 0, "k": [100.0, 100.0, 100.0]}
            },
            "shapes": [{
                "ty": "gr", "it": [
                    {"ty": "rc", "p": {"a": 0, "k": [0.0, 0.0]}, "s": {"a": 0, "k": [20.0, 20.0]}, "r": {"a": 0, "k": 0.0}},
                    {"ty": "fl", "c": {"a": 0, "k": [1.0, 0.0, 0.0, 1.0]}, "o": {"a": 0, "k": 100.0}},
                    {"ty": "tr", "p": {"a": 0, "k": [0.0, 0.0]}, "a": {"a": 0, "k": [0.0, 0.0]},
                     "s": {"a": 0, "k": [100.0, 100.0]}, "r": {"a": 0, "k": 0.0}, "o": {"a": 0, "k": 100.0}}
                ]
            }]
        }]
    })
    .to_string()
    .into_bytes()
}

fn project(dir: &Path, lottie_line: &str, duration: u32) {
    std::fs::write(dir.join("slide.json"), slide_json()).unwrap();
    let comp = format!(
        "composition name=\"main\" width=64 height=64 duration={duration} fps=30 background=\"#000000\" version=1 {{\n{lottie_line}\n}}\n"
    );
    std::fs::write(dir.join("composition.kdl"), comp).unwrap();
}

fn prepared(dir: &Path) -> render::render::Prepared {
    let v = validate::validate::validate_folder(dir);
    assert_eq!(
        v.status,
        validate::validate::Status::Valid,
        "{:?}",
        v.diagnostics
    );
    let comp = v.composition.unwrap();
    render::render::prepare_frames(&comp, &[comp.clone()], dir, Path::new("ffmpeg"))
        .expect("lottie-only preparation needs no decoder binary")
}

fn pixel(frame: &[u8], x: u32, y: u32) -> (u8, u8, u8) {
    let o = (y * 64 + x) as usize * 3;
    (frame[o], frame[o + 1], frame[o + 2])
}

#[test]
fn seek_renders_the_time_correct_frame_deterministically() {
    let dir = tmp("seek");
    project(
        &dir,
        "    lottie path=\"slide.json\" start=0 width=64 height=64",
        2,
    );
    let mut prep = prepared(&dir);
    let early = prep.cpu_frame(0).expect("frame 0 draws");
    let late = prep.cpu_frame(30).expect("frame 30 draws");
    assert_ne!(early, late, "the rect moves across the seek");
    // At t=0 the rect sits at x=10 (20px wide, centered at 10):
    // pixel (10,32) is red, pixel (50,32) is background black.
    assert_eq!(pixel(&early, 10, 32), (255, 0, 0));
    assert_eq!(pixel(&early, 50, 32), (0, 0, 0));
    // At t=1 the rect sits at x=50.
    assert_eq!(pixel(&late, 50, 32), (255, 0, 0));
    assert_eq!(pixel(&late, 10, 32), (0, 0, 0));
    // Reruns are byte-identical.
    let mut prep2 = prepared(&dir);
    assert_eq!(prep2.cpu_frame(0).expect("rerun draws"), early);
    assert_eq!(prep2.cpu_frame(30).expect("rerun draws"), late);
}

#[test]
fn loop_wrap_lands_on_the_base_instant() {
    let dir = tmp("loop");
    project(
        &dir,
        "    lottie path=\"slide.json\" start=0 duration=4 width=64 height=64 loop-mode=\"loop\"",
        4,
    );
    let mut prep = prepared(&dir);
    // t=0.5s (frame 15) and t=2.5s (frame 75, one period later).
    let base = prep.cpu_frame(15).expect("base draws");
    let wrapped = prep.cpu_frame(75).expect("wrapped draws");
    assert_eq!(base, wrapped, "loop wrap is time-correct");
    // A non-looping clip holds its edge frame past the window.
    let dir = tmp("hold");
    project(
        &dir,
        "    lottie path=\"slide.json\" start=0 duration=4 width=64 height=64",
        4,
    );
    let mut prep = prepared(&dir);
    let end = prep.cpu_frame(60).expect("window edge draws");
    let past = prep.cpu_frame(90).expect("past-end draws");
    assert_eq!(end, past, "past-end holds the edge frame");
}

#[test]
fn undecodable_source_refuses_before_the_first_frame() {
    let dir = tmp("refuse");
    std::fs::write(dir.join("bad.json"), b"{oops").unwrap();
    let comp = "composition name=\"main\" width=64 height=64 duration=1 fps=30 background=\"#000000\" version=1 {\n    lottie path=\"bad.json\" start=0 duration=1 width=64 height=64\n}\n";
    std::fs::write(dir.join("composition.kdl"), comp).unwrap();
    // Validation refuses first, naming the file and the cause.
    let v = validate::validate::validate_folder(&dir);
    assert_eq!(v.status, validate::validate::Status::Invalid);
    assert!(
        v.diagnostics
            .iter()
            .any(|d| d.code == "E_BAD_LOTTIE" && d.message.contains("bad.json")),
        "{:?}",
        v.diagnostics
            .iter()
            .map(|d| (d.code.clone(), d.message.clone()))
            .collect::<Vec<_>>()
    );
}
