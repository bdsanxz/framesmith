//! Sprint 3 T008 (sample-accurate 48 kHz stereo mix).
//!
//! Every audible sound starts within 1 ms of its written time,
//! overlapping sounds sum, the mix is byte-identical across reruns and
//! worker counts, soundless folders carry exact digital silence, and
//! undecodable inputs refuse before the first frame naming the file.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-mix-{}-{}-{}",
        tag,
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

fn ffmpeg(args: &[&str]) {
    let out = Command::new("ffmpeg")
        .args(["-y", "-v", "error"])
        .args(args)
        .output()
        .expect("run host ffmpeg");
    assert!(out.status.success(), "host ffmpeg failed: {args:?}");
}

/// Generate a pure sine tone wav at 48 kHz stereo.
fn gen_tone(path: &Path, freq: u32, seconds: f64) {
    ffmpeg(&[
        "-f",
        "lavfi",
        "-i",
        &format!("sine=frequency={freq}:duration={seconds}"),
        "-ar",
        "48000",
        "-ac",
        "2",
        &path.display().to_string(),
    ]);
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
    pairs.iter().map(|b| i16::from_le_bytes(*b)).collect()
}

/// First stereo frame index where either channel exceeds `threshold`.
fn onset_frame(pcm: &[i16], threshold: i16) -> Option<usize> {
    (0..pcm.len() / 2).find(|&i| pcm[2 * i].abs() > threshold || pcm[2 * i + 1].abs() > threshold)
}

fn rms(pcm: &[i16], from: usize, to: usize) -> f64 {
    let n = (to.min(pcm.len() / 2) - from.min(pcm.len() / 2)).max(1);
    let mut sum = 0f64;
    for i in from.min(pcm.len() / 2)..to.min(pcm.len() / 2) {
        sum += (pcm[2 * i] as f64).powi(2) + (pcm[2 * i + 1] as f64).powi(2);
    }
    (sum / (2 * n) as f64).sqrt()
}

fn peak(pcm: &[i16], from: usize, to: usize) -> i32 {
    let mut m = 0i32;
    for i in from.min(pcm.len() / 2)..to.min(pcm.len() / 2) {
        m = m
            .max(pcm[2 * i].abs() as i32)
            .max(pcm[2 * i + 1].abs() as i32);
    }
    m
}

fn ffprobe_audio(mp4: &Path) -> String {
    let out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "a",
            "-show_entries",
            "stream=codec_name,sample_rate,channels",
            "-of",
            "default=noprint_wrappers=1",
        ])
        .arg(mp4)
        .output()
        .expect("run host ffprobe");
    assert!(out.status.success());
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn write(folder: &Path, name: &str, content: &str) {
    std::fs::write(folder.join(name), content).unwrap();
}

// --- T008: sample accuracy --------------------------------------------------

#[test]
fn marked_audio_starts_within_1ms_of_its_written_time() {
    let root = tmp("onset");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    gen_tone(&folder.join("tone.wav"), 440, 2.0);
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=96 height=64 duration=2 fps=30 background=\"#000000\" version=1 {\n    audio path=\"tone.wav\" start=1.0 duration=1.0\n}\n",
    );
    let (code, out) = run(&["render", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("stage audio: ok"), "{out}");
    let pcm = decode_audio(&folder.join("out.mp4"));
    assert_eq!(pcm.len() / 2, 96000, "full-duration mix");
    let onset = onset_frame(&pcm, 500).expect("tone must be audible");
    println!(
        "onset: frame {onset}, written 48000, error {} samples",
        onset as i64 - 48000
    );
    assert!(
        onset.abs_diff(48000) <= 48,
        "onset {onset} is more than 1 ms (48 samples) from written time 48000"
    );
    // Silence before the written start is exact digital zero.
    let pre: Vec<i16> = pcm[..2 * 47000].to_vec();
    assert!(
        pre.iter().all(|&x| x == 0),
        "sound starts only at its written time"
    );
}

