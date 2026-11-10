//! Sprint 21 T001: native `lottie` clips through the public
//! [`validate_folder`] API — the same verdict `check` reports, with
//! the decoded markers asserted on the model.
//!
//! Covers every acceptance criterion of `T001-play-lottie-natively.md`:
//! a file seeks by film time (the clip resolves a finite window and a
//! loop mode), loop modes honor the file setting, markers expose as
//! named ranges (out-of-window markers drop into named notes),
//! undecodable files refuse naming file and cause, and expressions
//! evaluate to their baked values (decode-level, asserted here through
//! a file carrying an expression program).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use validate::validate::{validate_folder, Diagnostic, Status};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-validate-n21lottie-{}-{}-{}",
        tag,
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(folder: &Path, name: &str, content: &[u8]) {
    std::fs::write(folder.join(name), content).unwrap();
}

/// Smallest time-varying animation: a red rect sliding left to right
/// over 2s at 30fps, one in-window marker, one past-end marker, and
/// an expression program on opacity (baked to 100, never run).
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
            "ks": {
                "o": {"a": 0, "k": 100.0, "x": "wiggle(2,50)"},
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

/// One dotLottie pack holding `slide.json` under `id` with the given
/// manifest `loop` flag (stored entries; no compression needed).
fn pack_lottie(id: &str, loop_flag: bool) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default();
    zip.start_file("manifest.json", opts).unwrap();
    let manifest = format!(r#"{{"animations":[{{"id":"{id}","loop":{loop_flag}}}]}}"#);
    std::io::Write::write_all(&mut zip, manifest.as_bytes()).unwrap();
    zip.start_file(&format!("animations/{id}.json"), opts)
        .unwrap();
    std::io::Write::write_all(&mut zip, &slide_json()).unwrap();
    zip.finish().unwrap().into_inner()
}

fn comp_with(lottie_line: &str) -> String {
    format!(
        "composition name=\"main\" width=64 height=64 duration=4 background=\"#000\" version=1 {{\n{lottie_line}\n}}\n"
    )
}

fn first_lottie(dir: &Path) -> notation::model::LottieClip {
    let v = validate_folder(dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    match v.composition.unwrap().clips.into_iter().next().unwrap() {
        notation::model::Clip::Lottie(c) => c,
        other => panic!("expected a lottie, got {other:?}"),
    }
}

fn codes(ds: &[Diagnostic]) -> Vec<String> {
    ds.iter().map(|d| d.code.to_string()).collect()
}

fn assert_refusal(ds: &[Diagnostic], code: &str, needles: &[&str]) {
    assert!(!ds.is_empty(), "expected a refusal, got none");
    assert!(
        ds.iter().any(|d| d.code == code),
        "expected code {code} in {:?}",
        codes(ds)
    );
    let messages: Vec<String> = ds.iter().map(|d| d.message.clone()).collect();
    for needle in needles {
        assert!(
            messages.iter().any(|m| m.contains(needle)),
            "expected {needle:?} in {messages:?}"
        );
    }
}

#[test]
fn valid_lottie_resolves_window_markers_and_drop_note() {
    let dir = tmp("valid");
    write(&dir, "slide.json", &slide_json());
    write(
        &dir,
        "composition.kdl",
        comp_with("    lottie path=\"slide.json\" start=0 width=64 height=64").as_bytes(),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    // An omitted `duration` resolves to the intrinsic animation
    // length and echoes in `check`, exactly like footage inference.
    let clip = first_lottie(&dir);
    assert!(
        (clip.duration - 2.0).abs() < 1e-9,
        "intrinsic 2s, got {}",
        clip.duration
    );
    assert_eq!(clip.loop_mode, notation::model::LoopMode::None);
    // Markers expose as named ranges on the clip.
    assert_eq!(clip.markers.len(), 1);
    assert_eq!(clip.markers[0].name, "intro");
    assert!((clip.markers[0].start - 0.0).abs() < 1e-12);
    assert!((clip.markers[0].end - 1.0).abs() < 1e-12);
    // The past-end marker drops into a named verdict note, never a
    // diagnostic: the verdict stays valid.
    assert_eq!(v.marker_drops.len(), 1);
    assert_eq!(v.marker_drops[0].name, "past-end");
    assert_eq!(v.marker_drops[0].path, "slide.json");
    // The inference echoes for review.
    assert!(
        v.inferred
            .iter()
            .any(|i| i.kind == "lottie" && (i.duration - 2.0).abs() < 1e-9),
        "intrinsic length echoes: {:?}",
        v.inferred
            .iter()
            .map(|i| (i.kind, i.duration.clone(), i.detail.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn file_loop_setting_beats_plain_default_and_author_beats_file() {
    // A dotLottie pack with `loop:true` defaults to looping.
    let dir = tmp("pack-loop");
    write(&dir, "pack.lottie", &pack_lottie("slide", true));
    write(
        &dir,
        "composition.kdl",
        comp_with("    lottie path=\"pack.lottie\" start=0 width=64 height=64").as_bytes(),
    );
    let clip = first_lottie(&dir);
    assert_eq!(clip.loop_mode, notation::model::LoopMode::Loop);
    // An authored `loop-mode` wins over the file setting.
    let dir = tmp("author-wins");
    write(&dir, "pack.lottie", &pack_lottie("slide", true));
    write(
        &dir,
        "composition.kdl",
        comp_with("    lottie path=\"pack.lottie\" start=0 duration=2 width=64 height=64 loop-mode=\"ping-pong\"")
            .as_bytes(),
    );
    let clip = first_lottie(&dir);
    assert_eq!(clip.loop_mode, notation::model::LoopMode::PingPong);
    assert!((clip.duration - 2.0).abs() < 1e-9);
    // The underscore spelling is accepted, like every style key.
    let dir = tmp("underscore");
    write(&dir, "slide.json", &slide_json());
    write(
        &dir,
        "composition.kdl",
        comp_with("    lottie path=\"slide.json\" start=0 duration=1 width=64 height=64 loop_mode=\"loop\"")
            .as_bytes(),
    );
    let clip = first_lottie(&dir);
    assert_eq!(clip.loop_mode, notation::model::LoopMode::Loop);
}

#[test]
fn unknown_loop_mode_refuses_naming_the_mode() {
    let dir = tmp("bad-loop");
    write(&dir, "slide.json", &slide_json());
    write(
        &dir,
        "composition.kdl",
        comp_with("    lottie path=\"slide.json\" start=0 duration=1 width=64 height=64 loop-mode=\"bounce\"")
            .as_bytes(),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid);
    assert_refusal(&v.diagnostics, "E_BAD_LOTTIE", &["bounce", "loop-mode"]);
}

#[test]
fn undecodable_file_refuses_naming_file_and_cause() {
    let dir = tmp("corrupt");
    write(&dir, "bad.json", b"{oops".as_slice());
    write(
        &dir,
        "composition.kdl",
        comp_with("    lottie path=\"bad.json\" start=0 duration=1 width=64 height=64").as_bytes(),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid);
    assert_refusal(&v.diagnostics, "E_BAD_LOTTIE", &["bad.json", "undecodable"]);
    // A non-animation extension refuses before any decode.
    let dir = tmp("ext");
    write(&dir, "note.txt", b"{}");
    write(
        &dir,
        "composition.kdl",
        comp_with("    lottie path=\"note.txt\" start=0 duration=1 width=64 height=64").as_bytes(),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid);
    assert_refusal(&v.diagnostics, "E_BAD_LOTTIE", &["note.txt", ".json"]);
    // A missing file names the file.
    let dir = tmp("missing");
    write(
        &dir,
        "composition.kdl",
        comp_with("    lottie path=\"gone.json\" start=0 duration=1 width=64 height=64").as_bytes(),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid);
    assert_refusal(&v.diagnostics, "E_MISSING_FILE", &["gone.json"]);
    // A corrupt pack names the file and the cause.
    let dir = tmp("badpack");
    write(&dir, "pack.lottie", b"not a zip");
    write(
        &dir,
        "composition.kdl",
        comp_with("    lottie path=\"pack.lottie\" start=0 duration=1 width=64 height=64")
            .as_bytes(),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid);
    assert_refusal(&v.diagnostics, "E_BAD_LOTTIE", &["pack.lottie"]);
}

#[test]
fn unsupported_layer_refuses_naming_layer_and_type() {
    let dir = tmp("imglayer");
    let mut v: serde_json::Value = serde_json::from_slice(&slide_json()).unwrap();
    v["layers"][0]["ty"] = serde_json::json!(2);
    write(&dir, "img.json", &serde_json::to_vec(&v).unwrap());
    write(
        &dir,
        "composition.kdl",
        comp_with("    lottie path=\"img.json\" start=0 duration=1 width=64 height=64").as_bytes(),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid);
    assert_refusal(&v.diagnostics, "E_BAD_LOTTIE", &["\"box\"", "ty=2"]);
}
