//! Sprint 10 sound (T003 + T004): same-track crossfades and voice ducking.
//!
//! Consecutive overlaps blend (constant-power by default, linear on
//! request, hard-cut on `none`); overlong windows clamp to the audible
//! samples and report in `check`; beds duck under voice timeline
//! windows and return to full gain after; unknown voices and
//! out-of-range amounts refuse before frame one; and crossfaded plus
//! ducked mixes re-render byte-identically. Sample-exactness itself is
//! proven in the render unit tests; these tests prove the authored
//! behavior survives the lossy encode with wide margins.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-n08xfade-{tag}-{}-{}",
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

fn rms(pcm: &[i16], from: usize, to: usize) -> f64 {
    let frames = pcm.len() / 2;
    let (from, to) = (from.min(frames), to.min(frames));
    let n = (to - from).max(1);
    let mut sum = 0f64;
    for i in from..to {
        sum += (pcm[2 * i] as f64).powi(2);
    }
    (sum / n as f64).sqrt()
}

fn peak(pcm: &[i16], from: usize, to: usize) -> i32 {
    let frames = pcm.len() / 2;
    let mut m = 0i32;
    for i in from.min(frames)..to.min(frames) {
        m = m.max(pcm[2 * i].abs() as i32);
    }
    m
}

fn write(folder: &Path, name: &str, content: &str) {
    std::fs::write(folder.join(name), content).unwrap();
}

const HEAD3: &str =
    "composition name=\"t\" width=64 height=48 duration=3 fps=30 background=\"#000000\" version=1 {\n";

// --- T003: default overlaps blend constant-power -------------------------------

