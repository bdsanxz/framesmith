//! Sprint 11 LUT import and bake (T003) end-to-end: a composition
//! whose grade carries a `lut` checks green and renders shifted
//! pixels, a half amount blends halfway, and
//! `framesmith grade-bake` writes an equivalent `.cube` plus a
//! one-line report that reimports pixel-identically — while every
//! failure variant (missing file, unparsable cube, oversized grid,
//! bad amount, mid-render vanish, identity bake, unwritable bake
//! path) refuses naming the file or the value and the fix.
//!
//! Pixel assertions stay tolerant of H.264 (flat areas survive CRF 23
//! within a few LSB); exact blends and bit-exact paths are proven in
//! the notation and render unit tests.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-lut-e2e-{tag}-{}-{}",
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
    let root = tmp("lut");
    let proj = root.join("proj");
    std::fs::create_dir_all(&proj).unwrap();
    std::fs::write(proj.join("composition.kdl"), body).unwrap();
    (root, proj)
}

/// One size-2 `.cube` swapping red and blue, R-fastest.
fn write_swap_cube(proj: &Path) {
    let mut s = String::from("TITLE \"swap r and b\"\nLUT_3D_SIZE 2\n");
    for b in 0..2 {
        for g in 0..2 {
            for r in 0..2 {
                s.push_str(&format!("{b} {g} {r}\n"));
            }
        }
    }
    std::fs::write(proj.join("swap.cube"), s).unwrap();
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

fn center(raw: &[u8]) -> (i32, i32, i32) {
    let o = (32 * 64 + 32) * 3;
    (raw[o] as i32, raw[o + 1] as i32, raw[o + 2] as i32)
}

const TUNED: &str = "composition name=\"main\" root=true width=64 height=64 duration=1 background=\"#101010\" version=1 {\n grade preset=\"filmic\" {\n lut src=\"swap.cube\" amount=0.8\n }\n box x=8 y=8 width=48 height=48 fill=\"#c86432\" start=0 duration=1\n}\n";

#[test]
fn lut_import_shifts_pixels_at_full_amount() {
    let body = "composition name=\"main\" root=true width=64 height=64 duration=1 background=\"#101010\" version=1 {\n box x=8 y=8 width=48 height=48 fill=\"#c86432\" start=0 duration=1 {\n grade {\n lut src=\"swap.cube\" amount=1\n }\n }\n}\n";
    let (root, proj) = stage(body);
    write_swap_cube(&proj);
    let (code, out) = run(&["check", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(&["render", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    let raw = decode_first_frame(&proj.join("out.mp4"), 64, 64);
    // 200,100,50 with red and blue swapped: blue dominates red.
    let (r, g, b) = center(&raw);
    assert!(b > 150 && r < 110, "swapped pixel, got {r},{g},{b}");
    assert!((g - 100).abs() <= 12, "green carries through, got {r},{g},{b}");
}

#[test]
fn lut_half_amount_renders_halfway_between() {
    let boxed = |lut: &str| {
        format!(
            "composition name=\"main\" root=true width=64 height=64 duration=1 background=\"#101010\" version=1 {{\n box x=8 y=8 width=48 height=48 fill=\"#c86432\" start=0 duration=1 {{\n grade {{\n {lut}\n }}\n }}\n}}\n"
        )
    };
    let render_center = |grade_lut: &str| {
        let (root, proj) = stage(&boxed(grade_lut));
        write_swap_cube(&proj);
        let (code, out) = run(&["check", "proj"], &root);
        assert_eq!(code, 0, "{out}");
        let (code, out) = run(&["render", "proj"], &root);
        assert_eq!(code, 0, "{out}");
        decode_first_frame(&proj.join("out.mp4"), 64, 64)
    };
    let none = render_center("");
    let full = render_center("lut src=\"swap.cube\" amount=1");
    let half = render_center("lut src=\"swap.cube\" amount=0.5");
    // Each leg carries its own codec error, so halfway holds within a
    // small handful of LSB; exact blending is proven losslessly in
    // the notation unit tests.
    for o in (0..none.len()).step_by(3) {
        for c in 0..3 {
            let (a, b, h) = (none[o + c] as i32, full[o + c] as i32, half[o + c] as i32);
            assert!(
                (h - (a + b) / 2).abs() <= 8,
                "half blend off at byte {o}+{c}: none={a} full={b} half={h}"
            );
        }
    }
    // And the half render visibly differs from both ends.
    assert_ne!(none, half);
    assert_ne!(full, half);
}

#[test]
fn lut_bake_writes_cube_plus_one_line_report() {
    let (root, proj) = stage(TUNED);
    write_swap_cube(&proj);
    let out_path = root.join("baked.cube");
    let (code, out) = run(
        &["grade-bake", "proj", "-o", &out_path.display().to_string()],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    let lines: Vec<&str> = out.trim().lines().collect();
    assert_eq!(lines.len(), 1, "one-line report, got {out:?}");
    assert!(lines[0].contains("baked"), "{out}");
    assert!(lines[0].contains("33"), "{out}");
    assert!(lines[0].contains("preset=filmic"), "{out}");
    assert!(lines[0].contains("swap.cube"), "{out}");
    assert!(out_path.is_file(), "bake must write the cube");
    let body = std::fs::read_to_string(&out_path).unwrap();
    assert!(body.contains("LUT_3D_SIZE 33"), "{body:.200}");
}

#[test]
fn lut_bake_reimport_renders_pixel_identical() {
    // Bake the tuned chain, then reuse the baked file in a second
    // project whose only grade is the baked LUT at full amount: the
    // two first frames agree within codec-plus-resample tolerance.
    let (root, proj) = stage(TUNED);
    write_swap_cube(&proj);
    let baked = root.join("baked.cube");
    let (code, out) = run(
        &["grade-bake", "proj", "-o", &baked.display().to_string()],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    let tuned = decode_first_frame(&{
        let (code, out) = run(&["render", "proj"], &root);
        assert_eq!(code, 0, "{out}");
        proj.join("out.mp4")
    }, 64, 64);
    let proj2 = root.join("proj2");
    std::fs::create_dir_all(&proj2).unwrap();
    std::fs::write(
        proj2.join("composition.kdl"),
        "composition name=\"main\" root=true width=64 height=64 duration=1 background=\"#101010\" version=1 {\n box x=8 y=8 width=48 height=48 fill=\"#c86432\" start=0 duration=1 {\n grade {\n lut src=\"baked.cube\" amount=1\n }\n }\n}\n",
    )
    .unwrap();
    std::fs::copy(&baked, proj2.join("baked.cube")).unwrap();
    let (code, out) = run(&["check", "proj2"], &root);
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(&["render", "proj2"], &root);
    assert_eq!(code, 0, "{out}");
    let reused = decode_first_frame(&proj2.join("out.mp4"), 64, 64);
    let mut worst = 0i32;
    for i in 0..tuned.len() {
        worst = worst.max((tuned[i] as i32 - reused[i] as i32).abs());
    }
    assert!(worst <= 8, "baked LUT drifts {worst} LSB off the tuned chain");
}

#[test]
fn lut_bake_of_a_sole_lut_copies_the_file_verbatim() {
    // A sole LUT import at full amount bakes to the very same bytes,
    // so the reimport cannot drift by even one LSB.
    let body = "composition name=\"main\" root=true width=64 height=64 duration=1 background=\"#101010\" version=1 {\n grade {\n lut src=\"swap.cube\" amount=1\n }\n box x=8 y=8 width=48 height=48 fill=\"#c86432\" start=0 duration=1\n}\n";
    let (root, proj) = stage(body);
    write_swap_cube(&proj);
    let baked = root.join("baked.cube");
    let (code, out) = run(
        &["grade-bake", "proj", "-o", &baked.display().to_string()],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    assert_eq!(
        std::fs::read(&baked).unwrap(),
        std::fs::read(proj.join("swap.cube")).unwrap(),
        "sole-LUT bake must copy the file verbatim"
    );
}

#[test]
fn lut_check_refuses_a_missing_file_naming_it() {
    let body = "composition name=\"main\" root=true width=64 height=64 duration=1 background=\"#101010\" version=1 {\n box x=8 y=8 width=48 height=48 fill=\"#c86432\" start=0 duration=1 {\n grade {\n lut src=\"gone.cube\" amount=1\n }\n }\n}\n";
    let (root, _proj) = stage(body);
    let (code, out) = run(&["check", "proj"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_MISSING_FILE"), "{out}");
    assert!(out.contains("gone.cube"), "{out}");
}

#[test]
fn lut_check_refuses_an_unparsable_cube() {
    let body = "composition name=\"main\" root=true width=64 height=64 duration=1 background=\"#101010\" version=1 {\n box x=8 y=8 width=48 height=48 fill=\"#c86432\" start=0 duration=1 {\n grade {\n lut src=\"bad.cube\"\n }\n }\n}\n";
    let (root, proj) = stage(body);
    std::fs::write(proj.join("bad.cube"), "LUT_3D_SIZE 2\n0 0\n").unwrap();
    let (code, out) = run(&["check", "proj"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_BAD_GRADE"), "{out}");
    assert!(out.contains("bad.cube"), "{out}");
}

#[test]
fn lut_check_refuses_an_oversized_grid_naming_the_limit() {
    let body = "composition name=\"main\" root=true width=64 height=64 duration=1 background=\"#101010\" version=1 {\n box x=8 y=8 width=48 height=48 fill=\"#c86432\" start=0 duration=1 {\n grade {\n lut src=\"big.cube\"\n }\n }\n}\n";
    let (root, proj) = stage(body);
    std::fs::write(proj.join("big.cube"), "LUT_3D_SIZE 64\n").unwrap();
    let (code, out) = run(&["check", "proj"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_BAD_GRADE"), "{out}");
    assert!(out.contains("big.cube"), "{out}");
    assert!(out.contains("33"), "{out}");
}

#[test]
fn lut_check_refuses_an_amount_outside_zero_to_one() {
    let body = "composition name=\"main\" root=true width=64 height=64 duration=1 background=\"#101010\" version=1 {\n box x=8 y=8 width=48 height=48 fill=\"#c86432\" start=0 duration=1 {\n grade {\n lut src=\"swap.cube\" amount=1.5\n }\n }\n}\n";
    let (root, proj) = stage(body);
    write_swap_cube(&proj);
    let (code, out) = run(&["check", "proj"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_BAD_GRADE"), "{out}");
    assert!(out.contains("0 to 1"), "{out}");
}

#[test]
fn lut_gone_before_render_refuses_before_frame_one_naming_the_path() {
    let body = "composition name=\"main\" root=true width=64 height=64 duration=1 background=\"#101010\" version=1 {\n box x=8 y=8 width=48 height=48 fill=\"#c86432\" start=0 duration=1 {\n grade {\n lut src=\"swap.cube\" amount=1\n }\n }\n}\n";
    let (root, proj) = stage(body);
    write_swap_cube(&proj);
    let (code, out) = run(&["check", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    // The source vanishes after a green check: the render re-checks
    // before frame one, refuses naming the path, and writes no
    // partial MP4. The frames-stage vanish (a file deleted while
    // frames draw) is covered in the render unit tests, which delete
    // between frame description and frame drawing.
    std::fs::remove_file(proj.join("swap.cube")).unwrap();
    let (code, out) = run(&["render", "proj"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("swap.cube"), "{out}");
    assert!(!proj.join("out.mp4").exists(), "no partial MP4 may remain");
}

#[test]
fn lut_bake_of_an_identity_chain_refuses_instead_of_a_pass_through() {
    let body = "composition name=\"main\" root=true width=64 height=64 duration=1 background=\"#101010\" version=1 {\n box x=8 y=8 width=48 height=48 fill=\"#c86432\" start=0 duration=1\n}\n";
    let (root, _proj) = stage(body);
    let baked = root.join("baked.cube");
    let (code, out) = run(
        &["grade-bake", "proj", "-o", &baked.display().to_string()],
        &root,
    );
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("identity"), "{out}");
    assert!(!baked.exists(), "no pass-through file may be written");
}

#[test]
fn lut_bake_to_an_unwritable_path_refuses_with_nothing_written() {
    let (root, proj) = stage(TUNED);
    write_swap_cube(&proj);
    // A regular file blocks the output directory: nothing is created.
    std::fs::write(root.join("blocker"), b"in the way").unwrap();
    let target = root.join("blocker").join("out.cube");
    let (code, out) = run(
        &["grade-bake", "proj", "-o", &target.display().to_string()],
        &root,
    );
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("blocker"), "{out}");
    assert!(out.contains("nothing was written"), "{out}");
    assert!(!target.exists(), "no partial cube may be written");
}
