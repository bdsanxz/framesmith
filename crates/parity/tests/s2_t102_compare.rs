//! Sprint 2, ticket T102: the whole comparison of the relative-timing pair in one report. Each group of
//! agreed checks (container facts, pictures, the timing of each clip of a chain, the position of a moving
//! element) gives a pass or fail per check, with what was measured and the threshold it is held to; a failed
//! check is repeated with both; the exit code says passed (0), failed (1) or not run (3).
//!
//! Notes for maintainers
//! ---------------------
//! * No test here needs the reference or the product: the two videos of a comparison are drawn by `ffmpeg`
//!   from a description of the scene (four flat clips on a chain and one moving box, the way the pair is
//!   built, at a smaller size), so that one of them can be made to differ on purpose.
//! * The tests drive the built `parity` program, as the maintainer does. The real pair, rendered by the
//!   product and by the reference, is measured by the evidence script of the ticket.

mod common;

use common::{folder, tools};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

const GROUND: &str = "0x101820";
const COLOURS: [(&str, &str); 4] = [("a", "0xff5a36"), ("b", "0xffb000"), ("c", "0x3498db"), ("d", "0x2ecc71")];
const OTHER: &str = "0x9b59b6";
/// A ground so unlike the usual one that every picture differs.
const BRIGHT_GROUND: &str = "0x8090a0";

/// How one video of the scene is drawn.
#[derive(Clone)]
struct Scene {
    fps: u32,
    frames: u32,
    size: (u32, u32),
    /// First frame each clip shows on, and one past its last, in the order a, b, c, d.
    windows: [(u32, u32); 4],
    /// The colour behind the clips.
    ground: &'static str,
    /// A clip drawn in this colour instead of its own.
    recoloured: Option<(usize, &'static str)>,
    /// The moving box is this many pixels to the right on frames in the range.
    mover_shift: Option<((u32, u32), i32)>,
    codec: &'static str,
    pix_fmt: &'static str,
}

impl Default for Scene {
    fn default() -> Scene {
        Scene {
            fps: 30,
            frames: 60,
            size: (320, 180),
            windows: [(5, 20), (25, 40), (35, 50), (50, 58)],
            ground: GROUND,
            recoloured: None,
            mover_shift: None,
            codec: "libx264",
            pix_fmt: "yuv420p",
        }
    }
}

/// Draws the scene with ffmpeg: four 40 by 40 clips in places of their own, and a white 24 by 24 box that
/// moves 3 pixels a frame from frame 5 to frame 45, in the lower half.
fn draw(path: &Path, scene: &Scene) {
    let tools = tools();
    let seconds = f64::from(scene.frames) / f64::from(scene.fps);
    let source = |colour: &str, size: &str| format!("color=c={colour}:s={size}:r={}:d={seconds}", scene.fps);
    let mut graph = String::new();
    let mut last = "0".to_string();
    for (k, &(start, end)) in scene.windows.iter().enumerate() {
        let x = 10 + 60 * k;
        graph.push_str(&format!(
            "[{last}][{}]overlay=format=rgb:x={x}:y=10:enable='between(n,{start},{})'[v{k}];",
            k + 1,
            end - 1
        ));
        last = format!("v{k}");
    }
    let (range, shift) = scene.mover_shift.unwrap_or(((1, 0), 0));
    graph.push_str(&format!(
        "[{last}][5]overlay=format=rgb:x='10+3*clip(n-5,0,40)+if(between(n,{},{}),{shift},0)':y=120[v]",
        range.0, range.1
    ));
    let mut command = Command::new(&tools.ffmpeg);
    command.args(["-v", "error", "-y"]);
    let size = format!("{}x{}", scene.size.0, scene.size.1);
    command.args(["-f", "lavfi", "-i"]).arg(source(scene.ground, &size));
    for (k, (_, colour)) in COLOURS.iter().enumerate() {
        let colour = match scene.recoloured {
            Some((which, other)) if which == k => other,
            _ => colour,
        };
        command.args(["-f", "lavfi", "-i"]).arg(source(colour, "40x40"));
    }
    command.args(["-f", "lavfi", "-i"]).arg(source("0xffffff", "24x24"));
    command.args(["-filter_complex", &graph, "-map", "[v]", "-c:v", scene.codec]);
    if scene.codec == "libx264" {
        command.args(["-crf", "10", "-preset", "veryfast"]);
    } else {
        command.args(["-q:v", "2"]);
    }
    command.args([
        "-pix_fmt",
        scene.pix_fmt,
        "-r",
        &scene.fps.to_string(),
        "-colorspace",
        "bt709",
        "-color_primaries",
        "bt709",
        "-color_trc",
        "bt709",
    ]);
    let made = command.arg(path).output().expect("ffmpeg could not be started");
    assert!(made.status.success(), "ffmpeg failed: {}", String::from_utf8_lossy(&made.stderr));
}

/// Two drawn videos in a folder of the test's own: `ours.mp4` and `reference.mp4`.
fn videos(test: &str, ours: &Scene, reference: &Scene) -> (PathBuf, PathBuf, PathBuf) {
    let dir = folder(test);
    let (a, b) = (dir.join("ours.mp4"), dir.join("reference.mp4"));
    draw(&a, ours);
    draw(&b, reference);
    (dir, a, b)
}

struct Ran {
    out: String,
    code: i32,
}

/// The arguments that ask for every group on the scene above.
fn full(extra: &[&str]) -> Vec<String> {
    let mut args: Vec<String> = ["--background", "101820"].map(String::from).to_vec();
    for (name, colour) in COLOURS {
        args.push("--clip".into());
        args.push(format!("{name}={}", &colour[2..]));
    }
    args.extend(["--mover", "ffffff"].map(String::from));
    args.extend(extra.iter().map(|s| s.to_string()));
    args
}

fn compare(ours: &Path, reference: &Path, args: &[String]) -> Ran {
    let done = Command::new(env!("CARGO_BIN_EXE_parity"))
        .arg("compare")
        .arg(ours)
        .arg(reference)
        .args(args)
        .output()
        .unwrap();
    Ran {
        out: String::from_utf8_lossy(&done.stdout).into_owned(),
        code: done.status.code().unwrap(),
    }
}

/// The line of the report that starts with this check name (after two spaces of indent).
fn line<'a>(ran: &'a Ran, name: &str) -> &'a str {
    ran.out
        .lines()
        .find(|l| l.trim_start().starts_with(&format!("{name} ")) && l.contains(" | threshold: "))
        .unwrap_or_else(|| panic!("no check named {name} in:\n{}", ran.out))
}

