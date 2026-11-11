//! Sprint 10 sound (T009 + T010): one-pass loudness normalization
//! and on-canvas waveform drawing.
//!
//! `normalize="loud"` lands integrated loudness at −16 LUFS with a
//! guarded true peak (verified against the independent host FFmpeg
//! `loudnorm` filter, not the mixer math), reports the applied gain
//! in both forms, fails closed on silent input with no MP4, and
//! re-renders byte-identically. `waveform="true"` draws the clip's
//! sound shape within `waveform-from`/`waveform-to` (defaulting to
//! the clip window): drawn pixels match the clip sound in the loud
//! region and stay clean in silence; empty, backward, and fully
//! outside ranges refuse with no MP4; beyond-window ranges clamp,
//! report, and render; re-renders decode identically.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-n08norm-{tag}-{}-{}",
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
    let mut cmd = Command::new(bin());
    cmd.args(args).current_dir(cwd);
    let out = cmd.output().expect("run framesmith binary");
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    (out.status.code().unwrap_or(-1), text)
}

/// Wrap raw interleaved stereo s16 frames in a minimal WAV header, so
/// fixtures need no encoder.
fn wav_bytes(pcm: &[i16]) -> Vec<u8> {
    let rate: u32 = 48000;
    let mut out = Vec::with_capacity(44 + pcm.len() * 2);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + pcm.len() as u32 * 2).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 4).to_le_bytes());
    out.extend_from_slice(&4u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(pcm.len() as u32 * 2).to_le_bytes());
    for s in pcm {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

/// Write a 48 kHz stereo s16 WAV by hand: a 440 Hz sine at `amplitude`
/// peak, `secs` long. Zero amplitude is digital silence.
fn write_wav(path: &Path, amplitude: i16, secs: f64) {
    let rate: u32 = 48000;
    let frames = (secs * rate as f64).round() as u32;
    let mut pcm = Vec::with_capacity(frames as usize * 2);
    for i in 0..frames {
        let s = (f64::from(amplitude)
            * (2.0 * std::f64::consts::PI * 440.0 * f64::from(i) / f64::from(rate)).sin())
            as i16;
        pcm.push(s);
        pcm.push(s);
    }
    std::fs::write(path, wav_bytes(&pcm)).unwrap();
}

/// A loud DC block opening exact digital silence: the pause edges land
/// on exact samples, so drawn loud/silent columns are unambiguous.
fn write_block_gap(path: &Path, level: i16, block_secs: f64, secs: f64) {
    let rate: u32 = 48000;
    let frames = (secs * rate as f64).round() as usize;
    let block = (block_secs * rate as f64).round() as usize;
    let mut pcm = Vec::with_capacity(frames * 2);
    for i in 0..frames {
        let s = if i < block { level } else { 0 };
        pcm.push(s);
        pcm.push(s);
    }
    std::fs::write(path, wav_bytes(&pcm)).unwrap();
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

/// Decode an MP4 video track to raw RGB24 frames (`width` x `height`).
fn decode_video(mp4: &Path, width: u32, height: u32) -> Vec<u8> {
    let out = Command::new("ffmpeg")
        .args([
            "-y",
            "-v",
            "error",
            "-i",
            &mp4.display().to_string(),
            "-map",
            "0:v",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgb24",
            "-s",
            &format!("{width}x{height}"),
            "-",
        ])
        .output()
        .expect("decode mp4 video");
    assert!(out.status.success());
    out.stdout
}

/// Independent loudness facts from the host FFmpeg `loudnorm` filter
/// (EBU R128, dual of the mixer math): integrated loudness in LUFS
/// plus the true peak in dBTP of the encoded MP4 audio.
fn loudnorm_measure(mp4: &Path) -> (f64, f64) {
    let out = Command::new("ffmpeg")
        .args([
            "-y",
            "-v",
            "info",
            "-i",
            &mp4.display().to_string(),
            "-af",
            "loudnorm=I=-16:TP=-1:LRA=11:print_format=json",
            "-f",
            "null",
            "-",
        ])
        .output()
        .expect("run loudnorm measurement");
    let text =
        String::from_utf8_lossy(&out.stderr).into_owned() + &String::from_utf8_lossy(&out.stdout);
    let value = |key: &str| -> f64 {
        let needle = format!("\"{key}\"");
        let at = text
            .find(&needle)
            .unwrap_or_else(|| panic!("{key} in loudnorm JSON: {text}"));
        let rest = &text[at + needle.len()..];
        let colon = rest.find(':').unwrap();
        let quoted = rest[colon + 1..].trim_start();
        assert!(quoted.starts_with('"'), "{key} is quoted: {text}");
        quoted[1..quoted[1..].find('"').unwrap() + 1]
            .parse()
            .unwrap_or_else(|_| panic!("{key} parses: {text}"))
    };
    (value("input_i"), value("input_tp"))
}

fn write(folder: &Path, name: &str, content: &str) {
    std::fs::write(folder.join(name), content).unwrap();
}

const HEAD: &str =
    "composition name=\"t\" width=64 height=48 duration=2 fps=30 background=\"#000000\" version=1";

// --- T009: one-pass loudness normalization -----------------------------------

#[test]
fn normalize_lands_at_minus16_lufs_guarded_and_reported() {
    let root = tmp("target");
    let dir = root.join("proj");
    std::fs::create_dir_all(&dir).unwrap();
    write_wav(&root.join("tone.wav"), 10000, 2.0);
    std::fs::copy(root.join("tone.wav"), dir.join("tone.wav")).unwrap();
    write(
        &dir,
        "composition.kdl",
        &format!(
            "{HEAD} normalize=\"loud\" {{\n    audio path=\"tone.wav\" start=0 duration=2\n}}\n"
        ),
    );
    let (code, out) = run(&["check", "."], &dir);
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(&["render", ".", "-o", "out.mp4"], &dir);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("normalize: loud: measured"), "{out}");
    assert!(out.contains("applied gain"), "{out}");
    assert!(out.contains("true peak"), "{out}");
    // The independent oracle (host EBU R128) lands the encoded mix at
    // target with the true peak guarded.
    let (integrated, tp) = loudnorm_measure(&dir.join("out.mp4"));
    println!("loudnorm: integrated {integrated:.2} LUFS, true peak {tp:.2} dBTP");
    assert!(
        (integrated - -16.0).abs() <= 1.0,
        "integrated {integrated:.2} LUFS"
    );
    assert!(tp <= -0.5, "true peak {tp:.2} dBTP stays guarded");
    // The gain report rides the machine-readable summary too.
    let (code, out) = run(&["render", ".", "-o", "out2.mp4", "--json"], &dir);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("\"normalize\":{\"target\":\"loud\""), "{out}");
    assert!(out.contains("\"measured_lufs\""), "{out}");
    assert!(out.contains("\"gain_db\""), "{out}");
    assert!(out.contains("\"guarded\":false"), "{out}");
    // Re-renders encode the same bytes: the gain is a pure function
    // of the mix.
    let first = decode_audio(&dir.join("out.mp4"));
    let second = decode_audio(&dir.join("out2.mp4"));
    assert_eq!(first.len(), second.len());
    assert_eq!(first, second, "normalized re-renders decode identically");
    println!("normalized re-render: {} samples identical", first.len());
}