#[test]
fn default_overlap_blends_constant_power() {
    let root = tmp("cpblend");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    gen_tone(&folder.join("tone.wav"), 440, 3.0);
    // Two consecutive identical tones: a plain sum would double the
    // mid-overlap peak (2.0x) and a linear blend would hold unity
    // (1.0x); constant-power meets at ~1.414x.
    write(
        &folder,
        "composition.kdl",
        &format!(
            "{HEAD3}    audio path=\"tone.wav\" start=0 duration=2\n    audio path=\"tone.wav\" start=1 duration=2\n}}\n"
        ),
    );
    let (code, out) = run(&["check", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(&["render", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    let pcm = decode_audio(&folder.join("out.mp4"));
    // AAC priming/trim differs across FFmpeg versions (up to one
    // 1024-sample frame of overshoot on older builds).
    let samples = pcm.len() / 2;
    assert!(
        (144000..=144000 + 1024).contains(&samples),
        "full-duration mix, got {samples}"
    );
    let mid = peak(&pcm, 68000, 76000);
    let alone = peak(&pcm, 120000, 139200);
    println!("constant-power blend: mid-overlap peak={mid} alone peak={alone}");
    let ratio = mid as f64 / alone as f64;
    assert!(
        (ratio - 1.414).abs() < 0.25,
        "mid-overlap meets near equal-power, not a sum or unity: {ratio:.3}"
    );
}

#[test]
fn linear_overlap_holds_unity_on_identical_tones() {
    let root = tmp("linblend");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    gen_tone(&folder.join("tone.wav"), 440, 3.0);
    write(
        &folder,
        "composition.kdl",
        &format!(
            "{HEAD3}    audio path=\"tone.wav\" start=0 duration=2\n    audio path=\"tone.wav\" start=1 duration=2 crossfade=\"linear\"\n}}\n"
        ),
    );
    let (code, out) = run(&["render", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    let pcm = decode_audio(&folder.join("out.mp4"));
    let mid = peak(&pcm, 68000, 76000);
    let alone = peak(&pcm, 120000, 139200);
    println!("linear blend: mid-overlap peak={mid} alone peak={alone}");
    let ratio = mid as f64 / alone as f64;
    assert!(
        (ratio - 1.0).abs() < 0.15,
        "linear blends of identical tones hold unity: {ratio:.3}"
    );
}

// --- T003: none hard-cuts ---------------------------------------------------------

#[test]
fn none_hard_cuts_the_outgoing_clip() {
    let root = tmp("hardcut");
    gen_tone(&root.join("tone.wav"), 440, 3.0);
    // A loud tone handing off to digital silence: the default blend
    // keeps the outgoing tone audible into the overlap, while `none`
    // cuts it exactly at the handoff.
    for (tag, extra) in [("blend", ""), ("cut", " crossfade=\"none\"")] {
        let dir = root.join(tag);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::copy(root.join("tone.wav"), dir.join("tone.wav")).unwrap();
        write(
            &dir,
            "composition.kdl",
            &format!(
                "{HEAD3}    audio path=\"tone.wav\" start=0 duration=2\n    audio path=\"tone.wav\" start=1 duration=2 volume=\"-inf\"{extra}\n}}\n"
            ),
        );
        let (code, out) = run(&["check", "."], &dir);
        assert_eq!(code, 0, "{tag}: {out}");
        let (code, out) = run(&["render", ".", "-o", "out.mp4"], &dir);
        assert_eq!(code, 0, "{tag}: {out}");
    }
    let blend = decode_audio(&root.join("blend").join("out.mp4"));
    let cut = decode_audio(&root.join("cut").join("out.mp4"));
    let blend_early = peak(&blend, 49000, 58000);
    let cut_early = peak(&cut, 49000, 58000);
    let cut_late = peak(&cut, 86000, 95000);
    println!("hard cut: blend early={blend_early} cut early={cut_early} cut late={cut_late}");
    assert!(
        blend_early > 2000,
        "the default blend keeps the outgoing tone audible: {blend_early}"
    );
    assert!(
        cut_early < 1000,
        "the hard cut silences the outgoing clip at the handoff: {cut_early}"
    );
    assert!(
        cut_late < 1000,
        "the silent newcomer stays silent through the overlap: {cut_late}"
    );
}

// --- T003: overlong windows clamp and report ---------------------------------------

#[test]
fn overlong_overlap_clamps_and_reports() {
    let root = tmp("clamp");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    gen_tone(&folder.join("a.wav"), 440, 3.0);
    gen_tone(&folder.join("b.wav"), 660, 3.0);
    // The handoff overlap [1.5,2.5) runs past the 2 s composition end.
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=64 height=48 duration=2 fps=30 background=\"#000000\" version=1 {\n    audio path=\"a.wav\" start=0.5 duration=2\n    audio path=\"b.wav\" start=1.5 duration=1.5\n}\n",
    );
    let (code, out) = run(&["check", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("note: crossfade"), "{out}");
    assert!(out.contains("clamped"), "{out}");
    let (code, out) = run(&["check", "proj", "--json"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("\"crossfade_clamps\":[{"), "{out}");
    assert!(out.contains("\"shape\":\"constant-power\""), "{out}");
    assert!(out.contains("\"raw_end\":2.5"), "{out}");
    assert!(out.contains("\"eff_end\":2"), "{out}");
    let (code, out) = run(&["render", "proj"], &root);
    assert_eq!(code, 0, "{out}");
}

// --- T003/T004: determinism ----------------------------------------------------------

#[test]
fn crossfaded_and_ducked_mixes_rerun_byte_identically() {
    let root = tmp("det");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    gen_tone(&folder.join("bed.wav"), 440, 4.0);
    gen_tone(&folder.join("vox.wav"), 660, 2.0);
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=64 height=48 duration=4 fps=30 background=\"#000000\" version=1 {\n    audio path=\"bed.wav\" start=0 duration=4 id=\"bed\" duck-under=\"vox\" duck-amount=0.25\n    audio path=\"vox.wav\" start=1 duration=2 id=\"vox\"\n}\n",
    );
    let (code, out) = run(&["render", "proj", "-o", "a.mp4"], &root);
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(&["render", "proj", "-o", "b.mp4"], &root);
    assert_eq!(code, 0, "{out}");
    let a = decode_audio(&root.join("a.mp4"));
    let b = decode_audio(&root.join("b.mp4"));
    assert_eq!(a, b, "ducked mix re-renders byte-identically");
    println!("duck determinism: {} samples identical", a.len());
}

// --- T004: beds duck under voice windows ----------------------------------------------

#[test]
fn bed_ducks_under_voice_and_returns_to_full() {
    let root = tmp("duck");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    gen_tone(&folder.join("bed.wav"), 440, 4.0);
    gen_tone(&folder.join("vox.wav"), 660, 2.0);
    // A silent voice still drives the gate: voice activity is read
    // from timeline windows, never from signal, so the ducked bed
    // measures exactly against its unducked self.
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=64 height=48 duration=4 fps=30 background=\"#000000\" version=1 {\n    audio path=\"bed.wav\" start=0 duration=4 id=\"bed\" duck-under=\"vox\" duck-amount=0.25\n    audio path=\"vox.wav\" start=1 duration=2 id=\"vox\" volume=\"-inf\"\n}\n",
    );
    let (code, out) = run(&["check", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(&["render", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    let pcm = decode_audio(&folder.join("out.mp4"));
    // AAC priming/trim differs across FFmpeg versions (see above).
    let samples = pcm.len() / 2;
    assert!(
        (192000..=192000 + 1024).contains(&samples),
        "full-duration mix, got {samples}"
    );
    let outside = rms(&pcm, 9600, 38400);
    let under = rms(&pcm, 72000, 120000);
    let after = rms(&pcm, 153600, 182400);
    println!("duck rms: outside={outside:.1} under={under:.1} after={after:.1}");
    assert!(
        (under / outside - 0.25).abs() < 0.08,
        "bed scales to the declared gain under voice: {}",
        under / outside
    );
    assert!(
        (after / outside - 1.0).abs() < 0.15,
        "full gain returns exactly when voice stops: {}",
        after / outside
    );
}

// --- T003/T004: refusals name the reference and the bound -------------------------------

#[test]
fn sound_reference_and_range_problems_refuse() {
    let root = tmp("refuse");
    gen_tone(&root.join("tone.wav"), 440, 2.0);
    for (tag, body, code_want, text_want) in [
        (
            "unknown-voice",
            "    audio path=\"tone.wav\" start=0 duration=2 id=\"bed\" duck-under=\"ghost\" duck-amount=0.25\n",
            "E_UNKNOWN_REF",
            "\"ghost\"",
        ),
        (
            "duck-range",
            "    audio path=\"tone.wav\" start=0 duration=2 id=\"bed\" duck-under=\"bed\" duck-amount=1.5\n",
            "E_BAD_AUDIO",
            "0 to 1",
        ),
        (
            "bad-shape",
            "    audio path=\"tone.wav\" start=0 duration=2 crossfade=\"smooth\"\n",
            "E_BAD_AUDIO",
            "constant-power",
        ),
    ] {
        let dir = root.join(tag);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::copy(root.join("tone.wav"), dir.join("tone.wav")).unwrap();
        write(
            &dir,
            "composition.kdl",
            &format!(
                "composition name=\"t\" width=64 height=48 duration=2 fps=30 background=\"#000000\" version=1 {{\n{body}}}\n"
            ),
        );
        let (code, out) = run(&["check", "."], &dir);
        assert_eq!(code, 1, "{tag}: {out}");
        assert!(out.contains(code_want), "{tag}: {out}");
        assert!(out.contains(text_want), "{tag}: {out}");
        let (code, _) = run(&["render", ".", "-o", "out.mp4"], &dir);
        assert_eq!(code, 1, "{tag} refuses to render");
        assert!(!dir.join("out.mp4").exists(), "{tag}: no partial MP4");
    }
    // One machine-readable refusal carries code, location, and fix.
    let dir = root.join("duck-range");
    let (code, out) = run(&["check", ".", "--json"], &dir);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_BAD_AUDIO"), "{out}");
    assert!(out.contains("\"line\""), "{out}");
    assert!(out.contains("\"fix\""), "{out}");
}
