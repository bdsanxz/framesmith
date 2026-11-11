//! Sprint 7 motion (T010 + T011) end-to-end: pure seeded expressions
//! drive frame-exact motion, `replicate` clones expose `i`,
//! identical seeds render identical bytes, every invalid variant
//! refuses with its code and no MP4, animated sizes reflow wrapped
//! text at the changing frame, and static layouts render
//! byte-identically to unanimated motion.
//!
//! All fixtures use the host `ffmpeg` (like `cli_motion`).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-n07-expr-layout-{tag}-{}-{}",
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

fn frame_of(raw: &[u8], k: usize, w: u32, h: u32) -> &[u8] {
    let len = w as usize * h as usize * 3;
    &raw[k * len..(k + 1) * len]
}

/// Mean-x of strongly red pixels in one raw RGB24 frame.
fn red_centroid_x(frame: &[u8], w: u32) -> Option<f64> {
    let (mut sx, mut n) = (0u64, 0u64);
    for (i, px) in frame.as_chunks::<3>().0.iter().enumerate() {
        if px[0] > 150 && px[1] < 100 && px[2] < 100 {
            sx += (i as u32 % w) as u64;
            n += 1;
        }
    }
    if n == 0 {
        None
    } else {
        Some(sx as f64 / n as f64)
    }
}

/// Rows carrying red ink in one raw RGB24 frame.
fn red_rows(frame: &[u8], w: u32, h: u32) -> Vec<u32> {
    (0..h)
        .filter(|y| {
            (0..w).any(|x| {
                let o = (*y as usize * w as usize + x as usize) * 3;
                frame[o] > 150 && frame[o + 1] < 100 && frame[o + 2] < 100
            })
        })
        .collect()
}

/// Rows carrying any ink (any channel clearly off black).
fn ink_rows(frame: &[u8], w: u32, h: u32) -> Vec<u32> {
    (0..h)
        .filter(|y| {
            (0..w).any(|x| {
                let o = (*y as usize * w as usize + x as usize) * 3;
                frame[o] > 20 || frame[o + 1] > 20 || frame[o + 2] > 20
            })
        })
        .collect()
}

/// Contiguous ink-row runs, splitting on gaps wider than two rows.
fn bands(rows: &[u32]) -> usize {
    let mut n = 0;
    let mut prev = u32::MAX;
    for y in rows {
        if *y < prev || *y - prev > 2 {
            n += 1;
        }
        prev = *y;
    }
    n
}

const HEAD: &str =
    "composition name=\"t\" width=160 height=120 duration=2 fps=30 background=\"#000000\" version=1 {\n";
const BOX: &str = "    box x=10 y=40 width=20 height=20 fill=\"#ff0000\" start=0 duration=2\n";

// --- T010: expressions evaluate per frame ----------------------------------

#[test]
fn expr_drives_frame_exact_motion() {
    // T010: box x reads `10+t*30` over [0, 2): the centroid (10
    // right of x for the 20-wide box) probes 20/50/~88.3, and the
    // re-render is byte-identical.
    let root = tmp("expr-motion");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    write(
        &folder,
        "composition.kdl",
        &format!("{HEAD}{BOX}    tween target=0 property=\"x\" expr=\"10+t*30\" start=0 duration=2\n}}\n"),
    );
    assert_eq!(run(&["check", "proj"], &root).0, 0);
    assert_eq!(run(&["render", "proj", "-o", "out.mp4"], &root).0, 0);
    let raw = decode_raw(&root.join("out.mp4"), 60);
    let cx = |k: usize| red_centroid_x(frame_of(&raw, k, 160, 120), 160).unwrap();
    assert!((cx(0) - 20.0).abs() < 3.0, "expr starts, got {}", cx(0));
    assert!(
        (cx(30) - 50.0).abs() < 3.0,
        "expr midpoints, got {}",
        cx(30)
    );
    assert!((cx(59) - 79.0).abs() < 3.5, "expr ends, got {}", cx(59));
    assert_eq!(run(&["render", "proj", "-o", "again.mp4"], &root).0, 0);
    assert_eq!(
        std::fs::read(root.join("out.mp4")).unwrap(),
        std::fs::read(root.join("again.mp4")).unwrap(),
        "expression motion re-renders byte-identically"
    );
}

