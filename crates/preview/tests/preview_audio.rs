//! Audio-as-clock regressions (Sprint 14, T004 / CAP-004):
//! every acceptance criterion of `T004-play-audio-as-the-clock.md`,
//! including refusal variants.

use preview::{
    AudioPlay, Fps, GainErrorKind, MixErrorKind, MixInfo, PlayAudioErrorKind, PlaybackGain,
    PreviewOptions, PreviewSession, MIX_CHANNELS, MIX_RATE,
};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-preview-audio-{tag}-{}-{}",
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

fn silent_project(folder: &Path) {
    std::fs::write(
        folder.join("composition.kdl"),
        "composition name=\"t\" width=320 height=180 duration=1 fps=10 background=\"#112233\" version=1 {\n    text value=\"Hi\" start=0 duration=1 x=10 y=40 size=24 color=\"#ffffff\"\n}\n",
    )
    .unwrap();
}

fn sounding_project(folder: &Path) {
    write_wav(folder, "s.wav", 1.0);
    std::fs::write(
        folder.join("composition.kdl"),
        "composition name=\"t\" width=320 height=180 duration=1 fps=10 background=\"#112233\" version=1 {\n    text value=\"Hi\" start=0 duration=1 x=10 y=40 size=24 color=\"#ffffff\"\n    audio path=\"s.wav\" start=0 duration=1\n}\n",
    )
    .unwrap();
}

fn undecodable_project(folder: &Path) {
    std::fs::write(
        folder.join("bad.wav"),
        b"this is not audio, only text garbage",
    )
    .unwrap();
    std::fs::write(
        folder.join("composition.kdl"),
        "composition name=\"t\" width=320 height=180 duration=1 fps=10 background=\"#112233\" version=1 {\n    text value=\"Hi\" start=0 duration=1 x=10 y=40 size=24 color=\"#ffffff\"\n    audio path=\"bad.wav\" start=0 duration=1\n}\n",
    )
    .unwrap();
}

/// Sorted map of every file under `folder` to its bytes: the
/// never-touches-the-project oracle.
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
                let key = path
                    .strip_prefix(folder)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned();
                out.insert(key, std::fs::read(&path).unwrap());
            }
        }
    }
    out
}