/// The number after "largest difference" in a line of the report.
fn largest(text: &str) -> f64 {
    let after = text
        .split("largest difference ")
        .nth(1)
        .unwrap_or_else(|| panic!("no distance in: {text}"));
    after.split(' ').next().unwrap().parse().unwrap()
}

fn verdict_of(ran: &Ran, name: &str) -> String {
    let rest = line(ran, name).trim_start()[name.len()..].trim_start();
    if rest.starts_with("pass") {
        "pass".into()
    } else if rest.starts_with("fail") {
        "fail".into()
    } else {
        rest.to_string()
    }
}

#[test]
fn s2_t102_c1_the_report_shows_the_five_container_facts_each_identical_between_two_equal_videos() {
    let (_, a, b) = videos("s2_t102_c1_equal", &Scene::default(), &Scene::default());
    let ran = compare(&a, &b, &full(&[]));
    assert_eq!(ran.code, 0, "{}", ran.out);
    for (check, measured) in [
        ("container: frame count", "ours 60, reference 60"),
        ("container: frame rate", "ours 30/1, reference 30/1"),
        ("container: size", "ours 320x180, reference 320x180"),
        ("container: codec", "ours h264, reference h264"),
        ("container: pixel format", "ours yuv420p, reference yuv420p"),
    ] {
        assert_eq!(verdict_of(&ran, check), "pass", "{}", ran.out);
        let said = line(&ran, check);
        assert!(said.contains(measured), "{said}");
        assert!(said.ends_with("| threshold: identical"), "{said}");
    }
}

