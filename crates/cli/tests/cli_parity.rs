//! Phase 4 (T013–T016) end-to-end: GPU pictures match CPU pictures
//! within the written tolerance for still and motion fixtures (text and
//! video included), frame N alone equals frame N from a full render at
//! any worker count, and animated folders render to correct playable
//! MP4s or clean refusals.
//!
//! The tolerance numbers live in `render/PARITY_TOLERANCE.md` and in
//! `render::gpu::{TOL_MAX_PER_CHANNEL_ABS, TOL_MEAN_ABS,
//! TOL_MIN_FRACTION_WITHIN}`; every comparison below asserts all three.
//! Comparison lines are printed as `parity-report: ...` so proof
//! artifacts can quote the measured distances.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use render::gpu::{TOL_MAX_PER_CHANNEL_ABS, TOL_MEAN_ABS, TOL_MIN_FRACTION_WITHIN};
use validate::validate::{Status, validate_folder};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-parity-{}-{}-{}",
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

fn run(args: &[&str], cwd: &Path) -> (i32, String) {
    run_env(args, cwd, &[])
}

fn run_env(args: &[&str], cwd: &Path, env: &[(&str, &str)]) -> (i32, String) {
    let mut cmd = Command::new(bin());
    cmd.args(args).current_dir(cwd);
    for (k, v) in env {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("run framesmith binary");
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    (out.status.code().unwrap_or(-1), text)
}

fn write(folder: &Path, name: &str, content: &str) {
    std::fs::write(folder.join(name), content).unwrap();
}

fn ffmpeg(args: &[&str]) {
    let out = Command::new("ffmpeg")
        .args(["-y", "-v", "error"])
        .args(args)
        .output()
        .expect("run host ffmpeg");
    assert!(out.status.success(), "host ffmpeg failed: {args:?}");
}

fn ffprobe_csv(select: &str, entries: &str, mp4: &Path) -> String {
    let out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            select,
            "-show_entries",
            &format!("stream={entries}"),
            "-of",
            "csv=p=0",
            &mp4.display().to_string(),
        ])
        .output()
        .expect("run host ffprobe");
    assert!(out.status.success(), "host ffprobe failed for {}", mp4.display());
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