#[test]
fn trim_and_placement_anchor_the_sound() {
    let root = tmp("trim");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    gen_tone(&folder.join("tone.wav"), 440, 3.0);
    // trim-start selects the source window; placement selects the mix time.
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=96 height=64 duration=2 fps=30 background=\"#000000\" version=1 {\n    audio path=\"tone.wav\" start=0.5 duration=1.0 trim-start=1.0 trim-end=2.0\n}\n",
    );
    let (code, out) = run(&["render", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    let pcm = decode_audio(&folder.join("out.mp4"));
    let onset = onset_frame(&pcm, 500).expect("trimmed tone must be audible");
    println!("trim onset: frame {onset}, written 24000");
    assert!(
        onset.abs_diff(24000) <= 48,
        "trimmed onset {onset} misses written time 24000"
    );
    // The taken window is one second: sound ends back in silence.
    let tail = peak(&pcm, 73000, pcm.len() / 2);
    assert!(tail < 200, "trim window ends in silence, tail peak {tail}");
}

#[test]
fn overlapping_tracks_sum_without_silent_limiting() {
    let root = tmp("overlap");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    gen_tone(&folder.join("tone.wav"), 440, 1.0);
    // Two identical tracks at -12 dB each: the overlap must sum to
    // twice one track alone, with headroom left over (no clipping, no
    // limiter in the bus).
    for (tag, extra) in [
        ("single", ""),
        (
            "double",
            "    audio path=\"tone.wav\" start=0 duration=1.0 volume=-12\n",
        ),
    ] {
        let dir = root.join(tag);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::copy(folder.join("tone.wav"), dir.join("tone.wav")).unwrap();
        write(
            &dir,
            "composition.kdl",
            &format!(
                "composition name=\"t\" width=64 height=48 duration=1 fps=30 background=\"#000000\" version=1 {{\n    audio path=\"tone.wav\" start=0 duration=1.0 volume=-12\n{extra}}}\n"
            ),
        );
        let (code, out) = run(&["render", ".", "-o", "out.mp4"], &dir);
        assert_eq!(code, 0, "{tag}: {out}");
    }
    let single = decode_audio(&root.join("single").join("out.mp4"));
    let double = decode_audio(&root.join("double").join("out.mp4"));
    let ps = peak(&single, 5000, 40000);
    let pd = peak(&double, 5000, 40000);
    println!(
        "overlap peaks: single={ps} double={pd} ratio={:.3}",
        pd as f64 / ps as f64
    );
    assert!(
        (pd as f64 / ps as f64 - 2.0).abs() < 0.1,
        "overlap must sum: {ps} vs {pd}"
    );
}

#[test]
fn volume_and_linear_fades_shape_the_audible_track() {
    let root = tmp("gainfade");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    gen_tone(&folder.join("tone.wav"), 440, 2.0);
    let render_gain = |tag: &str, volume: &str, fade: &str| -> Vec<i16> {
        let dir = root.join(tag);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::copy(folder.join("tone.wav"), dir.join("tone.wav")).unwrap();
        write(
            &dir,
            "composition.kdl",
            &format!(
                "composition name=\"t\" width=64 height=48 duration=2 fps=30 background=\"#000000\" version=1 {{\n    audio path=\"tone.wav\" start=0 duration=2.0 volume={volume}{fade}\n}}\n"
            ),
        );
        let (code, out) = run(&["render", ".", "-o", "out.mp4"], &dir);
        assert_eq!(code, 0, "{tag}: {out}");
        decode_audio(&dir.join("out.mp4"))
    };
    // +6 dB doubles the RMS of 0 dB (linear gain, no limiting).
    let flat = render_gain("flat", "0", "");
    let loud = render_gain("loud", "6", "");
    let ratio = rms(&loud, 5000, 40000) / rms(&flat, 5000, 40000).max(1.0);
    println!("gain ratio (+6 dB vs 0 dB): {ratio:.3}");
    assert!(
        (ratio - 2.0).abs() < 0.3,
        "+6 dB must double amplitude: {ratio}"
    );
    // A one-second linear fade-in starts near silence and reaches unity.
    let faded = render_gain("fade", "0", " fade-in=1.0");
    let early = rms(&faded, 100, 2000);
    let late = rms(&faded, 43000, 46000);
    println!("fade rms: early={early:.1} late={late:.1}");
    assert!(early < late * 0.35, "fade-in must ramp from near silence");
    assert!(
        (late / rms(&flat, 43000, 46000).max(1.0) - 1.0).abs() < 0.2,
        "fade reaches unity"
    );
}

// --- T008: determinism and silence ------------------------------------------

#[test]
fn mix_is_byte_identical_across_reruns_and_workers() {
    let root = tmp("mix-determinism");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    gen_tone(&folder.join("tone.wav"), 660, 2.0);
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=96 height=64 duration=2 fps=30 background=\"#101010\" version=1 {\n    audio path=\"tone.wav\" start=0.25 duration=1.5 volume=-6 fade-in=0.25 fade-out=0.25\n    audio path=\"tone.wav\" start=0.75 duration=1.0 trim-start=0.5 volume=-12\n}\n",
    );
    let (code, out) = run(&["render", "proj", "-o", "a.mp4"], &root);
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(&["render", "proj", "-o", "b.mp4"], &root);
    assert_eq!(code, 0, "{out}");
    let (code, out) = run_env(
        &["render", "proj", "-o", "w.mp4"],
        &root,
        &[("FRAMESMITH_WORKERS", "8")],
    );
    assert_eq!(code, 0, "{out}");
    let audios: Vec<Vec<i16>> = ["a.mp4", "b.mp4", "w.mp4"]
        .iter()
        .map(|m| decode_audio(&root.join(m)))
        .collect();
    assert_eq!(audios[0], audios[1], "reruns mix byte-identically");
    assert_eq!(audios[0], audios[2], "worker counts mix byte-identically");
    println!(
        "mix-determinism: {} samples identical across reruns and workers",
        audios[0].len()
    );
}

