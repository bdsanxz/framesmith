//! Sprint 11 VFX (T005) end-to-end: an ordered `vfx` chain checks
//! green and renders to a standard MP4 whose decoded pixels carry
//! the chain, while a bad stage fails `check` naming the stage and
//! the fix.
//!
//! The MP4 leg proves the validate-then-render seam: rendering and
//! stream facts use the host FFmpeg, and the matte picture is
//! generated with it too. Pixel assertions stay tolerant of H.264
//! (flat areas survive CRF 23 within a few LSB), so they prove the
//! chain, not the codec.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-vfx-e2e-{tag}-{}-{}",
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

fn stage(body: &str) -> (PathBuf, PathBuf) {
    let root = tmp("vfx");
    let proj = root.join("proj");
    std::fs::create_dir_all(&proj).unwrap();
    std::fs::write(proj.join("composition.kdl"), body).unwrap();
    (root, proj)
}

fn render_mp4(body: &str) -> Vec<u8> {
    let (root, proj) = stage(body);
    let (code, out) = run(&["check", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(&["render", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    let mp4 = proj.join("out.mp4");
    assert!(mp4.is_file(), "render must write an MP4");
    decode_first_frame(&mp4, 64, 64)
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

fn box_variance(raw: &[u8]) -> f64 {
    // Variance of the red channel over the box interior (x/y 16..48),
    // clear of the edges H.264 rings on.
    let mut xs = Vec::new();
    for y in 16..48 {
        for x in 16..48 {
            xs.push(raw[(y * 64 + x) * 3] as f64);
        }
    }
    let mean = xs.iter().sum::<f64>() / xs.len() as f64;
    xs.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / xs.len() as f64
}

const NOISY: &str = "composition name=\"main\" root=true width=64 height=64 duration=1 background=\"#000\" version=1 {\n box x=8 y=8 width=48 height=48 fill=\"#c86432\" start=0 duration=1 {\n vfx {\n noise amount=1 seed=7\n warp grid=8 strength=4\n }\n }\n}\n";

#[test]
fn vfx_chain_checks_and_renders_visible_noise() {
    let raw = render_mp4(NOISY);
    // Full-strength seeded noise survives the codec: the box interior
    // carries far more variance than any flat H.264 area.
    let var = box_variance(&raw);
    assert!(var > 30.0, "noised box must carry variance, got {var}");
}

#[test]
fn vfx_inert_chain_renders_the_source_color() {
    // Every stage inert passes through: the box decodes in its
    // authored color, pixel-identical (through the codec) to no chain.
    let body = "composition name=\"main\" root=true width=64 height=64 duration=1 background=\"#000\" version=1 {\n box x=8 y=8 width=48 height=48 fill=\"#c86432\" start=0 duration=1 {\n vfx {\n noise amount=0 seed=7\n warp strength=0\n }\n }\n}\n";
    let raw = render_mp4(body);
    let o = (32 * 64 + 32) * 3;
    let (r, g, b) = (raw[o] as i32, raw[o + 1] as i32, raw[o + 2] as i32);
    assert!(
        (r - 200).abs() <= 16 && (g - 100).abs() <= 16 && (b - 50).abs() <= 16,
        "inert chain must decode in its authored color, got {r},{g},{b}"
    );
    assert!(
        box_variance(&raw) < 30.0,
        "inert chain must stay flat, variance {}",
        box_variance(&raw)
    );
}

#[test]
fn vfx_luma_matte_cuts_through_render() {
    // A white-left, black-right matte keeps the left half and cuts
    // the right half to the background.
    let root = tmp("matte");
    let proj = root.join("proj");
    std::fs::create_dir_all(&proj).unwrap();
    ffmpeg(&[
        "-f",
        "lavfi",
        "-i",
        "color=c=black:s=64x64:d=1",
        "-f",
        "lavfi",
        "-i",
        "color=c=white:s=32x64:d=1",
        "-filter_complex",
        "[1]format=rgba[fg];[0][fg]overlay=0:0",
        "-frames:v",
        "1",
        &proj.join("matte.png").display().to_string(),
    ]);
    let body = "composition name=\"main\" root=true width=64 height=64 duration=1 background=\"#000\" version=1 {\n box x=8 y=8 width=48 height=48 fill=\"#c86432\" start=0 duration=1 {\n vfx {\n luma-matte src=\"matte.png\"\n }\n }\n}\n";
    std::fs::write(proj.join("composition.kdl"), body).unwrap();
    let (code, out) = run(&["check", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(&["render", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    let raw = decode_first_frame(&proj.join("out.mp4"), 64, 64);
    // Left of the matte seam the box survives; right of it the
    // background shows through.
    let o = (32 * 64 + 20) * 3;
    let (r, g, b) = (raw[o] as i32, raw[o + 1] as i32, raw[o + 2] as i32);
    assert!(
        (r - 200).abs() <= 24 && (g - 100).abs() <= 24 && (b - 50).abs() <= 24,
        "white matte must keep the box color, got {r},{g},{b}"
    );
    let o = (32 * 64 + 44) * 3;
    let (r, g, b) = (raw[o] as i32, raw[o + 1] as i32, raw[o + 2] as i32);
    assert!(
        r <= 24 && g <= 24 && b <= 24,
        "black matte must cut to the background, got {r},{g},{b}"
    );
}

#[test]
fn vfx_check_refuses_unknown_and_duplicated_stages() {
    let bad_stage = NOISY.replace("noise amount=1 seed=7", "glow amount=1");
    let (root, _proj) = stage(&bad_stage);
    let (code, out) = run(&["check", "proj"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_BAD_VFX"), "{out}");
    for name in ["noise", "warp", "displacement", "luma-matte"] {
        assert!(out.contains(name), "check names {name}: {out}");
    }

    let dup_stage = NOISY.replace(
        "noise amount=1 seed=7\n warp grid=8 strength=4",
        "noise amount=1 seed=7\n noise seed=8",
    );
    let (root, _proj) = stage(&dup_stage);
    let (code, out) = run(&["check", "proj"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_BAD_VFX"), "{out}");
    assert!(out.contains("two `noise`"), "{out}");
    assert!(out.contains("at most once"), "{out}");

    let dup_vfx = NOISY.replace(
        "vfx {\n noise amount=1 seed=7\n warp grid=8 strength=4\n }",
        "vfx {\n noise\n }\n vfx {\n warp\n }",
    );
    let (root, _proj) = stage(&dup_vfx);
    let (code, out) = run(&["check", "proj"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_BAD_VFX"), "{out}");
    assert!(out.contains("two `vfx`"), "{out}");
}

#[test]
fn vfx_check_refuses_matte_without_src_and_missing_files() {
    let no_src = NOISY.replace(
        "vfx {\n noise amount=1 seed=7\n warp grid=8 strength=4\n }",
        "vfx {\n luma-matte amount=0.5\n }",
    );
    let (root, _proj) = stage(&no_src);
    let (code, out) = run(&["check", "proj"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_BAD_VFX"), "{out}");
    assert!(out.contains("`luma-matte`"), "{out}");
    assert!(out.contains("`src="), "{out}");

    let missing = NOISY.replace(
        "vfx {\n noise amount=1 seed=7\n warp grid=8 strength=4\n }",
        "vfx {\n luma-matte src=\"gone.png\"\n }",
    );
    let (root, _proj) = stage(&missing);
    let (code, out) = run(&["check", "proj"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_MISSING_FILE"), "{out}");
    assert!(out.contains("gone.png"), "{out}");
}
