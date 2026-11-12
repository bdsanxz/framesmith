//! Sprint 10 sound (T012) end-to-end: the committed sound reference
//! project exercises volume and pan envelopes, a same-track
//! constant-power crossfade, ducking under voice, delay plus reverb,
//! detected speech pauses, one-pass loudness normalization, and a
//! drawn waveform in one place. These tests stage that exact example
//! (never the live checkout dir) and prove it validates clean with
//! pauses, tails, and peaks listed, renders values matching
//! hand-computed sample ratios, re-renders identically, refuses
//! invalid sound variants before frame one, and range-renders in
//! agreement with the full mix. Host ffmpeg only.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-n08ref-{tag}-{}-{}",
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
    let out = Command::new(bin())
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("run framesmith binary");
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    (out.status.code().unwrap_or(-1), text)
}

/// Stage the committed sound reference (composition plus media) to a
/// temp dir; the checkout's example dir is never written.
fn stage_reference(tag: &str) -> (PathBuf, PathBuf) {
    let root = tmp(tag);
    let src = manifest_dir().join("examples/sound-reference");
    let dst = root.join("ref");
    std::fs::create_dir_all(dst.join("media")).unwrap();
    std::fs::copy(src.join("composition.kdl"), dst.join("composition.kdl")).unwrap();
    for e in std::fs::read_dir(src.join("media")).unwrap() {
        let e = e.unwrap();
        std::fs::copy(e.path(), dst.join("media").join(e.file_name())).unwrap();
    }
    (root, dst)
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

/// Goertzel magnitude (peak-amplitude estimate) of one tone in one
/// channel over a time window: the hand-computed oracle for the mix.
fn goertzel(pcm: &[i16], freq: f64, t0: f64, t1: f64, ch: usize) -> f64 {
    const RATE: f64 = 48_000.0;
    let (a, b) = ((t0 * RATE) as usize, (t1 * RATE) as usize);
    let w = 2.0 * std::f64::consts::PI * freq / RATE;
    let c = 2.0 * w.cos();
    let (mut s1, mut s2) = (0.0f64, 0.0f64);
    for i in a..b {
        let s0 = f64::from(pcm[2 * i + ch]) + c * s1 - s2;
        s2 = s1;
        s1 = s0;
    }
    let n = (b - a) as f64;
    (s1 * s1 + s2 * s2 - s1 * s2 * c).sqrt() / n * 2.0
}

/// Independent loudness facts from the host FFmpeg `loudnorm` filter.
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

#[test]
fn reference_validates_with_pauses_tails_peaks_and_normalizes() {
    let (_root, folder) = stage_reference("clean");
    let (code, out) = run(&["check", "."], &folder);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("is valid"), "{out}");
    assert!(
        out.contains(
            "silence audio \"vox\" (7:1): 1 range(s) at or below -40 dBFS lasting at least 0.5s: 1.750s..2.500s"
        ),
        "{out}"
    );
    assert!(
        out.contains(
            "note: effect tail clip #2 (9:1): delay tail 2.500s plus reverb tail 0.120s runs to 6.620s past the composition end; truncated to 4.000s"
        ),
        "{out}"
    );
    assert!(out.contains("master peak: -8.2 dBFS"), "{out}");
    let (code, out) = run(&["check", ".", "--json"], &folder);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("\"id\":\"vox\""), "{out}");
    assert!(
        out.contains("\"ranges\":[{\"start\":1.75,\"end\":2.5}]"),
        "{out}"
    );
    assert!(out.contains("\"delay_tail\":2.5"), "{out}");
    assert!(out.contains("\"master_peak\":"), "{out}");

    let (code, out) = run(&["render", ".", "-o", "out.mp4"], &folder);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("normalize: loud: measured"), "{out}");
    assert!(out.contains("applied gain"), "{out}");
    assert!(out.contains("master peak -8.2 dBFS"), "{out}");
    assert!(
        out.contains("loudest silent section 1.750s..2.500s"),
        "{out}"
    );
    let (integrated, tp) = loudnorm_measure(&folder.join("out.mp4"));
    println!("loudnorm: integrated {integrated:.2} LUFS, true peak {tp:.2} dBTP");
    assert!(
        (integrated - -16.0).abs() <= 1.0,
        "integrated {integrated:.2} LUFS"
    );
    assert!(tp <= -0.5, "true peak {tp:.2} dBTP stays guarded");

    let (code, out) = run(&["waveform", ".", "--buckets", "8"], &folder);
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains("bucket 5/8: 2.000s..2.500s peak -8.2 dBFS"),
        "{out}"
    );
    assert!(out.contains("master peak -8.2 dBFS"), "{out}");
}

