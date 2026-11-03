//! Sprint 10 sound (T005 + T006): echo/room parameters and pause detection.
//!
//! Validates through the public [`validate_folder`] API only: `delay`
//! (0..2 s) with `delay-feedback` (0..0.95) and `delay-mix` (0..1)
//! plus `reverb` (0..1) land on sound-bearing clips; out-of-range
//! values fail naming the bound; overlong tails report in
//! `effect_tails` while the verdict stays valid; `detect-silence`
//! with `silence-threshold` (plain dBFS) and `silence-min` (seconds
//! from 0) parses; and unparsable thresholds fail naming the accepted
//! form. Signal measurement itself (tap gains, pause ranges) is proven
//! in the render unit tests and the CLI end-to-end suite; these tests
//! prove the declarations validate.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use validate::validate::{validate_folder, Status};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-validate-n08fx-{tag}-{}-{}",
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

/// Minimal PCM WAV (48 kHz stereo s16) of `secs` seconds, written by
/// hand so validation tests need no encoder.
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

const OPEN4: &str =
    "composition name=\"main\" width=64 height=64 duration=4 background=\"#000\" version=1 {\n";

fn messages(dir: &Path) -> Vec<(String, String, String)> {
    validate_folder(dir)
        .diagnostics
        .iter()
        .map(|d| (d.code.to_string(), d.message.clone(), d.fix.clone()))
        .collect()
}

// --- valid effects -----------------------------------------------------------

#[test]
fn delay_reverb_and_detection_validate_on_audio() {
    let dir = tmp("valid");
    write_wav(&dir, "s.wav", 4.0);
    write_comp(
        &dir,
        &format!(
            "{OPEN4}    audio path=\"s.wav\" start=0 duration=4 id=\"bed\" delay=0.3 delay-feedback=0.5 delay-mix=0.8 reverb=0.2 detect-silence=true silence-threshold=-30 silence-min=0.25\n}}\n"
        ),
    );
    let verdict = validate_folder(&dir);
    assert_eq!(verdict.status, Status::Valid, "{:?}", verdict.diagnostics);
    let comp = verdict.composition.expect("valid verdict carries the mix");
    assert_eq!(comp.clips.len(), 1);
    match &comp.clips[0] {
        notation::model::Clip::Audio(a) => {
            assert_eq!(
                (a.fx.delay, a.fx.delay_feedback, a.fx.delay_mix),
                (0.3, 0.5, 0.8)
            );
            assert_eq!(a.fx.reverb, 0.2);
            assert!(a.fx.detect_silence);
            assert_eq!(a.fx.silence_threshold_db, -30.0);
            assert_eq!(a.fx.silence_min, 0.25);
        }
        other => panic!("expected an audio track, found {:?}", other.kind()),
    }
}

#[test]
fn effect_aliases_and_defaults_validate() {
    let dir = tmp("alias");
    write_wav(&dir, "s.wav", 4.0);
    write_comp(
        &dir,
        &format!(
            "{OPEN4}    audio path=\"s.wav\" start=0 duration=4 delay_feedback=0.25 delay_mix=0.5 detect_silence=\"true\" silence_threshold=-40 silence_min=0.5\n}}\n"
        ),
    );
    let verdict = validate_folder(&dir);
    assert_eq!(verdict.status, Status::Valid, "{:?}", verdict.diagnostics);
    match &verdict.composition.unwrap().clips[0] {
        notation::model::Clip::Audio(a) => {
            assert_eq!(a.fx.delay, 0.0);
            assert_eq!((a.fx.delay_feedback, a.fx.delay_mix), (0.25, 0.5));
            assert!(a.fx.detect_silence);
        }
        other => panic!("expected an audio track, found {:?}", other.kind()),
    }
}

// --- out-of-range effects refuse ----------------------------------------------

