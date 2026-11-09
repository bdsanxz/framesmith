//! Audio-rack regressions (Sprint 18, T007 / CAP-007): presets
//! audition on the real mix without writing and apply as one patch
//! turn, lane range edits rewrite points exactly, groups move
//! together, the meter reads the true master, and unrunnable chains
//! plus empty ranges refuse by name.

use preview::studio_audio::{
    apply_preset, audition_preset, copy_lane, define_audio_group, disband_audio_group,
    load_audio_groups, paste_lane, read_master_meter, rewrite_lane, E_RACK_EMPTY_CLIPBOARD,
    E_RACK_EMPTY_RANGE, E_RACK_STAGE_FAILED, E_RACK_UNKNOWN_PRESET,
};
use preview::LaneClipboard;
use sdk::{ClipRef, LaneEdit, OpenOptions, SdkSession};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-preview-rack-{tag}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn ffmpeg() -> PathBuf {
    let path = std::env::var_os("PATH").expect("PATH is set");
    for dir in std::env::split_paths(&path) {
        let cand = dir.join("ffmpeg");
        if cand.is_file() {
            return cand;
        }
    }
    panic!("tests need ffmpeg on PATH");
}

/// Minimal PCM WAV (48 kHz stereo s16) of `secs` seconds, written by
/// hand so audio tests need no encoder.
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

fn sounding_project(folder: &Path) {
    write_wav(folder, "s.wav", 2.0);
    std::fs::write(
        folder.join("composition.kdl"),
        "composition name=\"t\" width=320 height=180 duration=2 fps=10 background=\"#112233\" version=1 {\n    audio id=\"music\" path=\"s.wav\" start=0 duration=2\n    audio id=\"voice\" path=\"s.wav\" start=0 duration=2\n    key target=0 property=\"volume\" {\n        frame at=0 value=1\n        frame at=1 value=0.8\n        frame at=2 value=0.6\n    }\n    key target=1 property=\"volume\" {\n        frame at=0 value=0.5\n        frame at=2 value=0.5\n    }\n}\n",
    )
    .unwrap();
}

fn silent_project(folder: &Path) {
    std::fs::write(
        folder.join("composition.kdl"),
        "composition name=\"t\" width=320 height=180 duration=1 fps=10 background=\"#112233\" version=1 {\n    text value=\"Hi\" start=0 duration=1 x=10 y=40 size=24 color=\"#ffffff\"\n}\n",
    )
    .unwrap();
}

fn tree_bytes(folder: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut stack = vec![folder.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let mut entries: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().collect();
        entries.sort_by_key(|e| e.path());
        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                out.insert(
                    path.strip_prefix(folder)
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                    std::fs::read(&path).unwrap(),
                );
            }
        }
    }
    out
}

fn open(folder: &Path) -> SdkSession {
    SdkSession::open(folder, OpenOptions::default()).expect("valid open")
}

#[test]
fn meter_reads_the_true_master() {
    let dir = tmp("meter");
    sounding_project(&dir);
    let before = tree_bytes(&dir);
    let meter = read_master_meter(&dir, None, &ffmpeg()).expect("meter reads");
    assert!(!meter.silent);
    assert!(meter.sample_frames > 0);
    assert!(meter.peak > 0);
    assert!(meter.dbfs.is_some());
    assert!(
        !meter.level_text().contains("-inf"),
        "{}",
        meter.level_text()
    );

    // The true master: the same bytes a full render mixes, measured
    // with the same sample math.
    let session = open(&dir);
    let pcm = render::render::build_mix_preview(
        session.effective_composition(),
        session.all_compositions(),
        &dir,
        &ffmpeg(),
    )
    .expect("mix builds")
    .expect("mix audible");
    assert_eq!(meter.peak, render::peaks::peak_of(&pcm));
    assert_eq!(meter.sample_frames, pcm.len() / 2);
    // Metering writes nothing to the project.
    assert_eq!(tree_bytes(&dir), before);
}

#[test]
fn silent_project_meter_reads_exact_silence() {
    let dir = tmp("silent");
    silent_project(&dir);
    let meter = read_master_meter(&dir, None, &ffmpeg()).expect("silent meter reads");
    assert!(meter.silent);
    assert_eq!(meter.peak, 0);
    assert_eq!(meter.dbfs, None);
    assert_eq!(meter.linear, 0.0);
    assert!(!meter.hot);
    assert_eq!(meter.level_text(), "-inf dBFS");
}

#[test]
fn audition_previews_on_the_real_mix_without_writing() {
    let dir = tmp("audition");
    sounding_project(&dir);
    let before = tree_bytes(&dir);
    let report = audition_preset(&dir, None, "level-up", &ClipRef::by_id("music"), &ffmpeg())
        .expect("audition runs");
    assert_eq!(report.preset, "level-up");
    assert_eq!(report.stages.len(), 1);
    assert!(report.changed, "a +3 dB lift changes mixed samples");
    assert!(
        report.after.peak > report.before.peak,
        "before={} after={}",
        report.before.peak,
        report.after.peak
    );
    // Audition wrote nothing: composition, delta, and history
    // byte-identical.
    assert_eq!(tree_bytes(&dir), before);

    // The auditioned mix equals the applied mix: audition runs the
    // real chain, not a sketch.
    apply_preset(
        &dir,
        None,
        "level-up",
        &ClipRef::by_id("music"),
        "rack-test",
    )
    .expect("apply lands");
    let applied_meter = read_master_meter(&dir, None, &ffmpeg()).expect("meter reads");
    assert_eq!(applied_meter.peak, report.after.peak);
    assert_eq!(applied_meter.sample_frames, report.after.sample_frames);
}

