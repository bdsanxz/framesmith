//! Sprint 10 sound (T001 + T002): volume/pan envelopes validate.
//!
//! Validates through the public [`validate_folder`] API only: `key`,
//! `tween`, and `set` over `volume` (0..2) and `pan` (-1..1) land on
//! sound-bearing clips (`audio` tracks and `video` clips); out-of-range
//! values fail naming the accepted range; bad keyframe shapes fail
//! naming the stop; envelopes on other clip kinds fail naming the
//! kind; visual-only directive keys (`expr`, `stagger`, `repeat`,
//! `yoyo`, `repeat-delay`) fail on envelopes; and a static `pan` clip
//! key stays unsupported. Fade validation itself is untouched.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use validate::validate::{validate_folder, Status};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-validate-n08env-{tag}-{}-{}",
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
const TRACK: &str = "    audio path=\"s.wav\" start=0 duration=4\n";

fn comp_with(body: &str) -> String {
    format!("{OPEN}{TRACK}{body}}}\n")
}

fn diags(dir: &Path) -> Vec<(String, String, String, usize, usize)> {
    validate_folder(dir)
        .diagnostics
        .iter()
        .map(|d| {
            (
                d.code.to_string(),
                d.message.clone(),
                d.fix.clone(),
                d.line,
                d.column,
            )
        })
        .collect()
}

// --- valid envelopes ---------------------------------------------------------

