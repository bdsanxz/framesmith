//! Sprint 10 sound (T003 + T004): same-track crossfades and voice ducking.
//!
//! Validates through the public [`validate_folder`] API only:
//! `crossfade` (`constant-power` default, `linear`, `none`) and
//! `duck-under` plus `duck-amount` (0..1) land on sound-bearing clips
//! (`audio` tracks and `video` clips); bad shapes and out-of-range
//! amounts fail naming the accepted values; missing partners fail;
//! unknown voices fail naming the reference; voices naming marks,
//! compositions, or other compositions' clips fail; overlong overlap
//! windows clamp to the audible samples and report in
//! `crossfade_clamps` while the verdict stays valid.

use notation::model::Clip;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use validate::validate::{validate_folder, Status};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-validate-n08xfade-{tag}-{}-{}",
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

const OPEN: &str =
    "composition name=\"main\" width=64 height=64 duration=4 background=\"#000\" version=1 {\n";

fn messages(dir: &Path) -> Vec<(String, String, String)> {
    validate_folder(dir)
        .diagnostics
        .iter()
        .map(|d| (d.code.to_string(), d.message.clone(), d.fix.clone()))
        .collect()
}

// --- valid crossfades and ducking ------------------------------------------

#[test]
fn crossfade_and_ducking_validate_on_audio() {
    let dir = tmp("valid");
    write_wav(&dir, "a.wav", 4.0);
    write_wav(&dir, "b.wav", 4.0);
    write_comp(
        &dir,
        &format!(
            "{OPEN}    audio path=\"a.wav\" start=0 duration=2 id=\"bed\" duck-under=\"vox\" duck-amount=0.25\n    audio path=\"b.wav\" start=1 duration=2 id=\"vox\" crossfade=\"linear\"\n}}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    assert!(v.crossfade_clamps.is_empty());
    let comp = v.composition.expect("valid composition");
    match &comp.clips[0] {
        Clip::Audio(a) => {
            assert_eq!(a.crossfade, notation::model::Crossfade::ConstantPower);
            let duck = a.duck_under.as_ref().expect("bed ducks");
            assert_eq!(duck.target, "vox");
            assert_eq!(duck.amount, 0.25);
        }
        c => panic!("expected audio, got {c:?}"),
    }
    match &comp.clips[1] {
        Clip::Audio(a) => {
            assert_eq!(a.crossfade, notation::model::Crossfade::Linear);
            assert!(a.duck_under.is_none());
        }
        c => panic!("expected audio, got {c:?}"),
    }
}

#[test]
fn crossfade_none_and_underscore_aliases_validate() {
    let dir = tmp("aliases");
    write_wav(&dir, "a.wav", 4.0);
    write_wav(&dir, "b.wav", 4.0);
    std::fs::write(dir.join("clip.mp4"), b"fake").unwrap();
    write_comp(
        &dir,
        &format!(
            "{OPEN}    audio path=\"a.wav\" start=0 duration=2 id=\"bed\" duck_under=\"vox\" duck_amount=0.5\n    audio path=\"b.wav\" start=1 duration=2 id=\"vox\" crossfade=\"none\"\n    video path=\"clip.mp4\" start=0 duration=2 crossfade=\"none\"\n}}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    let comp = v.composition.expect("valid composition");
    match &comp.clips[1] {
        Clip::Audio(a) => {
            assert_eq!(a.crossfade, notation::model::Crossfade::None);
        }
        c => panic!("expected audio, got {c:?}"),
    }
    // The video voice shares no lane with the audio bed, so no handoff.
    assert!(v.crossfade_clamps.is_empty());
}

// --- crossfade refusals -----------------------------------------------------

#[test]
fn bad_crossfade_shape_refuses_naming_accepted_values() {
    let dir = tmp("badshape");
    write_wav(&dir, "a.wav", 4.0);
    write_comp(
        &dir,
        &format!("{OPEN}    audio path=\"a.wav\" start=0 duration=2 crossfade=\"loud\"\n}}\n"),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid);
    let ds = messages(&dir);
    assert_eq!(ds.len(), 1, "{ds:?}");
    assert_eq!(ds[0].0, "E_BAD_AUDIO");
    assert!(ds[0].1.contains("`crossfade`"), "{}", ds[0].1);
    assert!(ds[0].1.contains("`constant-power`"), "{}", ds[0].1);
    assert!(ds[0].1.contains("`linear`"), "{}", ds[0].1);
    assert!(ds[0].1.contains("`none`"), "{}", ds[0].1);
}

// --- ducking refusals ---------------------------------------------------------

#[test]
fn duck_amount_problems_refuse_naming_the_bound() {
    for (tag, body, want) in [
        (
            "over",
            "    audio path=\"a.wav\" start=0 duration=4 id=\"bed\" duck-under=\"vox\" duck-amount=1.5\n    audio path=\"a.wav\" start=1 duration=1 id=\"vox\"\n",
            "0 to 1",
        ),
        (
            "under",
            "    audio path=\"a.wav\" start=0 duration=4 id=\"bed\" duck-under=\"vox\" duck-amount=-0.5\n    audio path=\"a.wav\" start=1 duration=1 id=\"vox\"\n",
            "0 to 1",
        ),
        (
            "missing-amount",
            "    audio path=\"a.wav\" start=0 duration=4 id=\"bed\" duck-under=\"vox\"\n    audio path=\"a.wav\" start=1 duration=1 id=\"vox\"\n",
            "without `duck-amount`",
        ),
        (
            "missing-voice",
            "    audio path=\"a.wav\" start=0 duration=4 id=\"bed\" duck-amount=0.25\n",
            "without `duck-under`",
        ),
    ] {
        let dir = tmp(tag);
        write_wav(&dir, "a.wav", 4.0);
        write_comp(&dir, &format!("{OPEN}{body}}}\n"));
        let v = validate_folder(&dir);
        assert_eq!(v.status, Status::Invalid, "{tag}");
        let ds = messages(&dir);
        assert_eq!(ds.len(), 1, "{tag}: {ds:?}");
        assert_eq!(ds[0].0, "E_BAD_AUDIO", "{tag}");
        assert!(ds[0].1.contains("duck"), "{tag}: {}", ds[0].1);
        assert!(ds[0].1.contains(want), "{tag}: {}", ds[0].1);
    }
}

#[test]
fn unknown_voice_refuses_naming_the_reference() {
    let dir = tmp("unknown");
    write_wav(&dir, "a.wav", 4.0);
    write_comp(
        &dir,
        &format!(
            "{OPEN}    audio path=\"a.wav\" start=0 duration=4 id=\"bed\" duck-under=\"ghost\" duck-amount=0.25\n}}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid);
    let ds = messages(&dir);
    assert_eq!(ds.len(), 1, "{ds:?}");
    assert_eq!(ds[0].0, "E_UNKNOWN_REF");
    assert!(ds[0].1.contains("`duck-under`"), "{}", ds[0].1);
    assert!(ds[0].1.contains("\"ghost\""), "{}", ds[0].1);
}

#[test]
fn non_clip_voices_refuse() {
    // A mark answers the reference but carries no window.
    let dir = tmp("mark");
    write_wav(&dir, "a.wav", 4.0);
    write_comp(
        &dir,
        &format!(
            "{OPEN}    audio path=\"a.wav\" start=0 duration=4 id=\"bed\" duck-under=\"cue\" duck-amount=0.25\n    mark name=\"cue\" at=1\n}}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
    let ds = messages(&dir);
    assert_eq!(ds.len(), 1, "{ds:?}");
    assert_eq!(ds[0].0, "E_UNKNOWN_REF");
    assert!(ds[0].1.contains("a mark"), "{}", ds[0].1);
    // A voice in another composition never shares the bed's timeline.
    let dir = tmp("other");
    write_wav(&dir, "a.wav", 4.0);
    write_comp(
        &dir,
        "composition name=\"main\" root=true width=64 height=64 duration=4 background=\"#000\" version=1 {\n    audio path=\"a.wav\" start=0 duration=4 id=\"bed\" duck-under=\"vox\" duck-amount=0.25\n}\ncomposition name=\"side\" width=64 height=64 duration=4 background=\"#000\" version=1 {\n    audio path=\"a.wav\" start=0 duration=4 id=\"vox\"\n}\n",
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
    let ds = messages(&dir);
    assert_eq!(ds.len(), 1, "{ds:?}");
    assert_eq!(ds[0].0, "E_UNKNOWN_REF");
    assert!(ds[0].1.contains("another composition"), "{}", ds[0].1);
}

// --- overlong windows clamp and report ------------------------------------------

#[test]
fn overlong_overlap_clamps_and_reports_while_valid() {
    let dir = tmp("clamp");
    write_wav(&dir, "a.wav", 4.0);
    write_wav(&dir, "b.wav", 4.0);
    // The handoff overlap [1.5,2.5) runs past the 2 s composition end.
    write_comp(
        &dir,
        "composition name=\"main\" width=64 height=64 duration=2 background=\"#000\" version=1 {\n    audio path=\"a.wav\" start=0.5 duration=2\n    audio path=\"b.wav\" start=1.5 duration=1.5\n}\n",
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    assert_eq!(v.crossfade_clamps.len(), 1);
    let c = &v.crossfade_clamps[0];
    assert_eq!((c.early, c.late), (0, 1));
    assert_eq!(c.shape, "constant-power");
    assert_eq!((c.raw_start, c.raw_end), (1.5, 2.5));
    assert_eq!((c.eff_start, c.eff_end), (1.5, 2.0));
}