#[test]
fn out_of_range_effects_refuse_naming_the_bound() {
    for (tag, key, bound) in [
        ("delay-range", "delay=2.5", "0 to 2"),
        ("feedback-range", "delay-feedback=0.99", "0 to 0.95"),
        ("mix-range", "delay-mix=1.5", "0 to 1"),
        ("reverb-range", "reverb=-0.1", "0 to 1"),
        ("reverb-high", "reverb=1.5", "0 to 1"),
    ] {
        let dir = tmp(tag);
        write_wav(&dir, "s.wav", 4.0);
        write_comp(
            &dir,
            &format!("{OPEN4}    audio path=\"s.wav\" start=0 duration=4 {key}\n}}\n"),
        );
        let verdict = validate_folder(&dir);
        assert_eq!(verdict.status, Status::Invalid, "{tag}");
        let got = messages(&dir);
        assert_eq!(got.len(), 1, "{tag}: {got:?}");
        assert_eq!(got[0].0, "E_BAD_AUDIO", "{tag}: {got:?}");
        assert!(got[0].1.contains(bound), "{tag}: {got:?}");
        assert!(
            got[0].1.contains(key.split('=').next().unwrap()),
            "{tag}: {got:?}"
        );
    }
}

// --- unparsable detection thresholds refuse ------------------------------------

#[test]
fn unparsable_thresholds_refuse_naming_the_accepted_form() {
    for (tag, key, want) in [
        (
            "threshold-text",
            "detect-silence=true silence-threshold=\"loud\"",
            "plain dB number",
        ),
        (
            "switch-text",
            "detect-silence=\"maybe\"",
            "`true` and `false`",
        ),
        ("min-negative", "silence-min=-1", "0 seconds and up"),
    ] {
        let dir = tmp(tag);
        write_wav(&dir, "s.wav", 4.0);
        write_comp(
            &dir,
            &format!("{OPEN4}    audio path=\"s.wav\" start=0 duration=4 {key}\n}}\n"),
        );
        let verdict = validate_folder(&dir);
        assert_eq!(verdict.status, Status::Invalid, "{tag}");
        let got = messages(&dir);
        assert_eq!(got.len(), 1, "{tag}: {got:?}");
        assert_eq!(got[0].0, "E_BAD_AUDIO", "{tag}: {got:?}");
        assert!(got[0].1.contains(want), "{tag}: {got:?}");
        assert!(
            !got[0].2.is_empty(),
            "{tag}: a fix travels with the refusal"
        );
    }
}

// --- tail truncation reports ----------------------------------------------------

#[test]
fn overlong_tails_report_while_fitting_tails_stay_silent() {
    // A 0.3 s / 0.5 / 0.8 echo tails 3 s past a 2 s composition end.
    let dir = tmp("tail");
    write_wav(&dir, "s.wav", 2.0);
    write_comp(
        &dir,
        "composition name=\"main\" width=64 height=64 duration=2 background=\"#000\" version=1 {\n    audio path=\"s.wav\" start=0 duration=2 id=\"wet\" delay=0.3 delay-feedback=0.5 delay-mix=0.8\n}\n",
    );
    let verdict = validate_folder(&dir);
    assert_eq!(verdict.status, Status::Valid, "{:?}", verdict.diagnostics);
    assert_eq!(verdict.effect_tails.len(), 1);
    let tail = &verdict.effect_tails[0];
    assert_eq!(tail.clip, 0);
    assert_eq!(tail.id.as_deref(), Some("wet"));
    assert!((tail.delay_tail - 3.0).abs() < 1e-9, "{tail:?}");
    assert!((tail.raw_end - 5.0).abs() < 1e-9, "{tail:?}");
    assert_eq!(tail.eff_end, 2.0);
    // A short room tail that fits inside the composition reports nothing.
    let dir = tmp("fit");
    write_wav(&dir, "s.wav", 4.0);
    write_comp(
        &dir,
        &format!("{OPEN4}    audio path=\"s.wav\" start=0 duration=1 reverb=0.1\n}}\n"),
    );
    let verdict = validate_folder(&dir);
    assert_eq!(verdict.status, Status::Valid, "{:?}", verdict.diagnostics);
    assert!(
        verdict.effect_tails.is_empty(),
        "{:?}",
        verdict.effect_tails
    );
}
