//! Sprint 3 T010 (video sound through the 48 kHz stereo mix).
//!
//! A `video` clip with a sound stream plays through the same mix as
//! `audio` tracks, with the clip's volume and fades applied,
//! sample-aligned with its picture. Without declared keys the sound
//! mixes at unity gain; soundless video renders normally; an
//! undecodable stream refuses before the first frame naming the file.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-vsound-{tag}-{}-{}",
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

fn ffprobe(mp4: &Path, entries: &[&str]) -> String {
    let out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            &entries.join(":"),
            "-of",
            "default=noprint_wrappers=1",
        ])
        .arg(mp4)
        .output()
        .expect("run host ffprobe");
    assert!(out.status.success());
    String::from_utf8_lossy(&out.stdout).into_owned()
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

fn write(folder: &Path, name: &str, content: &str) {
    std::fs::write(folder.join(name), content).unwrap();
}

fn video_comp(volume_keys: &str) -> String {
    format!(
        "composition name=\"t\" width=64 height=48 duration=2 fps=30 background=\"#000000\" version=1 {{\n    video path=\"vsound.mp4\" start=0.5 duration=1.0{volume_keys}\n}}\n"
    )
}

// --- T010: unity default + sample alignment ----------------------------------

#[test]
fn video_sound_mixes_at_unity_by_default_and_aligns_with_picture() {
    let root = tmp("unity");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    gen_tone(&folder.join("tone.wav"), 440, 2.0);
    gen_video_with_sound(&folder, "vsound.mp4", &folder.join("tone.wav"), 2.0);
    write(&folder, "composition.kdl", &video_comp(""));
    let (code, out) = run(&["render", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("stage audio: ok"), "{out}");
    let pcm = decode_audio(&folder.join("out.mp4"));
    assert_eq!(pcm.len() / 2, 96000, "full-duration mix");
    let onset = onset_frame(&pcm, 500).expect("video sound must be audible");
    println!("video onset: frame {onset}, written 24000");
    assert!(onset.abs_diff(24000) <= 48, "video sound {onset} misses picture start 24000");
    assert!(pcm[..2 * 23000].iter().all(|&x| x == 0), "silence before the picture starts");
}

// --- T010: volume and fades ----------------------------------------------------

#[test]
fn video_volume_and_fades_shape_the_mix() {
    let root = tmp("shape");
    std::fs::create_dir_all(root.join("src")).unwrap();
    gen_tone(&root.join("src").join("tone.wav"), 440, 2.0);
    gen_video_with_sound(&root.join("src"), "vsound.mp4", &root.join("src").join("tone.wav"), 2.0);
    let render = |tag: &str, keys: &str| -> Vec<i16> {
        let dir = root.join(tag);
        std::fs::create_dir_all(&dir).unwrap();
        for f in ["tone.wav", "vsound.mp4"] {
            std::fs::copy(root.join("src").join(f), dir.join(f)).unwrap();
        }
        write(&dir, "composition.kdl", &video_comp(keys));
        let (code, out) = run(&["render", ".", "-o", "out.mp4"], &dir);
        assert_eq!(code, 0, "{tag}: {out}");
        decode_audio(&dir.join("out.mp4"))
    };
    // Unity default matches an explicit `volume=0`.
    let implicit = render("implicit", "");
    let explicit = render("explicit", " volume=0");
    let ri = rms(&implicit, 26000, 68000);
    let re = rms(&explicit, 26000, 68000);
    println!("unity rms: implicit={ri:.1} explicit={re:.1}");
    assert!((ri / re.max(1.0) - 1.0).abs() < 0.05, "unity default must equal volume=0");
    // -6 dB halves the amplitude of 0 dB.
    let quiet = render("quiet", " volume=-6");
    let rq = rms(&quiet, 26000, 68000);
    println!("volume rms: 0dB={re:.1} -6dB={rq:.1}");
    assert!((rq / re.max(1.0) - 0.501).abs() < 0.1, "-6 dB must halve amplitude");
    // A fade-in over the clip ramps from near silence to full gain.
    let faded = render("faded", " fade-in=0.5");
    let early = rms(&faded, 24100, 26000);
    let late = rms(&faded, 60000, 68000);
    println!("video fade rms: early={early:.1} late={late:.1}");
    assert!(early < late * 0.35, "video fade-in must ramp from near silence");
    assert!((late / re.max(1.0) - 1.0).abs() < 0.25, "video fade-in reaches full gain");
    // A fade-out over the clip falls back toward silence.
    let faded_out = render("fadedout", " fade-out=0.5");
    let tail = rms(&faded_out, 68000, 71500);
    let mid = rms(&faded_out, 30000, 40000);
    println!("video fade-out rms: mid={mid:.1} tail={tail:.1}");
    assert!(tail < mid * 0.35, "video fade-out must fall toward silence");
}

// --- T010: mixing beside audio tracks ------------------------------------------

#[test]
fn video_sound_mixes_with_audio_tracks_and_sums() {
    let root = tmp("sum");
    std::fs::create_dir_all(root.join("src")).unwrap();
    gen_tone(&root.join("src").join("v440.wav"), 440, 2.0);
    gen_tone(&root.join("src").join("a660.wav"), 660, 2.0);
    gen_video_with_sound(&root.join("src"), "vsound.mp4", &root.join("src").join("v440.wav"), 2.0);
    let render = |tag: &str, body: &str| -> Vec<i16> {
        let dir = root.join(tag);
        std::fs::create_dir_all(&dir).unwrap();
        for f in ["vsound.mp4", "a660.wav"] {
            std::fs::copy(root.join("src").join(f), dir.join(f)).unwrap();
        }
        write(
            &dir,
            "composition.kdl",
            &format!(
                "composition name=\"t\" width=64 height=48 duration=2 fps=30 background=\"#000000\" version=1 {{\n{body}\n}}\n"
            ),
        );
        let (code, out) = run(&["render", ".", "-o", "out.mp4"], &dir);
        assert_eq!(code, 0, "{tag}: {out}");
        decode_audio(&dir.join("out.mp4"))
    };
    let only_video = render(
        "only-video",
        "    video path=\"vsound.mp4\" start=0.5 duration=1.0",
    );
    let only_track = render(
        "only-track",
        "    audio path=\"a660.wav\" start=0.5 duration=1.0",
    );
    let both = render(
        "both",
        "    video path=\"vsound.mp4\" start=0.5 duration=1.0\n    audio path=\"a660.wav\" start=0.5 duration=1.0",
    );
    // Uncorrelated tones add in energy: |both|^2 ~= |video|^2 + |track|^2.
    let (rv, ra, rb) = (rms(&only_video, 26000, 68000), rms(&only_track, 26000, 68000), rms(&both, 26000, 68000));
    println!("sum rms: video={rv:.1} track={ra:.1} both={rb:.1}");
    let expect = (rv * rv + ra * ra).sqrt();
    assert!((rb / expect - 1.0).abs() < 0.2, "sounds must sum: {rb} vs {expect}");
}

// --- T010: silent video ---------------------------------------------------------

#[test]
fn soundless_video_renders_normally_with_silent_track() {
    let root = tmp("silent");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    ffmpeg(&[
        "-f",
        "lavfi",
        "-i",
        "testsrc=duration=2:size=64x48:rate=30",
        "-c:v",
        "libx264",
        "-pix_fmt",
        "yuv420p",
        "-an",
        &folder.join("quiet.mp4").display().to_string(),
    ]);
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=64 height=48 duration=2 fps=30 background=\"#000000\" version=1 {\n    video path=\"quiet.mp4\" start=0 duration=2\n}\n",
    );
    let (code, out) = run(&["render", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    let probe = ffprobe(
        &folder.join("out.mp4"),
        &["stream=codec_name", "stream=sample_rate", "stream=channels"],
    );
    for want in ["codec_name=h264", "codec_name=aac", "sample_rate=48000", "channels=2"] {
        assert!(probe.contains(want), "{probe}");
    }
    let pcm = decode_audio(&folder.join("out.mp4"));
    assert!(pcm.iter().all(|&x| x == 0), "no sound stream means a silent track");
}

// --- T010: undecodable stream ----------------------------------------------------

#[test]
fn undecodable_video_sound_refuses_before_the_first_frame() {
    let root = tmp("broken");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    // Valid picture, corrupt audio packets: the picture probe passes but
    // the audio stream cannot be decoded.
    ffmpeg(&[
        "-f",
        "lavfi",
        "-i",
        "sine=frequency=440:duration=1",
        "-ar",
        "48000",
        "-ac",
        "2",
        "-codec:a",
        "libmp3lame",
        &folder.join("ok.mp3").display().to_string(),
    ]);
    let raw = std::fs::read(folder.join("ok.mp3")).unwrap();
    let mut bad = raw.clone();
    let mut seed: u64 = 0x12345678;
    let mut next = || {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (seed >> 33) as u8
    };
    for b in bad.iter_mut().skip(raw.len() / 3).take(raw.len() / 3) {
        *b = next();
    }
    std::fs::write(folder.join("bad.mp3"), &bad).unwrap();
    ffmpeg(&[
        "-f",
        "lavfi",
        "-i",
        "testsrc=duration=1:size=64x48:rate=30",
        "-i",
        &folder.join("bad.mp3").display().to_string(),
        "-c:v",
        "libx264",
        "-pix_fmt",
        "yuv420p",
        "-c:a",
        "copy",
        "-shortest",
        &folder.join("broken_av.mp4").display().to_string(),
    ]);
    // The window fits the probed picture duration, so the failure is
    // the undecodable audio stream itself (not a too-short picture).
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=64 height=48 duration=1 fps=30 background=\"#000\" version=1 {\n    video path=\"broken_av.mp4\" start=0 duration=0.5\n}\n",
    );
    let (code, out) = run(&["check", "proj"], &root);
    assert_eq!(code, 0, "shape is valid; the failure is at decode: {out}");
    let (code, out) = run(&["render", "proj"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("broken_av.mp4"), "refusal names the file: {out}");
    assert!(out.contains("audio"), "refusal names the undecodable audio stream: {out}");
    assert!(!folder.join("out.mp4").exists(), "no MP4 on refusal");
    assert!(!folder.join("out.mp4.framesmith-part").exists());
}

// --- T010: container + picture preservation ---------------------------------------

#[test]
fn container_and_picture_guarantees_hold_with_mixed_sound() {
    let root = tmp("preserve");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    gen_tone(&folder.join("tone.wav"), 440, 2.0);
    gen_video_with_sound(&folder, "vsound.mp4", &folder.join("tone.wav"), 2.0);
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=64 height=48 duration=2 fps=30 background=\"#112233\" version=1 {\n    video path=\"vsound.mp4\" start=0 duration=2 volume=-6 fade-in=0.25 fade-out=0.25\n    audio path=\"tone.wav\" start=0 duration=2 volume=-12\n}\n",
    );
    let (code, out) = run(&["render", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    let probe = ffprobe(
        &folder.join("out.mp4"),
        &["stream=codec_name", "stream=pix_fmt", "stream=sample_rate", "stream=channels"],
    );
    for want in ["codec_name=h264", "pix_fmt=yuv420p", "codec_name=aac", "sample_rate=48000", "channels=2"] {
        assert!(probe.contains(want), "{probe}");
    }
    // Picture does not depend on the mix: the same composition with the
    // sound silenced decodes to identical video bytes.
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=64 height=48 duration=2 fps=30 background=\"#112233\" version=1 {\n    video path=\"vsound.mp4\" start=0 duration=2 volume=\"-inf\"\n}\n",
    );
    let (code, out) = run(&["render", "proj", "-o", "quiet.mp4"], &root);
    assert_eq!(code, 0, "{out}");
    let a = decode_video(&folder.join("out.mp4"), 64, 48);
    let b = decode_video(&root.join("quiet.mp4"), 64, 48);
    assert_eq!(a.len(), b.len());
    assert_eq!(a, b, "picture is unchanged by the mix contents");
}
