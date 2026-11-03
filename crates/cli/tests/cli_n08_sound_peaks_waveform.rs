//! Sprint 10 sound (T007 + T008 + T011): autoplay carriage, waveform
//! peaks, and hot-master QA.
//!
//! `autoplay` travels into `info` (human plus `--json`) with no render
//! effect (the toggle re-renders byte-identically); the render summary
//! reports per-section peaks plus the master peak and loudest silent
//! section in both forms; `waveform [--buckets N]` reports per-bucket
//! peaks for the mix in both forms, refuses bucket counts below 1 and
//! invalid projects like `check`; and hot masters warn naming the
//! peak in `check`, the summary, and `waveform` without changing any
//! exit code. Tolerances below are wide because the independent
//! measurement decodes the lossy MP4 while the reports measure the
//! staged pre-encode mix.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-n08peak-{tag}-{}-{}",
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

fn ffmpeg(args: &[&str]) {
    let out = Command::new("ffmpeg")
        .args(["-y", "-v", "error"])
        .args(args)
        .output()
        .expect("run host ffmpeg");
    assert!(out.status.success(), "host ffmpeg failed: {args:?}");
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

/// Write a 48 kHz stereo s16 WAV by hand (no encoder): a 440 Hz sine
/// at `amplitude` peak, `secs` long. Full scale (`32767`) masters hot
/// by construction; small amplitudes stay quiet.
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
/// on exact samples (no zero-crossing fuzz), so silence reports name
/// round windows and the silent peak is exactly zero.
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

fn peak_in(pcm: &[i16], from: usize, to: usize) -> i32 {
    let frames = pcm.len() / 2;
    let mut m = 0i32;
    for i in from.min(frames)..to.min(frames) {
        m = m.max(pcm[2 * i].abs() as i32);
        m = m.max(pcm[2 * i + 1].abs() as i32);
    }
    m
}

fn write(folder: &Path, name: &str, content: &str) {
    std::fs::write(folder.join(name), content).unwrap();
}

const HEAD: &str =
    "composition name=\"t\" width=64 height=48 duration=2 fps=30 background=\"#000000\" version=1";

// --- T007: autoplay carriage ---------------------------------------------------

#[test]
fn autoplay_travels_into_info_in_both_forms() {
    let root = tmp("infoauto");
    for (tag, attrs, want_human, want_json) in [
        (
            "on",
            " autoplay=true",
            "autoplay: true.",
            "\"autoplay\":true",
        ),
        ("off", "", "autoplay: false.", "\"autoplay\":false"),
    ] {
        let dir = root.join(tag);
        std::fs::create_dir_all(&dir).unwrap();
        write(
            &dir,
            "composition.kdl",
            &format!("{HEAD}{attrs} {{\n    text value=\"Hi\" start=0 duration=2\n}}\n"),
        );
        let (code, out) = run(&["info", "."], &dir);
        assert_eq!(code, 0, "{tag}: {out}");
        assert!(out.contains(want_human), "{tag}: {out}");
        let (code, out) = run(&["info", ".", "--json"], &dir);
        assert_eq!(code, 0, "{tag}: {out}");
        assert!(out.contains(want_json), "{tag}: {out}");
    }
}

#[test]
fn bad_autoplay_refuses_before_frame_one() {
    let root = tmp("badauto");
    let dir = root.join("proj");
    std::fs::create_dir_all(&dir).unwrap();
    write(
        &dir,
        "composition.kdl",
        &format!("{HEAD} autoplay=\"yes\" {{\n    text value=\"Hi\" start=0 duration=2\n}}\n"),
    );
    let (code, out) = run(&["check", "."], &dir);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_BAD_FLAG"), "{out}");
    assert!(out.contains("`autoplay`"), "{out}");
    let (code, out) = run(&["check", ".", "--json"], &dir);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_BAD_FLAG"), "{out}");
    assert!(out.contains("\"fix\""), "{out}");
    let (code, _) = run(&["render", ".", "-o", "out.mp4"], &dir);
    assert_eq!(code, 1, "bad autoplay refuses to render");
    assert!(!dir.join("out.mp4").exists(), "no partial MP4");
}

#[test]
fn autoplay_toggle_renders_byte_identically() {
    let root = tmp("autodet");
    gen_tone(&root.join("tone.wav"), 440, 2.0);
    for (tag, attrs) in [("plain", ""), ("flag", " autoplay=true")] {
        let dir = root.join(tag);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::copy(root.join("tone.wav"), dir.join("tone.wav")).unwrap();
        write(
            &dir,
            "composition.kdl",
            &format!("{HEAD}{attrs} {{\n    audio path=\"tone.wav\" start=0 duration=2\n}}\n"),
        );
        let (code, out) = run(&["render", ".", "-o", "out.mp4"], &dir);
        assert_eq!(code, 0, "{tag}: {out}");
    }
    let plain = decode_audio(&root.join("plain").join("out.mp4"));
    let flagged = decode_audio(&root.join("flag").join("out.mp4"));
    assert_eq!(plain, flagged, "autoplay has zero render effect");
    println!("autoplay toggle: {} samples identical", plain.len());
}

// --- T008: section and bucket peaks --------------------------------------------

#[test]
fn render_summary_peaks_match_check_and_independent_measurement() {
    let root = tmp("summpeaks");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    // Full-scale excess: the staged mix clamps at digital full scale.
    write_wav(&folder.join("hot.wav"), 30000, 2.0);
    write(
        &folder,
        "composition.kdl",
        &format!("{HEAD} {{\n    audio path=\"hot.wav\" start=0 duration=2 id=\"lead\"\n}}\n"),
    );
    let (code, out) = run(&["check", "proj", "--json"], &root);
    assert_eq!(code, 0, "{out}");
    let check_peak: f64 = out
        .split("\"master_peak\":")
        .nth(1)
        .expect("check reports master_peak")
        .split(',')
        .next()
        .unwrap()
        .parse()
        .expect("a JSON number");
    println!("check preview master peak (linear): {check_peak}");
    let (code, out) = run(&["render", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("master peak -0."), "{out}");
    assert!(
        out.contains("section audio \"lead\" 0.000s..2.000s peak -0."),
        "{out}"
    );
    let (code, out) = run(&["render", "proj", "-o", "j.mp4", "--json"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("\"peak_warning\""), "{out}");
    let summary_peak: f64 = out
        .split("\"master_peak\":")
        .nth(1)
        .expect("summary reports master_peak")
        .split(',')
        .next()
        .unwrap()
        .parse()
        .expect("a JSON number");
    assert!(
        (summary_peak - check_peak).abs() < 1e-12,
        "check preview and render summary name the same peak: {check_peak} vs {summary_peak}"
    );
    // Independently measured: the decoded MP4 peaks within a hair of
    // the reported pre-encode peak (lossy encode, wide margin).
    // `-o j.mp4` resolves against the runner cwd, not the project.
    let pcm = decode_audio(&root.join("j.mp4"));
    let measured = peak_in(&pcm, 0, pcm.len() / 2) as f64 / 32768.0;
    assert!(
        (measured - summary_peak).abs() < 0.05,
        "summary matches the decoded mix: {summary_peak} vs {measured}"
    );
}

#[test]
fn waveform_reports_per_bucket_peaks_in_both_forms() {
    let root = tmp("wavebuckets");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    gen_tone(&folder.join("tone.wav"), 440, 2.0);
    write(
        &folder,
        "composition.kdl",
        &format!("{HEAD} {{\n    audio path=\"tone.wav\" start=0 duration=2\n}}\n"),
    );
    let (code, out) = run(&["waveform", "proj", "--buckets", "4"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("4 bucket(s) over 96000 frame(s)"), "{out}");
    assert!(out.contains("bucket 1/4: 0.000s..0.500s peak -"), "{out}");
    assert!(out.contains("bucket 4/4: 1.500s..2.000s peak -"), "{out}");
    assert!(out.contains("master peak -"), "{out}");
    assert!(!out.contains("hot master"), "{out}");
    let (code, out) = run(&["waveform", "--json", "proj", "--buckets", "4"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("\"buckets\":[{"), "{out}");
    assert!(out.contains("\"peak_dbfs\":"), "{out}");
    assert!(out.contains("\"master_peak\":"), "{out}");
    // Buckets tile the mix without gaps: end of bucket i is the start
    // of bucket i+1, and the last bucket ends at the mix end.
    let starts: Vec<f64> = out
        .split("\"start\":")
        .skip(1)
        .map(|s| {
            s.split([',', '}'])
                .next()
                .unwrap()
                .parse()
                .expect("a JSON number")
        })
        .collect();
    let ends: Vec<f64> = out
        .split("\"end\":")
        .skip(1)
        .map(|s| {
            s.split([',', '}'])
                .next()
                .unwrap()
                .parse()
                .expect("a JSON number")
        })
        .collect();
    assert_eq!((starts.len(), ends.len()), (4, 4), "{out}");
    for i in 0..4 {
        let expect_s = i as f64 * 0.5;
        let expect_e = (i + 1) as f64 * 0.5;
        assert!((starts[i] - expect_s).abs() < 1e-9, "{out}");
        assert!((ends[i] - expect_e).abs() < 1e-9, "{out}");
    }
    // Default buckets (16) tile the same mix; every bucket peak tracks
    // the independently decoded MP4 within the lossy margin.
    let (code, out) = run(&["waveform", "--json", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    assert_eq!(out.matches("\"start\":").count(), 16, "{out}");
    let (code, out) = run(&["render", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    let pcm = decode_audio(&folder.join("out.mp4"));
    let (code, out) = run(&["waveform", "--json", "proj", "--buckets", "8"], &root);
    assert_eq!(code, 0, "{out}");
    let reported: Vec<f64> = out
        .split("\"peak\":")
        .skip(1)
        .take(8)
        .map(|s| s.split(',').next().unwrap().parse().expect("a JSON number"))
        .collect();
    assert_eq!(reported.len(), 8);
    for (i, r) in reported.iter().enumerate() {
        let frames = pcm.len() / 2;
        let from = (i * frames) / 8;
        let to = ((i + 1) * frames) / 8;
        let measured = peak_in(&pcm, from, to) as f64 / 32768.0;
        assert!(
            (measured - r).abs() < 0.03,
            "bucket {i} matches the decoded mix: {r} vs {measured}"
        );
    }
}

#[test]
fn waveform_bucket_counts_below_one_fail() {
    let root = tmp("wavebad");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    gen_tone(&folder.join("tone.wav"), 440, 2.0);
    write(
        &folder,
        "composition.kdl",
        &format!("{HEAD} {{\n    audio path=\"tone.wav\" start=0 duration=2\n}}\n"),
    );
    for buckets in ["0", "-3", "lots"] {
        let (code, out) = run(&["waveform", "proj", "--buckets", buckets], &root);
        assert_eq!(code, 1, "{buckets}: {out}");
        assert!(out.contains("--buckets"), "{buckets}: {out}");
        assert!(
            !out.contains("bucket 1/"),
            "{buckets}: nothing listed: {out}"
        );
        let (code, out) = run(&["waveform", "--json", "proj", "--buckets", buckets], &root);
        assert_eq!(code, 1, "{buckets}: {out}");
        assert!(out.contains("\"status\":\"invalid\""), "{buckets}: {out}");
    }
    // Usage errors exit 2 without listing.
    for args in [
        vec!["waveform", "proj", "--buckets", "4", "--buckets", "8"],
        vec!["waveform", "proj", "--bogus"],
        vec!["waveform"],
    ] {
        let (code, out) = run(&args, &root);
        assert_eq!(code, 2, "{args:?}: {out}");
    }
}

#[test]
fn waveform_refuses_invalid_projects_like_check() {
    let root = tmp("waverefuse");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    write(
        &folder,
        "composition.kdl",
        &format!("{HEAD} {{\n    text value=\"Hi\" start=0 duration=2 bogus=1\n}}\n"),
    );
    let (check_code, check_out) = run(&["check", "proj"], &root);
    assert_eq!(check_code, 1, "{check_out}");
    assert!(check_out.contains("E_UNKNOWN_KEY"), "{check_out}");
    let (code, out) = run(&["waveform", "proj"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_UNKNOWN_KEY"), "refuses like check: {out}");
    assert!(!out.contains("bucket 1/"), "no listing: {out}");
    let (code, out) = run(&["waveform", "--json", "proj"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_UNKNOWN_KEY"), "{out}");
    // An unknown composition refuses with no listing.
    write(
        &folder,
        "composition.kdl",
        &format!("{HEAD} {{\n    text value=\"Hi\" start=0 duration=2\n}}\n"),
    );
    let (code, out) = run(&["waveform", "proj", "ghost"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("unknown composition"), "{out}");
    // A vanished source names the file with no listing.
    let dir = root.join("gone");
    std::fs::create_dir_all(&dir).unwrap();
    write(
        &dir,
        "composition.kdl",
        &format!("{HEAD} {{\n    audio path=\"tone.wav\" start=0 duration=2\n}}\n"),
    );
    let (code, out) = run(&["waveform", "."], &dir);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("tone.wav"), "refusal names the file: {out}");
    // A soundless project lists digital silence, exit 0.
    let (code, out) = run(&["waveform", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("over 0 frame(s)"), "{out}");
    assert!(
        out.contains("master peak -inf dBFS (digital silence)."),
        "{out}"
    );
}

// --- T011: hot-master QA ---------------------------------------------------------

#[test]
fn hot_master_warns_naming_the_peak_without_changing_exit() {
    let root = tmp("hotwarn");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    write_wav(&folder.join("hot.wav"), 32767, 2.0);
    write(
        &folder,
        "composition.kdl",
        &format!("{HEAD} {{\n    audio path=\"hot.wav\" start=0 duration=2 id=\"lead\"\n}}\n"),
    );
    // Human check warns naming the peak, exit 0.
    let (code, out) = run(&["check", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("master peak: -0.0 dBFS."), "{out}");
    assert!(
        out.contains("warning: hot master: peak -0.0 dBFS is above -1.0 dBFS"),
        "{out}"
    );
    // Machine check warns with the same peak, exit 0.
    let (code, out) = run(&["check", "proj", "--json"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("\"master_peak_dbfs\":-0."), "{out}");
    assert!(
        out.contains("\"peak_warning\":\"warning: hot master: peak -0.0 dBFS"),
        "{out}"
    );
    // The render summary warns too, exit 0, and re-renders identically.
    let (code, out) = run(&["render", "proj", "-o", "a.mp4"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains("stage audio: warning: hot master: peak -0.0 dBFS"),
        "{out}"
    );
    // The waveform listing warns with the same peak, exit 0.
    let (code, out) = run(&["waveform", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("warning: hot master: peak -0.0 dBFS"), "{out}");
    let (code, out) = run(&["waveform", "--json", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains("\"peak_warning\":\"warning: hot master: peak -0.0 dBFS"),
        "{out}"
    );
    let (code, out) = run(&["render", "proj", "-o", "b.mp4", "--json"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains("\"peak_warning\":\"warning: hot master: peak -0.0 dBFS"),
        "{out}"
    );
    let a = decode_audio(&root.join("a.mp4"));
    let b = decode_audio(&root.join("b.mp4"));
    assert_eq!(a, b, "hot mixes re-render byte-identically");
    println!("hot determinism: {} samples identical", a.len());
}

#[test]
fn quiet_master_reports_without_warnings() {
    let root = tmp("quiet");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    gen_tone(&folder.join("tone.wav"), 440, 2.0);
    write(
        &folder,
        "composition.kdl",
        &format!("{HEAD} {{\n    audio path=\"tone.wav\" start=0 duration=2\n}}\n"),
    );
    let (code, out) = run(&["check", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("master peak: -"), "{out}");
    assert!(!out.contains("hot master"), "{out}");
    let (code, out) = run(&["check", "proj", "--json"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("\"peak_warning\":null"), "{out}");
    let (code, out) = run(&["render", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("master peak -"), "{out}");
    assert!(!out.contains("hot master"), "{out}");
    assert!(out.contains("no silent sections."), "{out}");
    let (code, out) = run(&["render", "proj", "-o", "j.mp4", "--json"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("\"peak_warning\":null"), "{out}");
    assert!(out.contains("\"loudest_silence\":null"), "{out}");
}

#[test]
fn render_summary_reports_the_loudest_silent_section() {
    let root = tmp("loudsilent");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    // A loud block opening exact digital silence: the pause edges land
    // on exact samples, so the silent peak is exactly zero.
    write_block_gap(&folder.join("speech.wav"), 20000, 0.5, 2.0);
    write(
        &folder,
        "composition.kdl",
        &format!(
            "{HEAD} {{\n    audio path=\"speech.wav\" start=0 duration=2 id=\"vox\" detect-silence=true\n}}\n"
        ),
    );
    let (code, out) = run(&["check", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("0.500s..2.000s"), "{out}");
    assert!(out.contains("master peak:"), "{out}");
    assert!(!out.contains("hot master"), "{out}");
    let (code, out) = run(&["render", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains("section audio \"vox\" 0.000s..2.000s peak -4.3 dBFS."),
        "{out}"
    );
    assert!(
        out.contains("loudest silent section 0.500s..2.000s peak -inf dBFS (digital silence)."),
        "{out}"
    );
    let (code, out) = run(&["render", "proj", "-o", "j.mp4", "--json"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains("\"loudest_silence\":{\"start\":0.5,\"end\":2,\"peak\":0,\"peak_dbfs\":null}"),
        "{out}"
    );
}

#[test]
fn range_render_peaks_cover_the_written_window() {
    let root = tmp("rangepeaks");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    write_wav(&folder.join("tone.wav"), 20000, 4.0);
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=64 height=48 duration=4 fps=30 background=\"#000000\" version=1 {\n    audio path=\"tone.wav\" start=0 duration=4 id=\"bed\"\n}\n",
    );
    let (code, out) = run(&["render", "proj", "--start", "1", "--end", "3"], &root);
    assert_eq!(code, 0, "{out}");
    // Section windows shift by the range start, so the report covers
    // exactly the written MP4.
    assert!(
        out.contains("section audio \"bed\" 0.000s..3.000s peak"),
        "{out}"
    );
    assert!(out.contains("master peak -"), "{out}");
}
