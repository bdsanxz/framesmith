//! Sprint 5 T002 (watch a trimmed, re-timed clip with aligned sound).
//!
//! A `video` clip with `trim-start`/`trim-end` and `rate` renders an MP4
//! whose picture shows exactly the source window at the declared rate
//! and whose sound starts within 1 ms of its scaled written time,
//! plainly resampled so pitch follows speed. A past-end `trim-end`
//! clamps and is reported; a source that ends before the window needs
//! refuses before frame one naming the file. Renders work offline
//! (`FRAMESMITH_OFFLINE=1`), re-render byte-identically, and carry a
//! widely playable container (H.264 yuv420p + AAC stereo, faststart).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-footage-render-{tag}-{}-{}",
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
    run_env(args, cwd, &[])
}

fn run_env(args: &[&str], cwd: &Path, envs: &[(&str, &str)]) -> (i32, String) {
    let out = Command::new(bin())
        .args(args)
        .envs(envs.iter().copied())
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

fn gen_clip(dir: &Path, name: &str, seconds: f64) {
    ffmpeg(&[
        "-f",
        "lavfi",
        "-i",
        &format!("testsrc=duration={seconds}:size=64x48:rate=30"),
        "-c:v",
        "libx264",
        "-pix_fmt",
        "yuv420p",
        &dir.join(name).display().to_string(),
    ]);
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

/// Mux a silent testsrc picture with `tone` (PCM stereo, no codec
/// padding) so the video sound onset is exactly measurable.
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
        .expect("decode mp4 video");
    assert!(out.status.success());
    out.stdout
}

fn ffprobe(mp4: &Path, entries: &str) -> String {
    let out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            entries,
            "-of",
            "default=noprint_wrappers=1",
            &mp4.display().to_string(),
        ])
        .output()
        .expect("run host ffprobe");
    assert!(out.status.success());
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn onset_frame(pcm: &[i16], threshold: i16) -> Option<usize> {
    (0..pcm.len() / 2).find(|&i| pcm[2 * i].abs() > threshold || pcm[2 * i + 1].abs() > threshold)
}

/// Zero-crossing count of the left channel over `[from, to)` frames.
fn crossings(pcm: &[i16], from: usize, to: usize) -> usize {
    let to = to.min(pcm.len() / 2);
    (from.max(1)..to)
        .filter(|&j| {
            (pcm[(j - 1) * 2] <= 0 && pcm[j * 2] > 0) || (pcm[(j - 1) * 2] >= 0 && pcm[j * 2] < 0)
        })
        .count()
}

/// Mean absolute byte difference between two frames.
fn frame_mad(a: &[u8], b: &[u8]) -> f64 {
    assert_eq!(a.len(), b.len());
    let sum: u64 = a
        .iter()
        .zip(b.iter())
        .map(|(&x, &y)| x.abs_diff(y) as u64)
        .sum();
    sum as f64 / a.len() as f64
}

/// Top-level MP4 box offsets: `(moov_at, mdat_at)` byte positions.
/// Faststart (streaming/playback-friendly) means `moov` before `mdat`.
fn moov_mdat_order(mp4: &Path) -> (Option<u64>, Option<u64>) {
    let bytes = std::fs::read(mp4).expect("read mp4");
    let mut moov = None;
    let mut mdat = None;
    let mut off = 0usize;
    while off + 8 <= bytes.len() {
        let mut size = u32::from_be_bytes(bytes[off..off + 4].try_into().unwrap()) as u64;
        let kind = &bytes[off + 4..off + 8];
        let mut head = 8u64;
        if size == 1 {
            if off + 16 > bytes.len() {
                break;
            }
            size = u64::from_be_bytes(bytes[off + 8..off + 16].try_into().unwrap());
            head = 16;
        } else if size == 0 {
            size = (bytes.len() - off) as u64;
        }
        if kind == b"moov" && moov.is_none() {
            moov = Some(off as u64);
        }
        if kind == b"mdat" && mdat.is_none() {
            mdat = Some(off as u64);
        }
        if size < head {
            break;
        }
        off += size as usize;
    }
    (moov, mdat)
}

fn write(folder: &Path, name: &str, content: &str) {
    std::fs::write(folder.join(name), content).unwrap();
}

fn comp(duration: f64, clip_line: &str) -> String {
    format!(
        "composition name=\"t\" width=64 height=48 duration={duration} fps=30 background=\"#000000\" version=1 {{\n    {clip_line}\n}}\n"
    )
}