#[test]
fn s2_t102_c1_a_different_count_rate_size_codec_or_pixel_format_fails_its_own_line_and_only_that_one() {
    let base = Scene::default();
    let cases: [(&str, Scene); 5] = [
        (
            "container: frame count",
            Scene {
                frames: 45,
                windows: [(5, 20), (25, 40), (35, 44), (44, 45)],
                ..base.clone()
            },
        ),
        ("container: frame rate", Scene { fps: 24, ..base.clone() }),
        (
            "container: size",
            Scene {
                size: (480, 270),
                ..base.clone()
            },
        ),
        (
            "container: codec",
            Scene {
                codec: "mpeg4",
                ..base.clone()
            },
        ),
        (
            "container: pixel format",
            Scene {
                pix_fmt: "yuv444p",
                ..base.clone()
            },
        ),
    ];
    for (k, (check, other)) in cases.iter().enumerate() {
        let (_, a, b) = videos(&format!("s2_t102_c1_differs_{k}"), &base, other);
        let ran = compare(&a, &b, &full(&[]));
        assert_eq!(ran.code, 1, "{check}:\n{}", ran.out);
        for (name, _) in cases.iter() {
            let expected = if name == check { "fail" } else { "pass" };
            assert_eq!(verdict_of(&ran, name), expected, "{check} differs:\n{}", ran.out);
        }
        assert!(ran.out.contains("result: failed"), "{}", ran.out);
    }
}

#[test]
fn s2_t102_c2_the_report_gives_the_similarity_at_each_named_frame_against_the_threshold_and_writes_the_sheet() {
    let (dir, a, b) = videos("s2_t102_c2_sheet", &Scene::default(), &Scene::default());
    let sheet = dir.join("out").join("sheet.png");
    let frames = [0, 4, 5, 19, 20, 34, 35, 57, 59];
    let list = frames.map(|n| n.to_string()).join(",");
    let ran = compare(&a, &b, &full(&["--frames", &list, "--sheet", sheet.to_str().unwrap()]));
    assert_eq!(ran.code, 0, "{}", ran.out);
    for n in frames {
        let name = format!("pictures: frame {n}");
        assert_eq!(verdict_of(&ran, &name), "pass", "{}", ran.out);
        let said = line(&ran, &name);
        assert!(
            said.contains("ssim ") && said.ends_with("| threshold: ssim at least 0.97"),
            "{said}"
        );
    }
    assert_eq!(
        ran.out.lines().filter(|l| l.contains("pictures: frame ")).count(),
        frames.len(),
        "{}",
        ran.out
    );
    assert!(line(&ran, "pictures: lowest of the sampled frames").contains("all 9 sampled frames"));
    // The sheet holds the same frames, both videos side by side.
    let bytes = std::fs::read(&sheet).expect("the sheet is written, in a folder made for it");
    assert!(measure::png_size(&bytes).is_some(), "the sheet is a PNG file");
    assert!(ran.out.contains("side-by-side sheet:"), "{}", ran.out);
    assert!(
        ran.out.contains(&format!("sampled frames (counting from 0): {frames:?} of 60")),
        "{}",
        ran.out
    );
}

#[test]
fn s2_t102_c2_a_picture_that_differs_falls_below_the_threshold_and_the_report_names_the_lowest_frame() {
    let other = Scene {
        ground: BRIGHT_GROUND,
        ..Scene::default()
    };
    let (_, a, b) = videos("s2_t102_c2_differs", &other, &Scene::default());
    let ran = compare(&a, &b, &full(&["--frames", "0,10,30,55"]));
    assert_eq!(ran.code, 1, "{}", ran.out);
    for n in [0, 10, 30, 55] {
        let name = format!("pictures: frame {n}");
        assert_eq!(verdict_of(&ran, &name), "fail", "{}", ran.out);
        let value: f64 = line(&ran, &name)
            .split("ssim ")
            .nth(1)
            .unwrap()
            .split(' ')
            .next()
            .unwrap()
            .parse()
            .unwrap();
        assert!(value < 0.97, "frame {n} scores {value}");
    }
    assert_eq!(verdict_of(&ran, "pictures: lowest of the sampled frames"), "fail");
    assert!(
        line(&ran, "pictures: lowest of the sampled frames").contains("below the threshold 0.97"),
        "{}",
        ran.out
    );
    // A change too small to see (one clip in another colour of nearly the same brightness) stays above it.
    let faint = Scene {
        recoloured: Some((0, OTHER)),
        ..Scene::default()
    };
    let (_, a, b) = videos("s2_t102_c2_faint", &faint, &Scene::default());
    let ran = compare(&a, &b, &full(&["--frames", "10"]));
    assert_eq!(verdict_of(&ran, "pictures: frame 10"), "pass", "{}", ran.out);
}

