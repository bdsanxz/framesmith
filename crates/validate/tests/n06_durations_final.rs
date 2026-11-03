//! Sprint 6 T003: the final instant and omitted-duration inference.
//!
//! Validates through the public [`validate_folder`] API only: stills
//! without `duration` run until the composition end, audio without
//! `duration` infers its source window at its rate (like video), a
//! composition without `duration` ends at the latest finite clip end,
//! every inferred length echoes for review, lengths below 1/60s fail
//! naming the value and the bound, and unbounded clips (loop,
//! infinite GIF, open nest) fail until `duration` is written. Sprint 5
//! video inference renders exactly as before.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use validate::validate::{validate_folder, Status};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-validate-n06t003-{tag}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_comp(dir: &Path, body: &str) {
    std::fs::write(dir.join("composition.kdl"), body).unwrap();
}

fn codes(dir: &Path) -> Vec<String> {
    validate_folder(dir)
        .diagnostics
        .iter()
        .map(|d| d.code.to_string())
        .collect()
}

fn messages(dir: &Path) -> Vec<String> {
    validate_folder(dir)
        .diagnostics
        .iter()
        .map(|d| d.message.clone())
        .collect()
}

fn durations(dir: &Path) -> Vec<f64> {
    validate_folder(dir)
        .composition
        .expect("valid composition")
        .clips
        .iter()
        .map(|c| c.duration())
        .collect()
}

/// Minimal PCM WAV (48 kHz stereo s16) of `secs` seconds, written by
/// hand so unit tests need no encoder: the host `ffprobe` still reads
/// its duration for inference.
fn write_wav(dir: &Path, name: &str, secs: f64) {
    let rate: u32 = 48000;
    let frames = (secs * rate as f64).round() as u32;
    let data_len = frames * 2 * 2;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 4).to_le_bytes());
    out.extend_from_slice(&4u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for i in 0..frames * 2 {
        let s =
            ((i as f64 * 440.0 * 2.0 * std::f64::consts::PI / rate as f64).sin() * 10000.0) as i16;
        out.extend_from_slice(&s.to_le_bytes());
    }
    std::fs::write(dir.join(name), out).unwrap();
}

/// Minimal infinitely looping one-frame GIF: header, screen
/// descriptor, global table, a NETSCAPE loop-count-0 extension, one
/// image, trailer. Enough for the content sniff, never decoded here.
fn write_infinite_gif(dir: &Path, name: &str) {
    let mut out = Vec::new();
    out.extend_from_slice(b"GIF89a");
    out.extend_from_slice(&[1, 0, 1, 0, 0x80, 0, 0]);
    out.extend_from_slice(&[0, 0, 0, 255, 255, 255]);
    out.extend_from_slice(b"\x21\xFF\x0BNETSCAPE2.0\x03\x01\x00\x00\x00");
    out.extend_from_slice(&[0x2C, 0, 0, 0, 0, 1, 0, 1, 0, 0, 2, 2, 0x4C, 1, 0]);
    out.extend_from_slice(&[0x3B]);
    std::fs::write(dir.join(name), out).unwrap();
}

const OPEN4: &str =
    "composition name=\"main\" width=64 height=64 duration=4 background=\"#000\" version=1 {\n";

// --- still inference ---------------------------------------------------------