/// A trimmed clip renders the source window frame by frame: output
/// frame `k` matches source frame `trim-start * fps + k` far better
/// than the untrimmed frame `k`, and within a tight lossy-codec bound.
#[test]
fn trimmed_clip_pictures_match_the_source_window() {
    let folder = tmp("trim-pic");
    gen_clip(&folder, "clip.mp4", 4.0);
    write(
        &folder,
        "composition.kdl",
        &comp(
            2.0,
            "video path=\"clip.mp4\" start=0 duration=2 trim-start=1 trim-end=3",
        ),
    );
    let (code, out) = run(&["render", ".", "-o", "out.mp4"], &folder);
    assert_eq!(code, 0, "{out}");
    let (w, h) = (64u32, 48u32);
    let rendered = decode_video(&folder.join("out.mp4"), w, h);
    let source = decode_video(&folder.join("clip.mp4"), w, h);
    let fs = (w * h * 3) as usize;
    assert_eq!(rendered.len(), 60 * fs, "2 s at 30 fps");
    assert_eq!(source.len(), 120 * fs, "4 s at 30 fps");
    let mut worst = 0.0f64;
    for k in [0usize, 7, 15, 29, 44, 59] {
        let got = &rendered[k * fs..(k + 1) * fs];
        let want = &source[(30 + k) * fs..(30 + k + 1) * fs];
        let wrong = &source[k * fs..(k + 1) * fs];
        let good = frame_mad(got, want);
        let bad = frame_mad(got, wrong);
        println!("frame {k}: mapped={good:.2} untrimmed={bad:.2}");
        assert!(
            good < 15.0,
            "frame {k} must match its window frame, mad={good:.2}"
        );
        assert!(good < bad * 0.5, "frame {k} must match the window better than the untrimmed frame ({good:.2} vs {bad:.2})");
        worst = worst.max(good);
    }
    println!("worst mapped mad: {worst:.2}");
}

/// A re-timed clip advances `rate` source seconds per composition
/// second: at rate 2, output frame `k` matches source frame
/// `(trim-start + 2k/fps) * fps`, and the MP4 lasts exactly the
/// window divided by the rate.
#[test]
fn retimed_clip_pictures_follow_rate() {
    let folder = tmp("rate-pic");
    gen_clip(&folder, "clip.mp4", 4.0);
    write(
        &folder,
        "composition.kdl",
        &comp(
            1.0,
            "video path=\"clip.mp4\" start=0 duration=1 trim-start=1 trim-end=3 rate=2",
        ),
    );
    let (code, out) = run(&["render", ".", "-o", "out.mp4"], &folder);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("rate 2x"), "{out}");
    let (w, h) = (64u32, 48u32);
    let rendered = decode_video(&folder.join("out.mp4"), w, h);
    let source = decode_video(&folder.join("clip.mp4"), w, h);
    let fs = (w * h * 3) as usize;
    assert_eq!(rendered.len(), 30 * fs, "1 s at 30 fps");
    for k in [0usize, 5, 14, 29] {
        let src_idx = 30 + 2 * k;
        let got = &rendered[k * fs..(k + 1) * fs];
        let want = &source[src_idx * fs..(src_idx + 1) * fs];
        let good = frame_mad(got, want);
        println!("frame {k}: source frame {src_idx}, mad={good:.2}");
        assert!(
            good < 15.0,
            "frame {k} must match source frame {src_idx}, mad={good:.2}"
        );
    }
    let dur = ffprobe(&folder.join("out.mp4"), "format=duration");
    let secs: f64 = dur.lines().find(|l| l.starts_with("duration=")).unwrap()["duration=".len()..]
        .parse()
        .unwrap();
    assert!(
        (secs - 1.0).abs() < 0.1,
        "output lasts the window/rate (1 s), got {secs}"
    );
}