#[test]
fn apply_preset_lands_as_one_patch_turn() {
    let dir = tmp("apply");
    sounding_project(&dir);
    let receipt = apply_preset(
        &dir,
        None,
        "fade-ends",
        &ClipRef::by_id("music"),
        "rack-test",
    )
    .expect("apply lands");
    assert_eq!(receipt.preset, "fade-ends");
    assert_eq!(receipt.applied, 2);
    assert_eq!(receipt.summaries.len(), 2);
    assert_eq!(receipt.actor, "rack-test");
    // One named history turn records the whole preset.
    let turns = sdk::history::list_turns(&dir).expect("history lists");
    assert_eq!(turns.turns.len(), 1);
    assert_eq!(turns.turns[0].actor, "rack-test");
    assert!(
        turns.turns[0].summary.contains("fade-in"),
        "{:?}",
        turns.turns[0].summary
    );
    assert!(
        turns.turns[0].summary.contains("fade-out"),
        "{:?}",
        turns.turns[0].summary
    );

    // The rows changed on the target clip only.
    let session = open(&dir);
    let music = session
        .effective_composition()
        .clips
        .iter()
        .find(|c| c.id() == Some("music"))
        .expect("music clip");
    match music {
        notation::model::Clip::Audio(c) => {
            assert_eq!(c.fade_in, 0.5);
            assert_eq!(c.fade_out, 0.5);
        }
        other => panic!("music stays audio: {other:?}"),
    }
    let voice = session
        .effective_composition()
        .clips
        .iter()
        .find(|c| c.id() == Some("voice"))
        .expect("voice clip");
    match voice {
        notation::model::Clip::Audio(c) => {
            assert_eq!(c.fade_in, 0.0);
            assert_eq!(c.fade_out, 0.0);
        }
        other => panic!("voice stays dry: {other:?}"),
    }

    // Walking undo back reverses the whole preset: both fades read
    // 0 again in the effective composition.
    let mut session = open(&dir);
    session.undo().expect("undo reverses fade-out");
    session.undo().expect("undo reverses fade-in");
    let music = session
        .effective_composition()
        .clips
        .iter()
        .find(|c| c.id() == Some("music"))
        .expect("music clip")
        .clone();
    match music {
        notation::model::Clip::Audio(c) => {
            assert_eq!(c.fade_in, 0.0);
            assert_eq!(c.fade_out, 0.0);
        }
        other => panic!("music stays audio: {other:?}"),
    }
}

#[test]
fn unrunnable_chains_and_unknown_presets_refuse_by_name() {
    let dir = tmp("refuse");
    sounding_project(&dir);
    let before = tree_bytes(&dir);

    // No preset by that name: the refusal lists the catalog and
    // writes nothing.
    let err = audition_preset(
        &dir,
        None,
        "wall-of-sound",
        &ClipRef::by_id("music"),
        &ffmpeg(),
    )
    .expect_err("unknown preset refuses");
    assert_eq!(err.code, E_RACK_UNKNOWN_PRESET);
    assert!(err.message.contains("level-up"), "{err:?}");
    assert_eq!(tree_bytes(&dir), before);

    // The fade-ends chain cannot run on a 0.4 s clip: the fade-in
    // stage refuses naming itself with the SDK cause.
    write_wav(&dir, "short.wav", 0.4);
    std::fs::write(
        dir.join("composition.kdl"),
        "composition name=\"t\" width=320 height=180 duration=0.4 fps=10 background=\"#112233\" version=1 {\n    audio id=\"blip\" path=\"short.wav\" start=0 duration=0.4\n}\n",
    )
    .unwrap();
    let before_short = tree_bytes(&dir);
    let err = apply_preset(
        &dir,
        None,
        "fade-ends",
        &ClipRef::by_id("blip"),
        "rack-test",
    )
    .expect_err("long fade on a short clip refuses");
    assert_eq!(err.code, E_RACK_STAGE_FAILED);
    assert!(err.message.contains("fade-ends"), "{err:?}");
    assert!(err.message.contains("fade-in"), "{err:?}");
    assert!(err.message.contains("longer than its clip"), "{err:?}");

    // Nothing persisted, nothing recorded: tree byte-identical and
    // no turn.
    assert_eq!(tree_bytes(&dir), before_short);
    let turns = sdk::history::list_turns(&dir).expect("history lists");
    assert!(turns.turns.is_empty());

    // A preset on a text clip fails its `level` stage naming it.
    let quiet = tmp("refuse-text");
    silent_project(&quiet);
    let err = apply_preset(&quiet, None, "level-up", &ClipRef::by_index(0), "rack-test")
        .expect_err("volume stage on text refuses");
    assert_eq!(err.code, E_RACK_STAGE_FAILED);
    assert!(err.message.contains("level-up"), "{err:?}");
    assert!(err.message.contains("`level`"), "{err:?}");
}