#[test]
fn expr_reads_declared_inputs() {
    // T010: `input amp` scales the sway `10+amp*sin(t*6.2832)`: at
    // t=0 the box sits at x=10, near t=0.25 it peaks at x=40.
    let root = tmp("expr-input");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    write(
        &folder,
        "composition.kdl",
        &format!("{HEAD}{BOX}    input name=\"amp\" value=30\n    tween target=0 property=\"x\" expr=\"10+amp*sin(t*6.2832)\" start=0 duration=2\n}}\n"),
    );
    assert_eq!(run(&["check", "proj"], &root).0, 0);
    assert_eq!(run(&["render", "proj", "-o", "out.mp4"], &root).0, 0);
    let raw = decode_raw(&root.join("out.mp4"), 60);
    let cx = |k: usize| red_centroid_x(frame_of(&raw, k, 160, 120), 160).unwrap();
    assert!(
        (cx(0) - 20.0).abs() < 3.0,
        "sway starts centered, got {}",
        cx(0)
    );
    // Frame 7/8 (t~0.25s) rides the sine peak: x~40, centroid ~50.
    let peak = cx(7).max(cx(8));
    assert!((peak - 50.0).abs() < 5.0, "sway peaks with amp, got {peak}");
}

#[test]
fn replicate_clones_expose_i_and_match_across_renders() {
    // T010: three clones read `10+i*30` for y, stacking rows at
    // y=10/40/70; a twin project with the same sources renders
    // identical bytes, so motion never depends on worker count.
    let mk = || {
        format!(
            "{HEAD}    replicate count=3 {{\n        box x=10 y=10 width=20 height=20 fill=\"#ff0000\" start=0 duration=2\n    }}\n    tween target=0 property=\"y\" expr=\"10+i*30\" start=0 duration=2\n    tween target=1 property=\"y\" expr=\"10+i*30\" start=0 duration=2\n    tween target=2 property=\"y\" expr=\"10+i*30\" start=0 duration=2\n}}\n"
        )
    };
    let root = tmp("replicate");
    let a = root.join("a");
    let b = root.join("b");
    std::fs::create_dir_all(&a).unwrap();
    std::fs::create_dir_all(&b).unwrap();
    write(&a, "composition.kdl", &mk());
    write(&b, "composition.kdl", &mk());
    assert_eq!(run(&["check", "."], &a).0, 0);
    assert_eq!(run(&["render", ".", "-o", "out.mp4"], &a).0, 0);
    assert_eq!(run(&["render", ".", "-o", "out.mp4"], &b).0, 0);
    assert_eq!(
        std::fs::read(a.join("out.mp4")).unwrap(),
        std::fs::read(b.join("out.mp4")).unwrap(),
        "identical seeds render identical motion across renders"
    );
    let raw = decode_raw(&a.join("out.mp4"), 60);
    let rows = red_rows(frame_of(&raw, 0, 160, 120), 160, 120);
    assert_eq!(
        rows.len(),
        60,
        "three 20-tall clones draw, got {}",
        rows.len()
    );
    assert_eq!(rows[0], 10, "clone 0 sits at y=10, got {}", rows[0]);
    assert_eq!(rows[20], 40, "clone 1 sits at y=40, got {}", rows[20]);
    assert_eq!(rows[40], 70, "clone 2 sits at y=70, got {}", rows[40]);
}

