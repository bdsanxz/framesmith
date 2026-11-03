//! Sprint 10 sound (T005 + T006): echo/room effects and pause detection.
//!
//! Delay taps land at exact multiples with decaying gains, reverb adds
//! a deterministic room tail, out-of-range parameters refuse naming
//! the bound, tails truncate at the composition end with a note,
//! beyond-blur filters still fail as Sprint 11, effected mixes
//! re-render byte-identically, `detect-silence` lists hand-measured
//! pauses in human and `--json` check output without altering audio,
//! and unparsable thresholds refuse naming the accepted form.
//! Sample-exactness itself is proven in the render unit tests; these
//! tests prove the authored behavior survives the lossy encode with
//! wide margins.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-n08fx-{tag}-{}-{}",
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

/// A short tone burst opening `secs` seconds of stereo audio.
fn gen_burst(path: &Path, freq: u32, burst: f64, secs: f64) {
    ffmpeg(&[
        "-f",
        "lavfi",
        "-i",
        &format!("sine=frequency={freq}:duration={burst}"),
        "-af",
        &format!("apad=whole_dur={secs}"),
        "-ar",
        "48000",
        "-ac",
        "2",
        "-t",
        &format!("{secs}"),
        &path.display().to_string(),
    ]);
}

/// A pure sine tone wav at 48 kHz stereo.
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

/// Tone, exact-digital silence, tone: pauses with hand-known edges.
fn gen_gap(path: &Path) {
    ffmpeg(&[
        "-f",
        "lavfi",
        "-i",
        "sine=frequency=440:duration=0.5",
        "-f",
        "lavfi",
        "-i",
        "anullsrc=r=48000:cl=stereo:d=1.0",
        "-f",
        "lavfi",
        "-i",
        "sine=frequency=660:duration=0.5",
        "-filter_complex",
        "[0:a]aformat=sample_fmts=fltp:channel_layouts=stereo[a0];[1:a]aformat=sample_fmts=fltp:channel_layouts=stereo[a1];[2:a]aformat=sample_fmts=fltp:channel_layouts=stereo[a2];[a0][a1][a2]concat=n=3:v=0:a=1",
        "-ar",
        "48000",
        "-ac",
        "2",
        &path.display().to_string(),
    ]);
}

