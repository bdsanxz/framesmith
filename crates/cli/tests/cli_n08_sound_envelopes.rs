//! Sprint 10 sound (T001 + T002): volume/pan envelopes ride per
//! sample, and fades null-test equal to explicit envelopes.
//!
//! Volume and pan keyframes shape the audible mix (ramp levels, move
//! sound across channels, hold jumps); out-of-range values and bad
//! keyframe shapes refuse naming the stop and the range; vanished
//! media stops the render with no partial MP4; sugared fades and
//! explicit-envelope twins decode identically; and enveloped renders
//! re-render byte-identically. Sample-exactness itself is proven in
//! the render unit tests; these tests prove the authored behavior
//! survives the lossy encode with wide margins.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-n08env-{tag}-{}-{}",
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

fn rms(pcm: &[i16], from: usize, to: usize, channel: usize) -> f64 {
    let frames = pcm.len() / 2;
    let (from, to) = (from.min(frames), to.min(frames));
    let n = (to - from).max(1);
    let mut sum = 0f64;
    for i in from..to {
        sum += (pcm[2 * i + channel] as f64).powi(2);
    }
    (sum / n as f64).sqrt()
}

fn peak(pcm: &[i16], from: usize, to: usize, channel: usize) -> i32 {
    let frames = pcm.len() / 2;
    let mut m = 0i32;
    for i in from.min(frames)..to.min(frames) {
        m = m.max(pcm[2 * i + channel].abs() as i32);
    }
    m
}

fn write(folder: &Path, name: &str, content: &str) {
    std::fs::write(folder.join(name), content).unwrap();
}

/// Mux a testsrc picture with `tone` (PCM stereo, no codec padding) so
/// video sound envelopes ride a measurable source.
fn gen_video_with_sound(dir: &Path, name: &str, tone: &Path, seconds: f64) {
    ffmpeg(&[
        "-f",
        "lavfi",
        "-i",
        &format!("testsrc=duration={seconds}:size=64x48:rate=30"),
        "-i",
        &tone.display().to_string(),
        "-c:v",
        "libx264",
        "-pix_fmt",
        "yuv420p",
        "-c:a",
        "pcm_s16le",
        "-shortest",
        &dir.join(name).display().to_string(),
    ]);
}

const HEAD: &str =
    "composition name=\"t\" width=64 height=48 duration=2 fps=30 background=\"#000000\" version=1 {\n";

// --- T001: enveloped volume matches hand math ---------------------------------

