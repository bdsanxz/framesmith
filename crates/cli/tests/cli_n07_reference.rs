//! Sprint 7 motion (T012) end-to-end: the committed motion reference
//! project exercises keyframes with mixed easings, stagger, repeat
//! with yoyo, a 3D turn, path motion with orientation, split
//! typewriter text, and a seeded expression in one place. These tests
//! stage that exact example (never the live checkout dir) and prove
//! it validates clean, renders frame-exact values at sampled
//! instants, re-renders byte-identically, refuses every invalid
//! motion variant before frame one, and nests plus range-renders in
//! agreement with the full render. Host ffmpeg only.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-n07ref-{tag}-{}-{}",
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

fn write(folder: &Path, name: &str, content: &str) {
    std::fs::create_dir_all(folder).unwrap();
    std::fs::write(folder.join(name), content).unwrap();
}

/// Stage the committed motion reference to a temp dir (composition
/// only; the checkout itself is never written).
fn stage_reference(tag: &str) -> (PathBuf, PathBuf) {
    let root = tmp(tag);
    let src = manifest_dir().join("examples/motion-reference");
    let dst = root.join("ref");
    std::fs::create_dir_all(&dst).unwrap();
    std::fs::copy(src.join("composition.kdl"), dst.join("composition.kdl")).unwrap();
    (root, dst)
}

fn decode(mp4: &Path, frames: usize, w: u32, h: u32) -> Vec<u8> {
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
            "-s",
            &format!("{w}x{h}"),
            "-",
        ])
        .output()
        .expect("decode mp4 with host ffmpeg");
    assert!(out.status.success());
    out.stdout
}

const W: u32 = 320;
const H: u32 = 280;

fn frame_of(raw: &[u8], k: usize) -> &[u8] {
    let len = W as usize * H as usize * 3;
    &raw[k * len..(k + 1) * len]
}

/// Mean-x of strongly red pixels restricted to a row band.
fn red_cx(frame: &[u8], y0: u32, y1: u32) -> Option<f64> {
    let (mut sx, mut n) = (0u64, 0u64);
    for y in y0..y1 {
        for x in 0..W {
            let o = (y as usize * W as usize + x as usize) * 3;
            if frame[o] > 150 && frame[o + 1] < 100 && frame[o + 2] < 100 {
                sx += x as u64;
                n += 1;
            }
        }
    }
    if n == 0 {
        None
    } else {
        Some(sx as f64 / n as f64)
    }
}

/// Mean-x of strongly green pixels restricted to a row band.
fn green_cx(frame: &[u8], y0: u32, y1: u32) -> Option<f64> {
    let (mut sx, mut n) = (0u64, 0u64);
    for y in y0..y1 {
        for x in 0..W {
            let o = (y as usize * W as usize + x as usize) * 3;
            if frame[o + 1] > 150 && frame[o] < 100 && frame[o + 2] < 100 {
                sx += x as u64;
                n += 1;
            }
        }
    }
    if n == 0 {
        None
    } else {
        Some(sx as f64 / n as f64)
    }
}

/// Height of the blue run in one column (rows 50..102).
fn blue_height(frame: &[u8], col: u32) -> u32 {
    (50..102)
        .filter(|&y| {
            let o = (y as usize * W as usize + col as usize) * 3;
            frame[o + 2] > 100 && frame[o] < 100 && frame[o + 1] < 100
        })
        .count() as u32
}

/// Bounding box of yellow pixels in the path-travel band.
fn yellow_bbox(frame: &[u8]) -> Option<(u32, u32, u32, u32)> {
    let mut xs = Vec::new();
    let mut ys = Vec::new();
    for y in 100..200 {
        for x in 0..W {
            let o = (y as usize * W as usize + x as usize) * 3;
            if frame[o] > 150 && frame[o + 1] > 150 && frame[o + 2] < 100 {
                xs.push(x);
                ys.push(y);
            }
        }
    }
    if xs.is_empty() {
        return None;
    }
    Some((
        *xs.iter().min().unwrap(),
        *xs.iter().max().unwrap(),
        *ys.iter().min().unwrap(),
        *ys.iter().max().unwrap(),
    ))
}