#[test]
fn reference_matches_hand_values_and_rerenders_identically() {
    let (_root, folder) = stage_reference("values");
    let (code, _) = run(&["render", ".", "-o", "out.mp4"], &folder);
    assert_eq!(code, 0);
    let pcm = decode_audio(&folder.join("out.mp4"));
    // AAC priming/trim differs across FFmpeg versions (up to one
    // 1024-sample frame of overshoot on older builds).
    let samples = pcm.len() / 2;
    assert!(
        (4 * 48_000..=4 * 48_000 + 1024).contains(&samples),
        "full-duration mix, got {samples}"
    );
    let bed = |t0: f64, t1: f64| goertzel(&pcm, 440.0, t0, t1, 0);
    // Volume ramp: env mean 1.0 over [0.7,0.9] vs 0.7 over [0.1,0.3].
    let ramp = bed(0.7, 0.9) / bed(0.1, 0.3);
    assert!((ramp - 1.0 / 0.7).abs() <= 0.05, "ramp {ramp}");
    // Ducking: the bed sits at 0.25 while the voice window sounds.
    let duck = bed(1.1, 1.4) / bed(0.7, 0.9);
    assert!((duck - 0.25).abs() <= 0.03, "duck {duck}");
    // Voice handoff at full left gain over the clean bed reference.
    let voice = goertzel(&pcm, 660.0, 1.1, 1.4, 0) / bed(0.7, 0.9);
    assert!(
        (voice - 9000.0 * 0.25775 / 4000.0).abs() <= 0.03,
        "voice {voice}"
    );
    // Sting handoff through delay plus reverb (documented DSP replay).
    let sting = goertzel(&pcm, 880.0, 2.05, 2.2, 0) / bed(0.7, 0.9);
    assert!((sting - 0.5919).abs() <= 0.03, "sting {sting}");
    // Delay decay: second tap over first equals the feedback.
    let decay = goertzel(&pcm, 880.0, 2.95, 3.05, 0) / goertzel(&pcm, 880.0, 2.7, 2.8, 0);
    assert!((decay - 0.5).abs() <= 0.06, "decay {decay}");
    // Pan sweep: the voice starts left and ends right of center.
    let lr = goertzel(&pcm, 660.0, 1.05, 1.2, 0) / goertzel(&pcm, 660.0, 1.05, 1.2, 1);
    let rl = goertzel(&pcm, 660.0, 1.55, 1.7, 1) / goertzel(&pcm, 660.0, 1.55, 1.7, 0);
    assert!(lr > 1.8, "voice starts left L/R={lr}");
    assert!(rl > 2.0, "voice ends right R/L={rl}");
    println!("ramp {ramp:.3} duck {duck:.3} voice {voice:.3} sting {sting:.3} decay {decay:.3} L/R {lr:.2} R/L {rl:.2}");

    let (code, _) = run(&["render", ".", "-o", "out-b.mp4"], &folder);
    assert_eq!(code, 0);
    let again = decode_audio(&folder.join("out-b.mp4"));
    assert_eq!(pcm, again, "re-renders decode identically");
}