#[test]
fn omitted_still_duration_runs_until_composition_end() {
    let dir = tmp("still-end");
    write_comp(
        &dir,
        &format!(
            "{OPEN4}\n             text value=\"Hi\" start=0\n             box x=0 y=0 width=8 height=8 fill=\"#ff0000\" start=1 duration=1\n             }}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    assert_eq!(durations(&dir), vec![4.0, 1.0]);
    let kinds: Vec<&str> = v.inferred.iter().map(|f| f.kind).collect();
    assert_eq!(kinds, vec!["text"], "{:?}", v.inferred);
    assert_eq!(v.inferred[0].duration, 4.0);
    assert!(
        v.inferred[0].detail.contains("composition end 4.000s"),
        "{:?}",
        v.inferred
    );
}

#[test]
fn omitted_still_with_relative_start_uses_resolved_start() {
    let dir = tmp("still-rel");
    write_comp(
        &dir,
        &format!(
            "{OPEN4}\n             text value=\"A\" start=0 duration=1 id=\"a\"\n             text value=\"B\" id=\"b\" start-after=\"a\" start-offset=0.5\n             }}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    // B starts at 1.5 and runs until the composition end: 2.5.
    assert_eq!(durations(&dir), vec![1.0, 2.5]);
    assert_eq!(v.inferred.len(), 1);
    assert_eq!(v.inferred[0].duration, 2.5);
}

#[test]
fn still_starting_at_composition_end_is_too_short() {
    let dir = tmp("still-past");
    write_comp(
        &dir,
        "composition name=\"main\" width=64 height=64 duration=2 background=\"#000\" version=1 {\n    box x=0 y=0 width=8 height=8 fill=\"#ff0000\" start=2\n}\n",
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
    assert_eq!(codes(&dir), vec!["E_BAD_TIMING"]);
    let msg = messages(&dir).join("\n");
    assert!(msg.contains("1/60"), "{msg}");
}

// --- audio inference ---------------------------------------------------------

#[test]
fn omitted_audio_duration_infers_source_window_and_rate() {
    let dir = tmp("audio-inf");
    write_wav(&dir, "s.wav", 4.0);
    write_comp(
        &dir,
        &format!(
            "{OPEN4}\n             audio path=\"s.wav\" start=0 trim-start=0.5 rate=2\n             }}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    // (4 - 0.5) / 2 = 1.75.
    assert_eq!(durations(&dir), vec![1.75]);
    assert_eq!(v.inferred.len(), 1);
    assert_eq!(v.inferred[0].kind, "audio");
    assert_eq!(v.inferred[0].label.as_deref(), Some("s.wav"));
    assert!((v.inferred[0].duration - 1.75).abs() < 1e-9);
    assert!(
        v.inferred[0].detail.contains("rate 2x"),
        "{:?}",
        v.inferred[0].detail
    );
}

#[test]
fn audio_rate_outside_range_is_refused() {
    let dir = tmp("audio-rate");
    write_wav(&dir, "s.wav", 2.0);
    write_comp(
        &dir,
        &format!("{OPEN4}\n             audio path=\"s.wav\" start=0 duration=1 rate=20\n             }}\n"),
    );
    assert_eq!(codes(&dir), vec!["E_BAD_AUDIO"]);
}

#[test]
fn audio_without_duration_and_without_knowable_end_asks_for_duration() {
    let dir = tmp("audio-unknown");
    std::fs::write(dir.join("s.wav"), b"not really audio").unwrap();
    write_comp(
        &dir,
        &format!("{OPEN4}\n             audio path=\"s.wav\" start=0\n             }}\n"),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
    assert_eq!(codes(&dir), vec!["E_BAD_AUDIO"]);
    let msg = messages(&dir).join("\n");
    assert!(msg.contains("duration"), "{msg}");
}

#[test]
fn written_audio_duration_stays_unmarked() {
    let dir = tmp("audio-written");
    write_wav(&dir, "s.wav", 4.0);
    write_comp(
        &dir,
        &format!(
            "{OPEN4}\n             audio path=\"s.wav\" start=0 duration=2\n             }}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    assert_eq!(durations(&dir), vec![2.0]);
    assert!(v.inferred.is_empty(), "{:?}", v.inferred);
}

// --- composition inference ---------------------------------------------------

#[test]
fn omitted_composition_duration_ends_at_latest_finite_clip_end() {
    let dir = tmp("comp-inf");
    write_comp(
        &dir,
        "composition name=\"main\" width=64 height=64 background=\"#000\" version=1 {\n    text value=\"A\" start=0 duration=1 id=\"a\"\n    box x=0 y=0 width=8 height=8 fill=\"#ff0000\" id=\"b\" start-after=\"a\" duration=1.5\n}\n",
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    let comp = v.composition.expect("valid composition");
    // The chain resolves B to [1, 2.5): the composition ends at 2.5.
    assert_eq!(comp.duration, 2.5);
    assert_eq!(v.inferred.len(), 1);
    assert_eq!(v.inferred[0].kind, "composition");
    assert_eq!(v.inferred[0].label.as_deref(), Some("main"));
    assert_eq!(v.inferred[0].duration, 2.5);
}

#[test]
fn follower_of_composition_resolves_after_composition_inference() {
    let dir = tmp("comp-target");
    write_comp(
        &dir,
        "composition name=\"main\" width=64 height=64 background=\"#000\" version=1 {\n    text value=\"A\" start=0 duration=2 id=\"a\"\n    text value=\"B\" id=\"b\" start-after=\"main\" duration=1\n}\n",
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    // The composition ends at A's end (2.0); B follows it at [2, 3).
    // B's overrun is a consequence of the inferred end, not a new
    // bound: the composition stays at 2.0.
    let comp = v.composition.expect("valid composition");
    assert_eq!(comp.duration, 2.0);
    let starts: Vec<f64> = comp.clips.iter().map(|c| c.start()).collect();
    assert_eq!(starts, vec![0.0, 2.0]);
}

#[test]
fn layered_pending_stills_settle_through_followers() {
    let dir = tmp("deep-chain");
    write_comp(
        &dir,
        "composition name=\"main\" width=64 height=64 duration=6 background=\"#000\" version=1 {\n    text value=\"A\" start=0 duration=1 id=\"a\"\n    text value=\"B\" id=\"b\" start-after=\"a\"\n    text value=\"C\" id=\"c\" start-after=\"b\" duration=1\n    text value=\"D\" id=\"d\" start-after=\"c\" duration=1\n}\n",
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    // B runs [1, 6); C follows at [6, 7); D at [7, 8).
    let starts: Vec<f64> = v
        .composition
        .expect("valid composition")
        .clips
        .iter()
        .map(|c| c.start())
        .collect();
    assert_eq!(starts, vec![0.0, 1.0, 6.0, 7.0]);
    assert_eq!(durations(&dir), vec![1.0, 5.0, 1.0, 1.0]);
}

#[test]
fn omitted_composition_with_no_finite_end_fails_once() {
    let dir = tmp("comp-empty");
    write_comp(
        &dir,
        "composition name=\"main\" width=64 height=64 background=\"#000\" version=1 {\n    text value=\"A\" start=0\n    text value=\"B\" start=1\n}\n",
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
    // One missing bound, one diagnostic: the stills stay silent while
    // the composition names the fix (no cascade).
    assert_eq!(codes(&dir), vec!["E_BAD_DURATION"]);
}

// --- tiny lengths ------------------------------------------------------------

#[test]
fn written_length_below_one_sixtieth_fails_naming_value_and_bound() {
    let dir = tmp("tiny-written");
    write_comp(
        &dir,
        &format!(
            "{OPEN4}\n             text value=\"Hi\" start=0 duration=0.01\n             }}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
    assert_eq!(codes(&dir), vec!["E_BAD_TIMING"]);
    let msg = messages(&dir).join("\n");
    assert!(msg.contains("0.01"), "{msg}");
    assert!(msg.contains("1/60"), "{msg}");
}

#[test]
fn tiny_composition_fails_naming_value_and_bound() {
    let dir = tmp("tiny-comp");
    write_comp(
        &dir,
        "composition name=\"main\" width=64 height=64 duration=0.01 background=\"#000\" version=1 {\n    text value=\"Hi\" start=0 duration=0.01\n}\n",
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
    let all = messages(&dir).join("\n");
    assert!(all.contains("1/60"), "{all}");
    assert!(
        codes(&dir).contains(&"E_BAD_DURATION".to_string()),
        "{:?}",
        codes(&dir)
    );
}

#[test]
fn empty_audio_window_without_duration_is_too_short() {
    let dir = tmp("tiny-audio");
    write_wav(&dir, "s.wav", 2.0);
    write_comp(
        &dir,
        &format!(
            "{OPEN4}\n             audio path=\"s.wav\" start=0 trim-start=1 trim-end=1\n             }}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
    assert_eq!(codes(&dir), vec!["E_BAD_TIMING"]);
    let msg = messages(&dir).join("\n");
    assert!(msg.contains("1/60"), "{msg}");
}

// --- unbounded refusals ------------------------------------------------------

#[test]
fn looping_video_without_duration_is_refused_until_written() {
    let dir = tmp("loop-open");
    write_comp(
        &dir,
        &format!(
            "{OPEN4}\n             video path=\"clip.mp4\" start=0 loop=true\n             }}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
    assert!(codes(&dir).contains(&"E_BAD_FOOTAGE".to_string()));
    let msg = messages(&dir).join("\n");
    assert!(msg.contains("duration"), "{msg}");
}

#[test]
fn infinite_gif_without_duration_is_refused_until_written() {
    let dir = tmp("gif-open");
    write_infinite_gif(&dir, "loop.gif");
    write_comp(
        &dir,
        &format!("{OPEN4}\n             video path=\"loop.gif\" start=0\n             }}\n"),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
    assert!(codes(&dir).contains(&"E_BAD_FOOTAGE".to_string()));
}

#[test]
fn open_nest_without_duration_is_refused_until_written() {
    let dir = tmp("nest-open");
    write_comp(
        &dir,
        &format!("{OPEN4}\n             nest nest=\"main\" start=0\n             }}\n"),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
    assert_eq!(codes(&dir), vec!["E_BAD_TIMING"]);
    let msg = messages(&dir).join("\n");
    assert!(msg.contains("unbounded"), "{msg}");
    assert!(msg.contains("duration"), "{msg}");
}

// --- Sprint 5 video inference is unchanged -----------------------------------

#[test]
fn video_inference_math_is_unchanged() {
    let dir = tmp("video-same");
    // Declared trims bound the window without any probe: the file only
    // needs to exist and read as non-GIF bytes.
    std::fs::write(dir.join("clip.mp4"), b"fake-bytes-for-trim-math").unwrap();
    write_comp(
        &dir,
        &format!(
            "{OPEN4}\n             video path=\"clip.mp4\" start=0 trim-start=1 trim-end=3 rate=2\n             }}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    assert_eq!(durations(&dir), vec![1.0]);
    assert_eq!(v.footage.len(), 1);
    assert!(v.footage[0].inferred);
    assert_eq!(v.footage[0].duration, 1.0);
    assert!(
        v.inferred.is_empty(),
        "video keeps its own echo: {:?}",
        v.inferred
    );
}
