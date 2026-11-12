//! Sprint 3 T009 (audible `audio` tracks: trim, volume, fades).
//!
//! Each track sounds exactly its trimmed window, silence/unity/+12 dB
//! render at their written relative gains, linear fades reach full gain
//! and silence at their written ends, and an over-long fade, volume
//! above +12 dB, or an undecodable format is refused naming the file
//! with no MP4 written.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-track-{tag}-{}-{}",
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
    let out = Command::new(bin())
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("run framesmith binary");
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

fn onset_frame(pcm: &[i16], threshold: i16) -> Option<usize> {
    (0..pcm.len() / 2).find(|&i| pcm[2 * i].abs() > threshold || pcm[2 * i + 1].abs() > threshold)
}

fn rms(pcm: &[i16], from: usize, to: usize) -> f64 {
    let lo = from.min(pcm.len() / 2);
    let hi = to.min(pcm.len() / 2);
    let n = (hi - lo).max(1);
    let mut sum = 0f64;
    for i in lo..hi {
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

fn write(folder: &Path, name: &str, content: &str) {
    std::fs::write(folder.join(name), content).unwrap();
}

// --- T009: trims on both ends ------------------------------------------------

#[test]
fn trims_on_both_ends_sound_exactly_the_window() {
    let root = tmp("trim2");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    gen_tone(&folder.join("tone.wav"), 440, 4.0);
    // Source window 1.0..3.0 (two seconds) placed at 0.5..2.5.
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=96 height=64 duration=3 fps=30 background=\"#000000\" version=1 {\n    audio path=\"tone.wav\" start=0.5 duration=2.0 trim-start=1.0 trim-end=3.0\n}\n",
    );
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
    let onset = onset_frame(&pcm, 500).expect("trimmed tone must be audible");
    println!("both-ends onset: frame {onset}, written 24000");
    assert!(
        onset.abs_diff(24000) <= 48,
        "onset {onset} misses written start 24000"
    );
    // Silence before the written start is exact digital zero.
    assert!(
        pcm[..2 * 23000].iter().all(|&x| x == 0),
        "nothing sounds before the window"
    );
    // The two-second window ends back in silence.
    assert!(
        peak(&pcm, 121500, pcm.len() / 2) < 500,
        "nothing sounds past the window end"
    );
}

// --- T009: silence, unity, +12 dB -------------------------------------------

#[test]
fn silence_unity_and_plus12_render_at_written_gains() {
    let root = tmp("gains");
    std::fs::create_dir_all(root.join("src")).unwrap();
    gen_tone(&root.join("src").join("tone.wav"), 440, 2.0);
    let render_gain = |tag: &str, volume: &str| -> Vec<i16> {
        let dir = root.join(tag);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::copy(root.join("src").join("tone.wav"), dir.join("tone.wav")).unwrap();
        write(
            &dir,
            "composition.kdl",
            &format!(
                "composition name=\"t\" width=64 height=48 duration=2 fps=30 background=\"#000000\" version=1 {{\n    audio path=\"tone.wav\" start=0 duration=2.0 volume=\"{volume}\"\n}}\n"
            ),
        );
        let (code, out) = run(&["render", ".", "-o", "out.mp4"], &dir);
        assert_eq!(code, 0, "{tag}: {out}");
        decode_audio(&dir.join("out.mp4"))
    };
    let silent = render_gain("silent", "-inf");
    assert!(
        silent.iter().all(|&x| x == 0),
        "silence volume renders digital zero"
    );
    let unity = render_gain("unity", "0");
    let loud = render_gain("loud", "12");
    let r0 = rms(&unity, 5000, 40000);
    let r12 = rms(&loud, 5000, 40000);
    let ratio = r12 / r0.max(1.0);
    println!("+12 dB vs unity rms ratio: {ratio:.3} (written 3.981)");
    assert!(
        (ratio - 3.981).abs() < 0.6,
        "+12 dB must quadruple amplitude: {ratio}"
    );
}

// --- T009: linear fades -------------------------------------------------------

#[test]
fn linear_fades_reach_full_gain_and_silence_at_written_ends() {
    let root = tmp("fades");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    gen_tone(&folder.join("tone.wav"), 440, 2.0);
    let render = |tag: &str, extra: &str| -> Vec<i16> {
        let dir = root.join(tag);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::copy(folder.join("tone.wav"), dir.join("tone.wav")).unwrap();
        write(
            &dir,
            "composition.kdl",
            &format!(
                "composition name=\"t\" width=64 height=48 duration=2 fps=30 background=\"#000000\" version=1 {{\n    audio path=\"tone.wav\" start=0 duration=2.0{extra}\n}}\n"
            ),
        );
        let (code, out) = run(&["render", ".", "-o", "out.mp4"], &dir);
        assert_eq!(code, 0, "{tag}: {out}");
        decode_audio(&dir.join("out.mp4"))
    };
    let flat = render("flat", "");
    let faded = render("faded", " fade-in=1.0 fade-out=1.0");
    let mid_flat = rms(&flat, 43000, 46000);
    // The ramp starts near silence and reaches unity at the fade end.
    let early = rms(&faded, 100, 2000);
    let late = rms(&faded, 43000, 46000);
    println!("fade rms: early={early:.1} late={late:.1} flat-mid={mid_flat:.1}");
    assert!(early < late * 0.35, "fade-in must ramp from near silence");
    assert!(
        (late / mid_flat.max(1.0) - 1.0).abs() < 0.25,
        "fade-in reaches full gain"
    );
    // The fade-out tail falls back toward silence.
    let tail = rms(&faded, 92000, 95500);
    assert!(
        tail < late * 0.3,
        "fade-out must fall toward silence: {tail} vs {late}"
    );
}

// --- T009: refusals ------------------------------------------------------------

#[test]
fn overlong_fade_loud_volume_and_undecodable_format_are_refused() {
    let root = tmp("refuse");
    // A fade longer than its one-second clip is a validation error.
    let long = root.join("long");
    std::fs::create_dir_all(&long).unwrap();
    gen_tone(&long.join("tone.wav"), 440, 2.0);
    write(
        &long,
        "composition.kdl",
        "composition name=\"t\" width=64 height=48 duration=1 fps=30 background=\"#000\" version=1 {\n    audio path=\"tone.wav\" start=0 duration=1 fade-in=1.5\n}\n",
    );
    let (code, out) = run(&["check", "long"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_BAD_AUDIO"), "{out}");
    let (code, out) = run(&["render", "long"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(!long.join("out.mp4").exists(), "no MP4 on refusal");
    // Volume above +12 dB is refused; exactly +12 dB stays valid.
    let loud = root.join("loud");
    std::fs::create_dir_all(&loud).unwrap();
    std::fs::copy(long.join("tone.wav"), loud.join("tone.wav")).unwrap();
    write(
        &loud,
        "composition.kdl",
        "composition name=\"t\" width=64 height=48 duration=1 fps=30 background=\"#000\" version=1 {\n    audio path=\"tone.wav\" start=0 duration=1 volume=13\n}\n",
    );
    let (code, out) = run(&["check", "loud"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_BAD_AUDIO"), "{out}");
    let (code, out) = run(&["render", "loud"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(!loud.join("out.mp4").exists(), "no MP4 on refusal");
    // An undecodable format passes shape validation but refuses at
    // render, naming the file and the audio stage, with no MP4.
    let bad = root.join("bad");
    std::fs::create_dir_all(&bad).unwrap();
    std::fs::write(bad.join("bad.wav"), b"this is not audio data at all").unwrap();
    write(
        &bad,
        "composition.kdl",
        "composition name=\"t\" width=64 height=48 duration=1 fps=30 background=\"#000\" version=1 {\n    audio path=\"bad.wav\" start=0 duration=1 trim-start=0 volume=-6 fade-in=0.1\n}\n",
    );
    let (code, out) = run(&["check", "bad"], &root);
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(&["render", "bad"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("bad.wav"), "refusal names the file: {out}");
    assert!(
        out.contains("audio stage"),
        "refusal names the audio stage: {out}"
    );
    assert!(!bad.join("out.mp4").exists(), "no MP4 on refusal");
    assert!(!bad.join("out.mp4.framesmith-part").exists());
}
