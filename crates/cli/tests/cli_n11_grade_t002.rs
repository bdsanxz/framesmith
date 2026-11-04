//! Sprint 11 grade detail stages (T002) end-to-end: a composition
//! whose clip grade carries `wheel`, `curve`, `hsl`, `vignette`, and
//! `grain` checks green and renders to a standard MP4 whose decoded
//! pixels carry the look, while a malformed curve list and an
//! out-of-range wheel band fail `check` naming the element and the fix.
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
        "framesmith-detail-e2e-{tag}-{}-{}",
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
    let root = tmp("detail");
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

const GOOD: &str = "composition name=\"main\" root=true width=64 height=64 duration=1 background=\"#101010\" version=1 {\n grade preset=\"filmic\" {\n vignette amount=0.4 softness=0.6\n }\n box x=8 y=8 width=48 height=48 fill=\"#c83c3c\" start=0 duration=1 {\n grade {\n wheel shadows=0.2 midtones=0.1 highlights=-0.1\n curve red=\"0,0 0.5,0.6 1,1\" master=\"0,0 1,1\"\n hsl hue-min=330 hue-max=30 saturation=0.2 lightness=0.2\n vignette amount=0.5 softness=0.5\n grain amount=0.3 size=1 seed=7\n }\n }\n}\n";

#[test]
fn detail_grade_checks_and_renders_the_look() {
    let (root, proj) = stage(GOOD);
    let (code, out) = run(&["check", "proj"], &root);
    assert_eq!(code, 0, "{out}");

    let (code, out) = run(&["render", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    let mp4 = proj.join("out.mp4");
    assert!(mp4.is_file(), "render must write an MP4");

    // The red box lifts pink through the curve plus the HSL lightness
    // shift: red stays dominant while green and blue rise well above
    // the authored 60.
    let raw = decode_first_frame(&mp4, 64, 64);
    let o = (32 * 64 + 32) * 3;
    let (r, g, b) = (raw[o] as i32, raw[o + 1] as i32, raw[o + 2] as i32);
    assert!(r > 150, "graded red must stay dominant, got {r},{g},{b}");
    assert!(
        g > 80 && b > 80,
        "HSL lightness must lift the box past the authored 60, got {r},{g},{b}"
    );
}

#[test]
fn detail_check_refuses_malformed_curve() {
    let bad = GOOD.replace("curve red=\"0,0 0.5,0.6 1,1\"", "curve red=\"0,0 0.5\"");
    let (root, _proj) = stage(&bad);
    let (code, out) = run(&["check", "proj"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_BAD_GRADE"), "{out}");
    assert!(out.contains("curve"), "{out}");
    assert!(out.contains("0,0 0.5,0.5 1,1"), "{out}");
}

#[test]
fn detail_check_refuses_out_of_range_wheel() {
    let bad = GOOD.replace("wheel shadows=0.2", "wheel shadows=5");
    let (root, _proj) = stage(&bad);
    let (code, out) = run(&["check", "proj"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_BAD_GRADE"), "{out}");
    assert!(out.contains("shadows"), "{out}");
    assert!(out.contains("-1 to 1"), "{out}");
}
