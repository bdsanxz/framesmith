//! Sprint 5 T008 (use other movie containers; broken files refuse cleanly).
//!
//! A `video` clip pointing at any container with a decodable video
//! stream (MKV, WebM, and similar besides MOV/MP4) renders like any
//! other source, with trim, rate, and fit applying. A file with no
//! decodable video stream — missing, corrupt, or a non-video file —
//! is refused before frame one with a diagnostic naming the file; no
//! blank picture is ever rendered and no partial MP4 is left.
//! Permission-denied and disappearing files refuse the same way.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-footage-containers-{tag}-{}-{}",
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

fn gen_testsrc(dir: &Path, name: &str, seconds: f64, extra: &[&str]) {
    let input = format!("testsrc=duration={seconds}:size=64x48:rate=30");
    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-y", "-v", "error", "-f", "lavfi", "-i", &input]);
    cmd.args(extra);
    cmd.arg(dir.join(name));
    let out = cmd.output().expect("run host ffmpeg");
    assert!(out.status.success(), "host ffmpeg failed for {name}");
}

fn decode_video(path: &Path, w: u32, h: u32) -> Vec<u8> {
    let out = Command::new("ffmpeg")
        .args([
            "-y",
            "-v",
            "error",
            "-i",
            &path.display().to_string(),
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
        .expect("decode video");
    assert!(out.status.success(), "decode failed for {}", path.display());
    out.stdout
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

fn write(folder: &Path, name: &str, content: &str) {
    std::fs::write(folder.join(name), content).unwrap();
}

fn comp(duration: f64, clip_line: &str) -> String {
    format!(
        "composition name=\"t\" width=64 height=48 duration={duration} fps=30 background=\"#000000\" version=1 {{\n    {clip_line}\n}}\n"
    )
}

fn assert_no_outputs(folder: &Path, name: &str) {
    assert!(
        !folder.join("out.mp4").exists(),
        "{name}: no partial MP4 may remain"
    );
    assert!(
        !folder.join("out.mp4.framesmith-part").exists(),
        "{name}: no staged partial may remain"
    );
}

/// An MKV source renders with trim, rate, and fit applying: output
/// frame `k` matches source frame `trim-start * fps + rate * k`.
#[test]
fn mkv_source_renders_with_trim_rate_and_fit() {
    let folder = tmp("mkv");
    gen_testsrc(
        &folder,
        "clip.mkv",
        4.0,
        &["-c:v", "libx264", "-pix_fmt", "yuv420p"],
    );
    write(
        &folder,
        "composition.kdl",
        &comp(1.0, "video path=\"clip.mkv\" start=0 duration=1 trim-start=1 trim-end=3 rate=2 fit=\"cover\""),
    );
    let (code, out) = run(&["check", "."], &folder);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("1.000s..3.000s"), "{out}");
    let (code, out) = run(&["render", ".", "-o", "out.mp4"], &folder);
    assert_eq!(code, 0, "{out}");
    let (w, h) = (64u32, 48u32);
    let rendered = decode_video(&folder.join("out.mp4"), w, h);
    let source = decode_video(&folder.join("clip.mkv"), w, h);
    let fs = (w * h * 3) as usize;
    assert_eq!(rendered.len(), 30 * fs, "1 s at 30 fps");
    for k in [0usize, 7, 15, 29] {
        let got = &rendered[k * fs..(k + 1) * fs];
        let want = &source[(30 + 2 * k) * fs..(30 + 2 * k + 1) * fs];
        let mad = frame_mad(got, want);
        println!("mkv frame {k}: mapped-mad={mad:.2}");
        assert!(
            mad < 15.0,
            "mkv frame {k} must follow trim+rate, mad={mad:.2}"
        );
    }
}

/// A WebM (VP9) source — a codec outside the old H.264/HEVC gate —
/// renders like any other source.
#[test]
fn webm_source_renders() {
    let folder = tmp("webm");
    gen_testsrc(
        &folder,
        "clip.webm",
        2.0,
        &["-c:v", "libvpx-vp9", "-b:v", "0", "-crf", "30"],
    );
    write(
        &folder,
        "composition.kdl",
        &comp(2.0, "video path=\"clip.webm\" start=0 duration=2"),
    );
    let (code, out) = run(&["check", "."], &folder);
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(&["render", ".", "-o", "out.mp4"], &folder);
    assert_eq!(code, 0, "{out}");
    let (w, h) = (64u32, 48u32);
    let rendered = decode_video(&folder.join("out.mp4"), w, h);
    let source = decode_video(&folder.join("clip.webm"), w, h);
    let fs = (w * h * 3) as usize;
    assert_eq!(rendered.len(), 60 * fs, "2 s at 30 fps");
    for k in [0usize, 15, 30, 59] {
        let mad = frame_mad(
            &rendered[k * fs..(k + 1) * fs],
            &source[k * fs..(k + 1) * fs],
        );
        println!("webm frame {k}: mapped-mad={mad:.2}");
        assert!(
            mad < 20.0,
            "webm frame {k} must match its source frame, mad={mad:.2}"
        );
    }
}

/// A missing source fails `check` naming the file, and `render`
/// refuses the same way — with no MP4 and no staged partial.
#[test]
fn missing_source_refuses_naming_the_file() {
    let folder = tmp("missing");
    write(
        &folder,
        "composition.kdl",
        &comp(2.0, "video path=\"gone.mp4\" start=0 duration=2"),
    );
    let (code, out) = run(&["check", "."], &folder);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_MISSING_FILE"), "{out}");
    assert!(out.contains("gone.mp4"), "{out}");
    let (code, out) = run(&["render", ".", "-o", "out.mp4"], &folder);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("gone.mp4"), "{out}");
    assert_no_outputs(&folder, "missing");
}

/// A corrupt file (not a container at all) refuses before frame one
/// naming the file, leaving no MP4 and no staged partial.
#[test]
fn corrupt_source_refuses_before_frame_one() {
    let folder = tmp("corrupt");
    let junk: Vec<u8> = (0..1024u32)
        .map(|i| (i.wrapping_mul(2654435761) >> 8) as u8)
        .collect();
    std::fs::write(folder.join("junk.mp4"), &junk).unwrap();
    write(
        &folder,
        "composition.kdl",
        &comp(2.0, "video path=\"junk.mp4\" start=0 duration=2"),
    );
    let (code, out) = run(&["render", ".", "-o", "out.mp4"], &folder);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("junk.mp4"), "{out}");
    assert_no_outputs(&folder, "corrupt");
}

