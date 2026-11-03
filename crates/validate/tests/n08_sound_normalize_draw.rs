//! Sprint 10 sound (T009 + T010): loudness target parsing and
//! on-canvas waveform range review.
//!
//! Validates through the public [`validate_folder`] API only:
//! `normalize="loud"` lands on the composition (anything else fails
//! naming the accepted target); `waveform` with `waveform-from`/`to`
//! lands on sound-bearing clips (`audio` tracks and `video` clips);
//! bad switches and unparsable bounds fail naming the accepted form;
//! bounds without the switch fail; empty and backward drawn ranges
//! fail naming the bounds; fully outside ranges fail with nothing to
//! draw; beyond-window ranges clamp and report in `waveform_clamps`
//! while the verdict stays valid. Silence measurement lives in the
//! mixer preview (`check`/`render` gate), not here.

use notation::model::Clip;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use validate::validate::{validate_folder, Status};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-validate-n08norm-{tag}-{}-{}",
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
    for _ in 0..frames * 2 {
        out.extend_from_slice(&0i16.to_le_bytes());
    }
    std::fs::write(dir.join(name), out).unwrap();
}

fn messages(dir: &Path) -> Vec<(String, String, String)> {
    let verdict = validate_folder(dir);
    assert_eq!(verdict.status, Status::Invalid, "{verdict:?}");
    verdict
        .diagnostics
        .iter()
        .map(|d| (d.code.to_string(), d.message.clone(), d.fix.clone()))
        .collect()
}

const HEAD: &str =
    "composition name=\"t\" width=64 height=48 duration=4 fps=30 background=\"#000000\" version=1";

#[test]
fn normalize_loud_validates_on_the_composition() {
    let dir = tmp("target");
    write_wav(&dir, "s.wav", 4.0);
    write_comp(
        &dir,
        &format!("{HEAD} normalize=\"loud\" {{\n    audio path=\"s.wav\" start=0 duration=4\n}}\n"),
    );
    let verdict = validate_folder(&dir);
    assert_eq!(verdict.status, Status::Valid, "{verdict:?}");
    let comp = verdict
        .composition
        .expect("valid verdict carries a composition");
    assert!(comp.normalize.active());
    assert_eq!(comp.normalize_loc, Some((1, 100)));
    assert!(verdict.waveform_clamps.is_empty());
}

#[test]
fn bad_normalize_targets_refuse_naming_the_accepted_target() {
    for (tag, value) in [
        ("word", "normalize=\"peak\""),
        ("bare", "normalize=true"),
        ("number", "normalize=2"),
    ] {
        let dir = tmp(tag);
        write_wav(&dir, "s.wav", 4.0);
        write_comp(
            &dir,
            &format!("{HEAD} {value} {{\n    audio path=\"s.wav\" start=0 duration=4\n}}\n"),
        );
        let got = messages(&dir);
        assert_eq!(got.len(), 1, "{tag}: {got:?}");
        assert_eq!(got[0].0, "E_BAD_AUDIO", "{tag}: {got:?}");
        assert!(got[0].1.contains("`loud`"), "{tag}: {got:?}");
        assert!(got[0].2.contains("remove the key"), "{tag}: {got:?}");
    }
}

#[test]
fn waveform_with_ranges_validates_on_sound_bearers() {
    let dir = tmp("draw");
    write_wav(&dir, "s.wav", 4.0);
    write_comp(
        &dir,
        &format!(
            "{HEAD} {{\n    audio path=\"s.wav\" start=0 duration=4 waveform=true waveform-from=1 waveform-to=3\n}}\n"
        ),
    );
    let verdict = validate_folder(&dir);
    assert_eq!(verdict.status, Status::Valid, "{verdict:?}");
    let Clip::Audio(a) = &verdict.compositions[0].clips[0] else {
        panic!("first clip is audio");
    };
    assert!(a.waveform.enabled);
    assert_eq!((a.waveform.from, a.waveform.to), (Some(1.0), Some(3.0)));
    assert!(verdict.waveform_clamps.is_empty());
}

