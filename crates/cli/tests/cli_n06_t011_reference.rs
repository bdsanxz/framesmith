//! Sprint 6 T011 end-to-end: the committed structured reference project.
//!
//! `examples/structured-reference` exercises the sprint in one place —
//! relative starts, a twice-used nested bumper with differing trims, a
//! hidden layer, a canvas preset, inferred durations, and two
//! compositions. These tests stage that exact example (never the live
//! checkout dir) and prove it validates clean with the authored
//! windows, renders root plus named plus sub-range MP4s with exact
//! windows and aligned sound, re-renders byte-identically, and refuses
//! its invalid variants before frame one. Host ffmpeg/ffprobe only.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-n06t011-{tag}-{}-{}",
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

fn run_stdout(args: &[&str], cwd: &Path) -> (i32, String) {
    let out = Command::new(bin())
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("run framesmith binary");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    )
}

fn write(folder: &Path, name: &str, content: &str) {
    std::fs::create_dir_all(folder).unwrap();
    std::fs::write(folder.join(name), content).unwrap();
}

/// Stage the committed reference example to a temp dir (composition
/// plus media), so the checkout itself is never written.
fn stage_reference(tag: &str) -> (PathBuf, PathBuf) {
    let root = tmp(tag);
    let src = manifest_dir().join("examples/structured-reference");
    let dst = root.join("ref");
    std::fs::create_dir_all(&dst).unwrap();
    std::fs::copy(src.join("composition.kdl"), dst.join("composition.kdl")).unwrap();
    std::fs::create_dir_all(dst.join("media")).unwrap();
    std::fs::copy(src.join("media/tone.wav"), dst.join("media/tone.wav")).unwrap();
    (root, dst)
}

