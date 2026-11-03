//! Sprint 5 T003 (loop a clip or hold its last frame).
//!
//! A `video` clip with `loop=true` repeats its source window — picture
//! and sound together — for the whole clip window, seamlessly enough
//! that loop points show no gap or slip. A clip without loop whose
//! window outruns the source holds the last decoded frame as a still
//! while its sound goes silent for the remainder. A looping clip
//! without an explicit `duration` fails `check` asking for one, with
//! no MP4 written. Loop composes with trim and rate: the repeated unit
//! is the trimmed window played at the declared rate.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-footage-loop-{tag}-{}-{}",
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

/// Mux a testsrc picture with `tone` (PCM stereo, no codec padding) so
/// the video sound onset and loop repetition are exactly measurable.
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

fn decode_audio(mp4: &Path) -> Vec<i16> {
    let out = Command::new("ffmpeg")
        .args([
            "-y", "-v", "error", "-i", &mp4.display().to_string(), "-map", "0:a", "-f", "s16le",
            "-acodec", "pcm_s16le", "-ar", "48000", "-ac", "2", "-",
        ])
        .output()
        .expect("decode mp4 audio");
    assert!(out.status.success());
    let (pairs, _) = out.stdout.as_chunks::<2>();
    pairs.iter().map(|b| i16::from_le_bytes(*b)).collect()
}

fn decode_video(mp4: &Path, w: u32, h: u32) -> Vec<u8> {
    let out = Command::new("ffmpeg")
        .args([
            "-y", "-v", "error", "-i", &mp4.display().to_string(), "-map", "0:v", "-f", "rawvideo",
            "-pix_fmt", "rgb24", "-s", &format!("{w}x{h}"), "-",
        ])
        .output()
        .expect("decode mp4 video");
    assert!(out.status.success());
    out.stdout
}

/// Zero-crossing count of the left channel over `[from, to)` frames.
fn crossings(pcm: &[i16], from: usize, to: usize) -> usize {
    let to = to.min(pcm.len() / 2);
    (from.max(1)..to)
        .filter(|&j| (pcm[(j - 1) * 2] <= 0 && pcm[j * 2] > 0) || (pcm[(j - 1) * 2] >= 0 && pcm[j * 2] < 0))
        .count()
}

/// Mean absolute byte difference between two frames.
fn frame_mad(a: &[u8], b: &[u8]) -> f64 {
    assert_eq!(a.len(), b.len());
    let sum: u64 = a.iter().zip(b.iter()).map(|(&x, &y)| x.abs_diff(y) as u64).sum();
    sum as f64 / a.len() as f64
}

/// Mean absolute sample difference between two same-length slices.
fn pcm_mad(a: &[i16], b: &[i16]) -> f64 {
    assert_eq!(a.len(), b.len());
    let sum: u64 = a.iter().zip(b.iter()).map(|(&x, &y)| x.abs_diff(y) as u64).sum();
    sum as f64 / a.len() as f64
}

fn write(folder: &Path, name: &str, content: &str) {
    std::fs::write(folder.join(name), content).unwrap();
}

fn comp(duration: f64, clip_line: &str) -> String {
    format!(
        "composition name=\"t\" width=64 height=48 duration={duration} fps=30 background=\"#000000\" version=1 {{\n    {clip_line}\n}}\n"
    )
}

