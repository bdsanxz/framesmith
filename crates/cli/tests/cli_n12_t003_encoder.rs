//! Sprint 12 T003 end-to-end: encode on hardware or software.
//!
//! `--encoder software` renders byte-identically (including re-renders
//! with pacing flags); `--keyint` pins the probed keyframe interval;
//! `--maxrate` caps probed bitrate peaks; explicit `videotoolbox`
//! without hardware refuses naming the cause and the software
//! fallback while `auto` succeeds on software; and every refusal
//! names the flag with a concrete fix before frame one, leaving no
//! partial output behind.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-n12t003-{tag}-{}-{}",
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
    let out = Command::new(bin())
        .args(args)
        .envs(env.iter().copied())
        .current_dir(cwd)
        .output()
        .expect("run framesmith binary");
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    (out.status.code().unwrap_or(-1), text)
}

fn write(folder: &Path, name: &str, content: &str) {
    std::fs::write(folder.join(name), content).unwrap();
}

fn ffprobe(args: &[&str], path: &Path) -> String {
    let out = Command::new("ffprobe")
        .args(["-v", "error"])
        .args(args)
        .arg(path)
        .output()
        .expect("run host ffprobe");
    assert!(
        out.status.success(),
        "ffprobe failed for {}",
        path.display()
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Small text composition for the fast tests: 2s at 15fps (30 frames).
fn project(root: &Path) -> PathBuf {
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=160 height=120 duration=2 fps=15 background=\"#112233\" version=1 {\n    text value=\"Hi\" start=0 duration=2 x=10 y=20 size=24 color=\"#ffffff\"\n}\n",
    );
    folder
}

/// Noise-footage composition for the peak-cap test: incompressible
/// pictures force the encoder to spend bits, so a cap binds instead
/// of passing vacuously on flat color.
fn noise_project(root: &Path) -> PathBuf {
    let folder = root.join("nproj");
    std::fs::create_dir_all(&folder).unwrap();
    let out = Command::new("ffmpeg")
        .args([
            "-y",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "nullsrc=s=640x480:r=30:d=2,noise=alls=80:allf=t",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
        ])
        .arg(folder.join("noise.mp4"))
        .output()
        .expect("generate noise footage");
    assert!(out.status.success(), "host ffmpeg noise source failed");
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=640 height=480 duration=2 fps=30 background=\"#000000\" version=1 {\n    video path=\"noise.mp4\" start=0 duration=2 fit=\"fill\"\n}\n",
    );
    folder
}

/// True when the host FFmpeg ships the H.264 hardware encoder: the
/// hardware-success tests run only there, so the suite stays green on
/// machines without VideoToolbox.
fn have_videotoolbox() -> bool {
    let out = Command::new("ffmpeg")
        .args(["-hide_banner", "-encoders"])
        .output()
        .expect("query host encoders");
    let text =
        String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr);
    text.lines().any(|line| {
        let mut parts = line.split_whitespace();
        parts.next();
        parts.next() == Some("h264_videotoolbox")
    })
}

fn frame_count(path: &Path) -> u64 {
    ffprobe(
        &[
            "-count_frames",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=nb_read_frames",
            "-of",
            "csv=p=0",
        ],
        path,
    )
    .trim()
    .parse()
    .expect("parse frame count")
}

fn codec_of(path: &Path) -> String {
    ffprobe(
        &[
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=codec_name",
            "-of",
            "csv=p=0",
        ],
        path,
    )
    .trim()
    .to_string()
}

/// Gaps (in frames) between consecutive I-frames of one video stream.
fn keyframe_gaps(path: &Path) -> Vec<u64> {
    let text = ffprobe(
        &[
            "-select_streams",
            "v:0",
            "-show_entries",
            "frame=pict_type",
            "-of",
            "csv=p=0",
        ],
        path,
    );
    let mut gaps = Vec::new();
    let mut last: Option<u64> = None;
    for (i, line) in text.lines().enumerate() {
        // Some frames carry trailing csv side-data fields (`I,...`);
        // only the first field names the picture type.
        let pict = line.trim().split(',').next().unwrap_or("");
        if pict == "I" {
            if let Some(l) = last {
                gaps.push(i as u64 - l);
            }
            last = Some(i as u64);
        }
    }
    gaps
}

/// Peak per-second video bitrate in Mbps (packet bytes per 1s window).
fn peak_mbps(path: &Path) -> f64 {
    let text = ffprobe(
        &[
            "-select_streams",
            "v:0",
            "-show_entries",
            "packet=pts_time,size",
            "-of",
            "csv=p=0",
        ],
        path,
    );
    let mut wins = std::collections::BTreeMap::new();
    for line in text.lines() {
        let mut parts = line.trim().split(',');
        let (Some(t), Some(s)) = (parts.next(), parts.next()) else {
            continue;
        };
        let (Ok(t), Ok(s)) = (t.parse::<f64>(), s.parse::<u64>()) else {
            continue;
        };
        *wins.entry(t.floor() as u64).or_insert(0u64) += s;
    }
    wins.values()
        .map(|b| *b as f64 * 8.0 / 1e6)
        .fold(0.0, f64::max)
}