#[test]
fn reference_refusals_and_range_agree() {
    let (_root, folder) = stage_reference("refuse");
    // A spot check of every refusal family: check and render agree
    // with the code and leave no MP4 behind.
    let cases = [
        (
            "volume-range",
            "key target=0 property=\"volume\" {\n        frame at=0 value=0\n        frame at=1 value=3\n    }",
            "E_BAD_MOTION",
        ),
        (
            "duck-id",
            "id=\"bed\" duck-under=\"ghost\" duck-amount=0.25",
            "E_UNKNOWN_REF",
        ),
        (
            "delay-range",
            "delay=2.5",
            "E_BAD_AUDIO",
        ),
        (
            "reverb-range",
            "reverb=2",
            "E_BAD_AUDIO",
        ),
        (
            "threshold",
            "detect-silence=true silence-threshold=\"loud\"",
            "E_BAD_AUDIO",
        ),
        (
            "normalize",
            "",
            "E_BAD_AUDIO",
        ),
    ];
    for (tag, extra, needle) in cases {
        let dir = folder.join(format!("bad-{tag}"));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::copy(folder.join("media/bed.wav"), dir.join("bed.wav")).unwrap();
        let body = if tag == "normalize" {
            "composition name=\"t\" width=64 height=48 duration=2 fps=30 background=\"#000000\" version=1 normalize=\"peak\" {\n    audio path=\"bed.wav\" start=0 duration=2\n}\n".to_string()
        } else if tag == "volume-range" {
            format!(
                "composition name=\"t\" width=64 height=48 duration=2 fps=30 background=\"#000000\" version=1 {{\n    audio path=\"bed.wav\" start=0 duration=2\n    {extra}\n}}\n"
            )
        } else {
            format!(
                "composition name=\"t\" width=64 height=48 duration=2 fps=30 background=\"#000000\" version=1 {{\n    audio path=\"bed.wav\" start=0 duration=2 {extra}\n}}\n"
            )
        };
        std::fs::write(dir.join("composition.kdl"), body).unwrap();
        let (code, out) = run(&["check", "."], &dir);
        assert_eq!(code, 1, "{tag}: {out}");
        assert!(out.contains(needle), "{tag}: {out}");
        let (code, _) = run(&["render", ".", "-o", "out.mp4"], &dir);
        assert_eq!(code, 1, "{tag}: bad sound refuses to render");
        assert!(!dir.join("out.mp4").exists(), "{tag}: no partial MP4");
    }

    let (code, _) = run(&["render", ".", "-o", "full.mp4"], &folder);
    assert_eq!(code, 0);
    let (code, out) = run(
        &[
            "render",
            ".",
            "-o",
            "range.mp4",
            "--start",
            "1",
            "--end",
            "3",
        ],
        &folder,
    );
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("rendering sub-range 1.000s..3.000s"), "{out}");
    let full = decode_audio(&folder.join("full.mp4"));
    let rng = decode_audio(&folder.join("range.mp4"));
    // AAC priming/trim differs across FFmpeg versions (see above).
    let samples = rng.len() / 2;
    assert!(
        (2 * 48_000..=2 * 48_000 + 1024).contains(&samples),
        "range mix, got {samples}"
    );
    for (freq, t0, t1) in [
        (440.0, 1.1, 1.4),
        (660.0, 1.1, 1.4),
        (440.0, 1.8, 1.95),
        (880.0, 2.05, 2.2),
        (880.0, 2.6, 2.9),
        (440.0, 2.6, 2.9),
    ] {
        for ch in [0usize, 1usize] {
            let a = goertzel(&full, freq, t0, t1, ch);
            let b = goertzel(&rng, freq, t0 - 1.0, t1 - 1.0, ch);
            let rel = (b - a).abs() / a;
            assert!(
                rel <= 0.03,
                "range [{t0},{t1}] f={freq} ch={ch}: {a} vs {b}"
            );
        }
    }
    println!("range agrees with the full mix across 12 tone/channel checks");
}