/// Clip sound starts within 1 ms of its scaled written time and stays
/// aligned through the clip, plainly resampled so pitch follows speed:
/// a 440 Hz source at rate 2 measures ~880 Hz over the clip window.
#[test]
fn trimmed_sound_aligns_and_pitch_follows_rate() {
    let folder = tmp("sound");
    gen_tone(&folder.join("tone.wav"), 440, 4.0);
    gen_video_with_sound(&folder, "vsound.mp4", &folder.join("tone.wav"), 4.0);
    write(
        &folder,
        "composition.kdl",
        &comp(
            2.0,
            "video path=\"vsound.mp4\" start=0.5 duration=1 trim-start=1 trim-end=3 rate=2",
        ),
    );
    let (code, out) = run(&["check", "."], &folder);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("1.000s..3.000s"), "{out}");
    let (code, out) = run(&["render", ".", "-o", "out.mp4"], &folder);
    assert_eq!(code, 0, "{out}");
    let pcm = decode_audio(&folder.join("out.mp4"));
    assert_eq!(pcm.len() / 2, 96000, "full-duration mix");
    let onset = onset_frame(&pcm, 500).expect("clip sound must be audible");
    println!("sound onset: frame {onset}, written 24000");
    assert!(
        onset.abs_diff(24000) <= 48,
        "sound {onset} misses its scaled written time 24000 by more than 1 ms"
    );
    assert!(
        pcm[..2 * 23000].iter().all(|&x| x == 0),
        "silence before the clip starts"
    );
    // The 1 s window carries ~880 Hz (440 Hz source at rate 2): ~1760
    // zero crossings, present in both halves (no mid-clip dropout).
    let early = crossings(&pcm, 24000, 48000);
    let late = crossings(&pcm, 48000, 72000);
    println!("crossings: early={early} late={late}");
    assert!(
        (early as i32 - 880).abs() <= 90,
        "first half carries doubled pitch, got {early}"
    );
    assert!(
        (late as i32 - 880).abs() <= 90,
        "second half stays aligned, got {late}"
    );
}

/// A unity-rate trimmed clip keeps its source pitch: the same window
/// at rate 1 measures ~440 Hz.
#[test]
fn unity_rate_trim_keeps_source_pitch() {
    let folder = tmp("unity-pitch");
    gen_tone(&folder.join("tone.wav"), 440, 4.0);
    gen_video_with_sound(&folder, "vsound.mp4", &folder.join("tone.wav"), 4.0);
    write(
        &folder,
        "composition.kdl",
        &comp(
            2.0,
            "video path=\"vsound.mp4\" start=0.5 duration=1 trim-start=1 trim-end=2",
        ),
    );
    let (code, out) = run(&["render", ".", "-o", "out.mp4"], &folder);
    assert_eq!(code, 0, "{out}");
    let pcm = decode_audio(&folder.join("out.mp4"));
    let onset = onset_frame(&pcm, 500).expect("clip sound must be audible");
    assert!(
        onset.abs_diff(24000) <= 48,
        "sound {onset} misses 24000 by more than 1 ms"
    );
    let mid = crossings(&pcm, 24000, 72000);
    println!("unity crossings over 1 s: {mid}");
    assert!(
        (mid as i32 - 880).abs() <= 90,
        "unity trim keeps 440 Hz (~880 crossings/s), got {mid}"
    );
}

/// A `trim-end` past the source end renders up to the true end: check
/// stays green and reports the clamp, and the render summary repeats it.
#[test]
fn past_end_trim_end_renders_to_the_true_end() {
    let folder = tmp("clamp-render");
    gen_clip(&folder, "clip.mp4", 4.0);
    // The window opens at 1 s and the past-end trim-end clamps to the
    // true 4 s end, so a 3 s clip plays the source right up to its end.
    write(
        &folder,
        "composition.kdl",
        &comp(
            3.0,
            "video path=\"clip.mp4\" start=0 duration=3 trim-start=1 trim-end=9",
        ),
    );
    let (code, out) = run(&["check", "."], &folder);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("clamped to"), "{out}");
    let (code, out) = run(&["render", ".", "-o", "out.mp4"], &folder);
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains("clamped to"),
        "render summary repeats the clamp: {out}"
    );
    let (w, h) = (64u32, 48u32);
    let rendered = decode_video(&folder.join("out.mp4"), w, h);
    let source = decode_video(&folder.join("clip.mp4"), w, h);
    let fs = (w * h * 3) as usize;
    assert_eq!(rendered.len(), 90 * fs, "3 s at 30 fps");
    // The last rendered frame comes from the true source end
    // (src 1 + 89/30 = 3.967 s, frame ~119), not from a held picture.
    let tail = &rendered[89 * fs..90 * fs];
    let near_end = frame_mad(tail, &source[119 * fs..120 * fs]);
    let early = frame_mad(tail, &source[30 * fs..31 * fs]);
    println!("tail: near-end={near_end:.2} early={early:.2}");
    assert!(
        near_end < 15.0,
        "tail must match the true end, mad={near_end:.2}"
    );
    assert!(
        near_end < early * 0.5,
        "tail must come from the true end ({near_end:.2} vs {early:.2})"
    );
}

