//! Sprint 3 T011 end-to-end fixture proof for render and audio.
//!
//! The finished pipeline must pass every render and audio fixture:
//! `ffprobe` shows H.264 yuv420p plus AAC stereo 48 kHz, backend and
//! worker variants agree per CAP-003, marked-audio fixtures verify
//! sample positions within 1 ms, and any fixture outside its bounds
//! is a named defect (fixture, track or frame, measured distance)
//! with no false pass. Refusals write no MP4.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-proof-{tag}-{}-{}",
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

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn run(args: &[&str], cwd: &Path) -> (i32, String) {
    run_env(args, cwd, &[])
}

fn run_env(args: &[&str], cwd: &Path, env: &[(&str, &str)]) -> (i32, String) {
    let mut cmd = Command::new(bin());
    cmd.args(args).current_dir(cwd);
    for (k, v) in env {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("run framesmith binary");
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    (out.status.code().unwrap_or(-1), text)
}

fn stage(src_name: &str, kind: &str) -> (PathBuf, PathBuf) {
    let root = tmp(kind);
    let src = manifest_dir().join(src_name);
    let dst = root.join("proj");
    copy_clean(&src, &dst);
    (root, dst)
}

fn copy_clean(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).unwrap();
    for e in std::fs::read_dir(src).unwrap() {
        let e = e.unwrap();
        let name = e.file_name().into_string().unwrap();
        if name == "logs" || name == "out.mp4" {
            continue;
        }
        let target = dst.join(&name);
        if e.file_type().unwrap().is_dir() {
            copy_clean(&e.path(), &target);
        } else {
            std::fs::copy(e.path(), &target).unwrap();
        }
    }
}

fn ffmpeg(args: &[&str]) {
    let out = Command::new("ffmpeg")
        .args(["-y", "-v", "error"])
        .args(args)
        .output()
        .expect("run host ffmpeg");
    assert!(out.status.success(), "host ffmpeg failed: {args:?}");
}

fn ffprobe_csv(mp4: &Path) -> String {
    let out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "stream=codec_name,pix_fmt,sample_rate,channels",
            "-of",
            "csv=p=0",
        ])
        .arg(mp4)
        .output()
        .expect("run host ffprobe");
    assert!(out.status.success());
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Decode the first `frames` video frames to raw rgb24 bytes.
fn decode_raw(mp4: &Path, frames: usize) -> Vec<u8> {
    let out = Command::new("ffmpeg")
        .args([
            "-y",
            "-v",
            "error",
            "-i",
            &mp4.display().to_string(),
            "-frames:v",
            &frames.to_string(),
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgb24",
            "-",
        ])
        .output()
        .expect("decode mp4");
    assert!(out.status.success());
    out.stdout
}

/// Decode an MP4 audio track to raw s16le 48 kHz stereo samples.
fn decode_audio(mp4: &Path) -> Vec<i16> {
    let out = Command::new("ffmpeg")
        .args([
            "-y",
            "-v",
            "error",
            "-i",
            &mp4.display().to_string(),
            "-map",
            "0:a",
            "-f",
            "s16le",
            "-acodec",
            "pcm_s16le",
            "-ar",
            "48000",
            "-ac",
            "2",
            "-",
        ])
        .output()
        .expect("decode mp4 audio");
    assert!(out.status.success());
    let (pairs, _) = out.stdout.as_chunks::<2>();
    pairs
        .iter()
        .map(|b| i16::from_le_bytes(*b))
        .collect()
}

fn onset_frame(pcm: &[i16], threshold: i16) -> Option<usize> {
    (0..pcm.len() / 2).find(|&i| pcm[2 * i].abs() > threshold || pcm[2 * i + 1].abs() > threshold)
}

/// The T011 verdict for one marked-audio fixture: `Ok(())` records a
/// pass; `Err` names the fixture, the track, and the measured
/// distance, and records no pass.
fn verify_onset(
    fixture: &str,
    track: &str,
    onset: usize,
    expected: usize,
    bound: usize,
) -> Result<(), String> {
    let dist = onset.abs_diff(expected);
    if dist <= bound {
        Ok(())
    } else {
        Err(format!(
            "FAIL fixture={fixture} track={track} frame={onset} expected={expected} \
             distance={dist} samples ({:.1} ms) bound={bound} samples; no pass recorded",
            dist as f64 / 48.0
        ))
    }
}