#[test]
fn seeded_random_scatters_deterministically() {
    // T010: each clone stacks on its own row (`10+i*30`) and takes
    // its x from `random(i,0,100)`; every row's left edge matches the
    // SplitMix64 hand draw within pixel tolerance, and the scatter
    // re-renders byte-identically.
    let mk = || {
        format!(
            "{HEAD}    replicate count=3 {{\n        box x=10 y=10 width=20 height=20 fill=\"#ff0000\" start=0 duration=2\n    }}\n    tween target=0 property=\"y\" expr=\"10+i*30\" start=0 duration=2\n    tween target=1 property=\"y\" expr=\"10+i*30\" start=0 duration=2\n    tween target=2 property=\"y\" expr=\"10+i*30\" start=0 duration=2\n    tween target=0 property=\"x\" expr=\"random(i,0,100)\" start=0 duration=2\n    tween target=1 property=\"x\" expr=\"random(i,0,100)\" start=0 duration=2\n    tween target=2 property=\"x\" expr=\"random(i,0,100)\" start=0 duration=2\n}}\n"
        )
    };
    // SplitMix64 hand values for seeds 0/1/2 scaled to [0, 100).
    let hand = [0u64, 1, 2].map(|s| {
        let mut x = s.wrapping_add(0x9e37_79b9_7f4a_7c15);
        x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        x ^= x >> 31;
        ((x >> 11) as f64) / ((1u64 << 53) as f64) * 100.0
    });
    let root = tmp("scatter");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    write(&folder, "composition.kdl", &mk());
    assert_eq!(run(&["check", "."], &folder).0, 0);
    assert_eq!(run(&["render", ".", "-o", "out.mp4"], &folder).0, 0);
    assert_eq!(run(&["render", ".", "-o", "again.mp4"], &folder).0, 0);
    assert_eq!(
        std::fs::read(folder.join("out.mp4")).unwrap(),
        std::fs::read(folder.join("again.mp4")).unwrap(),
        "seeded scatter re-renders byte-identically"
    );
    let raw = decode_raw(&folder.join("out.mp4"), 60);
    let frame = frame_of(&raw, 0, 160, 120);
    // One clone per row band; each row's left red edge probes that
    // clone's own draw.
    for (k, h) in hand.iter().enumerate() {
        let y0 = 10 + k as u32 * 30;
        let mut left = None;
        for x in 0..160u32 {
            let hit = (y0..y0 + 20).any(|y| {
                let o = (y as usize * 160 + x as usize) * 3;
                frame[o] > 150 && frame[o + 1] < 100 && frame[o + 2] < 100
            });
            if hit {
                left = Some(x);
                break;
            }
        }
        let edge = left.expect("clone draws its row");
        assert!(
            ((edge as f64) - h).abs() < 4.0,
            "clone {k} scatters to its draw, edge {edge} vs hand {h:.1}"
        );
    }
}

#[test]
fn expr_and_replicate_refusals_name_the_input_with_no_mp4() {
    // T010/REC-004: impure, unknown, unseeded, and out-of-range
    // expressions plus bad replicate counts fail check and render
    // with the code, leaving no MP4.
    let root = tmp("refuse");
    let cases = [
        (
            "impure",
            "    tween target=0 property=\"x\" expr=\"open(1)\" start=0 duration=1\n",
            "impure",
        ),
        (
            "unknown",
            "    tween target=0 property=\"x\" expr=\"t+zz\" start=0 duration=1\n",
            "unknown name `zz`",
        ),
        (
            "unseeded",
            "    tween target=0 property=\"x\" expr=\"random()\" start=0 duration=1\n",
            "needs a seed",
        ),
        (
            "endpoint",
            "    tween target=0 property=\"x\" expr=\"t\" to=5 start=0 duration=1\n",
            "conflicts with `expr`",
        ),
        (
            "zero-count",
            "    replicate count=0 {\n        box x=10 y=40 width=20 height=20 fill=\"#ff0000\" start=0 duration=2\n    }\n",
            "at least 1 clone",
        ),
        (
            "negative-count",
            "    replicate count=-1 {\n        box x=10 y=40 width=20 height=20 fill=\"#ff0000\" start=0 duration=2\n    }\n",
            "at least 1 clone",
        ),
    ];
    for (tag, motion, named) in cases {
        let folder = root.join(tag);
        std::fs::create_dir_all(&folder).unwrap();
        write(
            &folder,
            "composition.kdl",
            &format!("{HEAD}{BOX}{motion}}}\n"),
        );
        let (status, out) = run(&["check", "."], &folder);
        assert_eq!(status, 1, "{tag}: check must exit 1, got {status}: {out}");
        assert!(
            out.contains("E_BAD_MOTION"),
            "{tag}: diagnostics must name E_BAD_MOTION: {out}"
        );
        assert!(
            out.contains(named),
            "{tag}: diagnostics must name {named}: {out}"
        );
        let (rstatus, _) = run(&["render", ".", "-o", "out.mp4"], &folder);
        assert_eq!(rstatus, 1, "{tag}: render must exit 1");
        assert!(!folder.join("out.mp4").exists(), "{tag}: no MP4 on refusal");
    }
}