/// Sprint 5 T003 supersedes the old overrun refusal: a non-looping clip
/// whose window needs more source than the probe reports holds the last
/// decoded frame as a still for the remainder — never a guessed
/// picture, never a refusal.
#[test]
fn overrun_holds_the_last_frame() {
    let folder = tmp("short");
    gen_clip(&folder, "clip.mp4", 2.0);
    write(
        &folder,
        "composition.kdl",
        &comp(
            2.0,
            "video path=\"clip.mp4\" start=0 duration=2 trim-start=1",
        ),
    );
    let (code, out) = run(&["check", "."], &folder);
    assert_eq!(
        code, 0,
        "window 1..2 is valid, check echoes the 1 s effective length: {out}"
    );
    assert!(out.contains("effective length 1.000s"), "{out}");
    let (code, out) = run(&["render", ".", "-o", "out.mp4"], &folder);
    assert_eq!(code, 0, "{out}");
    let (w, h) = (64u32, 48u32);
    let rendered = decode_video(&folder.join("out.mp4"), w, h);
    let source = decode_video(&folder.join("clip.mp4"), w, h);
    let fs = (w * h * 3) as usize;
    assert_eq!(rendered.len(), 60 * fs, "2 s at 30 fps");
    assert_eq!(source.len(), 60 * fs, "2 s source at 30 fps");
    // The first second plays the window (output k matches source 30+k).
    let first = &rendered[0 * fs..1 * fs];
    let first_mad = frame_mad(first, &source[30 * fs..31 * fs]);
    println!("first frame mapped mad: {first_mad:.2}");
    assert!(
        first_mad < 15.0,
        "first frame must play the window, mad={first_mad:.2}"
    );
    // The overrun second holds the last source frame (index 59).
    for k in [30usize, 45, 59] {
        let got = &rendered[k * fs..(k + 1) * fs];
        let held = frame_mad(got, &source[59 * fs..60 * fs]);
        let wrong = frame_mad(got, &source[30 * fs..31 * fs]);
        println!("held frame {k}: held={held:.2} window-start={wrong:.2}");
        assert!(
            held < 15.0,
            "frame {k} must hold the last frame, mad={held:.2}"
        );
        assert!(
            held < wrong * 0.5,
            "frame {k} must hold, not replay ({held:.2} vs {wrong:.2})"
        );
    }
}

/// Rendering works with networking disabled, re-renders byte-identically,
/// and emits a widely playable MP4 (H.264 yuv420p + AAC stereo, faststart).
#[test]
fn offline_render_is_deterministic_and_playable() {
    let folder = tmp("offline");
    gen_clip(&folder, "clip.mp4", 4.0);
    // Rate 2 over the 1..3 s window renders exactly 1 s.
    write(
        &folder,
        "composition.kdl",
        &comp(
            1.0,
            "video path=\"clip.mp4\" start=0 duration=1 trim-start=1 trim-end=3 rate=2",
        ),
    );
    let offline = [("FRAMESMITH_OFFLINE", "1")];
    let (code, out) = run_env(&["render", ".", "-o", "out.mp4"], &folder, &offline);
    assert_eq!(code, 0, "offline render: {out}");
    let first = std::fs::read(folder.join("out.mp4")).unwrap();
    let (code, out) = run_env(&["render", ".", "-o", "out2.mp4"], &folder, &offline);
    assert_eq!(code, 0, "offline re-render: {out}");
    let second = std::fs::read(folder.join("out2.mp4")).unwrap();
    assert_eq!(first, second, "re-renders are byte-identical");
    println!("deterministic bytes: {}", first.len());
    // Playback suitability: H.264 yuv420p video plus AAC stereo audio
    // at 48 kHz, composition length, moov before mdat (faststart).
    let streams = ffprobe(
        &folder.join("out.mp4"),
        "stream=codec_name,pix_fmt,sample_rate,channels",
    );
    println!("{streams}");
    assert!(streams.contains("codec_name=h264"), "{streams}");
    assert!(streams.contains("pix_fmt=yuv420p"), "{streams}");
    assert!(streams.contains("codec_name=aac"), "{streams}");
    assert!(streams.contains("sample_rate=48000"), "{streams}");
    let dur = ffprobe(&folder.join("out.mp4"), "format=duration");
    let secs: f64 = dur.lines().find(|l| l.starts_with("duration=")).unwrap()["duration=".len()..]
        .parse()
        .unwrap();
    assert!((secs - 1.0).abs() < 0.1, "output lasts 1 s, got {secs}");
    let (moov, mdat) = moov_mdat_order(&folder.join("out.mp4"));
    println!("moov={moov:?} mdat={mdat:?}");
    assert!(moov.is_some() && mdat.is_some(), "both boxes present");
    assert!(moov.unwrap() < mdat.unwrap(), "faststart: moov before mdat");
}
