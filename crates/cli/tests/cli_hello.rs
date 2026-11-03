//! T001 proof: `examples/hello` renders with no configuration — no
//! config file, no project font beyond the documented fallback, no
//! audio hardware — to a standard MP4 with exit 0.
//!
//! The folder is staged to a temp dir so the proof never writes
//! `out.mp4` or `logs/` into the checkout itself. Like the other
//! end-to-end tests, rendering and stream facts use the host FFmpeg.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-hello-proof-{}-{}-{}",
        tag,
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

fn stage_hello() -> (PathBuf, PathBuf) {
    let root = tmp("hello");
    let src = manifest_dir().join("examples/hello");
    let dst = root.join("proj");
    std::fs::create_dir_all(&dst).unwrap();
    for e in std::fs::read_dir(&src).unwrap() {
        let e = e.unwrap();
        let name = e.file_name().into_string().unwrap();
        if name == "logs" || name == "out.mp4" {
            continue;
        }
        std::fs::copy(e.path(), dst.join(&name)).unwrap();
    }
    (root, dst)
}

fn ffprobe_streams(mp4: &Path) -> String {
    let out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "stream=codec_name,width,height,pix_fmt,sample_rate,channels",
            "-of",
            "default=noprint_wrappers=1",
        ])
        .arg(mp4)
        .output()
        .expect("run host ffprobe");
    assert!(out.status.success());
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn decode_first_frame(mp4: &Path) -> Vec<u8> {
    let out = Command::new("ffmpeg")
        .args([
            "-y",
            "-v",
            "error",
            "-i",
            &mp4.display().to_string(),
            "-frames:v",
            "1",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgb24",
            "-",
        ])
        .output()
        .expect("decode mp4");
    assert!(out.status.success());
    out.stdout
}

#[test]
fn hello_needs_no_configuration_beyond_composition() {
    // The committed example is one KDL file only: no framesmith.toml,
    // no font files, no audio inputs.
    let src = manifest_dir().join("examples/hello");
    let mut names: Vec<String> = std::fs::read_dir(&src)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    assert_eq!(names, vec!["composition.kdl".to_string()], "{names:?}");
    let kdl = std::fs::read_to_string(src.join("composition.kdl")).unwrap();
    assert!(
        !kdl.contains("font"),
        "hello must use the documented fallback, declaring no font"
    );
}

#[test]
fn hello_checks_and_renders_to_a_standard_mp4() {
    let (root, _folder) = stage_hello();
    let (code, out) = run(&["check", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("ok:"), "{out}");

    let (code, out) = run(&["render", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    // The fallback face is announced, never silent: either a system
    // font is named or the built-in default face is reported.
    assert!(
        out.contains("no `font` declared;"),
        "render must announce the fallback face: {out}"
    );
    // No audio input means the encoder still writes its silent track.
    assert!(out.contains("silent track"), "{out}");

    let mp4 = root.join("proj").join("out.mp4");
    assert!(mp4.is_file(), "render must write an MP4");
    let streams = ffprobe_streams(&mp4);
    for want in [
        "codec_name=h264",
        "width=640",
        "height=360",
        "pix_fmt=yuv420p",
        "codec_name=aac",
        "sample_rate=48000",
        "channels=2",
    ] {
        assert!(streams.contains(want), "{streams}");
    }

    // The greeting is drawn over the background color.
    let raw = decode_first_frame(&mp4);
    assert_eq!(raw.len(), 640 * 360 * 3);
    let marked = raw
        .chunks_exact(3)
        .filter(|p| {
            (p[0] as i32 - 0x1a) + (p[1] as i32 - 0x1a) + (p[2] as i32 - 0x2e) > 100
        })
        .count();
    assert!(marked > 100, "text must be drawn over the background");
}