fn decode_video(mp4: &Path, w: u32, h: u32) -> Vec<u8> {
    let out = Command::new("ffmpeg")
        .args([
            "-y",
            "-v",
            "error",
            "-i",
            &mp4.display().to_string(),
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
        .expect("decode mp4 with host ffmpeg");
    assert!(out.status.success());
    out.stdout
}

fn decode_audio(mp4: &Path) -> Vec<i16> {
    let out = Command::new("ffmpeg")
        .args([
            "-y",
            "-v",
            "error",
            "-i",
            &mp4.display().to_string(),
            "-map",
            "0:a",
            "-f",
            "s16le",
            "-ac",
            "2",
            "-ar",
            "48000",
            "-",
        ])
        .output()
        .expect("decode audio with host ffmpeg");
    assert!(out.status.success());
    let bytes = out.stdout;
    assert!(bytes.len() % 2 == 0);
    bytes
        .chunks_exact(2)
        .map(|c| i16::from_le_bytes([c[0], c[1]]))
        .collect()
}

fn center(frames: &[u8], frame: usize, w: u32, h: u32) -> (u8, u8, u8) {
    let stride = (w * h * 3) as usize;
    let at = frame * stride + ((h / 2 * w + w / 2) * 3) as usize;
    (frames[at], frames[at + 1], frames[at + 2])
}

fn near(p: (u8, u8, u8), want: (u8, u8, u8)) -> bool {
    const TOL: i16 = 60;
    (p.0 as i16 - want.0 as i16).abs() <= TOL
        && (p.1 as i16 - want.1 as i16).abs() <= TOL
        && (p.2 as i16 - want.2 as i16).abs() <= TOL
}

fn rms(samples: &[i16]) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum: f64 = samples.iter().map(|&x| (x as f64) * (x as f64)).sum();
    (sum / samples.len() as f64).sqrt()
}

// --- structure: check, timeline, info ----------------------------------------

#[test]
fn reference_validates_clean_with_the_authored_windows() {
    let (root, _) = stage_reference("structure");
    let (code, out) = run(&["check", "ref"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("is valid"), "{out}");
    assert!(out.contains("inferred audio"), "{out}");
    assert!(
        out.contains("inferred until composition end 4.000s"),
        "{out}"
    );
    assert!(out.contains("inferred composition \"main\""), "{out}");
    assert!(out.contains("inferred composition \"cutdown\""), "{out}");

    let (code, out) = run(&["compositions", "ref"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("\"main\" (root, 7 clip(s))"), "{out}");
    assert!(out.contains("\"bumper\" (2 clip(s))"), "{out}");
    assert!(out.contains("\"cutdown\" (1 clip(s))"), "{out}");

    // Every authored window appears verbatim in the timeline JSON:
    // the relative chain [0,1)+[1,2)+[2,3)+[3,4), the hidden ghost
    // listed at [1,2), and the inferred still/audio spans [0,4).
    let (code, json) = run_stdout(&["timeline", "--json", "ref"], &root);
    assert_eq!(code, 0, "{json}");
    for needle in [
        "\"kind\":\"box\",\"id\":\"intro\",\"track\":\"video\",\"label\":\"cold open\",\"hidden\":false,\"start\":0,\"end\":1",
        "\"kind\":\"nest\",\"id\":\"bumper-a\",\"track\":\"video\",\"label\":\"bumper red half\",\"hidden\":false,\"start\":1,\"end\":2",
        "\"kind\":\"nest\",\"id\":\"bumper-b\",\"track\":\"video\",\"label\":\"bumper green half\",\"hidden\":false,\"start\":2,\"end\":3",
        "\"kind\":\"box\",\"id\":\"outro\",\"track\":\"video\",\"label\":\"close\",\"hidden\":false,\"start\":3,\"end\":4",
        "\"kind\":\"box\",\"id\":\"ghost\",\"track\":\"video\",\"label\":\"cut layer\",\"hidden\":true,\"start\":1,\"end\":2",
        "\"kind\":\"text\",\"id\":\"title\",\"track\":\"graphics\",\"label\":\"super\",\"hidden\":false,\"start\":0,\"end\":4",
        "\"kind\":\"audio\",\"id\":\"bed\",\"track\":\"audio\",\"label\":\"room tone\",\"hidden\":false,\"start\":0,\"end\":4",
        "\"composition\":\"main\",\"root\":\"main\"",
        "\"duration\":4,\"fps\":\"30\",\"frames\":120",
    ] {
        assert!(json.contains(needle), "timeline misses {needle}: {json}");
    }

    // Both infos carry the canvas, preset, and inferred duration, and
    // the JSON is balanced (the top-level object closes).
    let (code, json) = run_stdout(&["info", "--json", "ref"], &root);
    assert_eq!(code, 0, "{json}");
    assert!(
        json.contains("\"canvas\":{\"width\":64,\"height\":64,\"preset\":null}"),
        "{json}"
    );
    assert!(
        json.contains("\"duration\":4,\"fps\":\"30\",\"frames\":120"),
        "{json}"
    );
    assert!(json.contains("\"hidden\":1"), "{json}");
    assert_eq!(
        json.matches('{').count(),
        json.matches('}').count(),
        "{json}"
    );
    assert!(json.trim_end().ends_with("\"hidden\":1}}"), "{json}");

    let (code, json) = run_stdout(
        &["info", "--json", "--composition", "cutdown", "ref"],
        &root,
    );
    assert_eq!(code, 0, "{json}");
    assert!(
        json.contains("\"canvas\":{\"width\":1080,\"height\":1080,\"preset\":\"square\"}"),
        "{json}"
    );
    assert!(
        json.contains("\"duration\":1,\"fps\":\"30\",\"frames\":30"),
        "{json}"
    );
    assert_eq!(
        json.matches('{').count(),
        json.matches('}').count(),
        "{json}"
    );
}

// --- renders: exact windows, aligned sound, determinism ---------------------

#[test]
fn reference_renders_root_named_and_range_with_aligned_sound() {
    let (root, _) = stage_reference("renders");

    let (code, out) = run(&["render", "ref", "-o", "root.mp4"], &root);
    assert_eq!(code, 0, "{out}");
    let frames = decode_video(&root.join("root.mp4"), 64, 64);
    assert_eq!(frames.len(), 120 * 64 * 64 * 3, "root holds 120 frames");
    for i in [0, 29, 30, 59] {
        assert!(
            near(center(&frames, i, 64, 64), (255, 0, 0)),
            "frame {i} is intro red"
        );
    }
    // The hidden ghost is green over [1,2): red here proves it stayed out.
    for i in [60, 74, 89] {
        assert!(
            near(center(&frames, i, 64, 64), (0, 255, 0)),
            "frame {i} is bumper green, ghost hidden"
        );
    }
    for i in [90, 104, 119] {
        assert!(
            near(center(&frames, i, 64, 64), (0, 0, 255)),
            "frame {i} is outro blue"
        );
    }
    let mix = decode_audio(&root.join("root.mp4"));
    // AAC priming/trim differs across FFmpeg versions (up to one
    // 1024-sample frame of overshoot on older builds).
    let samples = mix.len() / 2;
    assert!(
        (4 * 48000..=4 * 48000 + 1024).contains(&samples),
        "root mix spans 4 s stereo, got {samples}"
    );
    for (q, slice) in mix.chunks(mix.len() / 4).enumerate() {
        assert!(rms(slice) > 1500.0, "quarter {q} carries the bed tone");
    }

    let (code, out) = run(
        &["render", "ref", "-o", "cut.mp4", "--composition", "cutdown"],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    let cut = decode_video(&root.join("cut.mp4"), 1080, 1080);
    assert_eq!(cut.len(), 30 * 1080 * 1080 * 3, "cutdown holds 30 frames");
    let at = 15 * 1080 * 1080 * 3 + (540 * 1080 + 540) * 3;
    let px = (cut[at], cut[at + 1], cut[at + 2]);
    assert!(
        near(px, (17, 34, 51)),
        "cutdown center is #112233, got {px:?}"
    );

    let (code, out) = run(
        &[
            "render",
            "ref",
            "-o",
            "range.mp4",
            "--start",
            "1",
            "--end",
            "3",
        ],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("[range 1.000s..3.000s]"), "{out}");
    let ranged = decode_video(&root.join("range.mp4"), 64, 64);
    assert_eq!(ranged.len(), 60 * 64 * 64 * 3, "range holds 60 frames");
    assert!(
        near(center(&ranged, 0, 64, 64), (255, 0, 0)),
        "range starts red"
    );
    assert!(
        near(center(&ranged, 29, 64, 64), (255, 0, 0)),
        "range stays red"
    );
    assert!(
        near(center(&ranged, 30, 64, 64), (0, 255, 0)),
        "range cuts green"
    );
    assert!(
        near(center(&ranged, 59, 64, 64), (0, 255, 0)),
        "range ends green"
    );
    assert!(
        rms(&decode_audio(&root.join("range.mp4"))) > 1500.0,
        "range tone throughout"
    );

    // Re-renders are byte-identical.
    for (name, extra) in [
        ("root.mp4", vec![]),
        ("cut.mp4", vec!["--composition", "cutdown"]),
        ("range.mp4", vec!["--start", "1", "--end", "3"]),
    ] {
        let before = std::fs::read(root.join(name)).unwrap();
        let mut args = vec!["render", "ref", "-o", "again.mp4"];
        args.extend(extra);
        let (code, out) = run(&args, &root);
        assert_eq!(code, 0, "{out}");
        let after = std::fs::read(root.join("again.mp4")).unwrap();
        assert!(
            !before.is_empty() && before == after,
            "{name} re-render differs"
        );
    }
}

// --- refusals: invalid variants stop before frame one -----------------------

fn write_variant(root: &Path, tag: &str, kdl: &str) -> PathBuf {
    let folder = root.join(tag);
    write(&folder, "composition.kdl", kdl);
    folder
}

#[test]
fn reference_invalid_variants_refuse_before_frame_one() {
    let (root, _) = stage_reference("refusals");

    // (tag, kdl, check needle): each must fail check AND render with
    // exit 1 and leave no MP4 behind.
    let structural = [
        (
            "dup-id",
            "composition name=\"m\" width=64 height=64 duration=2 fps=30 background=\"#000000\" version=1 root=true {\nbox x=0 y=0 width=64 height=64 fill=\"#ff0000\" start=0 duration=1 id=\"dup\"\nbox x=0 y=0 width=64 height=64 fill=\"#0000ff\" start=1 duration=1 id=\"dup\"\n}",
            "E_DUP_ID",
        ),
        (
            "cycle",
            "composition name=\"m\" width=64 height=64 duration=2 fps=30 background=\"#000000\" version=1 root=true {\nbox x=0 y=0 width=64 height=64 fill=\"#ff0000\" duration=1 id=\"a\" start-after=\"b\"\nbox x=0 y=0 width=64 height=64 fill=\"#0000ff\" duration=1 id=\"b\" start-after=\"a\"\n}",
            "chain cycles",
        ),
        (
            "no-root",
            "composition name=\"m\" width=64 height=64 duration=1 fps=30 background=\"#000000\" version=1 {\nbox x=0 y=0 width=64 height=64 fill=\"#ff0000\" start=0 duration=1\n}\ncomposition name=\"c\" width=64 height=64 duration=1 fps=30 background=\"#000000\" version=1 {\nbox x=0 y=0 width=64 height=64 fill=\"#0000ff\" start=0 duration=1\n}",
            "E_AMBIGUOUS_ROOT",
        ),
        (
            "tiny",
            "composition name=\"m\" width=64 height=64 duration=2 fps=30 background=\"#000000\" version=1 root=true {\nbox x=0 y=0 width=64 height=64 fill=\"#ff0000\" start=0 duration=0.01\nbox x=0 y=0 width=64 height=64 fill=\"#0000ff\" start=1 duration=1\n}",
            "below the 1/60s minimum",
        ),
        (
            "circular-nest",
            "composition name=\"m\" width=64 height=64 duration=2 fps=30 background=\"#000000\" version=1 root=true {\nnest nest=\"m\" start=0 duration=1\nbox x=0 y=0 width=64 height=64 fill=\"#0000ff\" start=1 duration=1\n}",
            "chain cycles",
        ),
    ];
    for (tag, kdl, needle) in structural {
        let folder = write_variant(&root, tag, kdl);
        let name = folder.file_name().unwrap().to_str().unwrap().to_string();
        let (code, out) = run(&["check", &name], &root);
        assert_eq!(code, 1, "{tag} check must refuse: {out}");
        assert!(out.contains(needle), "{tag} check names {needle}: {out}");
        let mp4 = format!("{tag}.mp4");
        let (code, out) = run(&["render", &name, "-o", &mp4], &root);
        assert_eq!(code, 1, "{tag} render must refuse: {out}");
        assert!(out.contains(needle), "{tag} render names {needle}: {out}");
        assert!(!root.join(&mp4).exists(), "{tag} left a partial MP4");
        assert!(!folder.join("out.mp4").exists(), "{tag} left a default MP4");
    }

    // The empty sub-range refuses on the otherwise-valid reference.
    let (code, out) = run(
        &[
            "render",
            "ref",
            "-o",
            "empty.mp4",
            "--start",
            "3",
            "--end",
            "3",
        ],
        &root,
    );
    assert_eq!(code, 1, "empty range must refuse: {out}");
    assert!(out.contains("the window is empty"), "{out}");
    assert!(
        out.contains("accepted window is 0 <= start < end <= 4.000s"),
        "{out}"
    );
    assert!(
        !root.join("empty.mp4").exists(),
        "empty range left a partial MP4"
    );
}