// --- T011: layout recomputes every frame ------------------------------------

#[test]
fn animated_font_size_reflows_wrapped_text() {
    // T011: wrapped text tweening font-size 16->40 over [0, 1)
    // reflows at the changing frame: two bands on frame 0, three on
    // the last frame reaching the canvas bottom, with a transition
    // strictly inside the window (never a stale box).
    let root = tmp("reflow");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=160 height=120 duration=1 fps=30 background=\"#000000\" version=1 {\n    text value=\"aaa bbb ccc ddd eee\" x=5 y=10 size=16 color=\"#ffffff\" wrap=100 start=0 duration=1\n    tween target=0 property=\"font-size\" kind=\"from-to\" from=16 to=40 start=0 duration=1\n}\n",
    );
    assert_eq!(run(&["check", "proj"], &root).0, 0);
    assert_eq!(run(&["render", "proj", "-o", "out.mp4"], &root).0, 0);
    let raw = decode_raw(&root.join("out.mp4"), 30);
    let count = |k: usize| bands(&ink_rows(frame_of(&raw, k, 160, 120), 160, 120));
    // System fonts differ per OS, so pin growth, not exact counts:
    // at 16px the 19 chars always wrap, at 40px always multi-line.
    let first_bands = count(0);
    assert!(first_bands >= 2, "small text wraps over lines, got {first_bands}");
    let last = count(29);
    assert!(
        last >= 3 && last > first_bands,
        "grown text reflows wider, got {last} vs {first_bands}"
    );
    let change = (0..30).find(|k| count(*k) != first_bands).unwrap();
    assert!(
        change > 0 && change < 29,
        "reflow lands inside the window, got {change}"
    );
    assert_eq!(run(&["render", "proj", "-o", "again.mp4"], &root).0, 0);
    assert_eq!(
        std::fs::read(root.join("out.mp4")).unwrap(),
        std::fs::read(root.join("again.mp4")).unwrap(),
        "reflowed text re-renders byte-identically"
    );
}

#[test]
fn constant_motion_matches_static_layouts_byte_identically() {
    // T011: animated-but-constant metrics shape exactly the static
    // layout, so static projects render byte-identically with or
    // without the motion directives.
    let root = tmp("static-identical");
    let mk = |motion: &str| {
        format!(
            "composition name=\"t\" width=160 height=120 duration=1 fps=30 background=\"#000000\" version=1 {{\n    text value=\"aaa bbb ccc ddd eee\" x=5 y=10 size=16 color=\"#ffffff\" wrap=100 start=0 duration=1\n{motion}}}\n"
        )
    };
    let plain = root.join("plain");
    let animated = root.join("animated");
    std::fs::create_dir_all(&plain).unwrap();
    std::fs::create_dir_all(&animated).unwrap();
    write(&plain, "composition.kdl", &mk(""));
    write(
        &animated,
        "composition.kdl",
        &mk("    tween target=0 property=\"font-size\" kind=\"from-to\" from=16 to=16 start=0 duration=1\n"),
    );
    assert_eq!(run(&["check", "."], &plain).0, 0);
    assert_eq!(run(&["check", "."], &animated).0, 0);
    assert_eq!(run(&["render", ".", "-o", "out.mp4"], &plain).0, 0);
    assert_eq!(run(&["render", ".", "-o", "out.mp4"], &animated).0, 0);
    assert_eq!(
        std::fs::read(plain.join("out.mp4")).unwrap(),
        std::fs::read(animated.join("out.mp4")).unwrap(),
        "constant motion matches static layouts byte-identically"
    );
}

