//! Sprint 6 T007 end-to-end: nested compositions with per-use timing.
//!
//! A `nest` clip plays its target's local timeline with per-use
//! `trim-start`, `trim-end`, and `rate`; past its content the picture
//! holds the child's last frame while sound goes silent. Chains deeper
//! than 8 levels, circular chains, unbounded uses, and unknown names
//! refuse before frame one with no MP4 written. Renders re-run
//! byte-identically.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-n06t007-{tag}-{}-{}",
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

fn gen_tone(dir: &Path, seconds: u32) {
    std::fs::create_dir_all(dir).unwrap();
    ffmpeg(&[
        "-f",
        "lavfi",
        "-i",
        &format!("sine=frequency=440:duration={seconds}"),
        "-ar",
        "48000",
        "-ac",
        "2",
        &dir.join("tone.wav").display().to_string(),
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

fn is_green(p: (u8, u8, u8)) -> bool {
    p.1 > 180 && p.0 < 100 && p.2 < 100
}

fn rms(samples: &[i16]) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum: f64 = samples.iter().map(|&x| (x as f64) * (x as f64)).sum();
    (sum / samples.len() as f64).sqrt()
}

// The reusable bumper: red in [0, 1), green in [1, 2).
const BUMPER: &str = "composition name=\"bumper\" width=64 height=64 duration=2 fps=2 background=\"#000000\" version=1 {\n\
     box x=0 y=0 width=64 height=64 fill=\"#ff0000\" start=0 duration=1\n\
     box x=0 y=0 width=64 height=64 fill=\"#00ff00\" start=1 duration=1\n\
     }\n";

fn main_with(uses: &str, duration: u32) -> String {
    format!(
        "composition name=\"main\" root=true width=64 height=64 duration={duration} fps=2 background=\"#000000\" version=1 {{\n\
         {uses}\n\
         }}\n{BUMPER}"
    )
}

// --- picture -----------------------------------------------------------------

#[test]
fn nested_bumper_renders_the_child_timeline() {
    let root = tmp("picture");
    write(
        &root.join("p"),
        "composition.kdl",
        &main_with("nest nest=\"bumper\" start=0 duration=2", 2),
    );
    let (code, out) = run(&["check", "p"], &root);
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(&["render", "p"], &root);
    assert_eq!(code, 0, "{out}");
    let mp4 = root.join("p").join("out.mp4");
    assert!(mp4.is_file());
    let frames = decode_video(&mp4, 64, 64);
    assert_eq!(frames.len(), 4 * 64 * 64 * 3);
    // fps=2: frame k shows child second k/2.
    assert!(
        is_red(center(&frames, 0, 64, 64)),
        "frame 0 shows the red half"
    );
    assert!(
        is_red(center(&frames, 1, 64, 64)),
        "frame 1 shows the red half"
    );
    assert!(
        is_green(center(&frames, 2, 64, 64)),
        "frame 2 shows the green half"
    );
    assert!(
        is_green(center(&frames, 3, 64, 64)),
        "frame 3 shows the green half"
    );
}

#[test]
fn nested_child_with_different_fps_maps_by_time() {
    let root = tmp("fps");
    write(
        &root.join("p"),
        "composition.kdl",
        &format!(
            "composition name=\"main\" root=true width=64 height=64 duration=2 fps=2 background=\"#000000\" version=1 {{\n\
             nest nest=\"bumper\" start=0 duration=2\n\
             }}\n\
             composition name=\"bumper\" width=64 height=64 duration=2 fps=30 background=\"#000000\" version=1 {{\n\
             box x=0 y=0 width=64 height=64 fill=\"#ff0000\" start=0 duration=1\n\
             box x=0 y=0 width=64 height=64 fill=\"#00ff00\" start=1 duration=1\n\
             }}\n"
        ),
    );
    let (code, out) = run(&["render", "p"], &root);
    assert_eq!(code, 0, "{out}");
    let frames = decode_video(&root.join("p").join("out.mp4"), 64, 64);
    assert!(
        is_red(center(&frames, 0, 64, 64)),
        "t=0 shows child red at 30fps"
    );
    assert!(
        is_green(center(&frames, 2, 64, 64)),
        "t=1 shows child green at 30fps"
    );
}

#[test]
fn per_use_trims_select_different_halves() {
    let root = tmp("trims");
    write(
        &root.join("p"),
        "composition.kdl",
        &main_with(
            "nest nest=\"bumper\" start=0 duration=1 trim-start=1 trim-end=2\n\
             nest nest=\"bumper\" start=1 duration=1 trim-start=0 trim-end=1",
            2,
        ),
    );
    let (code, out) = run(&["render", "p"], &root);
    assert_eq!(code, 0, "{out}");
    let frames = decode_video(&root.join("p").join("out.mp4"), 64, 64);
    assert!(
        is_green(center(&frames, 0, 64, 64)),
        "first use shows the green half"
    );
    assert!(
        is_green(center(&frames, 1, 64, 64)),
        "first use shows the green half"
    );
    assert!(
        is_red(center(&frames, 2, 64, 64)),
        "second use shows the red half"
    );
    assert!(
        is_red(center(&frames, 3, 64, 64)),
        "second use shows the red half"
    );
}

#[test]
fn rate_two_plays_the_child_at_double_speed() {
    let root = tmp("rate");
    write(
        &root.join("p"),
        "composition.kdl",
        &main_with("nest nest=\"bumper\" start=0 duration=2 rate=2", 2),
    );
    let (code, out) = run(&["render", "p"], &root);
    assert_eq!(code, 0, "{out}");
    let frames = decode_video(&root.join("p").join("out.mp4"), 64, 64);
    // Child second 2c at parent second c: t=0 red, t=0.5 green, then the
    // 1s of content holds green for the rest of the use.
    assert!(
        is_red(center(&frames, 0, 64, 64)),
        "t=0 shows child 0 (red)"
    );
    assert!(
        is_green(center(&frames, 1, 64, 64)),
        "t=0.5 shows child 1 (green)"
    );
    assert!(
        is_green(center(&frames, 2, 64, 64)),
        "t=1 holds the last state"
    );
    assert!(
        is_green(center(&frames, 3, 64, 64)),
        "t=1.5 holds the last state"
    );
}

#[test]
fn hold_last_picture_past_the_content() {
    let root = tmp("hold");
    write(
        &root.join("p"),
        "composition.kdl",
        &main_with("nest nest=\"bumper\" start=0 duration=4", 4),
    );
    let (code, out) = run(&["render", "p"], &root);
    assert_eq!(code, 0, "{out}");
    let frames = decode_video(&root.join("p").join("out.mp4"), 64, 64);
    assert_eq!(frames.len(), 8 * 64 * 64 * 3);
    for k in 2..8 {
        assert!(
            is_green(center(&frames, k, 64, 64)),
            "frame {k} holds green past 2s"
        );
    }
}

#[test]
fn nest_order_layers_the_use_as_one_unit() {
    let root = tmp("order");
    // The parent blue box paints at order 1; the nest at order 0 sits
    // below it, so the whole nested picture hides behind blue.
    write(
        &root.join("p"),
        "composition.kdl",
        &format!(
            "composition name=\"main\" root=true width=64 height=64 duration=1 fps=2 background=\"#000000\" version=1 {{\n\
             box x=0 y=0 width=64 height=64 fill=\"#0000ff\" start=0 duration=1 order=1\n\
             nest nest=\"bumper\" start=0 duration=1 order=0\n\
             }}\n{BUMPER}"
        ),
    );
    let (code, out) = run(&["render", "p"], &root);
    assert_eq!(code, 0, "{out}");
    let frames = decode_video(&root.join("p").join("out.mp4"), 64, 64);
    let (r, g, b) = center(&frames, 0, 64, 64);
    assert!(
        b > 180 && r < 100 && g < 100,
        "blue covers the nest: r={r} g={g} b={b}"
    );
}

// --- sound -------------------------------------------------------------------

#[test]
fn hold_region_is_silent_while_content_sounds() {
    let root = tmp("sound");
    let p = root.join("p");
    gen_tone(&p, 2);
    write(
        &p,
        "composition.kdl",
        &format!(
            "composition name=\"main\" root=true width=64 height=64 duration=4 fps=2 background=\"#000000\" version=1 {{\n\
             nest nest=\"bumper\" start=0 duration=4\n\
             }}\n\
             composition name=\"bumper\" width=64 height=64 duration=2 fps=2 background=\"#000000\" version=1 {{\n\
             box x=0 y=0 width=64 height=64 fill=\"#ff0000\" start=0 duration=2\n\
             audio path=\"tone.wav\" start=0 duration=2\n\
             }}\n"
        ),
    );
    let (code, out) = run(&["render", "p"], &root);
    assert_eq!(code, 0, "{out}");
    let pcm = decode_audio(&root.join("p").join("out.mp4"));
    // AAC priming/trim differs across FFmpeg versions (up to one
    // 1024-sample frame of overshoot on older builds).
    let samples = pcm.len() / 2;
    assert!(
        (4 * 48000..=4 * 48000 + 1024).contains(&samples),
        "full-duration mix, got {samples}"
    );
    let half = pcm.len() / 2;
    assert!(
        rms(&pcm[..half]) > 200.0,
        "content half sounds: rms={}",
        rms(&pcm[..half])
    );
    assert!(
        rms(&pcm[half..]) < 50.0,
        "hold half is silent: rms={}",
        rms(&pcm[half..])
    );
    // The held picture still shows the child's red box at t=3.
    let frames = decode_video(&root.join("p").join("out.mp4"), 64, 64);
    assert!(is_red(center(&frames, 6, 64, 64)), "t=3 holds the red box");
}

#[test]
fn nested_audio_follows_trim_and_rate() {
    let root = tmp("sound-trim-rate");
    let p = root.join("p");
    gen_tone(&p, 2);
    write(
        &p,
        "composition.kdl",
        &format!(
            "composition name=\"main\" root=true width=64 height=64 duration=2 fps=2 background=\"#000000\" version=1 {{\n\
             nest nest=\"bumper\" start=0 duration=2 trim-start=0.5 rate=2\n\
             }}\n\
             composition name=\"bumper\" width=64 height=64 duration=2 fps=2 background=\"#000000\" version=1 {{\n\
             box x=0 y=0 width=64 height=64 fill=\"#ff0000\" start=0 duration=2\n\
             audio path=\"tone.wav\" start=0 duration=2\n\
             }}\n"
        ),
    );
    let (code, out) = run(&["render", "p"], &root);
    assert_eq!(code, 0, "{out}");
    let pcm = decode_audio(&root.join("p").join("out.mp4"));
    // AAC priming/trim differs across FFmpeg versions (see above).
    let samples = pcm.len() / 2;
    assert!(
        (2 * 48000..=2 * 48000 + 1024).contains(&samples),
        "full-duration mix, got {samples}"
    );
    // Content is child [0.5, 2) at double speed: 0.75s of tone, then
    // silence for the rest of the 2s use.
    let tone_end = (0.75 * 48000.0) as usize * 2;
    assert!(rms(&pcm[..tone_end]) > 200.0, "trimmed content sounds");
    assert!(
        rms(&pcm[tone_end + 4800..]) < 50.0,
        "past the content is silent"
    );
}

// --- refusals ------------------------------------------------------------------

fn write_two_comp(root: &Path, main_nest: &str, bumper_body: &str) {
    write(
        &root.join("p"),
        "composition.kdl",
        &format!(
            "composition name=\"main\" root=true width=64 height=64 duration=2 background=\"#000\" version=1 {{\n\
             {main_nest}\n\
             }}\n\
             composition name=\"bumper\" width=64 height=64 duration=2 background=\"#000\" version=1 {{\n\
             {bumper_body}\n\
             }}\n"
        ),
    );
}

#[test]
fn circular_nest_refuses_check_and_render() {
    let root = tmp("refuse-cycle");
    write(
        &root.join("p"),
        "composition.kdl",
        "composition name=\"a\" root=true width=64 height=64 duration=2 background=\"#000\" version=1 {\n\
         nest nest=\"b\" start=0 duration=1\n\
         }\n\
         composition name=\"b\" width=64 height=64 duration=2 background=\"#000\" version=1 {\n\
         nest nest=\"a\" start=0 duration=1\n\
         }\n",
    );
    let (code, out) = run(&["check", "p"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_BAD_TIMING"), "{out}");
    assert!(out.contains("\"a\" -> \"b\" -> \"a\""), "{out}");
    let (code, out) = run(&["render", "p"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(
        !root.join("p").join("out.mp4").exists(),
        "no partial MP4: {out}"
    );
}

#[test]
fn deep_nest_refuses_naming_the_limit() {
    let root = tmp("refuse-depth");
    let mut parts = Vec::new();
    for i in 0..10 {
        let use_line = if i < 9 {
            format!("nest nest=\"c{}\" start=0 duration=1", i + 1)
        } else {
            "box x=0 y=0 width=1 height=1 fill=\"#fff\" start=0 duration=1".to_string()
        };
        let mark = if i == 0 { " root=true" } else { "" };
        parts.push(format!(
            "composition name=\"c{i}\"{mark} width=64 height=64 duration=2 background=\"#000\" version=1 {{\n{use_line}\n}}\n"
        ));
    }
    write(&root.join("p"), "composition.kdl", &parts.join(""));
    let (code, out) = run(&["check", "p"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_BAD_TIMING"), "{out}");
    assert!(out.contains("8"), "{out}");
    assert!(out.contains("\"c0\"") && out.contains("\"c9\""), "{out}");
    let (code, out) = run(&["render", "p"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(
        !root.join("p").join("out.mp4").exists(),
        "no partial MP4: {out}"
    );
}

#[test]
fn unbounded_and_unknown_nests_refuse_before_frame_one() {
    let root = tmp("refuse-unbounded");
    write_two_comp(
        &root,
        "nest nest=\"bumper\" start=0",
        "box x=0 y=0 width=1 height=1 fill=\"#fff\" start=0 duration=1",
    );
    let (code, out) = run(&["check", "p"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(
        out.contains("E_BAD_TIMING") && out.contains("unbounded"),
        "{out}"
    );
    let (code, _) = run(&["render", "p"], &root);
    assert_eq!(code, 1);
    assert!(!root.join("p").join("out.mp4").exists());

    let root = tmp("refuse-unknown");
    write_two_comp(
        &root,
        "nest nest=\"ghost\" start=0 duration=1",
        "box x=0 y=0 width=1 height=1 fill=\"#fff\" start=0 duration=1",
    );
    let (code, out) = run(&["check", "p"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_UNKNOWN_REF"), "{out}");
    let (code, _) = run(&["render", "p"], &root);
    assert_eq!(code, 1);
    assert!(!root.join("p").join("out.mp4").exists());
}

// --- determinism and selection ---------------------------------------------------

#[test]
fn nested_rerender_is_byte_identical() {
    let root = tmp("identical");
    write(
        &root.join("p"),
        "composition.kdl",
        &main_with("nest nest=\"bumper\" start=0 duration=2 rate=2", 2),
    );
    let (code, out) = run(&["render", "p"], &root);
    assert_eq!(code, 0, "{out}");
    let first = std::fs::read(root.join("p").join("out.mp4")).unwrap();
    let (code, out) = run(&["render", "p"], &root);
    assert_eq!(code, 0, "{out}");
    let second = std::fs::read(root.join("p").join("out.mp4")).unwrap();
    assert_eq!(first, second, "re-renders are byte-identical");
}

#[test]
fn named_render_of_the_child_ignores_the_use() {
    let root = tmp("select-child");
    write(
        &root.join("p"),
        "composition.kdl",
        &main_with("nest nest=\"bumper\" start=0 duration=2", 2),
    );
    let (code, out) = run(&["render", "--composition", "bumper", "p"], &root);
    assert_eq!(code, 0, "{out}");
    let frames = decode_video(&root.join("p").join("out.mp4"), 64, 64);
    assert_eq!(frames.len(), 4 * 64 * 64 * 3, "child renders its own 2s");
    assert!(is_red(center(&frames, 0, 64, 64)));
    assert!(is_green(center(&frames, 3, 64, 64)));
}
