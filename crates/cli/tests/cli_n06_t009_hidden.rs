//! Sprint 6 T009 end-to-end: `hidden=true` skips picture and mix.
//!
//! A hidden box leaves the background untouched, a hidden tone leaves
//! silence, a hidden id still answers `start-after`, hidden errors
//! still refuse, and removing `hidden` restores byte-identical output.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-n06t009-{tag}-{}-{}",
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
    assert!(out.status.success(), "host ffmpeg failed");
}

fn write(folder: &Path, name: &str, content: &str) {
    std::fs::create_dir_all(folder).unwrap();
    std::fs::write(folder.join(name), content).unwrap();
}

fn gen_tone(path: &Path, seconds: u32) {
    ffmpeg(&[
        "-f",
        "lavfi",
        "-i",
        &format!("sine=frequency=440:duration={seconds}"),
        "-ar",
        "48000",
        "-ac",
        "2",
        &path.display().to_string(),
    ]);
}

fn decode_video(mp4: &Path, w: u32, h: u32) -> Vec<u8> {
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
            &format!("{w}x{h}"),
            "-",
        ])
        .output()
        .expect("decode mp4 with host ffmpeg");
    assert!(out.status.success());
    out.stdout
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
            "-ac",
            "2",
            "-ar",
            "48000",
            "-",
        ])
        .output()
        .expect("decode audio with host ffmpeg");
    assert!(out.status.success());
    let bytes = out.stdout;
    assert!(bytes.len() % 2 == 0);
    bytes
        .chunks_exact(2)
        .map(|c| i16::from_le_bytes([c[0], c[1]]))
        .collect()
}

fn center(frames: &[u8], frame: usize, w: u32, h: u32) -> (u8, u8, u8) {
    let stride = (w * h * 3) as usize;
    let at = frame * stride + ((h / 2 * w + w / 2) * 3) as usize;
    (frames[at], frames[at + 1], frames[at + 2])
}

fn is_red(p: (u8, u8, u8)) -> bool {
    p.0 > 180 && p.1 < 100 && p.2 < 100
}

fn rms(samples: &[i16]) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum: f64 = samples.iter().map(|&x| (x as f64) * (x as f64)).sum();
    (sum / samples.len() as f64).sqrt()
}

fn render(_p: &Path, root: &Path, out: &str) -> (i32, String) {
    run(&["render", "p", "-o", out], root)
}

// --- picture ---------------------------------------------------------------------

#[test]
fn hidden_box_renders_as_if_absent() {
    let root = tmp("picture");
    let hidden = "composition name=\"main\" width=64 height=64 duration=1 fps=30 background=\"#000000\" version=1 {\n\
         box x=0 y=0 width=64 height=64 fill=\"#ff0000\" start=0 duration=1 hidden=true\n\
         }\n";
    let clean = "composition name=\"main\" width=64 height=64 duration=1 fps=30 background=\"#000000\" version=1 {\n\
         }\n";
    let shown = "composition name=\"main\" width=64 height=64 duration=1 fps=30 background=\"#000000\" version=1 {\n\
         box x=0 y=0 width=64 height=64 fill=\"#ff0000\" start=0 duration=1\n\
         }\n";
    write(&root.join("p"), "composition.kdl", hidden);
    let (code, out) = render(&root.join("p"), &root, "hidden.mp4");
    assert_eq!(code, 0, "{out}");
    write(&root.join("q"), "composition.kdl", clean);
    let (code, out) = run(&["render", "q", "-o", "clean.mp4"], &root);
    assert_eq!(code, 0, "{out}");
    write(&root.join("s"), "composition.kdl", shown);
    let (code, out) = run(&["render", "s", "-o", "shown.mp4"], &root);
    assert_eq!(code, 0, "{out}");

    // The hidden box leaves only the background, exactly like no box.
    let hidden_frames = decode_video(&root.join("hidden.mp4"), 64, 64);
    assert!(!is_red(center(&hidden_frames, 0, 64, 64)), "hidden box draws nothing");
    let shown_frames = decode_video(&root.join("shown.mp4"), 64, 64);
    assert!(is_red(center(&shown_frames, 0, 64, 64)), "visible box draws red");
    assert_eq!(
        std::fs::read(root.join("hidden.mp4")).unwrap(),
        std::fs::read(root.join("clean.mp4")).unwrap(),
        "hidden renders byte-identically to never having the clip"
    );
}