/// White-ink pixels in the typewriter band (rows 195..235).
fn white_ink(frame: &[u8]) -> u64 {
    let mut n = 0u64;
    for y in 195..235 {
        for x in 0..W {
            let o = (y as usize * W as usize + x as usize) * 3;
            if frame[o] > 150 && frame[o + 1] > 150 && frame[o + 2] > 150 {
                n += 1;
            }
        }
    }
    n
}

/// Red rows in the replicate band (rows 236..280).
fn replicate_rows(frame: &[u8]) -> Vec<u32> {
    (236..280)
        .filter(|&y| {
            (0..W).any(|x| {
                let o = (y as usize * W as usize + x as usize) * 3;
                frame[o] > 150 && frame[o + 1] < 100 && frame[o + 2] < 100
            })
        })
        .collect()
}

#[test]
fn reference_renders_frame_exact_values_and_rerenders_identically() {
    // T012: the reference validates clean, every sampled instant
    // matches its hand-computed value, and the re-render is
    // byte-identical.
    let (_root, folder) = stage_reference("exact");
    let (status, out) = run(&["check", "."], &folder);
    assert_eq!(status, 0, "reference must validate clean: {out}");
    assert_eq!(run(&["render", ".", "-o", "out.mp4"], &folder).0, 0);
    let raw = decode(&folder.join("out.mp4"), 120, W, H);

    // Keyframes with mixed easings: stops 10/70/130 at 0/0.5/1s over
    // a 24-wide box (center sits 12 right of x). Segment one is
    // linear, segment two is in-quad.
    let key = |k: usize| red_cx(frame_of(&raw, k), 0, 28).unwrap();
    assert!(
        (key(7) - 50.0).abs() < 2.5,
        "linear segment, got {}",
        key(7)
    );
    assert!((key(15) - 82.0).abs() < 2.0, "middle stop, got {}", key(15));
    let seg2 = 70.0 + 60.0 * (7.0f64 / 15.0).powi(2) + 12.0;
    assert!(
        (key(22) - seg2).abs() < 3.0,
        "in-quad segment, got {}",
        key(22)
    );
    assert!((key(30) - 142.0).abs() < 2.0, "last stop, got {}", key(30));

    // Repeat with yoyo: from-to 10 -> 130 over [0, 1), two passes.
    let yoyo = |k: usize| green_cx(frame_of(&raw, k), 26, 54).unwrap();
    assert!(
        (yoyo(15) - 82.0).abs() < 3.0,
        "forward midpoint, got {}",
        yoyo(15)
    );
    assert!(
        (yoyo(30) - 142.0).abs() < 3.0,
        "turnaround, got {}",
        yoyo(30)
    );
    assert!(
        (yoyo(45) - yoyo(15)).abs() < 2.0,
        "reversed pass mirrors the first ({} vs {})",
        yoyo(45),
        yoyo(15)
    );

    // 3D turn: rotation-x 0 -> 60 over [0, 1) halves the 40-tall box
    // about its center row (height 40*cos here).
    assert_eq!(
        blue_height(frame_of(&raw, 0), 80),
        40,
        "flat box is full height"
    );
    assert!(
        (blue_height(frame_of(&raw, 15), 80) as f64 - 34.6).abs() < 3.5,
        "mid-turn foreshortens, got {}",
        blue_height(frame_of(&raw, 15), 80)
    );
    assert!(
        (blue_height(frame_of(&raw, 30), 80) as f64 - 20.0).abs() < 2.5,
        "60-degree turn halves, got {}",
        blue_height(frame_of(&raw, 30), 80)
    );

    // Path with orientation: M150,110 L150,150 over [0, 2); at frame
    // 30 the top-left rides (150, 130) and the 20x60 box, rotated 90
    // degrees to the south tangent, spans 60 wide and 20 tall.
    let (x0, x1, y0, y1) = yellow_bbox(frame_of(&raw, 30)).unwrap();
    assert!(
        ((x1 - x0 + 1) as f64 - 60.0).abs() < 4.0,
        "orient spans wide, got {}",
        x1 - x0 + 1
    );
    assert!(
        ((y1 - y0 + 1) as f64 - 20.0).abs() < 4.0,
        "orient spans flat, got {}",
        y1 - y0 + 1
    );
    assert!((x0 as f64 - 130.0).abs() < 3.0, "travel x, got {x0}");
    assert!((y0 as f64 - 150.0).abs() < 3.0, "travel y, got {y0}");

    // Split typewriter text reveals from nothing to full.
    assert_eq!(white_ink(frame_of(&raw, 0)), 0, "typewriter starts empty");
    assert!(
        white_ink(frame_of(&raw, 30)) > 100,
        "text reveals by frame 30"
    );
    assert!(
        white_ink(frame_of(&raw, 119)) > white_ink(frame_of(&raw, 30)),
        "the last frame shows more than frame 30"
    );

    // Seeded expression: clone rows sit at 236 + i*12 + the SplitMix64
    // hand draws for seeds 0/1/2 scaled to [0, 6) (5.30/3.40/3.55),
    // so the three 10-tall clones start at rows 242/252/264 and never
    // move (no `t` in the expression).
    let rows = replicate_rows(frame_of(&raw, 0));
    assert_eq!(
        rows.len(),
        30,
        "three 10-tall clones draw, got {}",
        rows.len()
    );
    assert!(
        (rows[0] as f64 - 241.3).abs() < 1.5,
        "seed 0 row, got {}",
        rows[0]
    );
    assert_eq!(
        replicate_rows(frame_of(&raw, 119)),
        rows,
        "seeded rows are static across the window"
    );

    // Re-render is byte-identical.
    assert_eq!(run(&["render", ".", "-o", "again.mp4"], &folder).0, 0);
    assert_eq!(
        std::fs::read(folder.join("out.mp4")).unwrap(),
        std::fs::read(folder.join("again.mp4")).unwrap(),
        "the reference re-renders byte-identically"
    );
}