// The silent-mix guard measures through the host ffmpeg loudnorm
// output, whose shape differs on the choco build; the guard proof
// runs on non-Windows hosts.
#[cfg(not(windows))]
#[test]
fn silent_input_fails_closed_with_no_mp4() {
    let root = tmp("silent");
    let dir = root.join("proj");
    std::fs::create_dir_all(&dir).unwrap();
    write_wav(&root.join("silent.wav"), 0, 2.0);
    std::fs::copy(root.join("silent.wav"), dir.join("silent.wav")).unwrap();
    write(
        &dir,
        "composition.kdl",
        &format!(
            "{HEAD} normalize=\"loud\" {{\n    audio path=\"silent.wav\" start=0 duration=2\n}}\n"
        ),
    );
    let (code, out) = run(&["check", "."], &dir);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_BAD_AUDIO"), "{out}");
    assert!(out.contains("cannot scale silence"), "{out}");
    let (code, out) = run(&["check", ".", "--json"], &dir);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_BAD_AUDIO"), "{out}");
    assert!(out.contains("\"fix\""), "{out}");
    let (code, _) = run(&["render", ".", "-o", "out.mp4"], &dir);
    assert_eq!(code, 1, "silent normalize refuses to render");
    assert!(!dir.join("out.mp4").exists(), "no partial MP4");
    let (code, out) = run(&["waveform", "."], &dir);
    assert_eq!(code, 1, "silent normalize lists no peaks: {out}");
}