#[test]
fn soundless_folder_carries_exact_digital_silence() {
    let root = tmp("silence");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=96 height=64 duration=1 fps=30 background=\"#112233\" version=1 {\n    text value=\"Hi\" start=0 duration=1\n}\n",
    );
    let (code, out) = run(&["render", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    let probe = ffprobe_audio(&folder.join("out.mp4"));
    for want in ["codec_name=aac", "sample_rate=48000", "channels=2"] {
        assert!(probe.contains(want), "{probe}");
    }
    let pcm = decode_audio(&folder.join("out.mp4"));
    assert_eq!(pcm.len() / 2, 48000, "full-duration silence");
    assert!(pcm.iter().all(|&x| x == 0), "silence is exact digital zero");
}

// --- T008: refusal before the first frame ------------------------------------

#[test]
fn undecodable_sound_refuses_before_first_frame_naming_the_file() {
    let root = tmp("bad-sound");
    // Corrupt bytes pass validation (shape is fine) but fail the decode.
    let folder = root.join("corrupt");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("bad.wav"), b"this is not audio data at all").unwrap();
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=64 height=48 duration=1 fps=30 background=\"#000\" version=1 {\n    audio path=\"bad.wav\" start=0 duration=1\n}\n",
    );
    let (code, out) = run(&["check", "corrupt"], &root);
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(&["render", "corrupt"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("bad.wav"), "refusal names the file: {out}");
    assert!(
        out.contains("audio stage"),
        "refusal names the audio stage: {out}"
    );
    assert!(!folder.join("out.mp4").exists(), "no MP4 on refusal");
    assert!(!folder.join("out.mp4.framesmith-part").exists());
    // Missing files fail even earlier, at validation.
    let missing = root.join("missing");
    std::fs::create_dir_all(&missing).unwrap();
    write(
        &missing,
        "composition.kdl",
        "composition name=\"t\" width=64 height=48 duration=1 fps=30 background=\"#000\" version=1 {\n    audio path=\"gone.wav\" start=0 duration=1\n}\n",
    );
    let (code, out) = run(&["check", "missing"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_MISSING_FILE"), "{out}");
    let (code, _) = run(&["render", "missing"], &root);
    assert_eq!(code, 1);
    assert!(!missing.join("out.mp4").exists());
}

#[test]
fn audio_validation_still_comes_from_t007() {
    let root = tmp("audio-validation");
    // Gain above +12 dB is invalid; later-sprint keys are unsupported.
    for (tag, body, code, want) in [
        ("loud", "volume=20", 1, "E_BAD_AUDIO"),
        ("effect", "effect=\"hall\"", 2, "E_UNSUPPORTED"),
    ] {
        let dir = root.join(tag);
        std::fs::create_dir_all(&dir).unwrap();
        gen_tone(&dir.join("tone.wav"), 440, 1.0);
        write(
            &dir,
            "composition.kdl",
            &format!(
                "composition name=\"t\" width=64 height=48 duration=1 fps=30 background=\"#000\" version=1 {{\n    audio path=\"tone.wav\" start=0 duration=1 {body}\n}}\n"
            ),
        );
        let (got, out) = run(&["check", tag], &root);
        assert_eq!(got, code, "{tag}: {out}");
        assert!(out.contains(want), "{tag}: {out}");
    }
}