#[test]
fn removing_hidden_restores_byte_identical_output() {
    let root = tmp("toggle");
    let with_flag = "composition name=\"main\" width=64 height=64 duration=1 fps=30 background=\"#000000\" version=1 {\n\
         box x=0 y=0 width=64 height=64 fill=\"#ff0000\" start=0 duration=1 hidden=false\n\
         }\n";
    let without = "composition name=\"main\" width=64 height=64 duration=1 fps=30 background=\"#000000\" version=1 {\n\
         box x=0 y=0 width=64 height=64 fill=\"#ff0000\" start=0 duration=1\n\
         }\n";
    write(&root.join("p"), "composition.kdl", with_flag);
    let (code, out) = render(&root.join("p"), &root, "flag.mp4");
    assert_eq!(code, 0, "{out}");
    write(&root.join("q"), "composition.kdl", without);
    let (code, out) = run(&["render", "q", "-o", "plain.mp4"], &root);
    assert_eq!(code, 0, "{out}");
    assert_eq!(
        std::fs::read(root.join("flag.mp4")).unwrap(),
        std::fs::read(root.join("plain.mp4")).unwrap(),
        "hidden=false matches the never-hidden render"
    );
}

// --- sound -----------------------------------------------------------------------

#[test]
fn hidden_tone_mixes_silence() {
    let root = tmp("sound");
    let comp = |hidden: &str| {
        format!(
            "composition name=\"main\" width=64 height=64 duration=2 fps=30 background=\"#000000\" version=1 {{\n\
             audio path=\"tone.wav\" start=0 duration=2{hidden}\n\
             }}\n"
        )
    };
    write(&root.join("p"), "composition.kdl", &comp(" hidden=true"));
    gen_tone(&root.join("p").join("tone.wav"), 2);
    let (code, out) = render(&root.join("p"), &root, "hidden.mp4");
    assert_eq!(code, 0, "{out}");
    write(&root.join("q"), "composition.kdl", &comp(""));
    gen_tone(&root.join("q").join("tone.wav"), 2);
    let (code, out) = run(&["render", "q", "-o", "shown.mp4"], &root);
    assert_eq!(code, 0, "{out}");

    let hidden_mix = decode_audio(&root.join("hidden.mp4"));
    assert!(rms(&hidden_mix) < 50.0, "hidden tone mixes silence");
    let shown_mix = decode_audio(&root.join("shown.mp4"));
    assert!(rms(&shown_mix) > 200.0, "visible tone is audible");

    // Silence by hiding equals silence by absence, byte for byte.
    write(
        &root.join("r"),
        "composition.kdl",
        "composition name=\"main\" width=64 height=64 duration=2 fps=30 background=\"#000000\" version=1 {\n\
         }\n",
    );
    let (code, out) = run(&["render", "r", "-o", "silent.mp4"], &root);
    assert_eq!(code, 0, "{out}");
    assert_eq!(
        std::fs::read(root.join("hidden.mp4")).unwrap(),
        std::fs::read(root.join("silent.mp4")).unwrap(),
        "hidden tone renders byte-identically to no audio"
    );
}

// --- nesting ---------------------------------------------------------------------

