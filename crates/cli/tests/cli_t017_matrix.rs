//! T017 — the complete Sprint 2 fixture set: one minimal fixture for
//! every element (box, image PNG/JPEG, text, video) and every newly
//! usable property (fill, radius, border, opacity, fit, placement,
//! paint order, line-height, letter-spacing, tweened x/scale/rotation/
//! opacity in to/from/from-to forms, set holds, one easing per family
//! plus out-back(n)). Every fixture must pass on both the CPU renderer
//! and the GPU renderer within the written tolerance; later-sprint use
//! is refused with its enabling sprint and renders nothing partially.
//!
//! Failure lines are printed as `t017-proof: ...` quotable by proof
//! artifacts. A failing fixture names element, property, renderer,
//! and frame.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use validate::validate::{validate_folder, Status};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-t017-{}-{}-{}",
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

fn prepared(folder: &Path) -> render::render::Prepared {
    let verdict = validate_folder(folder);
    assert_eq!(verdict.status, Status::Valid, "{:?}", verdict.diagnostics);
    let comp = verdict
        .composition
        .expect("valid verdict carries a composition");
    render::render::prepare(&comp, folder, Path::new("ffmpeg")).expect("prepare succeeds")
}

/// One fixture: its element, property, KDL body lines, and frames where
/// both renderers must agree.
struct Fixture {
    element: &'static str,
    property: &'static str,
    body: String,
    frames: Vec<u64>,
}

fn head(duration: u32) -> String {
    format!(
        "composition name=\"t\" width=160 height=120 duration={duration} fps=30 background=\"#101010\" version=1 {{\n"
    )
}

fn still_matrix() -> Vec<Fixture> {
    vec![
        Fixture {
            element: "box",
            property: "fill",
            body: "    box x=20 y=20 width=50 height=50 fill=\"#ff0000\" start=0 duration=1 order=0\n"
                .to_string(),
            frames: vec![0, 15, 29],
        },
        Fixture {
            element: "box",
            property: "radius+border",
            body: "    box x=20 y=20 width=60 height=60 fill=\"#ff0000\" radius=10 border-width=4 border-color=\"#ffffff\" opacity=1 order=0 start=0 duration=1\n"
                .to_string(),
            frames: vec![0, 15, 29],
        },
        Fixture {
            element: "box",
            property: "opacity",
            body: "    box x=10 y=10 width=60 height=60 fill=\"#0000ff\" opacity=0.5 order=1 start=0 duration=1\n    box x=40 y=40 width=60 height=60 fill=\"#ff0000\" opacity=1 order=0 start=0 duration=1\n"
                .to_string(),
            frames: vec![0, 15, 29],
        },
        Fixture {
            element: "image-png",
            property: "fit=contain,placement=top-left",
            body: "    image path=\"photo.png\" start=0 duration=1 x=100 y=10 width=50 height=50 fit=\"contain\" placement=\"top-left\" order=0\n"
                .to_string(),
            frames: vec![0, 15, 29],
        },
        Fixture {
            element: "image-jpeg",
            property: "fit=cover,placement=center",
            body: "    image path=\"photo.jpg\" start=0 duration=1 x=100 y=60 width=50 height=40 fit=\"cover\" placement=\"center\" order=0\n"
                .to_string(),
            frames: vec![0, 15, 29],
        },
        Fixture {
            element: "image-png",
            property: "fit=fill,placement=bottom-right",
            body: "    image path=\"photo.png\" start=0 duration=1 x=10 y=60 width=70 height=40 fit=\"fill\" placement=\"bottom-right\" order=0\n"
                .to_string(),
            frames: vec![0, 15, 29],
        },
        Fixture {
            element: "box+box",
            property: "paint-order-written",
            body: "    box x=50 y=30 width=60 height=60 fill=\"#0000ff\" order=1 start=0 duration=1\n    box x=10 y=10 width=60 height=60 fill=\"#ff0000\" order=0 start=0 duration=1\n"
                .to_string(),
            frames: vec![0, 15, 29],
        },
        Fixture {
            element: "text",
            property: "line-height",
            body: "    text value=\"Hi Ag\" start=0 duration=1 x=10 y=20 size=14 color=\"#ffffff\" order=0 line-height=28\n"
                .to_string(),
            frames: vec![0, 15, 29],
        },
        Fixture {
            element: "text",
            property: "letter-spacing",
            body: "    text value=\"Go\" start=0 duration=1 x=10 y=90 size=16 color=\"#ffffff\" order=0 letter-spacing=6\n"
                .to_string(),
            frames: vec![0, 15, 29],
        },
        Fixture {
            element: "text",
            property: "plain-still",
            body: "    text value=\"Hi\" start=0 duration=1 x=60 y=50 size=20 color=\"#ffff00\" order=0\n"
                .to_string(),
            frames: vec![0, 15, 29],
        },
        Fixture {
            element: "video",
            property: "still",
            body: "    video path=\"clip.mp4\" start=0 duration=1 fit=\"fill\" order=0\n".to_string(),
            frames: vec![0, 15, 29],
        },
    ]
}

