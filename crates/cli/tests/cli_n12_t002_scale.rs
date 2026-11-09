//! Sprint 12 T002 end-to-end: scale with presets and supersampling.
//!
//! Each `--resolution` preset renders at its exact dimensions with the
//! canvas aspect kept and even sides; `source` keeps the canvas size;
//! `supersample=1` is byte-identical to the default render with no
//! scaling work; higher supersampling renders visibly sharper small
//! text at the same output size; alpha compositing survives the scale
//! step; and both refusal variants name the flag with a concrete fix
//! before frame one, leaving no partial output behind.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-n12t002-{tag}-{}-{}",
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

fn write(folder: &Path, name: &str, content: &str) {
    std::fs::write(folder.join(name), content).unwrap();
}

fn ffprobe_show(path: &Path, entries: &str) -> String {
    let out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            entries,
            "-of",
            "default=noprint_wrappers=1",
        ])
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

fn streams(path: &Path) -> String {
    ffprobe_show(
        path,
        "stream=codec_name,width,height,pix_fmt,sample_rate,channels",
    )
}

/// Human composition used by the preset tests: text over color, so
/// every preset proves the text mark survives the scale step.
fn project(root: &Path) -> PathBuf {
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=160 height=120 duration=1 fps=15 background=\"#112233\" version=1 {\n    text value=\"Hi\" start=0 duration=1 x=10 y=20 size=24 color=\"#ffffff\"\n}\n",
    );
    folder
}

/// Decoded RGB24 frames of one video file.
fn decode_rgb(path: &Path, frames: usize, w: u32, h: u32) -> Vec<u8> {
    let out = Command::new("ffmpeg")
        .args([
            "-y",
            "-v",
            "error",
            "-i",
            &path.display().to_string(),
            "-frames:v",
            &frames.to_string(),
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgb24",
            "-",
        ])
        .output()
        .expect("decode video");
    assert!(out.status.success(), "decode failed for {}", path.display());
    assert_eq!(out.stdout.len(), frames * w as usize * h as usize * 3);
    out.stdout
}

fn marked_pixels(raw: &[u8], bg: (u8, u8, u8)) -> usize {
    raw.as_chunks::<3>()
        .0
        .iter()
        .filter(|p| {
            (p[0] as i32 - bg.0 as i32).abs()
                + (p[1] as i32 - bg.1 as i32).abs()
                + (p[2] as i32 - bg.2 as i32)
                > 100
        })
        .count()
}

#[test]
fn each_preset_renders_at_its_exact_dimensions() {
    let root = tmp("presets");
    project(&root);
    // (resolution flag, output file, expected width, expected height)
    let cases: &[(&str, &str, u32, u32)] = &[
        ("source", "out-source.mp4", 160, 120),
        ("1080p", "out-1080.mp4", 1440, 1080),
        ("720p", "out-720.mp4", 960, 720),
        ("480p", "out-480.mp4", 640, 480),
        ("square-1080", "out-square.mp4", 1080, 810),
    ];
    for (preset, name, w, h) in cases {
        let (code, out) = run(
            &[
                "render",
                "proj",
                "--resolution",
                preset,
                "-o",
                name,
            ],
            &root,
        );
        assert_eq!(code, 0, "render --resolution {preset}: {out}");
        assert!(
            out.contains(&format!("({w}x{h},")),
            "success line names the output size: {out}"
        );
        let target = root.join(name);
        let probe = streams(&target);
        assert!(probe.contains(&format!("width={w}")), "{preset}: {probe}");
        assert!(probe.contains(&format!("height={h}")), "{preset}: {probe}");
        assert_eq!(w % 2, 0, "even sides");
        assert_eq!(h % 2, 0, "even sides");
        // The text mark survives the scale step on every preset.
        let raw = decode_rgb(&target, 1, *w, *h);
        assert!(
            marked_pixels(&raw, (0x11, 0x22, 0x33)) > 50,
            "text drawn at {preset}"
        );
    }
}