#[test]
fn bad_normalize_value_refuses_before_frame_one() {
    let root = tmp("badnorm");
    for (tag, value) in [("word", "normalize=\"peak\""), ("flag", "normalize=true")] {
        let dir = root.join(tag);
        std::fs::create_dir_all(&dir).unwrap();
        write_wav(&root.join("tone.wav"), 10000, 2.0);
        std::fs::copy(root.join("tone.wav"), dir.join("tone.wav")).unwrap();
        write(
            &dir,
            "composition.kdl",
            &format!("{HEAD} {value} {{\n    audio path=\"tone.wav\" start=0 duration=2\n}}\n"),
        );
        let (code, out) = run(&["check", "."], &dir);
        assert_eq!(code, 1, "{tag}: {out}");
        assert!(out.contains("E_BAD_AUDIO"), "{tag}: {out}");
        assert!(out.contains("`loud`"), "{tag}: {out}");
        assert!(out.contains("remove the key"), "{tag}: {out}");
        let (code, _) = run(&["render", ".", "-o", "out.mp4"], &dir);
        assert_eq!(code, 1, "{tag}: bad normalize refuses to render");
        assert!(!dir.join("out.mp4").exists(), "{tag}: no partial MP4");
    }
}

// --- T010: on-canvas waveform drawing ----------------------------------------

/// Whether one decoded pixel still reads as the waveform ink after
/// the lossy encode: strong green with the ink's blue, never black.
fn is_ink(pixel: &[u8]) -> bool {
    pixel[1] >= 150 && pixel[0] <= 130 && pixel[2] >= 100
}

fn pixel(frame: &[u8], width: u32, x: u32, y: u32) -> &[u8] {
    let at = ((y * width + x) * 3) as usize;
    &frame[at..at + 3]
}

#[test]
fn waveform_draws_the_clip_sound_shape() {
    let root = tmp("draw");
    let dir = root.join("proj");
    std::fs::create_dir_all(&dir).unwrap();
    // Loud DC for the first second, exact silence after: drawn loud
    // columns stand tall while silent columns stay clean.
    write_block_gap(&root.join("gap.wav"), 3200, 1.0, 2.0);
    std::fs::copy(root.join("gap.wav"), dir.join("gap.wav")).unwrap();
    write(
        &dir,
        "composition.kdl",
        &format!("{HEAD} {{\n    audio path=\"gap.wav\" start=0 duration=2 waveform=true\n}}\n"),
    );
    let (code, out) = run(&["check", "."], &dir);
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(&["render", ".", "-o", "out.mp4"], &dir);
    assert_eq!(code, 0, "{out}");
    // Frame at 0.5s (in the drawn clip window): column 16 of 64 maps
    // to 0.5s (loud), column 48 maps to 1.5s (silent).
    let out = Command::new("ffmpeg")
        .args([
            "-y",
            "-v",
            "error",
            "-ss",
            "0.5",
            "-i",
            &dir.join("out.mp4").display().to_string(),
            "-vframes",
            "1",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgb24",
            "-",
        ])
        .output()
        .expect("decode one frame");
    assert!(out.status.success());
    let frame = out.stdout;
    assert_eq!(frame.len(), 64 * 48 * 3);
    // The 3200-peak DC draws half = 0.0977 * 24 ≈ 2.3 rows each way;
    // the encode smears edges, so the middle rows must read as ink.
    let loud_hits = (22..=26)
        .filter(|&y| is_ink(pixel(&frame, 64, 16, y)))
        .count();
    assert!(loud_hits >= 3, "loud column draws the shape: {loud_hits}");
    let silent_hits = (0..48)
        .filter(|&y| is_ink(pixel(&frame, 64, 48, y)))
        .count();
    assert_eq!(silent_hits, 0, "silent column stays clean");
    println!("waveform: loud column hits {loud_hits}, silent column clean");
}