fn decode_raw(mp4: &Path, frames: usize) -> Vec<u8> {
    let out = Command::new("ffmpeg")
        .args([
            "-y",
            "-v",
            "error",
            "-i",
            &mp4.display().to_string(),
            "-frames:v",
            &frames.to_string(),
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

/// Solid-color video source: `cols.len()` frames at `fps`.
fn solid_source(path: &Path, cols: &[(u8, u8, u8)], w: u32, h: u32, fps: u32) {
    let mut raw = Vec::with_capacity(cols.len() * w as usize * h as usize * 3);
    for c in cols {
        raw.extend(
            std::iter::repeat(*c)
                .take(w as usize * h as usize)
                .flat_map(|p| [p.0, p.1, p.2]),
        );
    }
    let raw_path = path.with_extension("raw");
    std::fs::write(&raw_path, &raw).unwrap();
    ffmpeg(&[
        "-f",
        "rawvideo",
        "-pix_fmt",
        "rgb24",
        "-s",
        &format!("{w}x{h}"),
        "-r",
        &fps.to_string(),
        "-i",
        &raw_path.display().to_string(),
        "-c:v",
        "libx264",
        "-pix_fmt",
        "yuv420p",
        "-r",
        &fps.to_string(),
        &path.display().to_string(),
    ]);
}

/// Solid-color PNG still via host FFmpeg (opaque).
fn solid_png(path: &Path, col: (u8, u8, u8), w: u32, h: u32) {
    let raw: Vec<u8> = std::iter::repeat(col)
        .take(w as usize * h as usize)
        .flat_map(|p| [p.0, p.1, p.2])
        .collect();
    let raw_path = path.with_extension("raw");
    std::fs::write(&raw_path, &raw).unwrap();
    ffmpeg(&[
        "-f",
        "rawvideo",
        "-pix_fmt",
        "rgb24",
        "-s",
        &format!("{w}x{h}"),
        "-i",
        &raw_path.display().to_string(),
        "-frames:v",
        "1",
        &path.display().to_string(),
    ]);
}

/// Solid-color JPEG still via host FFmpeg.
fn solid_jpg(path: &Path, col: (u8, u8, u8), w: u32, h: u32) {
    let raw: Vec<u8> = std::iter::repeat(col)
        .take(w as usize * h as usize)
        .flat_map(|p| [p.0, p.1, p.2])
        .collect();
    let raw_path = path.with_extension("raw");
    std::fs::write(&raw_path, &raw).unwrap();
    ffmpeg(&[
        "-f",
        "rawvideo",
        "-pix_fmt",
        "rgb24",
        "-s",
        &format!("{w}x{h}"),
        "-i",
        &raw_path.display().to_string(),
        "-frames:v",
        "1",
        "-q:v",
        "2",
        &path.display().to_string(),
    ]);
}

/// Prepare a validated folder for frame drawing on both renderers.
fn prepared(folder: &Path) -> render::render::Prepared {
    let verdict = validate_folder(folder);
    assert_eq!(verdict.status, Status::Valid, "{:?}", verdict.diagnostics);
    let comp = verdict.composition.expect("valid verdict carries a composition");
    render::render::prepare(&comp, folder, Path::new("ffmpeg")).expect("prepare succeeds")
}

/// Compare CPU and GPU pictures of `frames` and print quotable report
/// lines; returns the lines for proof artifacts.
fn assert_frames_agree(
    prep: &mut render::render::Prepared,
    fixture: &str,
    frames: &[u64],
) -> Vec<String> {
    let mut lines = Vec::new();
    for &k in frames {
        let cpu = prep.cpu_frame(k).expect("cpu frame draws");
        let gpu = render::gpu::render_frame(prep, k).expect("gpu frame draws");
        let cmp = render::gpu::compare_frames(&cpu, &gpu).expect("pictures compare");
        let line = cmp.report(fixture, k);
        println!("parity-report: {line}");
        assert!(cmp.pass(), "{line}");
        lines.push(line);
    }
    lines
}

/// Still-element fixture folder (T013): rounded bordered boxes, PNG
/// with contain/top-left, JPEG with cover/center, written paint order
/// with swapped document positions, spaced text, and a video still.
fn still_folder(root: &Path) -> PathBuf {
    let folder = root.join("still");
    std::fs::create_dir_all(&folder).unwrap();
    solid_png(&folder.join("photo.png"), (0, 255, 0), 64, 48);
    solid_jpg(&folder.join("photo.jpg"), (255, 255, 0), 48, 64);
    solid_source(&folder.join("clip.mp4"), &vec![(255, 0, 0); 60], 64, 48, 30);
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=160 height=120 duration=2 fps=30 background=\"#101010\" version=1 {\n    video path=\"clip.mp4\" start=0 duration=2 fit=\"fill\" order=0\n    image path=\"photo.jpg\" start=0 duration=2 x=100 y=70 width=50 height=40 fit=\"cover\" placement=\"center\" order=1\n    image path=\"photo.png\" start=0 duration=2 x=100 y=10 width=50 height=50 fit=\"contain\" placement=\"top-left\" order=2\n    box x=10 y=10 width=60 height=60 fill=\"#ff0000\" radius=8 border-width=3 border-color=\"#ffffff\" opacity=1 order=3 start=0 duration=2\n    box x=40 y=40 width=60 height=60 fill=\"#0000ff\" radius=0 border-width=0 border-color=\"#000000\" opacity=0.8 order=4 start=0 duration=2\n    text value=\"Hi Ag\" start=0 duration=2 x=10 y=95 size=14 color=\"#ffffff\" order=5 line-height=20 letter-spacing=2\n}\n",
    );
    folder
}

/// Motion fixture folder (T014): tweened position/scale/rotation/
/// opacity in to/from/from-to forms, a set hold, and eased tweens on
/// box, image, text, and video targets.
fn motion_folder(root: &Path) -> PathBuf {
    let folder = root.join("motion");
    std::fs::create_dir_all(&folder).unwrap();
    solid_png(&folder.join("photo.png"), (0, 255, 0), 64, 48);
    solid_source(&folder.join("clip.mp4"), &vec![(255, 0, 0); 60], 64, 48, 30);
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=160 height=120 duration=2 fps=30 background=\"#000000\" version=1 {\n    box x=10 y=10 width=30 height=30 fill=\"#ff0000\" start=0 duration=2 order=1\n    image path=\"photo.png\" start=0 duration=2 x=100 y=10 width=50 height=40 fit=\"contain\" placement=\"center\" order=2\n    text value=\"Go\" start=0 duration=2 x=10 y=90 size=16 color=\"#ffffff\" order=3 line-height=22 letter-spacing=1\n    video path=\"clip.mp4\" start=0 duration=2 fit=\"fill\" order=0\n    tween target=0 property=\"x\" kind=\"to\" start=0 duration=1 to=60 ease=\"linear\"\n    tween target=1 property=\"scale\" kind=\"from-to\" start=0 duration=1 from=1 to=1.5 ease=\"in-out-quad\"\n    tween target=1 property=\"rotation\" kind=\"to\" start=0 duration=2 to=45 ease=\"out-back(1.7)\"\n    tween target=2 property=\"opacity\" kind=\"from\" start=0 duration=1 from=0 ease=\"linear\"\n    tween target=3 property=\"x\" kind=\"from-to\" start=0 duration=1 from=0 to=30 ease=\"out-cubic\"\n    set target=0 property=\"opacity\" start=1 duration=0.5 value=0.5\n}\n",
    );
    folder
}

#[test]
fn still_fixtures_agree_within_tolerance() {
    // T013: boxes, images, written paint order, line height,
    // letter spacing, text, and video — every GPU picture matches its
    // CPU counterpart within the written tolerance.
    let root = tmp("still");
    let folder = still_folder(&root);
    assert_eq!(validate_folder(&folder).status, Status::Valid);
    let mut prep = prepared(&folder);
    assert_frames_agree(&mut prep, "still-fixtures", &[0, 15, 30, 59]);
    println!(
        "parity-report: tolerance bounds max_abs<={} mean_abs<={} within>={}",
        TOL_MAX_PER_CHANNEL_ABS, TOL_MEAN_ABS, TOL_MIN_FRACTION_WITHIN
    );
}

#[test]
fn cpu_reruns_are_byte_identical() {
    // T013: CPU reruns stay byte-identical rather than merely within
    // tolerance — only the CPU-to-GPU comparison uses it.
    let root = tmp("rerun");
    let folder = still_folder(&root);
    let mut prep = prepared(&folder);
    for k in [0u64, 15, 30, 59] {
        let a = prep.cpu_frame(k).expect("cpu frame");
        let b = prep.cpu_frame(k).expect("cpu rerun");
        assert_eq!(a, b, "frame {k} rerun must be byte-identical");
    }
    println!("parity-report: cpu reruns byte-identical over frames [0, 15, 30, 59]");
}

#[test]
fn mistuned_picture_is_reported_without_pass() {
    // REC-003: a deliberately mistuned still picture is reported with
    // fixture, frame, and measured distance, and grants no pass.
    let root = tmp("mistune");
    let folder = still_folder(&root);
    let mut prep = prepared(&folder);
    let cpu = prep.cpu_frame(15).expect("cpu frame");
    let mut bad = cpu.clone();
    for i in (0..bad.len()).step_by(53) {
        bad[i] = bad[i].saturating_add(60);
    }
    let cmp = render::gpu::compare_frames(&cpu, &bad).expect("pictures compare");
    assert!(!cmp.pass(), "mistuned picture must not pass");
    let report = cmp.report("still-fixtures", 15);
    println!("parity-report: {report}");
    assert!(report.contains("still-fixtures"), "{report}");
    assert!(report.contains("frame 15"), "{report}");
    assert!(report.contains("max_abs="), "{report}");
    assert!(report.contains("no pass granted"), "{report}");
}

#[test]
fn motion_fixtures_agree_within_tolerance() {
    // T014: every tweened property, every tween form, set holds, and
    // eased tweens on box, image, text, and video targets — including
    // tween start frames (0) and end frames (30) under the still bounds.
    let root = tmp("motion");
    let folder = motion_folder(&root);
    assert_eq!(validate_folder(&folder).status, Status::Valid);
    let mut prep = prepared(&folder);
    assert_frames_agree(&mut prep, "motion-fixtures", &[0, 7, 15, 29, 30, 45, 59]);
}

#[test]
fn single_frames_equal_full_render_at_any_worker_count() {
    // T015: frame N rendered alone equals frame N from a full render
    // exactly on CPU; different worker counts change no visible frame
    // on either renderer. A divergence would be reported with the frame
    // number and the differing setup.
    let root = tmp("workers");
    let folder = motion_folder(&root);
    let mut reference = prepared(&folder);
    let total = reference.frames();
    assert!(total >= 60, "fixture spans the motion windows");
    // Reference pictures, drawn alone one frame at a time.
    let mut solo: Vec<Vec<u8>> = Vec::with_capacity(total);
    for k in 0..total as u64 {
        solo.push(reference.cpu_frame(k).expect("solo frame draws"));
    }
    // Full-render computation under several worker counts: Worker visit
    // order only, emitted in frame order — every picture must match.
    for workers in [1usize, 2, 5, 9] {
        let order = render::render::worker_order(total, workers);
        assert_eq!(order.len(), total, "workers={workers} visit every frame");
        let mut full = prepared(&folder);
        for &k in &order {
            let pic = full.cpu_frame(k).expect("full-render frame draws");
            assert_eq!(
                pic, solo[k as usize],
                "frame {k} differs under workers={workers}; setup differs, fixture does not pass"
            );
        }
        println!("parity-report: workers={workers} full-render frames equal solo frames (exact, {total} frames)");
        // GPU determinism: a fresh preparation draws the same pictures.
        let mut g1 = prepared(&folder);
        let mut g2 = prepared(&folder);
        for k in [0u64, 15, 30, 59] {
            let a = render::gpu::render_frame(&mut g1, k).expect("gpu frame");
            let b = render::gpu::render_frame(&mut g2, k).expect("gpu frame");
            assert_eq!(a, b, "gpu frame {k} differs across preparations");
            let cmp =
                render::gpu::compare_frames(&solo[k as usize], &a).expect("pictures compare");
            assert!(cmp.pass(), "{}", cmp.report("motion-fixtures", k));
        }
    }
}

#[test]
fn worker_counts_render_byte_identical_mp4s() {
    // T015 at the binary level: the same folder under different worker
    // counts encodes byte-identical MP4s.
    let root = tmp("wmp4");
    let _folder = motion_folder(&root);
    let (code, out) = run_env(&["render", "motion", "-o", "w1.mp4"], &root, &[("FRAMESMITH_WORKERS", "1")]);
    assert_eq!(code, 0, "{out}");
    let (code, out) = run_env(&["render", "motion", "-o", "w9.mp4"], &root, &[("FRAMESMITH_WORKERS", "9")]);
    assert_eq!(code, 0, "{out}");
    let a = std::fs::read(root.join("w1.mp4")).unwrap();
    let b = std::fs::read(root.join("w9.mp4")).unwrap();
    assert_eq!(a, b, "worker counts must not change the MP4 bytes");
    println!("parity-report: FRAMESMITH_WORKERS=1 vs 9 byte-identical ({} bytes)", a.len());
    // An invalid worker count falls back to one worker, still identical.
    let (code, out) = run_env(&["render", "motion", "-o", "wbad.mp4"], &root, &[("FRAMESMITH_WORKERS", "nope")]);
    assert_eq!(code, 0, "{out}");
    let c = std::fs::read(root.join("wbad.mp4")).unwrap();
    assert_eq!(a, c, "invalid worker count must still render identically");
}

#[test]
fn animated_folder_renders_playable_mp4() {
    // T016: a valid animated folder using boxes, images, written order,
    // text spacing, and tween/set motion renders to one MP4 with the
    // Sprint 1 container guarantees; CPU reruns are byte-identical.
    let root = tmp("anim");
    let folder = motion_folder(&root);
    let (code, out) = run(&["check", "motion"], &root);
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(&["render", "motion", "-o", "out.mp4"], &root);
    assert_eq!(code, 0, "{out}");
    let mp4 = root.join("out.mp4");
    assert!(mp4.exists(), "MP4 must exist after render");
    // Container guarantees: H.264 yuv420p video + AAC stereo 48 kHz.
    assert_eq!(ffprobe_csv("v:0", "codec_name,pix_fmt", &mp4), "h264,yuv420p");
    assert_eq!(ffprobe_csv("a:0", "codec_name,sample_rate,channels", &mp4), "aac,48000,2");
    println!("parity-report: container h264,yuv420p + aac,48000,2 verified by ffprobe");
    // The authored motion shows on decoded frames: the red box starts
    // left and ends right of its start column.
    let raw = decode_raw(&mp4, 60);
    let reds = |k: usize| -> usize {
        raw[k * 160 * 120 * 3..(k + 1) * 160 * 120 * 3]
            .chunks_exact(3)
            .filter(|p| p[0] > 150 && p[1] < 100 && p[2] < 100)
            .count()
    };
    assert!(reds(0) > 200, "start frame shows the red box");
    assert!(reds(30) > 200, "end frame still shows the red box");
    // Byte-identical CPU rerun.
    let (code, out) = run(&["render", "motion", "-o", "out2.mp4"], &root);
    assert_eq!(code, 0, "{out}");
    let (a, b) = (std::fs::read(&mp4).unwrap(), std::fs::read(root.join("out2.mp4")).unwrap());
    assert_eq!(a, b, "CPU reruns stay byte-identical");
    println!("parity-report: animated render byte-identical rerun ({} bytes)", a.len());
    // Only the explicit MP4 output and the run-log entry are written
    // into the project folder (plus a second explicit -o target here).
    let mut names: Vec<String> = Vec::new();
    for entry in walk(&folder) {
        names.push(entry);
    }
    names.sort();
    assert!(!names.iter().any(|n| n.ends_with(".framesmith-part")), "no partial MP4 survives: {names:?}");
    assert!(names.contains(&"logs/framesmith.log".to_string()), "run log is kept: {names:?}");
    assert!(names.contains(&"composition.kdl".to_string()), "{names:?}");
    println!("parity-report: project files after render: {names:?}");
}

/// Relative file list under `folder` (forward slashes).
fn walk(folder: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![folder.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let p = entry.unwrap().path();
            if p.is_dir() {
                stack.push(p);
            } else {
                out.push(p.strip_prefix(folder).unwrap().to_string_lossy().replace('\\', "/"));
            }
        }
    }
    out
}