/// A non-video file refuses before frame one naming the file, leaving
/// no MP4 and no staged partial — never a blank picture.
#[test]
fn non_video_source_refuses_before_frame_one() {
    let folder = tmp("nonvideo");
    write(&folder, "notes.txt", "this is plain text, not a movie.\n");
    write(
        &folder,
        "composition.kdl",
        &comp(2.0, "video path=\"notes.txt\" start=0 duration=2"),
    );
    let (code, out) = run(&["render", ".", "-o", "out.mp4"], &folder);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("render refused"), "{out}");
    assert!(out.contains("notes.txt"), "{out}");
    assert_no_outputs(&folder, "non-video");
}

/// A file that disappears between `check` and `render` refuses at
/// render time with its path named, leaving nothing behind.
#[test]
fn disappearing_source_refuses_naming_the_file() {
    let folder = tmp("disappearing");
    gen_testsrc(
        &folder,
        "clip.mp4",
        2.0,
        &["-c:v", "libx264", "-pix_fmt", "yuv420p"],
    );
    write(
        &folder,
        "composition.kdl",
        &comp(2.0, "video path=\"clip.mp4\" start=0 duration=2"),
    );
    let (code, out) = run(&["check", "."], &folder);
    assert_eq!(code, 0, "{out}");
    std::fs::remove_file(folder.join("clip.mp4")).unwrap();
    let (code, out) = run(&["render", ".", "-o", "out.mp4"], &folder);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("clip.mp4"), "{out}");
    assert_no_outputs(&folder, "disappearing");
}

/// Make a file unreadable on any OS: chmod 000 on Unix, the
/// readonly attribute on Windows.
fn deny(path: &std::path::Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o000)).unwrap();
    }
    #[cfg(windows)]
    {
        std::fs::set_permissions(path, {
            let mut p = std::fs::metadata(path).unwrap().permissions();
            p.set_readonly(true);
            p
        })
        .unwrap();
    }
}

/// Restore readable access on any OS (inverse of [`deny`]).
fn allow(path: &std::path::Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o644)).unwrap();
    }
    #[cfg(windows)]
    {
        std::fs::set_permissions(path, {
            let mut p = std::fs::metadata(path).unwrap().permissions();
            p.set_readonly(false);
            p
        })
        .unwrap();
    }
}

/// A permission-denied file refuses with its path named, in both
/// `check` and `render`, leaving nothing behind.
#[test]
fn permission_denied_source_refuses_naming_the_file() {
    let folder = tmp("perms");
    gen_testsrc(
        &folder,
        "clip.mp4",
        2.0,
        &["-c:v", "libx264", "-pix_fmt", "yuv420p"],
    );
    write(
        &folder,
        "composition.kdl",
        &comp(2.0, "video path=\"clip.mp4\" start=0 duration=2"),
    );
    deny(&folder.join("clip.mp4"));
    // A privileged test runner can still open the file: without a real
    // denial there is nothing to assert, so report and stop.
    if std::fs::File::open(folder.join("clip.mp4")).is_ok() {
        println!("skip: process can open mode-000 files (privileged runner)");
        allow(&folder.join("clip.mp4"));
        return;
    }
    let (code, out) = run(&["check", "."], &folder);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("clip.mp4"), "{out}");
    let (code, out) = run(&["render", ".", "-o", "out.mp4"], &folder);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("clip.mp4"), "{out}");
    assert_no_outputs(&folder, "permission-denied");
    allow(&folder.join("clip.mp4"));
}