#[test]
fn reference_refusals_fail_before_frame_one() {
    // T012/REC-001..004: one invalid variant per motion feature fails
    // check and render with its code (plus file, line, column, and
    // fix) and writes no MP4. Exit 2 only for the later-sprint
    // filter, which names its sprint.
    const HEAD: &str = "composition name=\"t\" width=160 height=120 duration=2 fps=30 background=\"#000000\" version=1 {\n";
    const BOX: &str = "    box x=10 y=40 width=40 height=40 fill=\"#ff0000\" start=0 duration=2\n";
    let cases: &[(&str, &str, &str, i32)] = &[
        (
            "unordered-stops",
            "    key target=0 property=\"x\" {\n        frame at=1 value=110\n        frame at=0.5 value=60\n    }\n",
            "E_BAD_MOTION",
            1,
        ),
        (
            "duplicate-stops",
            "    key target=0 property=\"x\" {\n        frame at=0.5 value=60\n        frame at=0.5 value=110\n    }\n",
            "E_BAD_MOTION",
            1,
        ),
        (
            "unknown-property",
            "    key target=0 property=\"zebra\" {\n        frame at=0 value=1\n        frame at=1 value=2\n    }\n",
            "E_BAD_MOTION",
            1,
        ),
        (
            "unknown-ease",
            "    tween target=0 property=\"x\" kind=\"to\" to=10 start=0 duration=1 ease=\"nope\"\n",
            "E_BAD_EASING",
            1,
        ),
        (
            "endless-repeat",
            "    tween target=0 property=\"x\" kind=\"to\" to=10 start=0 duration=1 repeat=0\n",
            "E_BAD_MOTION",
            1,
        ),
        (
            "negative-stagger",
            "    tween target=0 property=\"x\" kind=\"to\" to=10 start=0 duration=1 stagger=-0.5\n",
            "E_BAD_MOTION",
            1,
        ),
        (
            "bad-origin",
            "    tween target=0 property=\"x\" kind=\"to\" to=10 start=0 duration=1 stagger=0.5 stagger-from=\"middle\"\n",
            "E_BAD_MOTION",
            1,
        ),
        (
            "bad-path",
            "    tween target=0 path=\"M10,\" start=0 duration=1\n",
            "E_BAD_MOTION",
            1,
        ),
        (
            "cross-morph",
            "    tween target=0 path=\"M10,40 L110,40\" path-to=\"M10,40 C20,40 30,40 40,40\" start=0 duration=1\n",
            "E_BAD_MOTION",
            1,
        ),
        (
            "orient-alone",
            "    tween target=0 property=\"x\" kind=\"from-to\" from=10 to=110 start=0 duration=1 orient=true\n",
            "E_BAD_MOTION",
            1,
        ),
        (
            "split-on-box",
            "    box x=10 y=40 width=40 height=40 fill=\"#ff0000\" start=0 duration=2 split=\"chars\"\n",
            "E_BAD_MOTION",
            1,
        ),
        (
            "impure-expr",
            "    tween target=0 property=\"x\" expr=\"open(1)\" start=0 duration=1\n",
            "E_BAD_MOTION",
            1,
        ),
        (
            "unseeded-random",
            "    tween target=0 property=\"x\" expr=\"random()\" start=0 duration=1\n",
            "E_BAD_MOTION",
            1,
        ),
        (
            "zero-replicate",
            "    replicate count=0 {\n        box x=10 y=40 width=20 height=20 fill=\"#ff0000\" start=0 duration=2\n    }\n",
            "E_BAD_MOTION",
            1,
        ),
        (
            "flat-perspective",
            "    box x=10 y=40 width=40 height=40 fill=\"#ff0000\" start=0 duration=2 rotation-x=10 perspective=0\n",
            "E_BAD_MOTION",
            1,
        ),
        (
            "later-sprint-filter",
            "    box x=10 y=40 width=40 height=40 fill=\"#ff0000\" start=0 duration=2 filter=\"contrast(2)\"\n",
            "E_UNSUPPORTED",
            2,
        ),
    ];
    for (tag, motion, code, want) in cases {
        let root = tmp(&format!("refuse-{tag}"));
        let folder = root.join("proj");
        write(
            &folder,
            "composition.kdl",
            &format!("{HEAD}{BOX}{motion}}}\n"),
        );
        let (status, out) = run(&["check", "."], &folder);
        assert_eq!(status, *want, "{tag}: check must exit {want}: {out}");
        assert!(
            out.contains(code),
            "{tag}: diagnostics must name {code}: {out}"
        );
        assert!(
            out.contains("error["),
            "{tag}: diagnostics carry the code shape: {out}"
        );
        assert!(
            out.contains("fix:"),
            "{tag}: diagnostics carry a fix: {out}"
        );
        let (rstatus, rout) = run(&["render", ".", "-o", "out.mp4"], &folder);
        assert_eq!(rstatus, *want, "{tag}: render must exit {want}: {rout}");
        assert!(!folder.join("out.mp4").exists(), "{tag}: no MP4 on refusal");
        assert!(
            !folder.join("composition.mp4").exists(),
            "{tag}: no stray MP4"
        );
    }
}