/// A small MP4 carrying a tone, for video-clip effect coverage.
fn gen_video(path: &Path, seconds: f64) {
    ffmpeg(&[
        "-f",
        "lavfi",
        "-i",
        &format!("testsrc=duration={seconds}:size=64x48:rate=30"),
        "-f",
        "lavfi",
        "-i",
        &format!("sine=frequency=440:duration={seconds}"),
        "-ar",
        "48000",
        "-ac",
        "2",
        "-shortest",
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

/// Read the first reported `{"start":..,"end":..}` pause from check JSON.
fn json_range(out: &str) -> (f64, f64) {
    let s = out.find("\"start\":").expect("a reported start");
    let e = out.find("\"end\":").expect("a reported end");
    let num = |at: usize| {
        out[at..]
            .chars()
            .take_while(|c| {
                c.is_ascii_digit() || *c == '.' || *c == '-' || *c == 'e' || *c == 'E' || *c == '+'
            })
            .collect::<String>()
            .parse::<f64>()
            .expect("a JSON number")
    };
    (num(s + 8), num(e + 6))
}

const HEAD2: &str =
    "composition name=\"t\" width=64 height=48 duration=2 fps=30 background=\"#000000\" version=1 {\n";

// --- T005: delay taps land at exact multiples ---------------------------------

#[test]
fn delay_taps_land_at_exact_multiples_with_decaying_gains() {
    let root = tmp("taps");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    // A 0.05 s burst opening 2 s: taps at 0.3/0.6/0.9/... carry the
    // burst at 0.8, 0.4, 0.2, ... (feedback 0.5).
    gen_burst(&folder.join("burst.wav"), 880, 0.05, 2.0);
    write(
        &folder,
        "composition.kdl",
        &format!(
            "{HEAD2}    audio path=\"burst.wav\" start=0 duration=2 delay=0.3 delay-feedback=0.5 delay-mix=0.8\n}}\n"
        ),
    );
    let (code, out) = run(&["check", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("note: effect tail"), "{out}");
    assert!(out.contains("truncated"), "{out}");
    let (code, out) = run(&["render", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    let pcm = decode_audio(&folder.join("out.mp4"));
    assert_eq!(pcm.len() / 2, 96000, "full-duration mix");
    let dry = peak(&pcm, 0, 2400);
    let tap1 = peak(&pcm, 14400, 16800);
    let tap2 = peak(&pcm, 28800, 31200);
    let tap3 = peak(&pcm, 43200, 45600);
    println!("echo taps: dry={dry} tap1={tap1} tap2={tap2} tap3={tap3}");
    assert!(dry > 2000, "the dry burst is audible: {dry}");
    let wet = tap1 as f64 / dry as f64;
    assert!(
        (wet - 0.8).abs() < 0.2,
        "first tap carries the wet level: {wet:.3}"
    );
    let decay1 = tap2 as f64 / tap1 as f64;
    let decay2 = tap3 as f64 / tap2 as f64;
    assert!(
        (decay1 - 0.5).abs() < 0.15,
        "taps decay by the feedback: {decay1:.3}"
    );
    assert!(
        (decay2 - 0.5).abs() < 0.2,
        "taps keep decaying: {decay2:.3}"
    );
}

// --- T005: reverb blooms a deterministic room -----------------------------------

#[test]
fn reverb_adds_a_deterministic_room_tail() {
    let root = tmp("room");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    // A 0.5 s tone in 2 s: the room tail rings past the dry end.
    gen_burst(&folder.join("tone.wav"), 440, 0.5, 2.0);
    for (tag, fx) in [("dry", ""), ("wet", " reverb=0.8")] {
        let dir = root.join(tag);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::copy(folder.join("tone.wav"), dir.join("tone.wav")).unwrap();
        write(
            &dir,
            "composition.kdl",
            &format!("{HEAD2}    audio path=\"tone.wav\" start=0 duration=2{fx}\n}}\n"),
        );
        let (code, out) = run(&["render", ".", "-o", "out.mp4"], &dir);
        assert_eq!(code, 0, "{tag}: {out}");
    }
    let dry = decode_audio(&root.join("dry").join("out.mp4"));
    let wet = decode_audio(&root.join("wet").join("out.mp4"));
    let dry_head = peak(&dry, 0, 24000);
    // The room's first taps land 30 ms past the dry end at 0.8 of the
    // dry level, so the 0.52..1.0 s window rings loud on wet and stays
    // silent on dry.
    let dry_tail = peak(&dry, 25000, 48000);
    let wet_tail = peak(&wet, 25000, 48000);
    println!("room tail: dry head={dry_head} dry tail={dry_tail} wet tail={wet_tail}");
    assert!(dry_head > 2000, "the dry tone is audible: {dry_head}");
    assert!(dry_tail < 1500, "the dry clip ends in silence: {dry_tail}");
    assert!(
        wet_tail > dry_tail * 5 && wet_tail > 1500,
        "the room tail rings past the dry end: {wet_tail} vs {dry_tail}"
    );
}

// --- T005: refusals, truncation notes, determinism -------------------------------

#[test]
fn effect_range_problems_refuse_with_no_partial_output() {
    let root = tmp("refuse");
    gen_tone(&root.join("tone.wav"), 440, 2.0);
    for (tag, body, text_want) in [
        ("delay-range", " delay=2.5", "0 to 2"),
        ("feedback-range", " delay-feedback=0.99", "0 to 0.95"),
        ("mix-range", " delay-mix=1.5", "0 to 1"),
        ("reverb-range", " reverb=-0.1", "0 to 1"),
    ] {
        let dir = root.join(tag);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::copy(root.join("tone.wav"), dir.join("tone.wav")).unwrap();
        write(
            &dir,
            "composition.kdl",
            &format!("{HEAD2}    audio path=\"tone.wav\" start=0 duration=2{body}\n}}\n"),
        );
        let (code, out) = run(&["check", "."], &dir);
        assert_eq!(code, 1, "{tag}: {out}");
        assert!(out.contains("E_BAD_AUDIO"), "{tag}: {out}");
        assert!(out.contains(text_want), "{tag}: {out}");
        let (code, _) = run(&["render", ".", "-o", "out.mp4"], &dir);
        assert_eq!(code, 1, "{tag} refuses to render");
        assert!(!dir.join("out.mp4").exists(), "{tag}: no partial MP4");
    }
    // One machine-readable refusal carries code, location, and fix.
    let dir = root.join("mix-range");
    let (code, out) = run(&["check", ".", "--json"], &dir);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_BAD_AUDIO"), "{out}");
    assert!(out.contains("\"fix\""), "{out}");
}

#[test]
fn tail_truncation_reports_in_human_and_json() {
    let root = tmp("trunc");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    gen_tone(&folder.join("tone.wav"), 660, 2.0);
    write(
        &folder,
        "composition.kdl",
        &format!(
            "{HEAD2}    audio path=\"tone.wav\" start=0 duration=2 id=\"wet\" delay=0.3 delay-feedback=0.5 delay-mix=0.8 reverb=0.5\n}}\n"
        ),
    );
    let (code, out) = run(&["check", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("note: effect tail"), "{out}");
    assert!(out.contains("\"wet\""), "{out}");
    assert!(out.contains("truncated"), "{out}");
    let (code, out) = run(&["check", "proj", "--json"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("\"effect_tails\":[{"), "{out}");
    assert!(out.contains("\"raw_end\":5"), "{out}");
    assert!(out.contains("\"eff_end\":2"), "{out}");
}

#[test]
fn beyond_blur_filters_still_fail_as_sprint_11() {
    let root = tmp("filter");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    gen_video(&folder.join("clip.mp4"), 2.0);
    write(
        &folder,
        "composition.kdl",
        &format!("{HEAD2}    video path=\"clip.mp4\" start=0 duration=2 filter=\"sepia(1)\"\n}}\n"),
    );
    let (code, out) = run(&["check", "proj"], &root);
    assert_eq!(code, 2, "{out}");
    assert!(out.contains("E_UNSUPPORTED"), "{out}");
    assert!(out.contains("Sprint 11"), "{out}");
    let (code, _) = run(&["render", "proj"], &root);
    assert_eq!(code, 2, "unsupported filters refuse before frame one");
    assert!(!folder.join("out.mp4").exists(), "no partial MP4");
}

#[test]
fn effected_mixes_rerun_byte_identically() {
    let root = tmp("det");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    gen_tone(&folder.join("tone.wav"), 440, 2.0);
    gen_video(&folder.join("clip.mp4"), 2.0);
    write(
        &folder,
        "composition.kdl",
        &format!(
            "{HEAD2}    audio path=\"tone.wav\" start=0 duration=2 delay=0.25 delay-feedback=0.4 delay-mix=0.6 reverb=0.5\n    video path=\"clip.mp4\" start=0 duration=2 delay=0.2 delay-mix=0.3\n}}\n"
        ),
    );
    let (code, out) = run(&["render", "proj", "-o", "a.mp4"], &root);
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(&["render", "proj", "-o", "b.mp4"], &root);
    assert_eq!(code, 0, "{out}");
    let a = decode_audio(&root.join("a.mp4"));
    let b = decode_audio(&root.join("b.mp4"));
    assert_eq!(a, b, "effected mixes re-render byte-identically");
    println!("effect determinism: {} samples identical", a.len());
}

// --- T006: pause detection ----------------------------------------------------------

#[test]
fn silence_ranges_match_hand_measured_pauses() {
    let root = tmp("pauses");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    // 0.5 s tone, 1.0 s exact silence, 0.5 s tone: one 0.5..1.5 pause.
    gen_gap(&folder.join("speech.wav"));
    write(
        &folder,
        "composition.kdl",
        &format!(
            "{HEAD2}    audio path=\"speech.wav\" start=0 duration=2 id=\"vox\" detect-silence=true\n}}\n"
        ),
    );
    let (code, out) = run(&["check", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("silence audio \"vox\""), "{out}");
    assert!(out.contains("0.500s..1.500s"), "{out}");
    let (code, out) = run(&["check", "proj", "--json"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("\"silences\":[{"), "{out}");
    // Sine zero-crossings fuzz the hand edges by a sample or two, so
    // the machine range must sit within 48 samples of 0.5..1.5.
    let (start, end) = json_range(&out);
    println!("pause map: start={start} end={end}");
    assert!(
        (start - 0.5).abs() < 0.001 && (end - 1.5).abs() < 0.001,
        "pause maps the hand-measured gap: {start}..{end}"
    );
    // A pure tone reports no pauses but still lists the clip.
    let dir = root.join("notone");
    std::fs::create_dir_all(&dir).unwrap();
    gen_tone(&dir.join("tone.wav"), 440, 2.0);
    write(
        &dir,
        "composition.kdl",
        &format!("{HEAD2}    audio path=\"tone.wav\" start=0 duration=2 detect-silence=true\n}}\n"),
    );
    let (code, out) = run(&["check", "."], &dir);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("no silent ranges"), "{out}");
}

#[test]
fn detection_never_alters_audio() {
    let root = tmp("pure");
    gen_gap(&root.join("speech.wav"));
    for (tag, extra) in [("off", ""), ("on", " detect-silence=true")] {
        let dir = root.join(tag);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::copy(root.join("speech.wav"), dir.join("speech.wav")).unwrap();
        write(
            &dir,
            "composition.kdl",
            &format!(
                "{HEAD2}    audio path=\"speech.wav\" start=0 duration=2 id=\"vox\"{extra}\n}}\n"
            ),
        );
        let (code, out) = run(&["render", ".", "-o", "out.mp4"], &dir);
        assert_eq!(code, 0, "{tag}: {out}");
    }
    let off = decode_audio(&root.join("off").join("out.mp4"));
    let on = decode_audio(&root.join("on").join("out.mp4"));
    assert_eq!(off, on, "detection leaves the mix byte-identical");
}

#[test]
fn unparsable_thresholds_refuse_with_no_partial_output() {
    let root = tmp("badd");
    gen_tone(&root.join("tone.wav"), 440, 2.0);
    let dir = root.join("proj");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::copy(root.join("tone.wav"), dir.join("tone.wav")).unwrap();
    write(
        &dir,
        "composition.kdl",
        &format!(
            "{HEAD2}    audio path=\"tone.wav\" start=0 duration=2 detect-silence=true silence-threshold=\"loud\"\n}}\n"
        ),
    );
    let (code, out) = run(&["check", "."], &dir);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_BAD_AUDIO"), "{out}");
    assert!(out.contains("plain dB number"), "{out}");
    let (code, _) = run(&["render", ".", "-o", "out.mp4"], &dir);
    assert_eq!(code, 1, "bad thresholds refuse to render");
    assert!(!dir.join("out.mp4").exists(), "no partial MP4");
}