#[test]
fn volume_and_pan_keyframes_validate_on_audio() {
    let dir = tmp("valid");
    write_wav(&dir, "s.wav", 4.0);
    write_comp(
        &dir,
        &comp_with(
            "    key target=0 property=\"volume\" {\n        frame at=0 value=0\n        frame at=2 value=1.5 ease=\"in-quad\"\n        frame at=4 value=1\n    }\n    tween target=0 property=\"pan\" kind=\"from-to\" start=0 duration=4 from=-1 to=1\n    set target=0 property=\"volume\" start=1 duration=1 value=0.5\n",
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    let comp = v.composition.expect("valid composition");
    assert_eq!(comp.keys.len(), 1);
    assert_eq!(comp.keys[0].property, "volume");
    assert_eq!(comp.keys[0].stops.len(), 3);
    assert_eq!(comp.tweens.len(), 1);
    assert_eq!(comp.tweens[0].property, "pan");
    assert_eq!(
        (comp.tweens[0].from, comp.tweens[0].to),
        (Some(-1.0), Some(1.0))
    );
    assert_eq!(comp.sets.len(), 1);
    assert_eq!(comp.sets[0].property, "volume");
    assert_eq!(comp.sets[0].value, 0.5);
}

#[test]
fn tween_forms_default_missing_endpoints_to_own_values() {
    let dir = tmp("forms");
    write_wav(&dir, "s.wav", 4.0);
    write_comp(
        &dir,
        &comp_with(
            "    tween target=0 property=\"volume\" kind=\"to\" start=0 duration=1 to=0.5\n    tween target=0 property=\"pan\" kind=\"from\" start=1 duration=1 from=0.5\n",
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    let comp = v.composition.expect("valid composition");
    // `to` starts from unity volume; `from` settles back to center pan.
    assert_eq!((comp.tweens[0].from, comp.tweens[0].to), (None, Some(0.5)));
    assert_eq!((comp.tweens[1].from, comp.tweens[1].to), (Some(0.5), None));
}

#[test]
fn volume_envelope_validates_on_video_sound() {
    let dir = tmp("video");
    std::fs::write(dir.join("clip.mp4"), b"not really video").unwrap();
    std::fs::write(
        dir.join("composition.kdl"),
        format!(
            "{OPEN}    video path=\"clip.mp4\" start=0 duration=4\n    key target=0 property=\"volume\" {{\n        frame at=0 value=0\n        frame at=4 value=1\n    }}\n}}\n"
        ),
    )
    .unwrap();
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    assert_eq!(v.composition.expect("valid composition").keys.len(), 1);
}

// --- out-of-range values name the range ---------------------------------------

#[test]
fn out_of_range_envelope_values_name_the_accepted_range() {
    for (tag, body, want) in [
        (
            "vol-stop",
            "    key target=0 property=\"volume\" {\n        frame at=0 value=0\n        frame at=1 value=2.5\n    }\n",
            "volume runs 0 (silence) to 2",
        ),
        (
            "vol-tween",
            "    tween target=0 property=\"volume\" kind=\"to\" start=0 duration=1 to=-0.5\n",
            "volume runs 0 (silence) to 2",
        ),
        (
            "pan-tween",
            "    tween target=0 property=\"pan\" kind=\"to\" start=0 duration=1 to=1.5\n",
            "pan runs -1 (left) to 1 (right)",
        ),
        (
            "pan-set",
            "    set target=0 property=\"pan\" start=0 duration=1 value=-2\n",
            "pan runs -1 (left) to 1 (right)",
        ),
    ] {
        let dir = tmp(tag);
        write_wav(&dir, "s.wav", 4.0);
        write_comp(&dir, &comp_with(body));
        let found = diags(&dir);
        assert!(
            found.iter().any(|(c, m, f, l, _)| {
                c == "E_BAD_MOTION" && m.contains(want) && !f.is_empty() && *l > 0
            }),
            "{tag}: {found:?}"
        );
    }
}

#[test]
fn keyframe_refusals_name_the_stop() {
    // A bad stop on a sound envelope names its stop, like visual keys.
    let dir = tmp("stop");
    write_wav(&dir, "s.wav", 4.0);
    write_comp(
        &dir,
        &comp_with(
            "    key target=0 property=\"volume\" {\n        frame at=0 value=0\n        frame at=1 value=5\n    }\n",
        ),
    );
    let found = diags(&dir);
    assert!(
        found.iter().any(|(c, m, _, _, _)| {
            c == "E_BAD_MOTION" && m.contains("stop 2") && m.contains("volume runs")
        }),
        "{found:?}"
    );
}

// --- bad shapes name the stop --------------------------------------------------

#[test]
fn bad_envelope_shapes_fail() {
    for (tag, body, want) in [
        (
            "single",
            "    key target=0 property=\"pan\" {\n        frame at=0 value=0\n    }\n",
            "at least two stops",
        ),
        (
            "unordered",
            "    key target=0 property=\"volume\" {\n        frame at=1 value=0\n        frame at=0 value=1\n    }\n",
            "time order",
        ),
        (
            "duplicate",
            "    key target=0 property=\"volume\" {\n        frame at=1 value=0\n        frame at=1 value=1\n    }\n",
            "duplicate times",
        ),
        (
            "missing-value",
            "    key target=0 property=\"volume\" {\n        frame at=0 value=0\n        frame at=1\n    }\n",
            "stop 2",
        ),
    ] {
        let dir = tmp(tag);
        write_wav(&dir, "s.wav", 4.0);
        write_comp(&dir, &comp_with(body));
        let found = diags(&dir);
        assert!(
            found
                .iter()
                .any(|(c, m, _, _, _)| c == "E_BAD_MOTION" && m.contains(want)),
            "{tag}: {found:?}"
        );
    }
}

// --- envelopes need sound-bearing targets ---------------------------------------

#[test]
fn envelopes_on_other_clip_kinds_name_the_kind() {
    let dir = tmp("kind");
    write_wav(&dir, "s.wav", 4.0);
    write_comp(
        &dir,
        &comp_with(
            "    text value=\"Hi\" start=0 duration=4\n    key target=1 property=\"volume\" {\n        frame at=0 value=0\n        frame at=1 value=1\n    }\n    tween target=1 property=\"pan\" kind=\"to\" start=0 duration=1 to=0\n",
        ),
    );
    let found = diags(&dir);
    assert_eq!(
        found
            .iter()
            .filter(|(c, _, _, _, _)| c == "E_BAD_MOTION")
            .count(),
        2,
        "{found:?}"
    );
    assert!(
        found
            .iter()
            .all(|(_, m, _, _, _)| { !m.contains("volume") || m.contains("audio or video") }),
        "{found:?}"
    );
    assert!(
        found
            .iter()
            .any(|(_, m, _, _, _)| m.contains("\"pan\"") && m.contains("text clip")),
        "{found:?}"
    );
}

// --- visual-only directive keys refuse on envelopes ------------------------------

#[test]
fn visual_only_keys_refuse_on_sound_envelopes() {
    for (tag, body, key) in [
        (
            "expr",
            "    tween target=0 property=\"volume\" kind=\"to\" start=0 duration=1 expr=\"t\"\n",
            "`expr`",
        ),
        (
            "stagger",
            "    tween target=0 property=\"pan\" kind=\"to\" start=0 duration=1 to=0 stagger=0.5\n",
            "`stagger`",
        ),
        (
            "repeat",
            "    tween target=0 property=\"volume\" kind=\"to\" start=0 duration=1 to=1 repeat=2\n",
            "`repeat`",
        ),
        (
            "yoyo",
            "    key target=0 property=\"pan\" yoyo=true {\n        frame at=0 value=0\n        frame at=1 value=1\n    }\n",
            "`yoyo`",
        ),
    ] {
        let dir = tmp(tag);
        write_wav(&dir, "s.wav", 4.0);
        write_comp(&dir, &comp_with(body));
        let found = diags(&dir);
        assert!(
            found.iter().any(|(c, m, f, l, _)| {
                c == "E_BAD_MOTION" && m.contains(key) && !f.is_empty() && *l > 0
            }),
            "{tag}: {found:?}"
        );
    }
}

// --- static pan stays unsupported --------------------------------------------------

#[test]
fn static_pan_clip_key_stays_unsupported() {
    let dir = tmp("static-pan");
    write_wav(&dir, "s.wav", 4.0);
    std::fs::write(
        dir.join("composition.kdl"),
        format!("{OPEN}    audio path=\"s.wav\" start=0 duration=4 pan=0.5\n}}\n"),
    )
    .unwrap();
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Unsupported, "{:?}", v.diagnostics);
    assert!(
        v.diagnostics
            .iter()
            .any(|d| d.code == "E_UNSUPPORTED" && d.message.contains("`pan`")),
        "{:?}",
        v.diagnostics
    );
}