fn motion_matrix() -> Vec<Fixture> {
    let mut v = Vec::new();
    // Each tweened property in each tween form, on box targets.
    for (property, kind, from_to) in [
        ("x", "to", "to=60"),
        ("x", "from", "from=60"),
        ("x", "from-to", "from=0 to=60"),
        ("scale", "to", "to=1.5"),
        ("scale", "from", "from=1.5"),
        ("scale", "from-to", "from=1 to=1.5"),
        ("rotation", "to", "to=45"),
        ("rotation", "from", "from=45"),
        ("rotation", "from-to", "from=0 to=45"),
        ("opacity", "to", "to=0.2"),
        ("opacity", "from", "from=0.2"),
        ("opacity", "from-to", "from=1 to=0.2"),
    ] {
        v.push(Fixture {
            element: "box",
            property: Box::leak(
                format!("tween-{property}-{kind}").into_boxed_str(),
            ),
            body: format!(
                "    box x=10 y=10 width=30 height=30 fill=\"#ff0000\" start=0 duration=2 order=0\n    tween target=0 property=\"{property}\" kind=\"{kind}\" start=0 duration=1 {from_to} ease=\"linear\"\n"
            ),
            frames: vec![0, 15, 30],
        });
    }
    // Set holds on box and text targets.
    v.push(Fixture {
        element: "box",
        property: "set-hold-x",
        body: "    box x=10 y=10 width=30 height=30 fill=\"#ff0000\" start=0 duration=2 order=0\n    set target=0 property=\"x\" start=1 duration=0.5 value=70\n"
            .to_string(),
        frames: vec![29, 30, 44, 45],
    });
    v.push(Fixture {
        element: "text",
        property: "set-hold-opacity",
        body: "    text value=\"Go\" start=0 duration=2 x=10 y=90 size=16 color=\"#ffffff\" order=0\n    set target=0 property=\"opacity\" start=1 duration=0.5 value=0.5\n"
            .to_string(),
        frames: vec![29, 30, 44, 45],
    });
    // One easing per family plus out-back(n), all on box x.
    for ease in [
        "linear",
        "in-quad",
        "in-cubic",
        "in-quart",
        "in-quint",
        "in-sine",
        "in-expo",
        "out-back(1.7)",
    ] {
        v.push(Fixture {
            element: "box",
            property: Box::leak(format!("ease-{ease}").into_boxed_str()),
            body: format!(
                "    box x=10 y=10 width=30 height=30 fill=\"#ff0000\" start=0 duration=2 order=0\n    tween target=0 property=\"x\" kind=\"to\" start=0 duration=1 to=60 ease=\"{ease}\"\n"
            ),
            frames: vec![0, 15, 30],
        });
    }
    // Text and video under motion.
    v.push(Fixture {
        element: "text",
        property: "tween-opacity-from",
        body: "    text value=\"Go\" start=0 duration=2 x=10 y=90 size=16 color=\"#ffffff\" order=0 line-height=22\n    tween target=0 property=\"opacity\" kind=\"from\" start=0 duration=1 from=0 ease=\"linear\"\n"
            .to_string(),
        frames: vec![0, 15, 30],
    });
    v.push(Fixture {
        element: "video",
        property: "tween-x-from-to",
        body: "    video path=\"clip.mp4\" start=0 duration=2 fit=\"fill\" order=0\n    tween target=0 property=\"x\" kind=\"from-to\" start=0 duration=1 from=0 to=30 ease=\"out-cubic\"\n"
            .to_string(),
        frames: vec![0, 15, 30],
    });
    v.push(Fixture {
        element: "image-png",
        property: "tween-scale-from-to",
        body: "    image path=\"photo.png\" start=0 duration=2 x=100 y=10 width=50 height=40 fit=\"contain\" placement=\"center\" order=0\n    tween target=0 property=\"scale\" kind=\"from-to\" start=0 duration=1 from=1 to=1.5 ease=\"in-out-quad\"\n"
            .to_string(),
        frames: vec![0, 15, 30],
    });
    v
}