#[test]
fn still_fixture_passes_with_container_backend_and_worker_agreement() {
    let (root, folder) = stage("fixtures/sprint2-still", "still11");
    let name = folder.file_name().unwrap().to_string_lossy().into_owned();
    let (code, out) = run(&["check", &name], &root);
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(&["render", &name], &root);
    assert_eq!(code, 0, "{out}");
    let mp4 = folder.join("out.mp4");
    let csv = ffprobe_csv(&mp4);
    assert!(csv.contains("h264,yuv420p"), "video must be H.264 yuv420p: {csv}");
    assert!(csv.contains("aac,48000,2"), "audio must be AAC 48 kHz stereo: {csv}");

    // Backend variants agree.
    let (code, out) = run(
        &["render", ".", "-o", "cpu.mp4", "--backend", "cpu"],
        &folder,
    );
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(&["render", ".", "-o", "auto.mp4"], &folder);
    assert_eq!(code, 0, "{out}");
    assert_eq!(
        decode_raw(&folder.join("cpu.mp4"), 30),
        decode_raw(&folder.join("auto.mp4"), 30),
        "cpu and auto backends must draw identical pictures"
    );
    assert_eq!(
        decode_audio(&folder.join("cpu.mp4")),
        decode_audio(&folder.join("auto.mp4")),
        "cpu and auto backends must mix identical sound"
    );

    // Worker variants agree.
    let (code, out) = run_env(
        &["render", ".", "-o", "w1.mp4"],
        &folder,
        &[("FRAMESMITH_WORKERS", "1")],
    );
    assert_eq!(code, 0, "{out}");
    let (code, out) = run_env(
        &["render", ".", "-o", "w8.mp4"],
        &folder,
        &[("FRAMESMITH_WORKERS", "8")],
    );
    assert_eq!(code, 0, "{out}");
    assert_eq!(
        decode_raw(&folder.join("w1.mp4"), 30),
        decode_raw(&folder.join("w8.mp4"), 30),
        "1 and 8 workers must draw identical pictures"
    );
    assert_eq!(
        decode_audio(&folder.join("w1.mp4")),
        decode_audio(&folder.join("w8.mp4")),
        "1 and 8 workers must mix identical sound"
    );
}

#[test]
fn marked_audio_verdict_names_fixture_track_and_distance_on_mistune() {
    let root = tmp("mistune11");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    ffmpeg(&[
        "-f",
        "lavfi",
        "-i",
        "sine=frequency=440:duration=2",
        "-ar",
        "48000",
        "-ac",
        "2",
        &folder.join("tone.wav").display().to_string(),
    ]);
    std::fs::write(
        folder.join("composition.kdl"),
        "composition name=\"t\" width=96 height=64 duration=2 fps=30 background=\"#000000\" version=1 {\n    audio path=\"tone.wav\" start=1.0 duration=1.0\n}\n",
    )
    .unwrap();
    let (code, out) = run(&["render", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    let pcm = decode_audio(&folder.join("out.mp4"));
    let onset = onset_frame(&pcm, 500).expect("tone must be audible");
    // True expectation passes within the 1 ms bound.
    verify_onset("marked-audio", "audio#0", onset, 48000, 48)
        .expect("onset must sit within 1 ms of its written time");
    // A deliberately mistuned expectation is a named defect, never a pass.
    let err = verify_onset("marked-audio", "audio#0", onset, 52800, 48).expect_err(
        "a fixture 100 ms outside its bounds must not pass",
    );
    assert!(err.contains("fixture=marked-audio"), "{err}");
    assert!(err.contains("track=audio#0"), "{err}");
    assert!(err.contains("distance="), "{err}");
    assert!(err.contains("no pass recorded"), "{err}");
}

#[test]
fn refusal_drills_write_no_mp4() {
    // Invalid input: exit 1, no MP4.
    let (root, folder) = stage("fixtures/invalid-unknown-key", "refuse11");
    let name = folder.file_name().unwrap().to_string_lossy().into_owned();
    let (code, out) = run(&["render", &name], &root);
    assert_eq!(code, 1, "{out}");
    assert!(!folder.join("out.mp4").exists(), "no partial MP4 on refusal");
    assert!(
        !folder.join("out.mp4.framesmith-part").exists(),
        "no staged leftover on refusal"
    );

    // Later-sprint input: exit 2 before the first frame, no MP4.
    let (root, folder) = stage("fixtures/later-audio", "later11");
    let name = folder.file_name().unwrap().to_string_lossy().into_owned();
    let (code, out) = run(&["render", &name], &root);
    assert_eq!(code, 2, "{out}");
    assert!(out.contains("Sprint 10"), "{out}");
    assert!(!folder.join("out.mp4").exists(), "no partial MP4 on refusal");

    // Undecodable audio: exit 1 naming the file and stage, no MP4.
    let root = tmp("badaudio11");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("bad.wav"), "not audio at all").unwrap();
    std::fs::write(
        folder.join("composition.kdl"),
        "composition name=\"t\" width=96 height=64 duration=1 fps=30 background=\"#000000\" version=1 {\n    audio path=\"bad.wav\" start=0 duration=1.0\n}\n",
    )
    .unwrap();
    let (code, out) = run(&["render", "proj"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("bad.wav"), "{out}");
    assert!(out.contains("audio stage"), "{out}");
    assert!(!folder.join("out.mp4").exists(), "no partial MP4 on refusal");
}
