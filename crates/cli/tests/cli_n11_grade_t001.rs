//! Sprint 11 grade (T001) end-to-end: a graded composition checks
//! green and renders to a standard MP4 whose decoded pixels carry the
//! look, while a bad preset fails `check` naming the accepted set.
//!
//! The MP4 leg proves the validate-then-render seam: like the other
//! end-to-end tests, rendering and stream facts use the host FFmpeg.
//! Pixel assertions stay tolerant of H.264 (flat areas survive CRF 23
//! within a few LSB), so they prove the look, not the codec.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-grade-e2e-{tag}-{}-{}",
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

fn stage(body: &str) -> (PathBuf, PathBuf) {
    let root = tmp("grade");
    let proj = root.join("proj");
    std::fs::create_dir_all(&proj).unwrap();
    std::fs::write(proj.join("composition.kdl"), body).unwrap();
    (root, proj)
}

fn decode_first_frame(mp4: &Path, w: u32, h: u32) -> Vec<u8> {
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
    assert_eq!(out.stdout.len(), (w * h * 3) as usize);
    out.stdout
}

const GOOD: &str = "composition name=\"main\" root=true width=64 height=64 duration=1 background=\"#000\" version=1 {\n grade preset=\"filmic\"\n lane track=\"graphics\" {\n grade saturation=-0.5\n }\n box x=8 y=8 width=48 height=48 fill=\"#c86432\" start=0 duration=1 {\n grade preset=\"noir\"\n }\n}\n";

#[test]
fn grade_checks_and_renders_the_look() {
    let (root, proj) = stage(GOOD);
    let (code, out) = run(&["check", "proj"], &root);
    assert_eq!(code, 0, "{out}");

    let (code, out) = run(&["render", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    let mp4 = proj.join("out.mp4");
    assert!(mp4.is_file(), "render must write an MP4");

    // The clip's `noir` wins over the composition `filmic`: the box
    // decodes gray, near the authored luma, far from the source hue.
    let raw = decode_first_frame(&mp4, 64, 64);
    let o = (32 * 64 + 32) * 3;
    let (r, g, b) = (raw[o] as i32, raw[o + 1] as i32, raw[o + 2] as i32);
    assert!(
        (r - g).abs() <= 16 && (g - b).abs() <= 16,
        "noir box must decode gray, got {r},{g},{b}"
    );
    assert!(
        (100..=140).contains(&((r + g + b) / 3)),
        "noir luma must sit near the authored 117, got {r},{g},{b}"
    );
}

#[test]
fn grade_check_refuses_unknown_preset() {
    let bad = GOOD.replace("grade preset=\"noir\"", "grade preset=\"vintage\"");
    let (root, _proj) = stage(&bad);
    let (code, out) = run(&["check", "proj"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_BAD_GRADE"), "{out}");
    for name in ["filmic", "noir", "teal-orange", "pastel", "bleach"] {
        assert!(out.contains(name), "check names {name}: {out}");
    }
}