/// A looping clip repeats its window across at least two loop points:
/// every output third matches the source window frame by frame, and
/// every sound third repeats the window unit with the same pitch.
#[test]
fn loop_repeats_window_picture_and_sound_across_two_loop_points() {
    let folder = tmp("repeat");
    gen_tone(&folder.join("tone.wav"), 440, 2.0);
    gen_video_with_sound(&folder, "clip.mp4", &folder.join("tone.wav"), 2.0);
    // 1 s window (0..1) looped over a 3 s clip: loop points at t=1, 2.
    write(
        &folder,
        "composition.kdl",
        &comp(3.0, "video path=\"clip.mp4\" start=0 duration=3 trim-start=0 trim-end=1 loop=true"),
    );
    let (code, out) = run(&["check", "."], &folder);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("loop on"), "{out}");
    let (code, out) = run(&["render", ".", "-o", "out.mp4"], &folder);
    assert_eq!(code, 0, "{out}");
    let (w, h) = (64u32, 48u32);
    let rendered = decode_video(&folder.join("out.mp4"), w, h);
    let source = decode_video(&folder.join("clip.mp4"), w, h);
    let fs = (w * h * 3) as usize;
    assert_eq!(rendered.len(), 90 * fs, "3 s at 30 fps");
    // Picture: each third replays source frames 0..30, including the
    // frames exactly on the loop points.
    for k in [0usize, 7, 29, 30, 45, 59, 60, 75, 89] {
        let got = &rendered[k * fs..(k + 1) * fs];
        let want = &source[(k % 30) * fs..(k % 30 + 1) * fs];
        let mad = frame_mad(got, want);
        println!("looped frame {k}: window-mad={mad:.2}");
        assert!(mad < 15.0, "frame {k} must repeat the window, mad={mad:.2}");
    }
    // Sound: three 1 s thirds, each the same 440 Hz unit.
    let pcm = decode_audio(&folder.join("out.mp4"));
    assert!(pcm.len() >= 3 * 48000 * 2, "at least 3 s of stereo mix, got {}", pcm.len() / 2);
    let pcm = &pcm[..3 * 48000 * 2];
    let thirds: Vec<&[i16]> =
        vec![&pcm[0..48000 * 2], &pcm[48000 * 2..96000 * 2], &pcm[96000 * 2..144000 * 2]];
    let xa: Vec<usize> =
        vec![crossings(&pcm, 1000, 47000), crossings(&pcm, 49000, 95000), crossings(&pcm, 97000, 143000)];
    println!("looped third crossings: {xa:?}");
    for (i, &x) in xa.iter().enumerate() {
        assert!(x > 700 && x < 1100, "third {i} must carry the 440 Hz unit, crossings={x}");
    }
    let rep01 = pcm_mad(thirds[0], thirds[1]);
    let rep12 = pcm_mad(thirds[1], thirds[2]);
    println!("looped sound repetition mad: {rep01:.1} / {rep12:.1}");
    assert!(rep01 < 600.0, "second pass must repeat the first, mad={rep01:.1}");
    assert!(rep12 < 600.0, "third pass must repeat the second, mad={rep12:.1}");
}

/// A non-looping overrun holds the last frame while its sound goes
/// silent: the tail pictures match the last source frame and the tail
/// mix measures digital silence.
#[test]
fn non_loop_overrun_holds_picture_and_silences_sound() {
    let folder = tmp("hold");
    gen_tone(&folder.join("tone.wav"), 440, 2.0);
    gen_video_with_sound(&folder, "clip.mp4", &folder.join("tone.wav"), 2.0);
    // Whole 2 s source in a 4 s clip: 2 s of play, 2 s of hold.
    write(&folder, "composition.kdl", &comp(4.0, "video path=\"clip.mp4\" start=0 duration=4"));
    let (code, out) = run(&["check", "."], &folder);
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(&["render", ".", "-o", "out.mp4"], &folder);
    assert_eq!(code, 0, "{out}");
    let (w, h) = (64u32, 48u32);
    let rendered = decode_video(&folder.join("out.mp4"), w, h);
    let source = decode_video(&folder.join("clip.mp4"), w, h);
    let fs = (w * h * 3) as usize;
    assert_eq!(rendered.len(), 120 * fs, "4 s at 30 fps");
    for k in [60usize, 90, 119] {
        let got = &rendered[k * fs..(k + 1) * fs];
        let held = frame_mad(got, &source[59 * fs..60 * fs]);
        println!("hold frame {k}: held-mad={held:.2}");
        assert!(held < 15.0, "frame {k} must hold the last frame, mad={held:.2}");
    }
    let pcm = decode_audio(&folder.join("out.mp4"));
    assert!(pcm.len() >= 4 * 48000 * 2, "at least 4 s of stereo mix, got {}", pcm.len() / 2);
    let pcm = &pcm[..4 * 48000 * 2];
    let head_x = crossings(&pcm, 1000, 95000);
    println!("head crossings: {head_x}");
    assert!(head_x > 1400 && head_x < 2100, "head must carry the 440 Hz tone, crossings={head_x}");
    let tail = &pcm[2 * 48000 * 2..];
    let peak = tail.iter().map(|s| s.abs()).max().unwrap_or(0);
    let mean: f64 =
        tail.iter().map(|s| s.abs() as u64).sum::<u64>() as f64 / tail.len() as f64;
    println!("silent tail: peak={peak} mean={mean:.2}");
    assert!(peak < 1000, "tail sound must be silent, peak={peak}");
    assert!(mean < 50.0, "tail sound must be silent, mean={mean:.2}");
}