fn avg_bps(path: &Path) -> f64 {
    ffprobe(
        &["-show_entries", "format=bit_rate", "-of", "csv=p=0"],
        path,
    )
    .trim()
    .parse()
    .expect("parse container bitrate")
}

#[test]
fn software_renders_byte_identically() {
    let root = tmp("software");
    project(&root);
    let (code, out) = run(&["render", "proj", "-o", "base.mp4"], &root);
    assert_eq!(code, 0, "{out}");
    // The default render and an explicit `--encoder software` agree
    // byte for byte.
    let (code, out) = run(
        &["render", "proj", "--encoder", "software", "-o", "sw.mp4"],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    assert_eq!(
        std::fs::read(root.join("base.mp4")).unwrap(),
        std::fs::read(root.join("sw.mp4")).unwrap(),
        "default vs --encoder software"
    );
    // Software re-renders agree, including with pacing flags set.
    let (code, out) = run(
        &[
            "render",
            "proj",
            "--encoder",
            "software",
            "--keyint",
            "5",
            "--maxrate",
            "8M",
            "-o",
            "paced-a.mp4",
        ],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(
        &[
            "render",
            "proj",
            "--encoder",
            "software",
            "--keyint",
            "5",
            "--maxrate",
            "8M",
            "-o",
            "paced-b.mp4",
        ],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    assert_eq!(
        std::fs::read(root.join("paced-a.mp4")).unwrap(),
        std::fs::read(root.join("paced-b.mp4")).unwrap(),
        "paced software re-render"
    );
}

#[test]
fn keyframe_interval_is_honored() {
    let root = tmp("keyint");
    project(&root);
    let (code, out) = run(
        &[
            "render",
            "proj",
            "--encoder",
            "software",
            "--keyint",
            "5",
            "-o",
            "k.mp4",
        ],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("keyint=5"), "{out}");
    let gaps = keyframe_gaps(&root.join("k.mp4"));
    assert!(gaps.len() >= 5, "interval forces repeats: {gaps:?}");
    assert!(
        gaps.iter().all(|g| *g <= 5),
        "no gap past the interval: {gaps:?}"
    );
}

#[test]
fn peak_cap_is_honored() {
    let root = tmp("maxrate");
    noise_project(&root);
    let (code, out) = run(
        &[
            "render",
            "nproj",
            "--video-bitrate",
            "4M",
            "--maxrate",
            "1M",
            "-o",
            "capped.mp4",
        ],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("maxrate=1M"), "{out}");
    // The cap binds on noise (an ignored cap would leave ~30 Mbps):
    // every 1s window and the container average stay near 1 Mbps (the
    // headroom above the cap covers the VBV burst of the opening GOP).
    let peak = peak_mbps(&root.join("capped.mp4"));
    assert!(peak <= 1.75, "per-second peak {peak} Mbps past 1M cap");
    let avg = avg_bps(&root.join("capped.mp4"));
    assert!(avg <= 1.6e6, "container average {avg} bps past 1M cap");
    assert_eq!(frame_count(&root.join("capped.mp4")), 60);
}

#[test]
fn explicit_hardware_without_it_refuses_with_fallback() {
    let root = tmp("hw-missing");
    project(&root);
    let hide = [("FRAMESMITH_NO_VIDEOTOOLBOX", "1")];
    let (code, out) = run_env(
        &["render", "proj", "--encoder", "videotoolbox", "-o", "x.mp4"],
        &root,
        &hide,
    );
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("--encoder videotoolbox"), "{out}");
    assert!(out.contains("h264_videotoolbox"), "{out}");
    assert!(out.contains("--encoder software"), "{out}");
    assert!(!root.join("x.mp4").exists(), "nothing written");
    assert!(
        !root.join("x.mp4.framesmith-part").exists(),
        "no staged file"
    );
}

#[test]
fn auto_without_hardware_succeeds_on_software() {
    let root = tmp("auto-fallback");
    project(&root);
    let hide = [("FRAMESMITH_NO_VIDEOTOOLBOX", "1")];
    let (code, out) = run_env(
        &["render", "proj", "--encoder", "auto", "-o", "a.mp4"],
        &root,
        &hide,
    );
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("selected software"), "{out}");
    assert_eq!(frame_count(&root.join("a.mp4")), 30);
    assert_eq!(codec_of(&root.join("a.mp4")), "h264");
}

#[test]
fn unknown_encoder_refuses_naming_the_values() {
    let root = tmp("bad-encoder");
    project(&root);
    let (code, out) = run(
        &["render", "proj", "--encoder", "cuda", "-o", "x.mp4"],
        &root,
    );
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("--encoder"), "{out}");
    assert!(out.contains("auto, software, cpu, videotoolbox, nvenc, vaapi, qsv, or amf"), "{out}");
    assert!(!root.join("x.mp4").exists(), "nothing written");
}