#[test]
fn odd_canvas_presets_still_land_even() {
    let root = tmp("odd");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=161 height=121 duration=1 fps=15 background=\"#112233\" version=1 {\n    text value=\"Hi\" start=0 duration=1 x=10 y=20 size=24 color=\"#ffffff\"\n}\n",
    );
    let (code, out) = run(
        &["render", "proj", "--resolution", "720p", "-o", "odd.mp4"],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    let probe = streams(&root.join("odd.mp4"));
    assert!(probe.contains("width=958"), "{probe}");
    assert!(probe.contains("height=720"), "{probe}");
}

#[test]
fn source_at_1x_is_byte_identical_to_default() {
    let root = tmp("identity");
    project(&root);
    let (code, out) = run(&["render", "proj", "-o", "a.mp4"], &root);
    assert_eq!(code, 0, "{out}");
    // Explicit identity flags render the same bytes as no flags.
    let (code, out) = run(
        &[
            "render",
            "proj",
            "--resolution",
            "source",
            "--supersample",
            "1",
            "-o",
            "b.mp4",
        ],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(&["render", "proj", "--supersample", "1", "-o", "c.mp4"], &root);
    assert_eq!(code, 0, "{out}");
    let (a, b, c) = (
        std::fs::read(root.join("a.mp4")).unwrap(),
        std::fs::read(root.join("b.mp4")).unwrap(),
        std::fs::read(root.join("c.mp4")).unwrap(),
    );
    assert_eq!(a, b, "explicit identity matches the default bytes");
    assert_eq!(a, c, "supersample 1 alone matches the default bytes");
}

/// Small-text composition where supersampling must show: two tiny
/// lines on a 640x480 canvas, so the 480p preset delivers canvas-size
/// frames and only the internal multiple differs.
fn small_text_project(root: &Path) -> PathBuf {
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=640 height=480 duration=1 fps=15 background=\"#112233\" version=1 {\n    text value=\"Sharp small text 0123456789\" start=0 duration=1 x=40 y=60 size=14 color=\"#ffffff\"\n    text value=\"Second line of tiny words here\" start=0 duration=1 x=40 y=100 size=12 color=\"#ffff00\"\n}\n",
    );
    folder
}

/// Mean solid-ink fraction and mean gradient energy of the text
/// region of one decoded frame.
fn text_metrics(raw: &[u8], w: u32, h: u32) -> (f64, f64) {
    assert_eq!(raw.len(), w as usize * h as usize * 3);
    let w = w as usize;
    // The two text lines live in rows 40..140, columns 20..620.
    let mut ink = 0usize;
    let mut total = 0usize;
    let mut grad = 0u64;
    let at = |x: usize, y: usize| -> [u8; 3] {
        let o = (y * w + x) * 3;
        [raw[o], raw[o + 1], raw[o + 2]]
    };
    for y in 40..140usize {
        for x in 20..620usize {
            let p = at(x, y);
            let luma = (p[0] as u32 + p[1] as u32 + p[2] as u32) / 3;
            total += 1;
            if luma > 200 {
                ink += 1;
            }
            if x + 1 < 620 {
                let q = at(x + 1, y);
                grad += (p[0] as i32 - q[0] as i32).unsigned_abs() as u64
                    + (p[1] as i32 - q[1] as i32).unsigned_abs() as u64
                    + (p[2] as i32 - q[2] as i32).unsigned_abs() as u64;
            }
            if y + 1 < 140 {
                let q = at(x, y + 1);
                grad += (p[0] as i32 - q[0] as i32).unsigned_abs() as u64
                    + (p[1] as i32 - q[1] as i32).unsigned_abs() as u64
                    + (p[2] as i32 - q[2] as i32).unsigned_abs() as u64;
            }
        }
    }
    (ink as f64 / total as f64, grad as f64 / total as f64)
}

