//! Sprint 7 motion (T005 + T006) end-to-end: 3D turns with origin and
//! perspective, plus animated visual properties.
//!
//! Renders through the built binary with host-ffmpeg fixtures and
//! decodes the MP4s (lossy: every pixel assertion carries the H.264
//! tolerance it needs): turns foreshorten through the origin (flat
//! default, trapezoid with perspective), visual tweens interpolate
//! frame-exactly, refusals name the bound with no MP4, and re-renders
//! are byte-identical.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-n07-turn-{tag}-{}-{}",
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

fn ffmpeg(args: &[&str]) {
    let out = Command::new("ffmpeg")
        .args(["-y", "-v", "error"])
        .args(args)
        .output()
        .expect("run host ffmpeg");
    assert!(out.status.success(), "host ffmpeg failed: {args:?}");
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

fn px(frame: &[u8], w: u32, x: u32, y: u32) -> (u8, u8, u8) {
    let o = (y * w + x) as usize * 3;
    (frame[o], frame[o + 1], frame[o + 2])
}

/// Height of the red run in one column (red = r dominant).
fn red_height(frame: &[u8], w: u32, h: u32, col: u32) -> u32 {
    (0..h)
        .filter(|&j| {
            let (r, g, b) = px(frame, w, col, j);
            r > 100 && r > g + 40 && r > b + 40
        })
        .count() as u32
}

const HEAD: &str =
    "composition name=\"t\" width=160 height=120 duration=2 fps=30 background=\"#000000\" version=1 {\n";

// --- T005: 3D turns ----------------------------------------------------------

#[test]
fn flat_turn_foreshortens_box_through_center() {
    // rx=60 flat halves the 40-tall box about its center row (30):
    // rows 20..39 read red, the neighbors read black.
    let root = tmp("flat60");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    write(
        &folder,
        "composition.kdl",
        &format!(
            "{HEAD}    box x=60 y=40 width=40 height=40 fill=\"#ff0000\" start=0 duration=2 rotation-x=60\n}}\n"
        ),
    );
    assert_eq!(run(&["check", "proj"], &root).0, 0);
    assert_eq!(run(&["render", "proj", "-o", "out.mp4"], &root).0, 0);
    let raw = decode_raw(&root.join("out.mp4"), 60);
    let f0 = frame_of(&raw, 0, 160, 120);
    let is_red = |x: u32, y: u32| {
        let (r, g, b) = px(f0, 160, x, y);
        r > 150 && g < 100 && b < 100
    };
    // y=40..80 halves about row 60 to rows 50..69.
    for y in [50u32, 55, 60, 65, 69] {
        assert!(is_red(80, y), "row {y} fills");
    }
    assert!(!is_red(80, 48), "above stays empty");
    assert!(!is_red(80, 72), "below stays empty");
    // Full width keeps (horizontal axis turn): columns 60..99 at row 60.
    assert!(is_red(62, 60) && is_red(97, 60), "width keeps");
    // Re-render is byte-identical.
    assert_eq!(run(&["render", "proj", "-o", "again.mp4"], &root).0, 0);
    assert_eq!(
        std::fs::read(root.join("out.mp4")).unwrap(),
        std::fs::read(root.join("again.mp4")).unwrap(),
        "turned re-render is byte-identical"
    );
}

#[test]
fn origin_top_pins_the_top_edge() {
    // The same rx=60 turn about the top edge pins row 40 and halves
    // downward (rows 40..59), leaving the center rows of the
    // center-origin turn empty.
    let root = tmp("toporg");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    write(
        &folder,
        "composition.kdl",
        &format!(
            "{HEAD}    box x=60 y=40 width=40 height=40 fill=\"#ff0000\" start=0 duration=2 rotation-x=60 origin=\"top-left\"\n}}\n"
        ),
    );
    assert_eq!(run(&["check", "proj"], &root).0, 0);
    assert_eq!(run(&["render", "proj", "-o", "out.mp4"], &root).0, 0);
    let raw = decode_raw(&root.join("out.mp4"), 60);
    let f0 = frame_of(&raw, 0, 160, 120);
    let is_red = |x: u32, y: u32| {
        let (r, g, b) = px(f0, 160, x, y);
        r > 150 && g < 100 && b < 100
    };
    assert!(is_red(80, 41), "top pins");
    assert!(is_red(80, 58), "bottom halves to 59");
    assert!(!is_red(80, 60), "past the half stays empty");
    assert!(!is_red(80, 65), "a center origin would fill row 65");
}

#[test]
fn perspective_turn_makes_a_trapezoid() {
    // ry=45 flat foreshortens symmetrically (equal edge heights);
    // with perspective=200 the near edge stands taller.
    for (tag, persp, symmetric) in [("flat", "", true), ("persp", "perspective=200", false)] {
        let root = tmp(&format!("trap-{tag}"));
        let folder = root.join("proj");
        std::fs::create_dir_all(&folder).unwrap();
        write(
            &folder,
            "composition.kdl",
            &format!(
                "{HEAD}    box x=40 y=40 width=80 height=40 fill=\"#ff0000\" start=0 duration=2 rotation-y=45 {persp}\n}}\n"
            ),
        );
        assert_eq!(run(&["check", "proj"], &root).0, 0);
        assert_eq!(run(&["render", "proj", "-o", "out.mp4"], &root).0, 0);
        let raw = decode_raw(&root.join("out.mp4"), 60);
        let f0 = frame_of(&raw, 0, 160, 120);
        let cols: Vec<u32> = (0..160)
            .filter(|&i| red_height(f0, 160, 120, i) > 0)
            .collect();
        assert!(!cols.is_empty(), "{tag}: something draws");
        let (lh, rh) = (
            red_height(f0, 160, 120, cols[0]),
            red_height(f0, 160, 120, cols[cols.len() - 1]),
        );
        if symmetric {
            assert!(
                (lh as i32 - rh as i32).abs() <= 4,
                "flat edges match: {lh} vs {rh}"
            );
        } else {
            assert!(
                lh > rh + 5,
                "near edge {lh} stands taller than far edge {rh}"
            );
        }
    }
}

#[test]
fn bad_turns_refuse_with_no_mp4() {
    // Non-positive perspective and unknown origins fail check (exit 1)
    // and render with no MP4.
    for (tag, clip, code) in [
        ("persp-zero", "perspective=0", "E_BAD_MOTION"),
        ("persp-neg", "perspective=-50", "E_BAD_MOTION"),
        ("origin-bad", "origin=\"nowhere\"", "E_BAD_MOTION"),
        ("filter-bad", "filter=\"contrast(2)\"", "E_UNSUPPORTED"),
    ] {
        let root = tmp(&format!("refuse-{tag}"));
        let folder = root.join("proj");
        std::fs::create_dir_all(&folder).unwrap();
        write(
            &folder,
            "composition.kdl",
            &format!(
                "{HEAD}    box x=4 y=4 width=20 height=12 fill=\"#ff0000\" start=0 duration=2 rotation-x=10 {clip}\n}}\n"
            ),
        );
        let (status, out) = run(&["check", "proj"], &root);
        let want = if code == "E_UNSUPPORTED" { 2 } else { 1 };
        assert_eq!(status, want, "{tag}: check must exit {want}: {out}");
        assert!(
            out.contains(code),
            "{tag}: diagnostics must name {code}: {out}"
        );
        if code == "E_UNSUPPORTED" {
            assert!(out.contains("Sprint 11"), "{tag}: names Sprint 11: {out}");
        }
        let (rstatus, _) = run(&["render", "proj", "-o", "out.mp4"], &root);
        assert_eq!(rstatus, want, "{tag}: render must exit {want}");
        assert!(!root.join("out.mp4").exists(), "{tag}: no MP4 on refusal");
    }
}

#[test]
fn turned_video_renders_and_rerenders_identically() {
    // A turned video draws (the blit-dest path projects) and
    // re-renders byte-identically.
    let root = tmp("vidturn");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    ffmpeg(&[
        "-f",
        "lavfi",
        "-i",
        "testsrc=duration=2:size=64x48:rate=30",
        "-c:v",
        "libx264",
        "-pix_fmt",
        "yuv420p",
        &folder.join("clip.mp4").display().to_string(),
    ]);
    write(
        &folder,
        "composition.kdl",
        &format!("{HEAD}    video path=\"clip.mp4\" start=0 duration=2 rotation-y=30 perspective=300\n}}\n"),
    );
    assert_eq!(run(&["check", "proj"], &root).0, 0);
    assert_eq!(run(&["render", "proj", "-o", "out.mp4"], &root).0, 0);
    let mp4 = root.join("out.mp4");
    assert!(mp4.exists() && std::fs::metadata(&mp4).unwrap().len() > 1000);
    assert_eq!(run(&["render", "proj", "-o", "again.mp4"], &root).0, 0);
    assert_eq!(
        std::fs::read(&mp4).unwrap(),
        std::fs::read(root.join("again.mp4")).unwrap(),
        "turned video re-renders byte-identically"
    );
}

// --- T006: animated visual properties ----------------------------------------

#[test]
fn width_tween_grows_box_frame_exactly() {
    // Width 20->100 and height 20->60 over [0, 2) linear: frame 15
    // (t=0.5) spans 40x30 px, frame 45 (t=1.5) spans 80x50 px.
    let root = tmp("width");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    write(
        &folder,
        "composition.kdl",
        &format!(
            "{HEAD}    box x=10 y=40 width=20 height=20 fill=\"#ff0000\" start=0 duration=2\n    tween target=0 property=\"width\" kind=\"from-to\" start=0 duration=2 from=20 to=100\n    tween target=0 property=\"height\" kind=\"from-to\" start=0 duration=2 from=20 to=60\n}}\n"
        ),
    );
    assert_eq!(run(&["check", "proj"], &root).0, 0);
    assert_eq!(run(&["render", "proj", "-o", "out.mp4"], &root).0, 0);
    let raw = decode_raw(&root.join("out.mp4"), 60);
    let run_at = |k: usize| {
        let f = frame_of(&raw, k, 160, 120);
        (0..160)
            .filter(|&x| {
                let (r, g, b) = px(f, 160, x, 50);
                r > 150 && g < 100 && b < 100
            })
            .count()
    };
    let col_at = |k: usize| {
        let f = frame_of(&raw, k, 160, 120);
        (0..120)
            .filter(|&y| {
                let (r, g, b) = px(f, 160, 30, y);
                r > 150 && g < 100 && b < 100
            })
            .count()
    };
    assert!(
        (run_at(15) as i32 - 40).abs() <= 3,
        "frame 15 spans 40, got {}",
        run_at(15)
    );
    assert!(
        (run_at(45) as i32 - 80).abs() <= 3,
        "frame 45 spans 80, got {}",
        run_at(45)
    );
    assert!(
        (col_at(15) as i32 - 30).abs() <= 3,
        "frame 15 rises 30, got {}",
        col_at(15)
    );
    assert!(
        (col_at(45) as i32 - 50).abs() <= 3,
        "frame 45 rises 50, got {}",
        col_at(45)
    );
}

#[test]
fn fill_tween_recolors_through_purple() {
    // Fill red->blue over [0, 2): the ends read red/blue, the midpoint
    // reads purple (both channels high, green low).
    let root = tmp("fill");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    write(
        &folder,
        "composition.kdl",
        &format!(
            "{HEAD}    box x=60 y=40 width=40 height=40 fill=\"#ff0000\" start=0 duration=2\n    tween target=0 property=\"fill\" kind=\"from-to\" start=0 duration=2 from=\"#ff0000\" to=\"#0000ff\"\n}}\n"
        ),
    );
    assert_eq!(run(&["check", "proj"], &root).0, 0);
    assert_eq!(run(&["render", "proj", "-o", "out.mp4"], &root).0, 0);
    let raw = decode_raw(&root.join("out.mp4"), 60);
    let center = |k: usize| px(frame_of(&raw, k, 160, 120), 160, 80, 60);
    let (r0, g0, b0) = center(0);
    assert!(
        r0 > 180 && g0 < 80 && b0 < 80,
        "start reads red: {r0},{g0},{b0}"
    );
    let (r1, g1, b1) = center(30);
    assert!(
        r1 > 90 && b1 > 90 && g1 < 90,
        "midpoint reads purple: {r1},{g1},{b1}"
    );
    let (r2, g2, b2) = center(59);
    assert!(
        b2 > 180 && g2 < 80 && r2 < 80,
        "end reads blue: {r2},{g2},{b2}"
    );
}

#[test]
fn blur_tween_softens_then_holds() {
    // Blur 0->6 over [0, 1): frame 0 draws the sharp edge, frame 15
    // (t=0.5, sigma=3) spreads it, frame 30 holds sigma 6. The center
    // never changes.
    let root = tmp("blurtween");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    write(
        &folder,
        "composition.kdl",
        &format!(
            "{HEAD}    box x=60 y=40 width=40 height=40 fill=\"#ff0000\" start=0 duration=2\n    tween target=0 property=\"blur\" kind=\"from-to\" start=0 duration=1 from=0 to=6\n}}\n"
        ),
    );
    assert_eq!(run(&["check", "proj"], &root).0, 0);
    assert_eq!(run(&["render", "proj", "-o", "out.mp4"], &root).0, 0);
    let raw = decode_raw(&root.join("out.mp4"), 60);
    let at = |k: usize, x: u32, y: u32| px(frame_of(&raw, k, 160, 120), 160, x, y);
    // Interior of the box stays solid red at every frame.
    for k in [0usize, 15, 30, 59] {
        let (r, g, b) = at(k, 80, 60);
        assert!(
            r > 180 && g < 80 && b < 80,
            "frame {k} center stays red: {r},{g},{b}"
        );
    }
    // Just inside the left edge (x=62): sharp at frame 0, spread later.
    let (r0, _, _) = at(0, 62, 60);
    let (r15, _, _) = at(15, 62, 60);
    assert!(r0 > 200, "frame 0 edge is sharp: {r0}");
    assert!(r15 < r0 - 20, "frame 15 edge spreads: {r15} vs {r0}");
    // Just outside the box (x=56): black at frame 0, lit at frame 15.
    let (o0, _, _) = at(0, 56, 60);
    let (o15, _, _) = at(15, 56, 60);
    assert!(o0 < 40, "frame 0 outside stays dark: {o0}");
    assert!(o15 > 20, "frame 15 softness reaches outside: {o15}");
}

#[test]
fn static_filter_blur_draws_soft() {
    // filter="blur(2)" softens the edge while the interior matches the
    // unfiltered box.
    let mut edge = Vec::new();
    let mut center = Vec::new();
    for (tag, filter) in [("plain", ""), ("blurred", "filter=\"blur(2)\"")] {
        let root = tmp(&format!("staticblur-{tag}"));
        let folder = root.join("proj");
        std::fs::create_dir_all(&folder).unwrap();
        write(
            &folder,
            "composition.kdl",
            &format!(
                "{HEAD}    box x=60 y=40 width=40 height=40 fill=\"#ff0000\" start=0 duration=1 {filter}\n}}\n"
            ),
        );
        assert_eq!(run(&["check", "proj"], &root).0, 0);
        assert_eq!(run(&["render", "proj", "-o", "out.mp4"], &root).0, 0);
        let raw = decode_raw(&root.join("out.mp4"), 30);
        let f0 = frame_of(&raw, 0, 160, 120);
        edge.push(px(f0, 160, 61, 60));
        center.push(px(f0, 160, 80, 60));
    }
    for (i, c) in center.iter().enumerate() {
        assert!(
            c.0 > 180 && c.1 < 80 && c.2 < 80,
            "center {i} stays red: {c:?}"
        );
    }
    assert!(
        (center[0].0 as i16 - center[1].0 as i16).abs() <= 12,
        "interiors match"
    );
    let (r0, r1) = (edge[0].0, edge[1].0);
    assert!(r0 > 150, "plain edge stays sharp: {r0}");
    assert!(r1 + 20 < r0, "blurred edge softens: {r1} vs {r0}");
}

#[test]
fn set_and_key_drive_visual_properties() {
    // set width=60 holds over [0.5, 1.5); key radius 0->20 peaks
    // halfway; clip-shape refuses.
    let root = tmp("setkey");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    write(
        &folder,
        "composition.kdl",
        &format!(
            "{HEAD}    box x=10 y=40 width=20 height=20 fill=\"#ff0000\" start=0 duration=2\n    set target=0 property=\"width\" start=0.5 duration=1 value=60\n    key target=0 property=\"radius\" {{\n        frame at=0 value=0\n        frame at=2 value=20\n    }}\n}}\n"
        ),
    );
    assert_eq!(run(&["check", "proj"], &root).0, 0);
    assert_eq!(run(&["render", "proj", "-o", "out.mp4"], &root).0, 0);
    let raw = decode_raw(&root.join("out.mp4"), 60);
    let run_at = |k: usize| {
        let f = frame_of(&raw, k, 160, 120);
        (0..160)
            .filter(|&x| {
                let (r, g, b) = px(f, 160, x, 50);
                r > 150 && g < 100 && b < 100
            })
            .count()
    };
    assert!(
        (run_at(0) as i32 - 20).abs() <= 3,
        "before the set: {}",
        run_at(0)
    );
    assert!(
        (run_at(15) as i32 - 60).abs() <= 3,
        "inside the set: {}",
        run_at(15)
    );
    assert!(
        (run_at(45) as i32 - 20).abs() <= 3,
        "past the set: {}",
        run_at(45)
    );
    // Radius keys round the corners halfway: the corner pixel fills at
    // frame 0 and empties by the end.
    let corner = |k: usize| px(frame_of(&raw, k, 160, 120), 160, 10, 40);
    let (cr0, _, _) = corner(0);
    let (cr59, _, _) = corner(59);
    assert!(cr0 > 150, "square corner fills at frame 0: {cr0}");
    assert!(cr59 < 100, "round corner empties at frame 59: {cr59}");
    // clip-shape is not a property.
    let root = tmp("clipshape");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    write(
        &folder,
        "composition.kdl",
        &format!(
            "{HEAD}    box x=10 y=40 width=20 height=20 fill=\"#ff0000\" start=0 duration=2\n    tween target=0 property=\"clip-shape\" kind=\"to\" start=0 duration=1 to=1\n}}\n"
        ),
    );
    let (status, out) = run(&["check", "proj"], &root);
    assert_eq!(status, 1, "clip-shape must fail: {out}");
    assert!(out.contains("E_BAD_MOTION"), "names E_BAD_MOTION: {out}");
}

#[test]
fn font_size_tween_grows_text() {
    // Font size 16->48 over [0, 2): the ink column count grows from
    // the first frame to the last.
    let root = tmp("fontsize");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    write(
        &folder,
        "composition.kdl",
        &format!(
            "{HEAD}    text value=\"Hi\" x=20 y=40 size=16 start=0 duration=2\n    tween target=0 property=\"font-size\" kind=\"from-to\" start=0 duration=2 from=16 to=48\n}}\n"
        ),
    );
    assert_eq!(run(&["check", "proj"], &root).0, 0);
    assert_eq!(run(&["render", "proj", "-o", "out.mp4"], &root).0, 0);
    let raw = decode_raw(&root.join("out.mp4"), 60);
    let ink_cols = |k: usize| {
        let f = frame_of(&raw, k, 160, 120);
        (0..160)
            .filter(|&x| {
                (0..120).any(|y| {
                    let (r, g, b) = px(f, 160, x, y);
                    r > 60 || g > 60 || b > 60
                })
            })
            .count()
    };
    assert!(
        ink_cols(59) > ink_cols(0) + 5,
        "ink widens: {} vs {}",
        ink_cols(0),
        ink_cols(59)
    );
}

#[test]
fn letter_spacing_tween_widens_text() {
    // Letter spacing 0->12 over [0, 2): the ink column count grows
    // from the first frame to the last.
    let root = tmp("spacing");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    write(
        &folder,
        "composition.kdl",
        &format!(
            "{HEAD}    text value=\"Hi\" x=20 y=40 size=24 start=0 duration=2\n    tween target=0 property=\"letter-spacing\" kind=\"from-to\" start=0 duration=2 from=0 to=12\n}}\n"
        ),
    );
    assert_eq!(run(&["check", "proj"], &root).0, 0);
    assert_eq!(run(&["render", "proj", "-o", "out.mp4"], &root).0, 0);
    let raw = decode_raw(&root.join("out.mp4"), 60);
    // Spacing inserts empty columns between glyphs, so the ink span
    // (last minus first lit column) grows, not the lit count.
    let ink_span = |k: usize| {
        let f = frame_of(&raw, k, 160, 120);
        let lit: Vec<u32> = (0..160)
            .filter(|&x| {
                (0..120).any(|y| {
                    let (r, g, b) = px(f, 160, x, y);
                    r > 60 || g > 60 || b > 60
                })
            })
            .collect();
        assert!(!lit.is_empty(), "text draws at frame {k}");
        lit[lit.len() - 1] - lit[0]
    };
    assert!(
        ink_span(59) > ink_span(0) + 5,
        "spacing widens: {} vs {}",
        ink_span(0),
        ink_span(59)
    );
}