fn to_bytes(pcm: &[i16]) -> Vec<u8> {
    let mut out = Vec::with_capacity(pcm.len() * 2);
    for s in pcm {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

#[test]
fn play_sounds_the_same_mix_the_render_mixes() {
    let root = tmp("same-mix");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    sounding_project(&folder);
    let mut session =
        PreviewSession::open(&folder, PreviewOptions::default()).expect("valid project opens");

    let pcm = session
        .mix_pcm()
        .expect("mix serves")
        .expect("project sounds")
        .to_vec();
    assert_eq!(MIX_RATE, 48000);
    assert_eq!(MIX_CHANNELS, 2);
    assert_eq!(pcm.len(), 48000 * 2, "one second of stereo mix");
    assert!(pcm.iter().any(|&s| s != 0), "the sine mix is audible");

    // Independent oracle 1: the same mix computation the render path
    // calls, run straight without any session.
    let verdict = validate::validate::validate_folder_with_variables_strict_themed(
        &folder,
        &validate::validate::VariableOverrides::new(),
        false,
        None,
    );
    assert!(matches!(verdict.status, validate::validate::Status::Valid));
    let comp = verdict.composition.clone().unwrap();
    let expected =
        render::render::build_mix_preview(&comp, &verdict.compositions, &folder, &ffmpeg())
            .expect("mix builds")
            .expect("project sounds");
    assert_eq!(
        pcm, expected,
        "served mix must equal the render mix exactly"
    );

    // Independent oracle 2: the staged render mix — the bytes the MP4
    // encodes — through the audio-stage path with a real preparation.
    let prep = render::render::prepare_frames(&comp, &verdict.compositions, &folder, &ffmpeg())
        .expect("prepares");
    let stage = root.join("stage.pcm");
    render::render::build_audio_plan(&comp, &folder, &ffmpeg(), &prep, &stage).expect("stages");
    let staged = std::fs::read(&stage).unwrap();
    assert_eq!(
        to_bytes(&pcm),
        staged,
        "served mix must equal the staged render mix byte-identically"
    );

    // Mix facts behind the page clock.
    let info = session
        .mix_info()
        .expect("info serves")
        .expect("project sounds");
    assert_eq!(info.sample_rate, 48000);
    assert_eq!(info.channels, 2);
    assert_eq!(info.sample_frames, 48000);
    assert!((info.duration_secs - 1.0).abs() < 1e-12);
    assert!((info.mix_secs() - 1.0).abs() < 1e-9);

    // The mix is cached: a repeat serves identical bytes.
    let again = session
        .mix_pcm()
        .expect("mix repeats")
        .expect("project sounds")
        .to_vec();
    assert_eq!(pcm, again);
}

#[test]
fn playback_position_follows_the_mix_clock() {
    // Pure mapping at 10fps over 10 frames.
    let fps = Fps::from_int(10);
    assert_eq!(MixInfo::frame_for_mix_time(fps, 10, 0.0), 0);
    assert_eq!(MixInfo::frame_for_mix_time(fps, 10, 0.35), 3);
    assert_eq!(MixInfo::frame_for_mix_time(fps, 10, 0.9), 9);
    assert_eq!(
        MixInfo::frame_for_mix_time(fps, 10, 5.0),
        9,
        "parks on the last frame"
    );
    assert_eq!(MixInfo::frame_for_mix_time(fps, 10, -1.0), 0);
    assert_eq!(MixInfo::frame_for_mix_time(fps, 10, f64::NAN), 0);
    assert_eq!(
        MixInfo::frame_for_mix_time(fps, 0, 0.5),
        0,
        "empty timeline points nowhere"
    );
    assert!((MixInfo::mix_time_for_frame(fps, 3) - 0.3).abs() < 1e-9);

    // Session mapping matches the session timeline.
    let root = tmp("clock");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    sounding_project(&folder);
    let session =
        PreviewSession::open(&folder, PreviewOptions::default()).expect("valid project opens");
    assert_eq!(session.frame_for_mix_time(0.35), 3);
    assert_eq!(session.frame_for_mix_time(5.0), 9);

    // Transport follows the mix: each audio timeupdate points the
    // playhead, keeps the playing state, and clamps past the end.
    let mut t = session.transport();
    t.play().expect("play acts");
    t.sync_to_mix_time(0.35);
    assert_eq!(t.position(), 3);
    assert!(
        t.is_playing(),
        "sync never pauses the audio-driven playhead"
    );
    t.sync_to_mix_time(5.0);
    assert_eq!(t.position(), 9);
    assert!(t.is_playing(), "clamping at the end never stops the mix");
    t.sync_to_mix_time(0.0);
    assert_eq!(t.position(), 0, "rewinding the mix rewinds the playhead");
    t.pause();
    t.sync_to_mix_time(0.5);
    assert_eq!(t.position(), 5);
    assert!(!t.is_playing(), "sync never starts a paused playhead");

    // Empty timelines stay safe to call unconditionally.
    let mut empty = preview::Transport::new(0, Fps::from_int(10), 0.0);
    empty.sync_to_mix_time(0.5);
    assert_eq!(empty.position(), 0);
}

#[test]
fn mute_and_volume_ride_the_page_without_touching_the_project() {
    let root = tmp("gain");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    sounding_project(&folder);
    let before = tree_bytes(&folder);
    let mut session =
        PreviewSession::open(&folder, PreviewOptions::default()).expect("valid project opens");
    let pcm = session
        .mix_pcm()
        .expect("mix serves")
        .expect("project sounds")
        .to_vec();
    assert!(
        pcm.iter().any(|&s| s.abs() > 1000),
        "needs loud samples to scale"
    );

    // Full volume sounds the served mix untouched.
    let mut gain = PlaybackGain::new();
    assert!(!gain.muted());
    assert_eq!(gain.volume(), 1.0);
    assert_eq!(gain.effective_gain(), 1.0);
    assert_eq!(gain.apply(&pcm), pcm);

    // Muting maps every sample to digital silence.
    gain.set_muted(true);
    assert!(gain.muted());
    assert_eq!(gain.effective_gain(), 0.0);
    assert_eq!(gain.apply(&pcm), vec![0i16; pcm.len()]);
    gain.set_muted(false);

    // Half volume scales amplitude linearly.
    gain.set_volume(0.5).expect("0.5 acts");
    assert_eq!(gain.volume(), 0.5);
    let half = gain.apply(&pcm);
    assert_eq!(half.len(), pcm.len());
    for (&full, &scaled) in pcm.iter().zip(half.iter()) {
        let want = ((full as f32 * 0.5).round() as i16).abs();
        assert!(
            (scaled.abs() - want).abs() <= 1,
            "half volume halves amplitude"
        );
    }
    assert!(half.iter().any(|&s| s != 0), "half volume still sounds");

    // Zero volume is silent but unmuted.
    gain.set_volume(0.0).expect("0 acts");
    assert_eq!(gain.apply(&pcm), vec![0i16; pcm.len()]);

    assert_eq!(
        tree_bytes(&folder),
        before,
        "gain never touches the project"
    );
}

#[test]
fn bad_volume_refuses_keeping_the_level() {
    let mut gain = PlaybackGain::new();
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.5, 1.5, 2.0] {
        let err = gain.set_volume(bad).expect_err("bad level refuses");
        assert_eq!(err.kind, GainErrorKind::BadVolume);
        assert!(err.message.contains("0"), "{err}");
        assert!(err.message.contains("1"), "{err}");
        assert_eq!(gain.volume(), 1.0, "the level stays where it was");
    }
    gain.set_volume(0.25).expect("0.25 acts");
    assert_eq!(gain.volume(), 0.25);
}

#[test]
fn soundless_project_plays_video_silently_with_no_error() {
    let root = tmp("silent");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    silent_project(&folder);
    let mut session =
        PreviewSession::open(&folder, PreviewOptions::default()).expect("valid project opens");

    assert!(session
        .mix_pcm()
        .expect("silence is not an error")
        .is_none());
    assert!(session
        .mix_info()
        .expect("silence is not an error")
        .is_none());
    assert!(session
        .mix_wav()
        .expect("silence is not an error")
        .is_none());

    let mut t = session.transport();
    let play = session.play_with_audio(&mut t).expect("silent play acts");
    assert_eq!(play, AudioPlay::Silent);
    assert!(!play.has_sound());
    assert!(play.info().is_none());
    assert!(t.is_playing(), "video still plays, silently");
}

#[test]
fn undecodable_mix_blocks_play_naming_the_cause() {
    let root = tmp("undecodable");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    undecodable_project(&folder);
    // Validation only checks presence and format: the corrupt file
    // opens a session, then the mix itself refuses.
    let mut session =
        PreviewSession::open(&folder, PreviewOptions::default()).expect("project opens");

    let err = session.mix_pcm().expect_err("corrupt mix refuses");
    assert_eq!(err.kind, MixErrorKind::Undecodable);
    assert!(err.message.contains("bad.wav"), "{err}");

    let mut t = session.transport();
    let blocked = session
        .play_with_audio(&mut t)
        .expect_err("play is blocked");
    assert_eq!(blocked.kind, PlayAudioErrorKind::MixBlocked);
    assert!(blocked.message.contains("bad.wav"), "{blocked}");
    assert!(!t.is_playing(), "blocked play never starts");
    assert_eq!(t.position(), 0, "blocked play never moves the playhead");

    // The refusal is stable: repeats name the same cause.
    let again = session.mix_pcm().expect_err("still corrupt");
    assert_eq!(again, err);
}

#[test]
fn loud_normalization_without_sound_previews_silently_like_check() {
    let root = tmp("loud-silent");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(
        folder.join("composition.kdl"),
        "composition name=\"t\" width=320 height=180 duration=1 fps=10 background=\"#112233\" version=1 normalize=\"loud\" {\n    text value=\"Hi\" start=0 duration=1 x=10 y=40 size=24 color=\"#ffffff\"\n}\n",
    )
    .unwrap();
    let mut session =
        PreviewSession::open(&folder, PreviewOptions::default()).expect("project opens");

    // The mix preview reports no audible mix — the same `None` `check`
    // treats as valid — so review-time playback stays silent with no
    // error. The loudness refusal fires later, at render time, where
    // silence cannot scale to target.
    assert!(session
        .mix_pcm()
        .expect("silence is not an error")
        .is_none());

    let mut t = session.transport();
    let play = session.play_with_audio(&mut t).expect("silent play acts");
    assert_eq!(play, AudioPlay::Silent);
    assert!(t.is_playing(), "video still plays, silently");
}

#[test]
fn sound_play_on_an_empty_timeline_refuses_naming_the_reason() {
    let root = tmp("empty-play");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    sounding_project(&folder);
    let mut session =
        PreviewSession::open(&folder, PreviewOptions::default()).expect("valid project opens");

    // The mix is fine, but there is no frame to point at: the
    // transport refusal names its reason and nothing moves.
    let mut empty = preview::Transport::new(0, Fps::from_int(10), 0.0);
    let refused = session
        .play_with_audio(&mut empty)
        .expect_err("empty play refuses");
    assert_eq!(refused.kind, PlayAudioErrorKind::TransportRefused);
    assert!(refused.message.contains("empty"), "{refused}");
}

#[test]
fn silent_loud_mix_blocks_play_naming_normalize() {
    let root = tmp("silent-loud");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    // Audible-declared but measuring silent: the file decodes, the
    // `-inf` volume zeroes every sample, and loudness has nothing to
    // measure — the same refusal the render reports.
    write_wav(&folder, "s.wav", 1.0);
    std::fs::write(
        folder.join("composition.kdl"),
        "composition name=\"t\" width=320 height=180 duration=1 fps=10 background=\"#112233\" version=1 normalize=\"loud\" {\n    text value=\"Hi\" start=0 duration=1 x=10 y=40 size=24 color=\"#ffffff\"\n    audio path=\"s.wav\" start=0 duration=1 volume=\"-inf\"\n}\n",
    )
    .unwrap();
    let mut session =
        PreviewSession::open(&folder, PreviewOptions::default()).expect("project opens");

    let err = session.mix_pcm().expect_err("silent loud mix refuses");
    assert_eq!(err.kind, MixErrorKind::Refused);
    assert!(err.message.contains("normalize"), "{err}");

    let mut t = session.transport();
    let blocked = session
        .play_with_audio(&mut t)
        .expect_err("play is blocked");
    assert_eq!(blocked.kind, PlayAudioErrorKind::MixBlocked);
    assert!(blocked.message.contains("normalize"), "{blocked}");
    assert!(!t.is_playing(), "blocked play never starts");
}

#[test]
fn fixed_mix_heals_after_reload_invalidation() {
    let root = tmp("heals");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    undecodable_project(&folder);
    let mut session =
        PreviewSession::open(&folder, PreviewOptions::default()).expect("project opens");
    assert!(session.mix_pcm().is_err(), "corrupt mix refuses");

    // The maker replaces the corrupt source; reload drops the cached
    // refusal and the same computation serves the fixed mix.
    write_wav(&folder, "bad.wav", 1.0);
    session.invalidate();
    let pcm = session
        .mix_pcm()
        .expect("fixed mix serves")
        .expect("project sounds")
        .to_vec();
    assert_eq!(pcm.len(), 48000 * 2);
    assert!(pcm.iter().any(|&s| s != 0));

    let mut t = session.transport();
    let play = session.play_with_audio(&mut t).expect("fixed play acts");
    assert!(play.has_sound());
    assert!(t.is_playing());
}

#[test]
fn mix_wav_encodes_a_playable_header() {
    let root = tmp("wav");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    sounding_project(&folder);
    let mut session =
        PreviewSession::open(&folder, PreviewOptions::default()).expect("valid project opens");
    let pcm = session
        .mix_pcm()
        .expect("mix serves")
        .expect("project sounds")
        .to_vec();
    let wav = session
        .mix_wav()
        .expect("wav serves")
        .expect("project sounds");

    assert_eq!(&wav[0..4], b"RIFF");
    assert_eq!(&wav[8..16], b"WAVEfmt ");
    assert_eq!(u16::from_le_bytes([wav[20], wav[21]]), 1, "PCM");
    assert_eq!(u16::from_le_bytes([wav[22], wav[23]]), 2, "stereo");
    assert_eq!(
        u32::from_le_bytes([wav[24], wav[25], wav[26], wav[27]]),
        48000
    );
    assert_eq!(&wav[36..40], b"data");
    let data_len = u32::from_le_bytes([wav[40], wav[41], wav[42], wav[43]]) as usize;
    assert_eq!(data_len, pcm.len() * 2);
    assert_eq!(wav.len(), 44 + data_len);
    assert_eq!(
        &wav[44..],
        &to_bytes(&pcm)[..],
        "samples ride little-endian after the header"
    );
}