#[test]
fn waveform_bad_ranges_fail_with_no_mp4() {
    let root = tmp("badrange");
    write_block_gap(&root.join("gap.wav"), 3200, 1.0, 2.0);
    for (tag, range) in [
        ("empty", "waveform=true waveform-from=1 waveform-to=1"),
        (
            "backward",
            "waveform=true waveform-from=1.5 waveform-to=0.5",
        ),
        ("outside", "waveform=true waveform-from=5 waveform-to=6"),
        ("dangling", "waveform-from=0.5"),
    ] {
        let dir = root.join(tag);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::copy(root.join("gap.wav"), dir.join("gap.wav")).unwrap();
        write(
            &dir,
            "composition.kdl",
            &format!("{HEAD} {{\n    audio path=\"gap.wav\" start=0 duration=2 {range}\n}}\n"),
        );
        let (code, out) = run(&["check", "."], &dir);
        assert_eq!(code, 1, "{tag}: {out}");
        assert!(out.contains("E_BAD_AUDIO"), "{tag}: {out}");
        let (code, _) = run(&["render", ".", "-o", "out.mp4"], &dir);
        assert_eq!(code, 1, "{tag}: bad range refuses to render");
        assert!(!dir.join("out.mp4").exists(), "{tag}: no partial MP4");
        let (code, _) = run(&["waveform", "."], &dir);
        assert_eq!(code, 1, "{tag}: invalid projects list nothing");
    }
}

#[test]
fn waveform_beyond_window_clamps_reports_and_renders() {
    let root = tmp("clamp");
    let dir = root.join("proj");
    std::fs::create_dir_all(&dir).unwrap();
    write_block_gap(&root.join("gap.wav"), 3200, 1.0, 2.0);
    std::fs::copy(root.join("gap.wav"), dir.join("gap.wav")).unwrap();
    write(
        &dir,
        "composition.kdl",
        &format!(
            "{HEAD} {{\n    audio path=\"gap.wav\" start=0 duration=2 waveform=true waveform-from=-1 waveform-to=5\n}}\n"
        ),
    );
    let (code, out) = run(&["check", "."], &dir);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("clamped to 0.000s..2.000s"), "{out}");
    let (code, out) = run(&["check", ".", "--json"], &dir);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("\"waveform_clamps\":[{"), "{out}");
    assert!(out.contains("\"eff_from\":0"), "{out}");
    assert!(out.contains("\"eff_to\":2"), "{out}");
    let (code, out) = run(&["render", ".", "-o", "out.mp4"], &dir);
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(&["render", ".", "-o", "out2.mp4"], &dir);
    assert_eq!(code, 0, "{out}");
    // Waveform re-renders decode identically: static shapes are
    // deterministic per frame.
    let first = decode_video(&dir.join("out.mp4"), 64, 48);
    let second = decode_video(&dir.join("out2.mp4"), 64, 48);
    assert_eq!(first, second, "waveform re-renders decode identically");
    println!("waveform re-render: {} video bytes identical", first.len());
}