#[test]
fn s2_t102_c2_without_a_list_the_frames_are_evenly_spaced_and_a_frame_beyond_the_end_is_refused() {
    let (_, a, b) = videos("s2_t102_c2_default", &Scene::default(), &Scene::default());
    let ran = compare(&a, &b, &full(&["--samples", "5"]));
    assert_eq!(ran.code, 0, "{}", ran.out);
    assert_eq!(ran.out.lines().filter(|l| l.contains("pictures: frame ")).count(), 5);
    assert!(
        ran.out.contains("sampled frames (counting from 0): [0, 15, 30, 44, 59] of 60"),
        "{}",
        ran.out
    );
    let refused = compare(&a, &b, &full(&["--frames", "3,60"]));
    assert_eq!(refused.code, 2, "{}", refused.out);
    assert!(refused.out.contains("frame 60 was asked for"), "{}", refused.out);
    let both = compare(&a, &b, &full(&["--frames", "3", "--samples", "4"]));
    assert_eq!(both.code, 2, "{}", both.out);
}

#[test]
fn s2_t102_c3_each_clip_of_the_chain_is_reported_under_its_name_with_the_frame_it_appears_and_disappears_on() {
    let (_, a, b) = videos("s2_t102_c3_chain", &Scene::default(), &Scene::default());
    let ran = compare(&a, &b, &full(&["--frames", "0"]));
    assert_eq!(ran.code, 0, "{}", ran.out);
    for (name, (start, end)) in ["a", "b", "c", "d"].iter().zip(Scene::default().windows) {
        let appears = format!("timing: clip {name} appears");
        let gone = format!("timing: clip {name} disappears");
        assert_eq!(verdict_of(&ran, &appears), "pass", "{}", ran.out);
        assert_eq!(verdict_of(&ran, &gone), "pass", "{}", ran.out);
        assert!(
            line(&ran, &appears).contains(&format!("ours frame {start}, reference frame {start}, 0 frame(s) apart")),
            "{}",
            line(&ran, &appears)
        );
        assert!(
            line(&ran, &gone).contains(&format!("ours frame {end}, reference frame {end}, 0 frame(s) apart")),
            "{}",
            line(&ran, &gone)
        );
        assert!(line(&ran, &appears).ends_with("| threshold: at most 1 frame apart"));
    }
}

#[test]
fn s2_t102_c3_one_frame_apart_passes_two_frames_apart_fails_and_only_for_the_clip_that_moved() {
    // Ours has clip c one frame late, and clip d two frames late; the others are where they should be.
    let mut late = Scene::default();
    late.windows[2] = (36, 51);
    late.windows[3] = (52, 58);
    let (_, a, b) = videos("s2_t102_c3_late", &late, &Scene::default());
    let ran = compare(&a, &b, &full(&["--frames", "0"]));
    assert_eq!(ran.code, 1, "{}", ran.out);
    for (check, expected) in [
        ("timing: clip a appears", "pass"),
        ("timing: clip a disappears", "pass"),
        ("timing: clip b appears", "pass"),
        ("timing: clip b disappears", "pass"),
        ("timing: clip c appears", "pass"),
        ("timing: clip c disappears", "pass"),
        ("timing: clip d appears", "fail"),
        ("timing: clip d disappears", "pass"),
    ] {
        assert_eq!(verdict_of(&ran, check), expected, "{check}:\n{}", ran.out);
    }
    assert!(line(&ran, "timing: clip c appears").contains("ours frame 36, reference frame 35, 1 frame(s) apart"));
    assert!(line(&ran, "timing: clip d appears").contains("ours frame 52, reference frame 50, 2 frame(s) apart"));
}