#[test]
fn waveform_underscore_aliases_validate() {
    let dir = tmp("alias");
    write_wav(&dir, "s.wav", 4.0);
    write_comp(
        &dir,
        &format!(
            "{HEAD} {{\n    audio path=\"s.wav\" start=0 duration=4 waveform=true waveform_from=1 waveform_to=3\n}}\n"
        ),
    );
    let verdict = validate_folder(&dir);
    assert_eq!(verdict.status, Status::Valid, "{verdict:?}");
    let Clip::Audio(a) = &verdict.compositions[0].clips[0] else {
        panic!("first clip is audio");
    };
    assert_eq!((a.waveform.from, a.waveform.to), (Some(1.0), Some(3.0)));
}

#[test]
fn bad_waveform_switches_and_bounds_refuse() {
    for (tag, keys) in [
        ("switch", "waveform=\"maybe\""),
        ("bound", "waveform=true waveform-from=\"soon\""),
        ("dangling", "waveform-from=1"),
    ] {
        let dir = tmp(tag);
        write_wav(&dir, "s.wav", 4.0);
        write_comp(
            &dir,
            &format!("{HEAD} {{\n    audio path=\"s.wav\" start=0 duration=4 {keys}\n}}\n"),
        );
        let got = messages(&dir);
        assert_eq!(got.len(), 1, "{tag}: {got:?}");
        assert_eq!(got[0].0, "E_BAD_AUDIO", "{tag}: {got:?}");
    }
}

#[test]
fn empty_and_backward_drawn_ranges_refuse_naming_the_bounds() {
    for (tag, from, to) in [("empty", 2.0, 2.0), ("backward", 3.0, 1.0)] {
        let dir = tmp(tag);
        write_wav(&dir, "s.wav", 4.0);
        write_comp(
            &dir,
            &format!(
                "{HEAD} {{\n    audio path=\"s.wav\" start=0 duration=4 waveform=true waveform-from={from} waveform-to={to}\n}}\n"
            ),
        );
        let got = messages(&dir);
        assert_eq!(got.len(), 1, "{tag}: {got:?}");
        assert_eq!(got[0].0, "E_BAD_AUDIO", "{tag}: {got:?}");
        assert!(got[0].1.contains("empty or backward"), "{tag}: {got:?}");
    }
}

#[test]
fn fully_outside_drawn_ranges_fail_with_nothing_to_draw() {
    let dir = tmp("outside");
    write_wav(&dir, "s.wav", 4.0);
    write_comp(
        &dir,
        &format!(
            "{HEAD} {{\n    audio path=\"s.wav\" start=0 duration=2 waveform=true waveform-from=2.5 waveform-to=3.5\n}}\n"
        ),
    );
    let got = messages(&dir);
    assert_eq!(got.len(), 1, "{got:?}");
    assert_eq!(got[0].0, "E_BAD_AUDIO", "{got:?}");
    assert!(got[0].1.contains("nothing would draw"), "{got:?}");
}

#[test]
fn beyond_window_drawn_ranges_clamp_and_report() {
    let dir = tmp("clamp");
    write_wav(&dir, "s.wav", 4.0);
    write_comp(
        &dir,
        &format!(
            "{HEAD} {{\n    audio path=\"s.wav\" id=\"bed\" start=1 duration=2 waveform=true waveform-from=0 waveform-to=5\n}}\n"
        ),
    );
    let verdict = validate_folder(&dir);
    assert_eq!(verdict.status, Status::Valid, "{verdict:?}");
    assert_eq!(verdict.waveform_clamps.len(), 1);
    let clamp = &verdict.waveform_clamps[0];
    assert_eq!(clamp.clip, 0);
    assert_eq!(clamp.id.as_deref(), Some("bed"));
    assert_eq!(clamp.kind, "audio");
    assert_eq!((clamp.raw_from, clamp.raw_to), (0.0, 5.0));
    assert_eq!((clamp.eff_from, clamp.eff_to), (1.0, 3.0));
}

#[test]
fn hidden_waveform_clips_report_nothing() {
    let dir = tmp("hidden");
    write_wav(&dir, "s.wav", 4.0);
    write_comp(
        &dir,
        &format!(
            "{HEAD} {{\n    audio path=\"s.wav\" start=1 duration=2 hidden=true waveform=true waveform-from=0 waveform-to=5\n}}\n"
        ),
    );
    let verdict = validate_folder(&dir);
    assert_eq!(verdict.status, Status::Valid, "{verdict:?}");
    assert!(verdict.waveform_clamps.is_empty());
}