#[test]
fn grouped_lanes_move_together_in_one_turn() {
    let dir = tmp("groups");
    sounding_project(&dir);
    define_audio_group(
        &dir,
        None,
        "choir",
        &["music".to_string(), "voice".to_string()],
    )
    .expect("group defines");
    let load = load_audio_groups(&dir);
    assert!(load.restored);
    assert_eq!(
        load.groups.groups["t"]["choir"],
        vec!["music".to_string(), "voice".to_string()]
    );

    // A rewrite addressing one member fans out to both as one turn.
    let receipt = rewrite_lane(
        &dir,
        None,
        &ClipRef::by_id("music"),
        "volume",
        0.0,
        2.0,
        &LaneEdit::Ramp {
            from: Some("0".to_string()),
            to: Some("0".to_string()),
        },
        "rack-test",
    )
    .expect("group rewrite lands");
    assert_eq!(receipt.applied, 2, "{receipt:?}");

    // Both lanes carry the fresh ramp.
    let session = open(&dir);
    for (idx, id) in [(0usize, "music"), (1usize, "voice")] {
        let lanes: Vec<Vec<(f64, f64)>> = session
            .effective_composition()
            .keys
            .iter()
            .filter(|k| k.target == idx && k.property == "volume")
            .map(|k| k.stops.iter().map(|s| (s.at, s.value)).collect())
            .collect();
        let fresh = lanes.last().cloned().unwrap();
        assert_eq!(fresh, vec![(0.0, 0.0), (2.0, 0.0)], "{id}: {lanes:?}");
    }

    // Defining a group over a silent clip refuses naming the member.
    let err = define_audio_group(
        &dir,
        None,
        "bad",
        &["music".to_string(), "ghost".to_string()],
    )
    .expect_err("ghost member refuses");
    assert!(err.message.contains("\"ghost\""), "{err:?}");
    // Disbanding a missing group refuses naming it.
    let err = disband_audio_group(&dir, None, "ghost-group").expect_err("missing group refuses");
    assert!(err.message.contains("\"ghost-group\""), "{err:?}");
}

#[test]
fn lane_copy_paste_round_trips_through_the_rack() {
    let dir = tmp("clipboard");
    sounding_project(&dir);
    let music = ClipRef::by_id("music");
    let clipboard = copy_lane(&dir, None, &music, "volume", 0.0, 2.0).expect("copy reads");
    assert_eq!(clipboard.property, "volume");
    assert_eq!(
        clipboard.stops.iter().map(|s| s.at).collect::<Vec<_>>(),
        vec![0.0, 1.0, 2.0]
    );

    // Paste onto the voice lane: the points land exactly.
    let receipt = paste_lane(
        &dir,
        None,
        &clipboard,
        &ClipRef::by_id("voice"),
        "rack-test",
    )
    .expect("paste lands");
    assert_eq!(receipt.applied, 1);
    let session = open(&dir);
    let lanes: Vec<(f64, f64)> = session
        .effective_composition()
        .keys
        .iter()
        .filter(|k| k.target == 1 && k.property == "volume")
        .flat_map(|k| k.stops.iter().map(|s| (s.at, s.value)))
        .collect();
    assert_eq!(lanes, vec![(0.0, 1.0), (1.0, 0.8), (2.0, 0.6)]);

    // Copying an empty range refuses naming the range.
    let err = copy_lane(&dir, None, &music, "pan", 0.0, 2.0).expect_err("empty copy refuses");
    assert_eq!(err.code, E_RACK_EMPTY_RANGE);
    assert!(err.message.contains("0..2"), "{err:?}");

    // Pasting an empty clipboard refuses instead of pasting silence.
    let err = paste_lane(&dir, None, &LaneClipboard::default(), &music, "rack-test")
        .expect_err("empty paste refuses");
    assert_eq!(err.code, E_RACK_EMPTY_CLIPBOARD);
}

#[test]
fn undecodable_mix_refuses_naming_the_file() {
    let dir = tmp("badmix");
    std::fs::write(dir.join("bad.wav"), b"this is not audio, only text garbage").unwrap();
    std::fs::write(
        dir.join("composition.kdl"),
        "composition name=\"t\" width=320 height=180 duration=1 fps=10 background=\"#112233\" version=1 {\n    audio id=\"music\" path=\"bad.wav\" start=0 duration=1\n}\n",
    )
    .unwrap();
    let err = read_master_meter(&dir, None, &ffmpeg()).expect_err("bad mix refuses");
    assert!(err.message.contains("bad.wav"), "{err:?}");
    let err = audition_preset(&dir, None, "level-up", &ClipRef::by_id("music"), &ffmpeg())
        .expect_err("bad audition refuses");
    assert!(err.message.contains("bad.wav"), "{err:?}");
}