#[test]
fn s2_t102_c3_a_clip_that_never_shows_in_one_video_fails_and_a_bad_clip_name_is_a_mistake() {
    let mut missing = Scene::default();
    missing.windows[1] = (0, 1);
    missing.recoloured = Some((1, OTHER));
    let (_, a, b) = videos("s2_t102_c3_missing", &missing, &Scene::default());
    let ran = compare(&a, &b, &full(&["--frames", "0"]));
    assert_eq!(ran.code, 1, "{}", ran.out);
    assert_eq!(verdict_of(&ran, "timing: clip b appears"), "fail", "{}", ran.out);
    assert!(
        line(&ran, "timing: clip b appears").contains("ours never, reference frame 25"),
        "{}",
        ran.out
    );
    for bad in [vec!["--clip", "x=zzzzzz"], vec!["--clip", "=ff0000"], vec!["--clip", "p q=ff0000"]] {
        let mut args: Vec<String> = bad.iter().map(|s| s.to_string()).collect();
        args.extend(["--frames", "0"].map(String::from));
        let refused = compare(&a, &b, &args);
        assert_eq!(refused.code, 2, "{bad:?}: {}", refused.out);
    }
    let twice: Vec<String> = ["--clip", "x=ff0000", "--clip", "x=00ff00"].map(String::from).to_vec();
    assert_eq!(compare(&a, &b, &twice).code, 2);
}

#[test]
fn s2_t102_c4_the_position_is_compared_on_every_frame_and_equal_videos_pass_within_one_pixel() {
    let (_, a, b) = videos("s2_t102_c4_equal", &Scene::default(), &Scene::default());
    let ran = compare(&a, &b, &full(&["--frames", "0"]));
    assert_eq!(ran.code, 0, "{}", ran.out);
    let said = line(&ran, "position: moving element");
    assert_eq!(verdict_of(&ran, "position: moving element"), "pass");
    assert!(said.contains("60 frames compared"), "{said}");
    assert!(said.contains("largest difference 0.00 px"), "{said}");
    assert!(said.ends_with("| threshold: at most 1 px apart on every frame"), "{said}");
}

/// The frame the report names for the largest distance.
fn named_frame(text: &str) -> u32 {
    text.split(" at frame ").nth(1).unwrap().split(' ').next().unwrap().parse().unwrap()
}

#[test]
fn s2_t102_c4_a_box_three_pixels_off_on_one_frame_only_fails_and_names_that_frame_and_the_distance() {
    let off = |frame: u32| Scene {
        mover_shift: Some(((frame, frame), 3)),
        ..Scene::default()
    };
    let (_, a, b) = videos("s2_t102_c4_one_frame", &off(30), &Scene::default());
    let ran = compare(&a, &b, &full(&["--frames", "0"]));
    assert_eq!(ran.code, 1, "{}", ran.out);
    let said = line(&ran, "position: moving element");
    assert_eq!(verdict_of(&ran, "position: moving element"), "fail", "{}", ran.out);
    assert!((2.5..3.5).contains(&largest(said)), "{said}");
    assert!(said.contains("limit 1 px"), "{said}");
    // Everything else about the scene is the same, so only the position fails.
    assert_eq!(verdict_of(&ran, "timing: clip a appears"), "pass");
    assert_eq!(verdict_of(&ran, "container: size"), "pass");

    // The frame named follows the frame that is off: ten frames later in the video, ten frames later in the report.
    let first = named_frame(said);
    let (_, a, b) = videos("s2_t102_c4_one_frame_later", &off(40), &Scene::default());
    let later = compare(&a, &b, &full(&["--frames", "0"]));
    assert_eq!(named_frame(line(&later, "position: moving element")), first + 10, "{}", later.out);
}