/// A looping clip without `duration` fails `check` asking for an
/// explicit `duration`, and neither `check` nor `render` writes an MP4.
#[test]
fn loop_without_duration_fails_asking_for_duration() {
    let folder = tmp("unbounded");
    gen_tone(&folder.join("tone.wav"), 440, 2.0);
    gen_video_with_sound(&folder, "clip.mp4", &folder.join("tone.wav"), 2.0);
    write(
        &folder,
        "composition.kdl",
        &comp(2.0, "video path=\"clip.mp4\" start=0 loop=true"),
    );
    let (code, out) = run(&["check", "."], &folder);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_BAD_FOOTAGE"), "{out}");
    assert!(out.contains("duration"), "{out}");
    assert!(!folder.join("out.mp4").exists(), "check writes no MP4");
    let (code, out) = run(&["render", ".", "-o", "out.mp4"], &folder);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("duration"), "{out}");
    assert!(!folder.join("out.mp4").exists(), "no MP4 survives the refusal");
    assert!(!folder.join("out.mp4.framesmith-part").exists(), "no partial survives the refusal");
}

/// Loop composes with trim and rate: the repeated unit is the trimmed
/// window played at the declared rate — a 1..3 s window at rate 2 is a
/// 1 s unit repeating every composition second, picture and sound.
#[test]
fn loop_composes_with_trim_and_rate() {
    let folder = tmp("unit");
    gen_tone(&folder.join("tone.wav"), 440, 4.0);
    gen_video_with_sound(&folder, "clip.mp4", &folder.join("tone.wav"), 4.0);
    write(
        &folder,
        "composition.kdl",
        &comp(3.0, "video path=\"clip.mp4\" start=0 duration=3 trim-start=1 trim-end=3 rate=2 loop=true"),
    );
    let (code, out) = run(&["check", "."], &folder);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("1.000s..3.000s"), "{out}");
    let (code, out) = run(&["render", ".", "-o", "out.mp4"], &folder);
    assert_eq!(code, 0, "{out}");
    let (w, h) = (64u32, 48u32);
    let rendered = decode_video(&folder.join("out.mp4"), w, h);
    let source = decode_video(&folder.join("clip.mp4"), w, h);
    let fs = (w * h * 3) as usize;
    assert_eq!(rendered.len(), 90 * fs, "3 s at 30 fps");
    // Unit frame k maps to source frame 30 + ((2k) mod 60): frame 0 and
    // frame 30 (the first loop point) both show source frame 30, frame
    // 15 shows source frame 60.
    for (k, src) in [(0usize, 30usize), (15, 60), (30, 30), (45, 60), (60, 30), (75, 60)] {
        let got = &rendered[k * fs..(k + 1) * fs];
        let mad = frame_mad(got, &source[src * fs..(src + 1) * fs]);
        println!("looped unit frame {k}: src={src} mad={mad:.2}");
        assert!(mad < 15.0, "frame {k} must show unit frame {src}, mad={mad:.2}");
    }
    // Sound: three 1 s thirds at ~880 Hz (440 Hz played double speed).
    let pcm = decode_audio(&folder.join("out.mp4"));
    assert!(pcm.len() >= 3 * 48000 * 2, "at least 3 s of stereo mix, got {}", pcm.len() / 2);
    let pcm = &pcm[..3 * 48000 * 2];
    let xa: Vec<usize> =
        vec![crossings(&pcm, 1000, 47000), crossings(&pcm, 49000, 95000), crossings(&pcm, 97000, 143000)];
    println!("pitched loop crossings: {xa:?}");
    for (i, &x) in xa.iter().enumerate() {
        assert!(x > 1400 && x < 2200, "third {i} must carry the doubled unit, crossings={x}");
    }
}