#[test]
fn volume_key_shapes_the_audible_ramp() {
    let root = tmp("volramp");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    gen_tone(&folder.join("tone.wav"), 440, 2.0);
    // Ride 0 -> 1 over the first second, then down to half by the end.
    write(
        &folder,
        "composition.kdl",
        &format!(
            "{HEAD}    audio path=\"tone.wav\" start=0 duration=2.0\n    key target=0 property=\"volume\" {{\n        frame at=0 value=0\n        frame at=1 value=1\n        frame at=2 value=0.5\n    }}\n}}\n"
        ),
    );
    let (code, out) = run(&["render", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    let pcm = decode_audio(&folder.join("out.mp4"));
    // AAC priming/trim differs across FFmpeg versions (up to one
    // 1024-sample frame of overshoot on older builds).
    let samples = pcm.len() / 2;
    assert!(
        (96000..=96000 + 1024).contains(&samples),
        "full-duration mix, got {samples}"
    );
    let early = peak(&pcm, 480, 4800, 0);
    let mid = peak(&pcm, 43000, 48000, 0);
    let end = peak(&pcm, 91000, 95500, 0);
    println!("vol ramp peaks: early={early} mid={mid} end={end}");
    assert!(
        early < mid / 3,
        "ramp starts near silence: {early} vs {mid}"
    );
    assert!(
        (end as f64 / mid as f64 - 0.5).abs() < 0.2,
        "ramp ends near half: {end} vs {mid}"
    );
}

#[test]
fn tween_and_set_envelopes_ride_with_easing() {
    let root = tmp("tweenset");
    // An eased tween rides volume up; two sets step it down mid-clip.
    for (tag, extra) in [
        (
            "tween",
            "    tween target=0 property=\"volume\" kind=\"from-to\" start=0 duration=2 from=0.2 to=1.0 ease=\"in-quad\"\n",
        ),
        (
            "sets",
            "    set target=0 property=\"volume\" start=0 duration=1 value=1.0\n    set target=0 property=\"volume\" start=1 duration=1 value=0.2\n",
        ),
    ] {
        let dir = root.join(tag);
        std::fs::create_dir_all(&dir).unwrap();
        gen_tone(&dir.join("tone.wav"), 440, 2.0);
        write(
            &dir,
            "composition.kdl",
            &format!("{HEAD}    audio path=\"tone.wav\" start=0 duration=2.0\n{extra}}}\n"),
        );
        let (code, out) = run(&["render", ".", "-o", "out.mp4"], &dir);
        assert_eq!(code, 0, "{tag}: {out}");
        let pcm = decode_audio(&dir.join("out.mp4"));
        let first = rms(&pcm, 4800, 24000, 0);
        let second = rms(&pcm, 72000, 91000, 0);
        println!("{tag}: first-half rms={first:.1} second-half rms={second:.1}");
        if tag == "tween" {
            assert!(
                first < second * 0.7,
                "eased tween rides up: {first} vs {second}"
            );
        } else {
            assert!(
                first > second * 3.0,
                "sets step down 1.0 -> 0.2: {first} vs {second}"
            );
        }
    }
}

// --- T001: enveloped pan moves sound across channels ----------------------------

#[test]
fn pan_key_moves_sound_across_channels() {
    let root = tmp("pansweep");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    gen_tone(&folder.join("tone.wav"), 440, 2.0);
    // Dual-mono source swept left -> right across the clip.
    write(
        &folder,
        "composition.kdl",
        &format!(
            "{HEAD}    audio path=\"tone.wav\" start=0 duration=2.0\n    key target=0 property=\"pan\" {{\n        frame at=0 value=-1\n        frame at=2 value=1\n    }}\n}}\n"
        ),
    );
    let (code, out) = run(&["render", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    let pcm = decode_audio(&folder.join("out.mp4"));
    // Narrow windows near the edges, where the sweep still sits past
    // 80% to one side (pan -0.95..-0.8, then 0.8..0.95).
    let (ll, rl) = (rms(&pcm, 2400, 9600, 0), rms(&pcm, 2400, 9600, 1));
    let (lr, rr) = (rms(&pcm, 86400, 93600, 0), rms(&pcm, 86400, 93600, 1));
    println!("pan sweep: start L={ll:.1} R={rl:.1}; end L={lr:.1} R={rr:.1}");
    assert!(rl < ll * 0.25, "sweep starts left: {ll} vs {rl}");
    assert!(lr < rr * 0.25, "sweep ends right: {lr} vs {rr}");
    // Center holds both channels near unity.
    let mid_l = rms(&pcm, 45000, 51000, 0);
    let mid_r = rms(&pcm, 45000, 51000, 1);
    assert!(
        (mid_l / mid_r - 1.0).abs() < 0.3,
        "center stays balanced: {mid_l} vs {mid_r}"
    );
}

// --- T001: unity envelope preserves bytes; reruns are identical ------------------

#[test]
fn constant_unity_envelope_matches_no_envelope() {
    let root = tmp("unity");
    // A flat 1.0 envelope must decode exactly like no envelope: the
    // envelope path preserves unity bit for bit through the encode.
    for (tag, extra) in [
        ("plain", ""),
        (
            "env",
            "    key target=0 property=\"volume\" {\n        frame at=0 value=1\n        frame at=2 value=1\n    }\n",
        ),
    ] {
        let dir = root.join(tag);
        std::fs::create_dir_all(&dir).unwrap();
        gen_tone(&dir.join("tone.wav"), 440, 2.0);
        write(
            &dir,
            "composition.kdl",
            &format!("{HEAD}    audio path=\"tone.wav\" start=0 duration=2.0 volume=-6\n{extra}}}\n"),
        );
        let (code, out) = run(&["render", ".", "-o", "out.mp4"], &dir);
        assert_eq!(code, 0, "{tag}: {out}");
    }
    let plain = decode_audio(&root.join("plain").join("out.mp4"));
    let enveloped = decode_audio(&root.join("env").join("out.mp4"));
    assert_eq!(plain, enveloped, "unity envelope preserves every sample");
}

#[test]
fn enveloped_render_reruns_byte_identically() {
    let root = tmp("envdet");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    gen_tone(&folder.join("tone.wav"), 440, 2.0);
    write(
        &folder,
        "composition.kdl",
        &format!(
            "{HEAD}    audio path=\"tone.wav\" start=0 duration=2.0\n    key target=0 property=\"volume\" {{\n        frame at=0 value=0.2\n        frame at=2 value=1.8 ease=\"in-out-sine\"\n    }}\n    tween target=0 property=\"pan\" kind=\"from-to\" start=0 duration=2 from=-0.5 to=0.5\n}}\n"
        ),
    );
    let (code, out) = run(&["render", "proj", "-o", "a.mp4"], &root);
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(&["render", "proj", "-o", "b.mp4"], &root);
    assert_eq!(code, 0, "{out}");
    let a = decode_audio(&root.join("a.mp4"));
    let b = decode_audio(&root.join("b.mp4"));
    assert_eq!(a, b, "enveloped mix re-renders byte-identically");
    println!("enveloped determinism: {} samples identical", a.len());
}

// --- T001: envelopes ride video sound too --------------------------------------------

#[test]
fn volume_envelope_rides_video_sound() {
    let root = tmp("videoenv");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    gen_tone(&folder.join("tone.wav"), 440, 2.0);
    gen_video_with_sound(&folder, "clip.mp4", &folder.join("tone.wav"), 2.0);
    write(
        &folder,
        "composition.kdl",
        &format!(
            "{HEAD}    video path=\"clip.mp4\" start=0 duration=2.0\n    key target=0 property=\"volume\" {{\n        frame at=0 value=0\n        frame at=1 value=1\n        frame at=2 value=1\n    }}\n}}\n"
        ),
    );
    let (code, out) = run(&["check", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(&["render", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    let pcm = decode_audio(&folder.join("out.mp4"));
    let early = peak(&pcm, 480, 4800, 0);
    let late = peak(&pcm, 43000, 48000, 0);
    println!("video envelope peaks: early={early} late={late}");
    assert!(
        early < late / 3,
        "video sound rides the envelope: {early} vs {late}"
    );
}

// --- T001: refusals name the stop and the range -----------------------------------

#[test]
fn envelope_problems_refuse_naming_stop_and_range() {
    let root = tmp("envrefuse");
    for (tag, body, want) in [
        (
            "vol-range",
            "    key target=0 property=\"volume\" {\n        frame at=0 value=1\n        frame at=1 value=3\n    }\n",
            "volume runs 0 (silence) to 2",
        ),
        (
            "pan-range",
            "    tween target=0 property=\"pan\" kind=\"to\" start=0 duration=1 to=-1.5\n",
            "pan runs -1 (left) to 1 (right)",
        ),
        (
            "single-stop",
            "    key target=0 property=\"pan\" {\n        frame at=0 value=0\n    }\n",
            "at least two stops",
        ),
        (
            "unordered",
            "    key target=0 property=\"volume\" {\n        frame at=1 value=0\n        frame at=0 value=1\n    }\n",
            "time order",
        ),
        (
            "wrong-target",
            "    text value=\"Hi\" start=0 duration=2\n    key target=1 property=\"volume\" {\n        frame at=0 value=0\n        frame at=1 value=1\n    }\n",
            "needs an audio or video",
        ),
    ] {
        let dir = root.join(tag);
        std::fs::create_dir_all(&dir).unwrap();
        gen_tone(&dir.join("tone.wav"), 440, 2.0);
        write(
            &dir,
            "composition.kdl",
            &format!("{HEAD}    audio path=\"tone.wav\" start=0 duration=2.0\n{body}}}\n"),
        );
        let (code, out) = run(&["check", "."], &dir);
        assert_eq!(code, 1, "{tag}: {out}");
        assert!(out.contains("E_BAD_MOTION"), "{tag}: {out}");
        assert!(out.contains(want), "{tag}: {out}");
        let (code, _) = run(&["render", ".", "-o", "out.mp4"], &dir);
        assert_eq!(code, 1, "{tag} refuses to render");
        assert!(!dir.join("out.mp4").exists(), "{tag}: no partial MP4");
    }
    // One machine-readable refusal carries code and location.
    let dir = root.join("vol-range");
    let (code, out) = run(&["check", ".", "--json"], &dir);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_BAD_MOTION"), "{out}");
    assert!(out.contains("\"line\""), "{out}");
}

// --- T001: vanished media stops with no partial MP4 ----------------------------------

#[test]
fn vanished_media_stops_render_with_no_partial_mp4() {
    let root = tmp("vanished");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    gen_tone(&folder.join("tone.wav"), 440, 2.0);
    write(
        &folder,
        "composition.kdl",
        &format!(
            "{HEAD}    audio path=\"tone.wav\" start=0 duration=2.0\n    key target=0 property=\"volume\" {{\n        frame at=0 value=0.5\n        frame at=2 value=1.0\n    }}\n}}\n"
        ),
    );
    let (code, out) = run(&["check", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    std::fs::remove_file(folder.join("tone.wav")).unwrap();
    let (code, out) = run(&["render", "proj"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("tone.wav"), "refusal names the file: {out}");
    assert!(!folder.join("out.mp4").exists(), "no MP4 on refusal");
    assert!(
        !folder.join("out.mp4.framesmith-part").exists(),
        "no encode part file"
    );
    assert!(
        !folder.join("out.mp4.framesmith-mix.pcm").exists(),
        "no staged mix left behind"
    );
}

// --- T002: fades null-test equal to explicit envelopes -------------------------------

#[test]
fn fade_twins_null_test_equal() {
    let root = tmp("nulltest");
    gen_tone(&root.join("tone.wav"), 440, 2.0);
    // Non-overlapping fades: each spells as one volume segment.
    let faded = format!("{HEAD}    audio path=\"tone.wav\" start=0 duration=2.0 fade-in=0.5\n}}\n");
    let twin_in = format!(
        "{HEAD}    audio path=\"tone.wav\" start=0 duration=2.0\n    key target=0 property=\"volume\" {{\n        frame at=0 value=0\n        frame at=0.5 value=1\n        frame at=2 value=1\n    }}\n}}\n"
    );
    for (tag, body) in [("faded", faded.as_str()), ("twin-in", twin_in.as_str())] {
        let dir = root.join(tag);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::copy(root.join("tone.wav"), dir.join("tone.wav")).unwrap();
        write(&dir, "composition.kdl", body);
        let (code, out) = run(&["check", "."], &dir);
        assert_eq!(code, 0, "{tag}: {out}");
        let (code, out) = run(&["render", ".", "-o", "out.mp4"], &dir);
        assert_eq!(code, 0, "{tag}: {out}");
    }
    let faded = decode_audio(&root.join("faded").join("out.mp4"));
    let twin = decode_audio(&root.join("twin-in").join("out.mp4"));
    // Fade-in twin: identical through the fade and unity after it.
    assert_eq!(faded, twin, "fade-in nulls against its explicit twin");
    println!("fade-in null: {} samples identical", faded.len());
}

#[test]
fn fade_out_twin_nulls_test_equal() {
    let root = tmp("nullout");
    gen_tone(&root.join("tone.wav"), 440, 2.0);
    let faded =
        format!("{HEAD}    audio path=\"tone.wav\" start=0 duration=2.0 fade-out=0.5\n}}\n");
    let twin = format!(
        "{HEAD}    audio path=\"tone.wav\" start=0 duration=2.0\n    key target=0 property=\"volume\" {{\n        frame at=0 value=1\n        frame at=1.5 value=1\n        frame at=2 value=0\n    }}\n}}\n"
    );
    for (tag, body) in [("faded", faded.as_str()), ("twin", twin.as_str())] {
        let dir = root.join(tag);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::copy(root.join("tone.wav"), dir.join("tone.wav")).unwrap();
        write(&dir, "composition.kdl", body);
        let (code, out) = run(&["render", ".", "-o", "out.mp4"], &dir);
        assert_eq!(code, 0, "{tag}: {out}");
    }
    let faded = decode_audio(&root.join("faded").join("out.mp4"));
    let twin = decode_audio(&root.join("twin").join("out.mp4"));
    assert_eq!(faded, twin, "fade-out nulls against its explicit twin");
}

// --- T002: fade diagnostics keep file, line, column, and fix --------------------------

#[test]
fn bad_fade_keeps_located_diagnostics() {
    let root = tmp("badfade");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    gen_tone(&folder.join("tone.wav"), 440, 2.0);
    write(
        &folder,
        "composition.kdl",
        &format!("{HEAD}    audio path=\"tone.wav\" start=0 duration=2.0 fade-in=5.0\n}}\n"),
    );
    let (code, out) = run(&["check", "proj"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_BAD_AUDIO"), "{out}");
    assert!(out.contains("fade-in"), "{out}");
    let (code, out) = run(&["check", "proj", "--json"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_BAD_AUDIO"), "{out}");
    assert!(out.contains("\"line\""), "{out}");
    assert!(out.contains("\"fix\""), "{out}");
}