#[test]
fn s2_t102_c5_when_every_check_passes_the_report_says_passed_with_exit_zero_and_points_at_the_sheet() {
    let (dir, a, b) = videos("s2_t102_c5_passes", &Scene::default(), &Scene::default());
    let sheet = dir.join("sheet.png");
    let ran = compare(&a, &b, &full(&["--sheet", sheet.to_str().unwrap()]));
    assert_eq!(ran.code, 0, "{}", ran.out);
    assert!(ran.out.trim_end().ends_with("result: passed"), "{}", ran.out);
    assert!(!ran.out.contains("failed checks"), "{}", ran.out);
    assert!(!ran.out.contains(" fail "), "{}", ran.out);
    assert!(sheet.is_file());
    assert!(ran.out.contains(&format!("side-by-side sheet: {}", sheet.display())), "{}", ran.out);
    // Every group is in the report that passed: five container facts, the pictures, eight events, the position.
    let groups = |prefix: &str| ran.out.lines().filter(|l| l.trim_start().starts_with(prefix)).count();
    assert_eq!(groups("container: "), 5);
    assert!(groups("pictures: ") >= 2);
    assert_eq!(groups("timing: "), 8);
    assert_eq!(groups("position: "), 1);
}

#[test]
fn s2_t102_c6_a_failed_check_is_repeated_with_its_measured_value_and_its_threshold_and_the_exit_code_is_one() {
    let off = Scene {
        mover_shift: Some(((20, 40), 6)),
        codec: "mpeg4",
        ..Scene::default()
    };
    let (_, a, b) = videos("s2_t102_c6_fails", &off, &Scene::default());
    let ran = compare(&a, &b, &full(&["--frames", "0,25,30"]));
    assert_eq!(ran.code, 1, "{}", ran.out);
    assert!(ran.out.trim_end().ends_with("result: failed"), "{}", ran.out);
    let (_, closing) = ran
        .out
        .split_once("failed checks (the check, the measured value, the threshold):\n")
        .expect("a failed comparison closes with the failed checks");
    let closing = closing.split("information only").next().unwrap();
    // The codec check: named, measured, with its threshold.
    assert!(
        closing.contains("container: codec\n    measured:  ours mpeg4, reference h264\n    threshold: identical\n"),
        "{closing}"
    );
    // The position check: named, the measured distance, its threshold.
    let at = closing.find("position: moving element\n").expect("the position check is repeated");
    let position = &closing[at..];
    assert!(position.contains("    measured:  largest difference "), "{position}");
    assert!((5.5..6.5).contains(&largest(position)), "{position}");
    assert!(
        position.contains("    threshold: at most 1 px apart on every frame\n"),
        "{position}"
    );
    // A check that passed is not repeated.
    assert!(!closing.contains("container: size"), "{closing}");
    assert!(!closing.contains("timing: clip a appears"), "{closing}");
}

#[test]
fn s2_t102_c6_the_json_report_carries_the_threshold_of_each_check_and_a_comparison_never_run_is_not_passed() {
    let off = Scene {
        windows: [(5, 20), (25, 40), (36, 50), (50, 58)],
        ..Scene::default()
    };
    let (_, a, b) = videos("s2_t102_c6_json", &off, &Scene::default());
    let ran = compare(&a, &b, &full(&["--frames", "0", "--json"]));
    assert_eq!(ran.code, 0, "one frame late is inside the limit:\n{}", ran.out);
    let report: Value = serde_json::from_str(&ran.out).unwrap();
    assert_eq!(report["result"], "passed");
    let checks = report["checks"].as_array().unwrap();
    assert!(checks.iter().all(|c| !c["threshold"].as_str().unwrap().is_empty()), "{report}");
    let appears = checks.iter().find(|c| c["name"] == "timing: clip c appears").unwrap();
    assert_eq!(appears["threshold"], "at most 1 frame apart");

    // The exit code of a tool that cannot run is not the code of a pass: a missing video is a mistake.
    let missing = compare(&a, Path::new("/nonexistent/reference.mp4"), &full(&[]));
    assert_eq!(missing.code, 2, "{}", missing.out);
    assert!(!missing.out.contains("result: passed"), "{}", missing.out);
}