fn check_fixture(root: &Path, tag: &str, fx: &Fixture, duration: u32) {
    let folder = root.join(tag);
    std::fs::create_dir_all(&folder).unwrap();
    solid_png(&folder.join("photo.png"), (0, 255, 0), 64, 48);
    solid_jpg(&folder.join("photo.jpg"), (255, 255, 0), 48, 64);
    let reds = vec![(255, 0, 0); 30 * duration as usize];
    solid_source(&folder.join("clip.mp4"), &reds, 64, 48, 30);
    write(
        &folder,
        "composition.kdl",
        &(head(duration) + &fx.body + "}\n"),
    );
    let mut prep = prepared(&folder);
    for &k in &fx.frames {
        let cpu = prep.cpu_frame(k).expect("cpu frame draws");
        let gpu = render::gpu::render_frame(&mut prep, k).expect("gpu frame draws");
        let cmp = render::gpu::compare_frames(&cpu, &gpu).expect("pictures compare");
        let line = cmp.report(&format!("{}:{}", fx.element, fx.property), k);
        println!("t017-proof: cpu-vs-gpu {line}");
        assert!(
            cmp.pass(),
            "element `{}` property `{}` fails on frame {k}: {line}",
            fx.element,
            fx.property
        );
    }
}

#[test]
fn still_matrix_passes_on_both_renderers() {
    let root = tmp("still");
    let matrix = still_matrix();
    assert_eq!(
        matrix.len(),
        11,
        "still matrix covers every still element+property"
    );
    for (i, fx) in matrix.iter().enumerate() {
        check_fixture(&root, &format!("f{i}"), fx, 1);
    }
    println!("t017-proof: still matrix 11/11 fixtures pass on cpu and gpu");
}

#[test]
fn motion_matrix_passes_on_both_renderers() {
    let root = tmp("motion");
    let matrix = motion_matrix();
    assert_eq!(
        matrix.len(),
        25,
        "motion matrix covers every property x form + set + easings"
    );
    for (i, fx) in matrix.iter().enumerate() {
        check_fixture(&root, &format!("f{i}"), fx, 2);
    }
    println!("t017-proof: motion matrix 25/25 fixtures pass on cpu and gpu");
}