#[test]
fn animated_box_width_draws_current_extents() {
    // T011: a box tweening width 20->60 over [0, 2) draws its current
    // extents every frame — 20/30/~39/~59 wide — never a stale box.
    let root = tmp("box-width");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    write(
        &folder,
        "composition.kdl",
        &format!("{HEAD}    box x=10 y=40 width=20 height=40 fill=\"#ff0000\" start=0 duration=2\n    tween target=0 property=\"width\" kind=\"from-to\" from=20 to=60 start=0 duration=2\n}}\n"),
    );
    assert_eq!(run(&["check", "proj"], &root).0, 0);
    assert_eq!(run(&["render", "proj", "-o", "out.mp4"], &root).0, 0);
    let raw = decode_raw(&root.join("out.mp4"), 60);
    let width = |k: usize| {
        let frame = frame_of(&raw, k, 160, 120);
        let xs: Vec<u32> = (0..160 * 120)
            .filter(|i| frame[i * 3] > 150 && frame[i * 3 + 1] < 100 && frame[i * 3 + 2] < 100)
            .map(|i| (i % 160) as u32)
            .collect();
        assert!(!xs.is_empty(), "frame {k} draws the box");
        xs.iter().max().unwrap() - xs.iter().min().unwrap() + 1
    };
    assert!((width(0) as f64 - 20.0).abs() < 3.0, "got {}", width(0));
    assert!((width(15) as f64 - 30.0).abs() < 3.0, "got {}", width(15));
    assert!((width(29) as f64 - 39.3).abs() < 3.5, "got {}", width(29));
    assert!((width(59) as f64 - 59.3).abs() < 3.5, "got {}", width(59));
    assert_eq!(run(&["render", "proj", "-o", "again.mp4"], &root).0, 0);
    assert_eq!(
        std::fs::read(root.join("out.mp4")).unwrap(),
        std::fs::read(root.join("again.mp4")).unwrap(),
        "animated boxes re-render byte-identically"
    );
}

#[test]
fn existing_tween_set_motion_renders_byte_identically_with_pinned_values() {
    // T010/T011 regression: existing tween/set and easing behavior
    // renders exactly as before — pinned pixels plus byte-identical
    // bytes.
    let root = tmp("regression");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    write(
        &folder,
        "composition.kdl",
        &format!("{HEAD}{BOX}    tween target=0 property=\"x\" kind=\"from-to\" from=10 to=110 start=0 duration=1 ease=\"in-quad\"\n    set target=0 property=\"y\" start=1 duration=1 value=60\n}}\n"),
    );
    assert_eq!(run(&["check", "proj"], &root).0, 0);
    assert_eq!(run(&["render", "proj", "-o", "out.mp4"], &root).0, 0);
    let raw = decode_raw(&root.join("out.mp4"), 60);
    // In-quad at frame 15 (t=0.5 of a 1s window): 10+100*0.25=35,
    // centroid 45; the set holds y=60 from frame 30.
    let cx15 = red_centroid_x(frame_of(&raw, 15, 160, 120), 160).unwrap();
    assert!(
        (cx15 - 45.0).abs() < 3.0,
        "in-quad pins frame 15, got {cx15}"
    );
    assert_eq!(run(&["render", "proj", "-o", "again.mp4"], &root).0, 0);
    assert_eq!(
        std::fs::read(root.join("out.mp4")).unwrap(),
        std::fs::read(root.join("again.mp4")).unwrap(),
        "untouched motion re-renders byte-identically"
    );
}