#[test]
fn bad_keyint_refuses_naming_the_shape() {
    let root = tmp("bad-keyint");
    project(&root);
    for bad in ["0", "-4", "many"] {
        let name = format!("k-{bad}.mp4");
        let (code, out) = run(&["render", "proj", "--keyint", bad, "-o", &name], &root);
        assert_eq!(code, 1, "{bad}: {out}");
        assert!(out.contains("--keyint"), "{bad}: {out}");
        assert!(out.contains("positive whole number"), "{bad}: {out}");
        assert!(!root.join(&name).exists(), "{bad}: nothing written");
        assert!(
            !root.join(format!("{name}.framesmith-part")).exists(),
            "{bad}: no staged file"
        );
    }
}

#[test]
fn bad_maxrate_refuses_naming_the_shape() {
    let root = tmp("bad-maxrate");
    project(&root);
    for bad in ["fast", "-5M", "0"] {
        let safe = bad.replace('-', "neg");
        let name = format!("m-{safe}.mp4");
        let (code, out) = run(&["render", "proj", "--maxrate", bad, "-o", &name], &root);
        assert_eq!(code, 1, "{bad}: {out}");
        assert!(out.contains("--maxrate"), "{bad}: {out}");
        assert!(out.contains("8M"), "{bad}: {out}");
        assert!(!root.join(&name).exists(), "{bad}: nothing written");
    }
}

#[test]
fn videotoolbox_on_a_format_without_hardware_refuses() {
    let root = tmp("hw-format");
    project(&root);
    for (format, name) in [("webm", "x.webm"), ("gif", "x.gif")] {
        let (code, out) = run(
            &[
                "render",
                "proj",
                "--format",
                format,
                "--encoder",
                "videotoolbox",
                "-o",
                name,
            ],
            &root,
        );
        assert_eq!(code, 1, "{format}: {out}");
        assert!(out.contains("--encoder videotoolbox"), "{format}: {out}");
        assert!(
            out.contains(&format!("--format {format}")),
            "{format}: {out}"
        );
        assert!(out.contains("--encoder software"), "{format}: {out}");
        assert!(!root.join(name).exists(), "{format}: nothing written");
    }
}

#[test]
fn duplicate_encoder_flag_exits_two() {
    let root = tmp("dup-encoder");
    project(&root);
    let (code, out) = run(
        &[
            "render",
            "proj",
            "--encoder",
            "software",
            "--encoder",
            "auto",
            "-o",
            "x.mp4",
        ],
        &root,
    );
    assert_eq!(code, 2, "{out}");
    assert!(out.contains("duplicate --encoder"), "{out}");
    assert!(!root.join("x.mp4").exists(), "nothing written");
}

#[test]
fn json_report_names_the_encoder_controls() {
    let root = tmp("json-encoder");
    project(&root);
    let out = Command::new(bin())
        .args([
            "render",
            "proj",
            "--encoder",
            "software",
            "--keyint",
            "30",
            "--maxrate",
            "8M",
            "-o",
            "j.mp4",
            "--json",
        ])
        .current_dir(&root)
        .output()
        .expect("run framesmith binary");
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(stdout.contains("\"encoder\":\"software\""), "{stdout}");
    assert!(stdout.contains("\"hardware\":false"), "{stdout}");
    assert!(stdout.contains("\"keyint\":30"), "{stdout}");
    assert!(stdout.contains("\"maxrate\":\"8M\""), "{stdout}");
}

#[test]
fn hardware_encodes_where_present() {
    if !have_videotoolbox() {
        return;
    }
    let root = tmp("hw-present");
    project(&root);
    // Explicit hardware writes a decodable stream (verified by decode,
    // never by hash: hardware bytes need not match software bytes).
    let (code, out) = run(
        &["render", "proj", "--encoder", "videotoolbox", "-o", "v.mp4"],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("h264_videotoolbox"), "{out}");
    assert_eq!(codec_of(&root.join("v.mp4")), "h264");
    assert_eq!(frame_count(&root.join("v.mp4")), 30);
    // `auto` on the same machine succeeds without failing.
    let (code, out) = run(
        &["render", "proj", "--encoder", "auto", "-o", "a.mp4"],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("selected hardware"), "{out}");
    assert_eq!(frame_count(&root.join("a.mp4")), 30);
}