#[test]
fn hidden_nest_skips_picture_and_sound() {
    let root = tmp("nest");
    let child = "composition name=\"child\" width=64 height=64 duration=2 fps=30 background=\"#000000\" version=1 {\n\
         box x=0 y=0 width=64 height=64 fill=\"#ff0000\" start=0 duration=2\n\
         audio path=\"tone.wav\" start=0 duration=2\n\
         }\n";
    let parent = |hidden: &str| {
        format!(
            "composition name=\"main\" root=true width=64 height=64 duration=2 fps=30 background=\"#000000\" version=1 {{\n\
             nest nest=\"child\" start=0 duration=2{hidden}\n\
             }}\n{child}"
        )
    };
    write(&root.join("p"), "composition.kdl", &parent(" hidden=true"));
    gen_tone(&root.join("p").join("tone.wav"), 2);
    let (code, out) = render(&root.join("p"), &root, "hidden.mp4");
    assert_eq!(code, 0, "{out}");
    write(&root.join("q"), "composition.kdl", &parent(""));
    gen_tone(&root.join("q").join("tone.wav"), 2);
    let (code, out) = run(&["render", "q", "-o", "shown.mp4"], &root);
    assert_eq!(code, 0, "{out}");

    let hidden_frames = decode_video(&root.join("hidden.mp4"), 64, 64);
    assert!(!is_red(center(&hidden_frames, 0, 64, 64)), "hidden nest draws nothing");
    let shown_frames = decode_video(&root.join("shown.mp4"), 64, 64);
    assert!(is_red(center(&shown_frames, 0, 64, 64)), "visible nest draws the child");
    assert!(rms(&decode_audio(&root.join("hidden.mp4"))) < 50.0, "hidden nest is silent");
    assert!(rms(&decode_audio(&root.join("shown.mp4"))) > 200.0, "visible nest sounds");
}

// --- references and validation -----------------------------------------------------

#[test]
fn hidden_id_answers_references_and_validates_cleanly() {
    let root = tmp("refs");
    write(
        &root.join("p"),
        "composition.kdl",
        "composition name=\"main\" width=64 height=64 duration=2 fps=30 background=\"#000000\" version=1 {\n\
         box x=0 y=0 width=8 height=8 fill=\"#ff0000\" start=0 duration=1 hidden=true id=\"base\"\n\
         text value=\"Follows\" duration=1 start-after=\"base\"\n\
         }\n",
    );
    let (code, out) = run(&["check", "p"], &root);
    assert_eq!(code, 0, "{out}");
    let (code, out) = render(&root.join("p"), &root, "refs.mp4");
    assert_eq!(code, 0, "{out}");
    assert!(root.join("refs.mp4").is_file());
}

#[test]
fn hidden_errors_still_refuse_check_and_render() {
    let root = tmp("errors");
    write(
        &root.join("p"),
        "composition.kdl",
        "composition name=\"main\" width=64 height=64 duration=2 fps=30 background=\"#000000\" version=1 {\n\
         text value=\"A\" start=0 duration=1 hidden=\"yes\"\n\
         }\n",
    );
    let (code, text) = run(&["check", "p"], &root);
    assert_eq!(code, 1, "{text}");
    assert!(text.contains("E_BAD_HIDDEN"), "{text}");
    assert!(text.contains("2:"), "diagnostic names the line: {text}");
    let (code, text) = render(&root.join("p"), &root, "bad.mp4");
    assert_eq!(code, 1, "render refuses too: {text}");
    assert!(text.contains("E_BAD_HIDDEN"), "{text}");
    assert!(!root.join("bad.mp4").exists(), "no partial MP4");

    write(
        &root.join("q"),
        "composition.kdl",
        "composition name=\"main\" width=64 height=64 duration=2 fps=30 background=\"#000000\" version=1 {\n\
         box x=0 y=0 width=8 height=8 fill=\"#ff0000\" start=0 duration=1 hidden=true bogus=1\n\
         }\n",
    );
    let (code, text) = run(&["check", "q"], &root);
    assert_eq!(code, 1, "{text}");
    assert!(text.contains("E_UNKNOWN_KEY"), "hidden unknown keys still fail: {text}");
}