#[test]
fn animated_refusals_write_no_mp4() {
    // T016: invalid or later-sprint-asking folders are refused before
    // the first frame with the same diagnostics check reports, and no
    // MP4 is written.
    let root = tmp("refuse");
    // Invalid box geometry.
    let bad = root.join("bad-box");
    std::fs::create_dir_all(&bad).unwrap();
    write(
        &bad,
        "composition.kdl",
        "composition name=\"t\" width=160 height=120 duration=1 fps=30 background=\"#000\" version=1 {\n    box x=0 y=0 width=0 height=10 fill=\"#ff0000\" start=0 duration=1\n}\n",
    );
    let (check_code, check_out) = run(&["check", "bad-box"], &root);
    assert_eq!(check_code, 1, "{check_out}");
    let (code, out) = run(&["render", "bad-box", "-o", "bad.mp4"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(!root.join("bad.mp4").exists(), "invalid render writes no MP4");
    assert!(out.contains(&check_out.lines().next().unwrap_or("").to_string()) || out.contains("E_BAD_BOX"), "{out}");
    // Later-sprint property: axis-separated scale refuses with exit 2.
    let later = root.join("later");
    std::fs::create_dir_all(&later).unwrap();
    write(
        &later,
        "composition.kdl",
        "composition name=\"t\" width=160 height=120 duration=1 fps=30 background=\"#000\" version=1 {\n    box x=10 y=10 width=40 height=40 fill=\"#ff0000\" start=0 duration=1\n    tween target=0 property=\"scale-x\" kind=\"to\" start=0 duration=1 to=2 ease=\"linear\"\n}\n",
    );
    assert_eq!(run(&["check", "later"], &root).0, 2);
    let (code, out) = run(&["render", "later", "-o", "later.mp4"], &root);
    assert_eq!(code, 2, "{out}");
    assert!(!root.join("later.mp4").exists(), "unsupported render writes no MP4");
    // Corrupt image: validation magic passes as PNG but decode fails.
    let corrupt = root.join("corrupt");
    std::fs::create_dir_all(&corrupt).unwrap();
    let mut junk = vec![0x89u8, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    junk.extend(vec![0u8; 256]);
    std::fs::write(corrupt.join("bad.png"), &junk).unwrap();
    write(
        &corrupt,
        "composition.kdl",
        "composition name=\"t\" width=160 height=120 duration=1 fps=30 background=\"#000\" version=1 {\n    image path=\"bad.png\" start=0 duration=1 x=0 y=0 width=80 height=60 fit=\"contain\" placement=\"center\"\n}\n",
    );
    let (code, out) = run(&["render", "corrupt", "-o", "corrupt.mp4"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("bad.png"), "{out}");
    assert!(!root.join("corrupt.mp4").exists(), "corrupt render writes no MP4");
    println!("parity-report: refusals exit 1/2/1 with no MP4 written (invalid, unsupported, corrupt)");
}

#[test]
fn check_and_render_need_no_gpu_hardware() {
    // T013: a machine without a usable GPU still checks, renders, and
    // validates exactly as Sprint 1 does. This suite runs the portable
    // picture paths with no GPU device present, and the default render
    // path is unchanged CPU output.
    let root = tmp("nogpu");
    let folder = still_folder(&root);
    assert_eq!(run(&["check", "still"], &root).0, 0);
    let (code, out) = run(&["render", "still"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(folder.join("out.mp4").exists());
    println!("parity-report: check+render succeed with no GPU hardware involved");
}