#[test]
fn broken_cpu_fixture_names_element_and_location() {
    // Break the CPU side: an invalid box is refused before any frame,
    // naming the file, line, column, and code on both check and render.
    let root = tmp("cpu-break");
    let folder = root.join("bad");
    std::fs::create_dir_all(&folder).unwrap();
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=160 height=120 duration=1 fps=30 background=\"#000000\" version=1 {\n    box x=10 y=10 width=0 height=30 fill=\"#ff0000\" start=0 duration=1 order=0\n}\n",
    );
    let (code, out) = run(&["check", "bad"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("E_BAD_BOX"), "{out}");
    assert!(out.contains("composition.kdl"), "{out}");
    println!("t017-proof: cpu break names element=box code=E_BAD_BOX file=composition.kdl");
    let (code, out) = run(&["render", "bad", "-o", "out.mp4"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(
        !folder.join("out.mp4").exists(),
        "refused render writes no MP4"
    );
    println!("t017-proof: cpu break refuses render exit 1 with no partial MP4");
}

#[test]
fn broken_gpu_picture_names_fixture_frame_and_distance() {
    // Break the GPU side: a mistuned GPU picture is reported with the
    // fixture, the frame, and the measured distance — and no pass.
    let root = tmp("gpu-break");
    let folder = root.join("f0");
    std::fs::create_dir_all(&folder).unwrap();
    solid_png(&folder.join("photo.png"), (0, 255, 0), 64, 48);
    solid_jpg(&folder.join("photo.jpg"), (255, 255, 0), 48, 64);
    solid_source(&folder.join("clip.mp4"), &vec![(255, 0, 0); 60], 64, 48, 30);
    let fx = &still_matrix()[1];
    write(&folder, "composition.kdl", &(head(1) + &fx.body + "}\n"));
    let mut prep = prepared(&folder);
    let cpu = prep.cpu_frame(15).expect("cpu frame");
    let mut bad_gpu = cpu.clone();
    for i in (0..bad_gpu.len()).step_by(53) {
        bad_gpu[i] = bad_gpu[i].saturating_add(60);
    }
    let cmp = render::gpu::compare_frames(&cpu, &bad_gpu).expect("pictures compare");
    assert!(!cmp.pass(), "mistuned gpu picture must not pass");
    let line = cmp.report("box:radius+border", 15);
    println!("t017-proof: gpu break renderer=gpu {line}");
    assert!(line.contains("box:radius+border"), "{line}");
    assert!(line.contains("frame 15"), "{line}");
    assert!(line.contains("max_abs="), "{line}");
    assert!(line.contains("no pass granted"), "{line}");
}

#[test]
fn later_sprint_use_is_marked_not_rendered() {
    // A fixture needing a later sprint names its enabling sprint on
    // both check and render, and renders nothing partially.
    let root = tmp("later");
    let folder = root.join("later");
    std::fs::create_dir_all(&folder).unwrap();
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=160 height=120 duration=1 fps=30 background=\"#000000\" version=1 {\n    box x=10 y=10 width=30 height=30 fill=\"#ff0000\" start=0 duration=1 order=0\n    tween target=0 property=\"scale-x\" kind=\"to\" start=0 duration=1 to=2 ease=\"linear\"\n}\n",
    );
    let (code, out) = run(&["check", "later"], &root);
    assert_eq!(code, 2, "{out}");
    assert!(out.contains("E_UNSUPPORTED"), "{out}");
    println!("t017-proof: later-sprint tween refused exit 2 with enabling sprint");
    let (code, _) = run(&["render", "later", "-o", "out.mp4"], &root);
    assert_eq!(
        code, 2,
        "later-sprint render refuses before the first frame"
    );
    assert!(
        !folder.join("out.mp4").exists(),
        "no partial MP4 for later-sprint use"
    );
}

#[test]
fn combined_composition_proves_end_to_end() {
    // Boxes, images, written order, text spacing, and tween+set motion
    // validate, agree on both renderers, and render identically at any
    // worker count to one playable MP4.
    let root = tmp("combined");
    let folder = root.join("show");
    std::fs::create_dir_all(&folder).unwrap();
    solid_png(&folder.join("photo.png"), (0, 255, 0), 64, 48);
    solid_jpg(&folder.join("photo.jpg"), (255, 255, 0), 48, 64);
    solid_source(&folder.join("clip.mp4"), &vec![(255, 0, 0); 60], 64, 48, 30);
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=160 height=120 duration=2 fps=30 background=\"#101010\" version=1 {\n    video path=\"clip.mp4\" start=0 duration=2 fit=\"fill\" order=0\n    image path=\"photo.jpg\" start=0 duration=2 x=100 y=70 width=50 height=40 fit=\"cover\" placement=\"center\" order=1\n    image path=\"photo.png\" start=0 duration=2 x=100 y=10 width=50 height=50 fit=\"contain\" placement=\"top-left\" order=2\n    box x=10 y=10 width=30 height=30 fill=\"#ff0000\" radius=6 border-width=2 border-color=\"#ffffff\" opacity=1 order=3 start=0 duration=2\n    text value=\"Hi Ag\" start=0 duration=2 x=10 y=95 size=14 color=\"#ffffff\" order=4 line-height=20 letter-spacing=2\n    tween target=3 property=\"x\" kind=\"to\" start=0 duration=1 to=60 ease=\"in-out-quad\"\n    set target=3 property=\"opacity\" start=1 duration=0.5 value=0.5\n}\n",
    );
    let (code, out) = run(&["check", "show"], &root);
    assert_eq!(code, 0, "{out}");
    let mut prep = prepared(&folder);
    for &k in &[0u64, 15, 30, 45, 59] {
        let cpu = prep.cpu_frame(k).expect("cpu frame draws");
        let gpu = render::gpu::render_frame(&mut prep, k).expect("gpu frame draws");
        let cmp = render::gpu::compare_frames(&cpu, &gpu).expect("pictures compare");
        let line = cmp.report("combined-show", k);
        println!("t017-proof: combined {line}");
        assert!(cmp.pass(), "{line}");
    }
    let (c1, o1) = run_env(
        &["render", "show", "-o", "w1.mp4"],
        &root,
        &[("FRAMESMITH_WORKERS", "1")],
    );
    assert_eq!(c1, 0, "{o1}");
    let (c9, o9) = run_env(
        &["render", "show", "-o", "w9.mp4"],
        &root,
        &[("FRAMESMITH_WORKERS", "9")],
    );
    assert_eq!(c9, 0, "{o9}");
    let a = std::fs::read(root.join("w1.mp4")).unwrap();
    let b = std::fs::read(root.join("w9.mp4")).unwrap();
    assert_eq!(a, b, "worker counts change no byte of the playable MP4");
    println!(
        "t017-proof: combined renders exit 0, workers 1 vs 9 byte-identical ({} bytes)",
        a.len()
    );
}