#[test]
fn supersampling_sharpens_small_text_at_fixed_output_size() {
    let root = tmp("sharp");
    small_text_project(&root);
    // Lossless PNG frames isolate the scaling sharpness: the encoder
    // never smooths the difference away.
    let (code, out) = run(
        &[
            "render",
            "proj",
            "--format",
            "pngseq",
            "--resolution",
            "480p",
            "--supersample",
            "1",
            "-o",
            "p1",
        ],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(
        &[
            "render",
            "proj",
            "--format",
            "pngseq",
            "--resolution",
            "480p",
            "--supersample",
            "4",
            "-o",
            "p4",
        ],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    // Both deliver the same canvas-size frames, fifteen each.
    for dir in ["p1", "p4"] {
        let names: Vec<String> = std::fs::read_dir(root.join(dir))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names.len(), 15, "{dir} holds every frame");
        let probe = streams(&root.join(dir).join("frame-0000.png"));
        assert!(probe.contains("width=640"), "{probe}");
        assert!(probe.contains("height=480"), "{probe}");
    }
    let (raw1, raw4) = (
        decode_rgb(&root.join("p1/frame-0000.png"), 1, 640, 480),
        decode_rgb(&root.join("p4/frame-0000.png"), 1, 640, 480),
    );
    let (ink1, grad1) = text_metrics(&raw1, 640, 480);
    let (ink4, grad4) = text_metrics(&raw4, 640, 480);
    assert!(ink1 > 0.0 && ink4 > 0.0, "both render solid text ink");
    // Supersampled stems stay filled instead of dropping out, and the
    // edge energy rises at the same output size.
    assert!(
        ink4 / ink1 > 1.15,
        "4x keeps more solid ink ({ink4:.4} vs {ink1:.4})"
    );
    assert!(
        grad4 / grad1 > 1.02,
        "4x carries more edge energy ({grad4:.3} vs {grad1:.3})"
    );
}

#[test]
fn alpha_compositing_survives_scaling() {
    let root = tmp("alpha-scale");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    // Half-transparent red still over color: decoded RGBA carries
    // alpha < 255, compositing happens at draw time.
    let out = Command::new("ffmpeg")
        .args([
            "-y",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "nullsrc=size=32x32:rate=1",
            "-vf",
            "format=rgba,geq=r=255:g=0:b=0:a=128",
            "-frames:v",
            "1",
            "-pix_fmt",
            "rgba",
            &folder.join("half.png").display().to_string(),
        ])
        .output()
        .expect("write alpha still");
    assert!(out.status.success());
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=640 height=480 duration=1 fps=15 background=\"#112233\" version=1 {\n    image path=\"half.png\" start=0 duration=1 x=100 y=100 width=200 height=150 fit=\"fill\" placement=\"top-left\" order=1\n}\n",
    );
    let (code, out) = run(&["render", "proj", "-o", "r0.mp4"], &root);
    assert_eq!(code, 0, "{out}");
    // 480p on a 640x480 canvas delivers canvas-size frames through
    // the internal 2x multiple, so both pictures compare directly.
    let (code, out) = run(
        &[
            "render",
            "proj",
            "--resolution",
            "480p",
            "--supersample",
            "2",
            "-o",
            "r2.mp4",
        ],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    let (raw0, raw2) = (
        decode_rgb(&root.join("r0.mp4"), 1, 640, 480),
        decode_rgb(&root.join("r2.mp4"), 1, 640, 480),
    );
    // The overlay interior composites identically through the scale
    // step: a lost alpha plane would show the background instead.
    let overlay = |raw: &[u8]| -> Vec<u8> {
        let mut v = Vec::new();
        for y in 150..250usize {
            for x in 150..350usize {
                let o = (y * 640 + x) * 3;
                v.extend_from_slice(&raw[o..o + 3]);
            }
        }
        v
    };
    let (o0, o2) = (overlay(&raw0), overlay(&raw2));
    let diff: u64 = o0
        .iter()
        .zip(o2.iter())
        .map(|(a, b)| (*a as i32 - *b as i32).unsigned_abs() as u64)
        .sum();
    let mean_diff = diff as f64 / o0.len() as f64;
    assert!(
        mean_diff < 1.0,
        "alpha composite survives scaling ({mean_diff:.3})"
    );
    // And the composite really is a blend, not opaque red or missing.
    let mean = |raw: &[u8]| -> (f64, f64, f64) {
        let (mut r, mut g, mut b) = (0u64, 0u64, 0u64);
        for px in raw.chunks_exact(3) {
            r += px[0] as u64;
            g += px[1] as u64;
            b += px[2] as u64;
        }
        let n = raw.len() as f64 / 3.0;
        (r as f64 / n, g as f64 / n, b as f64 / n)
    };
    let (r, g, b) = mean(&o2);
    let dist_bg = ((r - 0x11 as f64).abs() + (g - 0x22 as f64).abs() + (b - 0x33 as f64).abs()) / 3.0;
    let dist_red = ((r - 255.0).abs() + g.abs() + b.abs()) / 3.0;
    assert!(dist_bg > 30.0, "blended, not background ({r:.1},{g:.1},{b:.1})");
    assert!(dist_red > 30.0, "blended, not opaque red ({r:.1},{g:.1},{b:.1})");
}

#[test]
fn scaled_gif_and_pngseq_deliver_at_preset_size() {
    let root = tmp("sequences");
    project(&root);
    // GIF at 480p: loops the composition at the preset size.
    let (code, out) = run(
        &[
            "render",
            "proj",
            "--format",
            "gif",
            "--resolution",
            "480p",
            "-o",
            "s.gif",
        ],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    let out = Command::new("ffmpeg")
        .args([
            "-y",
            "-v",
            "error",
            "-i",
            &root.join("s.gif").display().to_string(),
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgb24",
            "-",
        ])
        .output()
        .expect("decode gif");
    assert!(out.status.success());
    assert_eq!(out.stdout.len(), 15 * 640 * 480 * 3);
    // PNG sequences number every frame at the preset size.
    let (code, out) = run(
        &[
            "render",
            "proj",
            "--format",
            "pngseq",
            "--resolution",
            "720p",
            "-o",
            "stills",
        ],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    let names: Vec<String> = std::fs::read_dir(root.join("stills"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names.len(), 15);
    let probe = streams(&root.join("stills/frame-0000.png"));
    assert!(probe.contains("width=960"), "{probe}");
    assert!(probe.contains("height=720"), "{probe}");
}

#[test]
fn unknown_preset_fails_naming_accepted_values() {
    let root = tmp("bad-preset");
    project(&root);
    let (code, out) = run(
        &["render", "proj", "--resolution", "8k", "-o", "x.mp4"],
        &root,
    );
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("--resolution"), "{out}");
    assert!(
        out.contains("source, 1080p, 720p, 480p, square-1080, or 4k"),
        "{out}"
    );
    assert!(out.contains("no output was written"), "{out}");
    assert!(!root.join("x.mp4").exists(), "nothing written");
    assert!(
        !root.join("x.mp4.framesmith-part").exists(),
        "no staged file"
    );
    // A duplicated flag is a usage error, before anything renders.
    let (code, out) = run(
        &[
            "render",
            "proj",
            "--resolution",
            "720p",
            "--resolution",
            "480p",
            "-o",
            "y.mp4",
        ],
        &root,
    );
    assert_eq!(code, 2, "{out}");
    assert!(!root.join("y.mp4").exists(), "nothing written");
}

#[test]
fn bad_supersample_fails_naming_range() {
    let root = tmp("bad-ss");
    project(&root);
    for bad in ["0", "5", "2.5", "abc"] {
        let (code, out) = run(
            &["render", "proj", "--supersample", bad, "-o", "x.mp4"],
            &root,
        );
        assert_eq!(code, 1, "supersample {bad}: {out}");
        assert!(out.contains("--supersample"), "{out}");
        assert!(out.contains("from 1 to 4"), "{out}");
        assert!(out.contains("no output was written"), "{out}");
        assert!(!root.join("x.mp4").exists(), "nothing written");
        assert!(
            !root.join("x.mp4.framesmith-part").exists(),
            "no staged file"
        );
    }
}

#[test]
fn json_report_names_resolution_and_supersample() {
    let root = tmp("json-scale");
    project(&root);
    let out = Command::new(bin())
        .args([
            "render",
            "proj",
            "--resolution",
            "720p",
            "--supersample",
            "2",
            "-o",
            "j.mp4",
            "--json",
        ])
        .current_dir(&root)
        .output()
        .expect("run framesmith binary");
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(stdout.contains("\"resolution\":\"720p\""), "{stdout}");
    assert!(stdout.contains("\"supersample\":2"), "{stdout}");
    let probe = streams(&root.join("j.mp4"));
    assert!(probe.contains("width=960"), "{probe}");
    assert!(probe.contains("height=720"), "{probe}");
}