#[test]
fn reference_nest_and_range_match_the_full_render() {
    // T012/J-04: the `wrap` composition nests `main`, and a [1, 3)
    // sub-range re-renders the same instants. Both agree with the
    // full render's motion values; decoded pixels agree up to rare
    // edge/H.264 dust (far below 0.05% of pixels).
    let (_root, folder) = stage_reference("nest-range");
    assert_eq!(run(&["render", ".", "-o", "full.mp4"], &folder).0, 0);
    assert_eq!(
        run(
            &["render", ".", "-o", "wrap.mp4", "--composition", "wrap"],
            &folder
        )
        .0,
        0
    );
    assert_eq!(
        run(
            &[
                "render",
                ".",
                "-o",
                "range.mp4",
                "--start",
                "1",
                "--end",
                "3"
            ],
            &folder
        )
        .0,
        0
    );
    let full = decode(&folder.join("full.mp4"), 120, W, H);
    let wrap = decode(&folder.join("wrap.mp4"), 120, W, H);
    let range = decode(&folder.join("range.mp4"), 60, W, H);

    // Motion values agree across all three renders.
    let key_full = red_cx(frame_of(&full, 30), 0, 28).unwrap();
    let key_wrap = red_cx(frame_of(&wrap, 30), 0, 28).unwrap();
    let key_range = red_cx(frame_of(&range, 0), 0, 28).unwrap();
    assert!(
        (key_full - 142.0).abs() < 2.0,
        "full key stop, got {key_full}"
    );
    assert!(
        (key_wrap - key_full).abs() < 2.0,
        "nest key stop, got {key_wrap}"
    );
    assert!(
        (key_range - key_full).abs() < 2.0,
        "range key stop, got {key_range}"
    );
    let turn_full = blue_height(frame_of(&full, 60), 80);
    assert_eq!(
        turn_full,
        blue_height(frame_of(&wrap, 60), 80),
        "nest holds the turn end"
    );
    assert_eq!(
        turn_full,
        blue_height(frame_of(&range, 30), 80),
        "range holds the turn end"
    );
    assert_eq!(
        yellow_bbox(frame_of(&full, 30)),
        yellow_bbox(frame_of(&wrap, 30)),
        "nest path bbox matches"
    );
    // Range frame 30 is the same instant as full frame 60 (t=2.0s):
    // the path travel has landed on its end hold.
    assert_eq!(
        yellow_bbox(frame_of(&full, 60)),
        yellow_bbox(frame_of(&range, 30)),
        "range path bbox matches"
    );
    // Text raster can differ by a pixel across text engines and
    // processes; exact counts hold on macOS, so the bound allows
    // raster dust while still catching any real divergence.
    let nest_ink = white_ink(frame_of(&full, 90));
    let wrap_ink = white_ink(frame_of(&wrap, 90));
    assert!(
        (nest_ink as i64 - wrap_ink as i64).abs() <= 2,
        "nest typewriter ink matches: {nest_ink} vs {wrap_ink}"
    );
    let full_ink = white_ink(frame_of(&full, 89));
    let range_ink = white_ink(frame_of(&range, 59));
    assert!(
        (full_ink as i64 - range_ink as i64).abs() <= 2,
        "range typewriter ink matches: {full_ink} vs {range_ink}"
    );

    // Decoded pixels agree up to rare edge dust.
    let diff_frac = |a: &[u8], b: &[u8]| -> f64 {
        let n = a
            .chunks_exact(3)
            .zip(b.chunks_exact(3))
            .filter(|(p, q)| {
                (p[0] as i32 - q[0] as i32).abs()
                    + (p[1] as i32 - q[1] as i32).abs()
                    + (p[2] as i32 - q[2] as i32).abs()
                    > 60
            })
            .count();
        n as f64 / (a.len() / 3) as f64
    };
    assert!(
        diff_frac(&full, &wrap) < 0.0005,
        "nest pixels match the full render"
    );
    let full_mid =
        full[30 * W as usize * H as usize * 3..90 * W as usize * H as usize * 3].to_vec();
    assert!(
        diff_frac(&full_mid, &range) < 0.0005,
        "range pixels match the full window"
    );
}
