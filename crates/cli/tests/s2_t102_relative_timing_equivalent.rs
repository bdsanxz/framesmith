//! Sprint 2, ticket T102: the render of the relative-timing pair, made by `framesmith render`, is judged by
//! the comparison tool against a second video of the same scene on the agreed checks (container facts,
//! picture similarity at sampled frames, the frames on which each clip of the chain appears and disappears,
//! and the tracked position of the moving box). A check that fails is named with its measured value and its
//! threshold, and the tool's exit code says so.
//!
//! Notes for maintainers
//! ---------------------
//! * The second video is not the reference's render (that program is not part of the repository or of any
//!   test). It is the same scene drawn by `ffmpeg` from the description in the fixture's comment: ground
//!   `#101820`; four 120 by 120 clips at 30 frames a second, `a` on frames 30 to 89, `b` 105 to 164,
//!   `c` 150 to 209, `d` 210 to 239; a white 60 by 60 box that moves 2 px a frame from frame 15 to frame 255.
//!   The drawing is itself checked against that description before it is used (`the_drawing_is_the_scene`).
//!   The real pair, with the reference's own render, is measured by the evidence script of the ticket.
//! * The tool is the `parity` program built next to the `framesmith` program of the same build; when it is
//!   not there yet (`cargo test -p cli` builds only this package) the test builds it.
//! * The checks of the full report are run once and shared between the tests that read it, because each run
//!   decodes every frame of both videos.

mod common;

use common::*;
use measure::{Frame, Rgb};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

const WHITE: Rgb = [0xff, 0xff, 0xff];
const COLOURS: [(&str, &str); 4] = [("a", "ff5a36"), ("b", "ffb000"), ("c", "3498db"), ("d", "2ecc71")];
/// First frame each clip shows on, and one past its last.
const WINDOWS: [(u32, u32); 4] = [(30, 90), (105, 165), (150, 210), (210, 240)];
const LEFTS: [u32; 4] = [40, 190, 340, 490];
/// The frames on which something happens, and the first and last.
const EVENT_FRAMES: &str = "0,29,30,89,90,104,105,149,150,164,165,209,210,239,240,269";

/// How the second video is drawn.
#[derive(Clone, Default)]
struct Drawn {
    /// Frames each clip is shown late by (a, b, c, d).
    late: [u32; 4],
    /// The moving box is this many pixels to the right.
    mover_shift: i32,
    /// A clip drawn in this colour instead of its own: (index, colour as RRGGBB).
    recoloured: Option<(usize, &'static str)>,
    /// The pixel format of the file; the usual one when empty.
    pix_fmt: &'static str,
}

/// Draws the scene with ffmpeg. The expression for the box is written one frame behind the frame count
/// that the overlay filter reports, which is one ahead of the frame that is drawn (checked below).
fn draw(path: &Path, how: &Drawn) {
    let mut command = Command::new(ffmpeg());
    command.args(["-v", "error", "-y", "-f", "lavfi", "-i", "color=c=0x101820:s=640x360:r=30:d=9"]);
    for (k, (_, colour)) in COLOURS.iter().enumerate() {
        let colour = how.recoloured.filter(|(which, _)| *which == k).map_or(*colour, |(_, c)| c);
        command
            .args(["-f", "lavfi", "-i"])
            .arg(format!("color=c=0x{colour}:s=120x120:r=30:d=9"));
    }
    command.args(["-f", "lavfi", "-i", "color=c=0xffffff:s=60x60:r=30:d=9"]);
    let mut graph = String::new();
    let mut last = "0".to_string();
    for (k, (start, end)) in WINDOWS.iter().enumerate() {
        graph.push_str(&format!(
            "[{last}][{}]overlay=format=rgb:x={}:y=40:enable='between(n,{},{})'[v{k}];",
            k + 1,
            LEFTS[k],
            start + how.late[k],
            end - 1 + how.late[k]
        ));
        last = format!("v{k}");
    }
    graph.push_str(&format!(
        "[{last}][5]overlay=format=rgb:x='{}+2*clip(n-16,0,240)':y=240[v]",
        40 + how.mover_shift
    ));
    command.args([
        "-filter_complex",
        &graph,
        "-map",
        "[v]",
        "-c:v",
        "libx264",
        "-crf",
        "12",
        "-preset",
        "veryfast",
    ]);
    command.args(["-pix_fmt", if how.pix_fmt.is_empty() { "yuv420p" } else { how.pix_fmt }]);
    command.args([
        "-r",
        "30",
        "-colorspace",
        "bt709",
        "-color_primaries",
        "bt709",
        "-color_trc",
        "bt709",
    ]);
    let made = command.arg(path).output().expect("ffmpeg could not be started");
    assert!(made.status.success(), "ffmpeg: {}", String::from_utf8_lossy(&made.stderr));
}

/// The left edge of the white box in the lower part of a picture.
fn mover_left(frame: &Frame) -> u32 {
    frame
        .crop(0, 200, 640, 160)
        .bounds(WHITE, 24)
        .expect("the white box is in the picture")
        .left
}

/// The scratch folder shared by the tests of this file: made once, so the videos are made once.
fn shared() -> &'static PathBuf {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| scratch("s2_t102_shared"))
}

/// Our render of the pair, made once by the render command.
fn ours() -> &'static PathBuf {
    static VIDEO: OnceLock<PathBuf> = OnceLock::new();
    VIDEO.get_or_init(|| {
        let copy = shared().join("pair-relative-timing");
        copy_tree(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/pair-relative-timing"),
            &copy,
        );
        let video = shared().join("ours.mp4");
        let ran = framesmith(&["render", text(&copy), "--output", text(&video)]);
        assert_eq!(ran.code, 0, "{}", ran.all());
        video
    })
}

/// A drawn video of the scene, made once for each name.
fn drawn(name: &str, how: &Drawn) -> PathBuf {
    let path = shared().join(format!("{name}.mp4"));
    // The tests of a file run side by side: whoever gets here first draws it, the others wait for the file.
    static DRAWING: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = DRAWING.lock().unwrap_or_else(|e| e.into_inner());
    if !path.is_file() {
        draw(&path, how);
    }
    path
}

/// The comparison tool of this build.
fn parity() -> &'static PathBuf {
    static PROGRAM: OnceLock<PathBuf> = OnceLock::new();
    PROGRAM.get_or_init(|| {
        let program = Path::new(BIN).parent().unwrap().join("parity");
        if !program.is_file() {
            let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
            let built = Command::new(cargo)
                .args(["build", "-p", "parity", "--quiet"])
                .current_dir(env!("CARGO_MANIFEST_DIR"))
                .output()
                .unwrap();
            assert!(built.status.success(), "{}", String::from_utf8_lossy(&built.stderr));
        }
        assert!(program.is_file(), "no comparison tool at {}", program.display());
        program
    })
}

#[derive(Clone)]
struct Said {
    out: String,
    code: i32,
}

fn tool(args: &[String]) -> Said {
    let done = Command::new(parity()).args(args).output().unwrap();
    Said {
        out: String::from_utf8_lossy(&done.stdout).into_owned(),
        code: done.status.code().unwrap(),
    }
}

fn strings(args: &[&str]) -> Vec<String> {
    args.iter().map(|s| s.to_string()).collect()
}

/// The arguments that name the pair's clips by colour, the box and the ground.
fn clips_and_mover() -> Vec<String> {
    let mut args = strings(&["--background", "101820"]);
    for (name, colour) in COLOURS {
        args.extend([String::from("--clip"), format!("{name}={colour}")]);
    }
    args.extend(strings(&["--mover", "ffffff"]));
    args
}

/// `parity <command> <ours> <other> ...`
fn run(command: &str, other: &Path, rest: &[String]) -> Said {
    let mut args = strings(&[command, text(ours()), text(other)]);
    args.extend_from_slice(rest);
    tool(&args)
}

/// The whole report on the pair against the drawing, with the sheet; made once.
fn whole() -> &'static (Said, PathBuf) {
    static REPORT: OnceLock<(Said, PathBuf)> = OnceLock::new();
    REPORT.get_or_init(|| {
        let other = drawn("same", &Drawn::default());
        let sheet = shared().join("sheet.png");
        let mut args = clips_and_mover();
        args.extend(strings(&["--frames", EVENT_FRAMES, "--sheet", text(&sheet)]));
        (run("compare", &other, &args), sheet)
    })
}

/// The line of a report for one check: it starts, after the indent, with the name and a space.
fn line<'a>(said: &'a Said, name: &str) -> &'a str {
    said.out
        .lines()
        .find(|l| l.trim_start().starts_with(&format!("{name} ")))
        .unwrap_or_else(|| panic!("no check named {name} in:\n{}", said.out))
}

fn verdict(said: &Said, name: &str) -> &'static str {
    let rest = line(said, name).trim_start()[name.len()..].trim_start();
    if rest.starts_with("pass") {
        "pass"
    } else if rest.starts_with("fail") {
        "fail"
    } else {
        "other"
    }
}

fn number_after(text: &str, marker: &str) -> f64 {
    text.split(marker)
        .nth(1)
        .unwrap_or_else(|| panic!("no '{marker}' in: {text}"))
        .split(' ')
        .next()
        .unwrap()
        .parse()
        .unwrap()
}

#[test]
fn s2_t102_the_drawing_is_the_scene() {
    // Before the drawing stands in for the second half of the pair, it is held to the scene's description.
    let video = drawn("same", &Drawn::default());
    let frames = measure::frames_from_video(&ffmpeg(), &video, 640, 360).unwrap();
    assert_eq!(frames.len(), 270);
    for (k, ((_, colour), (start, end))) in COLOURS.iter().zip(WINDOWS).enumerate() {
        let colour = common_rgb(colour);
        for n in [start - 1, start, end - 1, end] {
            let shown = frames[n as usize].count(colour, 24) >= 10_000;
            assert_eq!(shown, (start..end).contains(&n), "clip {k}, frame {n}");
        }
    }
    let lefts: Vec<u32> = frames.iter().map(mover_left).collect();
    for (n, left) in lefts.iter().enumerate() {
        assert_eq!(*left, 40 + 2 * (n.clamp(15, 255) as u32 - 15), "frame {n}");
    }
}

fn common_rgb(text: &str) -> Rgb {
    let part = |at: usize| u8::from_str_radix(&text[at..at + 2], 16).unwrap();
    [part(0), part(2), part(4)]
}

#[test]
fn s2_t102_c1_the_report_shows_frame_count_rate_size_codec_and_pixel_format_identical_between_the_two_videos() {
    let (said, _) = whole();
    assert_eq!(said.code, 0, "{}", said.out);
    for (check, measured) in [
        ("container: frame count", "ours 270, reference 270"),
        ("container: frame rate", "ours 30/1, reference 30/1"),
        ("container: size", "ours 640x360, reference 640x360"),
        ("container: codec", "ours h264, reference h264"),
        ("container: pixel format", "ours yuv420p, reference yuv420p"),
    ] {
        assert_eq!(verdict(said, check), "pass", "{}", said.out);
        let printed = line(said, check);
        assert!(
            printed.contains(measured) && printed.ends_with("| threshold: identical"),
            "{printed}"
        );
    }

    // Control: a second video in another pixel format differs on that line, and on that line only.
    let other = drawn(
        "other-format",
        &Drawn {
            pix_fmt: "yuv444p",
            ..Drawn::default()
        },
    );
    let facts = run("facts", &other, &[]);
    assert_eq!(facts.code, 1, "{}", facts.out);
    assert!(facts.out.contains("yuv420p") && facts.out.contains("yuv444p"), "{}", facts.out);
    assert_eq!(verdict(&facts, "pixel format"), "fail");
    for same in ["frame count", "frame rate", "size", "codec"] {
        assert_eq!(verdict(&facts, same), "pass", "{}", facts.out);
    }
}

#[test]
fn s2_t102_c2_the_report_shows_a_similarity_of_at_least_0_97_at_the_sampled_frames_and_the_sheet_is_written() {
    let (said, sheet) = whole();
    assert_eq!(said.code, 0, "{}", said.out);
    let mut seen = 0;
    for n in EVENT_FRAMES.split(',') {
        let name = format!("pictures: frame {n}");
        assert_eq!(verdict(said, &name), "pass", "{}", said.out);
        let printed = line(said, &name);
        assert!(number_after(printed, "ssim ") >= 0.97, "{printed}");
        assert!(printed.ends_with("| threshold: ssim at least 0.97"), "{printed}");
        seen += 1;
    }
    assert_eq!(seen, 16);
    assert!(line(said, "pictures: lowest of the sampled frames").contains("all 16 sampled frames"));
    // The sheet that goes with the report: a PNG of both videos' frames, side by side.
    let bytes = std::fs::read(sheet).expect("the sheet is written");
    let (width, height) = measure::png_size(&bytes).expect("the sheet is a PNG file");
    assert!(width >= 480 && height >= 270 * 8, "{width}x{height}");

    // Control: a clip drawn in a very different brightness is below 0.97 on the frames that show it.
    let other = drawn(
        "black-a",
        &Drawn {
            recoloured: Some((0, "000000")),
            ..Drawn::default()
        },
    );
    let pictures = run("pictures", &other, &strings(&["--frames", "0,30,89,269"]));
    assert_eq!(pictures.code, 1, "{}", pictures.out);
    for n in [30, 89] {
        assert_eq!(verdict(&pictures, &format!("frame {n}")), "fail", "{}", pictures.out);
        assert!(number_after(line(&pictures, &format!("frame {n}")), "ssim ") < 0.97);
    }
    for n in [0, 269] {
        assert_eq!(verdict(&pictures, &format!("frame {n}")), "pass", "{}", pictures.out);
    }
}

#[test]
fn s2_t102_c3_for_each_clip_of_the_chain_the_frame_it_appears_on_and_the_frame_it_disappears_on_are_within_one_frame() {
    let (said, _) = whole();
    for ((name, _), (start, end)) in COLOURS.iter().zip(WINDOWS) {
        for (what, frame) in [("appears", start), ("disappears", end)] {
            let check = format!("timing: clip {name} {what}");
            assert_eq!(verdict(said, &check), "pass", "{}", said.out);
            let printed = line(said, &check);
            assert!(
                printed.contains(&format!("ours frame {frame}, reference frame {frame}")),
                "{printed}"
            );
            assert!(printed.ends_with("| threshold: at most 1 frame apart"), "{printed}");
        }
    }

    // Control: clip b drawn one frame late is inside the limit, two frames late is outside it, and the
    // other three clips are unaffected.
    let only_clips = strings(&[
        "--clip",
        "a=ff5a36",
        "--clip",
        "b=ffb000",
        "--clip",
        "c=3498db",
        "--clip",
        "d=2ecc71",
        "--background",
        "101820",
    ]);
    let one = drawn(
        "b-late-1",
        &Drawn {
            late: [0, 1, 0, 0],
            ..Drawn::default()
        },
    );
    let timing = run("timing", &one, &only_clips);
    assert_eq!(timing.code, 0, "{}", timing.out);
    assert!(line(&timing, "clip b appears").contains("ours frame 105, reference frame 106, 1 frame(s) apart"));
    let two = drawn(
        "b-late-2",
        &Drawn {
            late: [0, 2, 0, 0],
            ..Drawn::default()
        },
    );
    let timing = run("timing", &two, &only_clips);
    assert_eq!(timing.code, 1, "{}", timing.out);
    assert_eq!(verdict(&timing, "clip b appears"), "fail");
    assert!(line(&timing, "clip b appears").contains("ours frame 105, reference frame 107, 2 frame(s) apart"));
    for same in [
        "clip a appears",
        "clip a disappears",
        "clip c appears",
        "clip c disappears",
        "clip d appears",
        "clip d disappears",
    ] {
        assert_eq!(verdict(&timing, same), "pass", "{}", timing.out);
    }
}

#[test]
fn s2_t102_c4_the_tracked_position_of_the_moving_box_differs_by_at_most_one_pixel_on_every_frame() {
    let (said, _) = whole();
    let printed = line(said, "position: moving element");
    assert_eq!(verdict(said, "position: moving element"), "pass", "{}", said.out);
    assert!(printed.contains("270 frames compared"), "{printed}");
    assert!(number_after(printed, "largest difference ") <= 1.0, "{printed}");
    assert!(printed.ends_with("| threshold: at most 1 px apart on every frame"), "{printed}");

    // Control: the box drawn 3 px to the right is outside the limit, and the report says by how much.
    let other = drawn(
        "mover-3",
        &Drawn {
            mover_shift: 3,
            ..Drawn::default()
        },
    );
    let tracked = run("timing", &other, &strings(&["--background", "101820", "--mover", "ffffff"]));
    assert_eq!(tracked.code, 1, "{}", tracked.out);
    assert_eq!(verdict(&tracked, "position"), "fail");
    let distance = number_after(line(&tracked, "position"), "largest difference ");
    assert!((2.5..3.5).contains(&distance), "{}", tracked.out);
}

#[test]
fn s2_t102_c5_when_every_check_passes_the_report_says_passed_and_the_sheet_is_there_to_go_with_it() {
    let (said, sheet) = whole();
    assert_eq!(said.code, 0, "{}", said.out);
    assert!(said.out.trim_end().ends_with("result: passed"), "{}", said.out);
    assert!(!said.out.contains("failed checks"), "{}", said.out);
    assert!(!said.out.lines().any(|l| l.contains(" fail ")), "{}", said.out);
    assert!(
        said.out.contains(&format!("side-by-side sheet: {}", sheet.display())),
        "{}",
        said.out
    );
    assert!(std::fs::metadata(sheet).unwrap().len() > 10_000);
    // Every group of the agreed checks is in it: 5 facts, the pictures, 8 events, the position.
    let count = |prefix: &str| said.out.lines().filter(|l| l.trim_start().starts_with(prefix)).count();
    assert_eq!(
        (count("container: "), count("timing: "), count("position: ")),
        (5, 8, 1),
        "{}",
        said.out
    );
}

#[test]
fn s2_t102_c6_a_check_that_fails_is_named_with_the_measured_value_and_the_threshold_and_the_exit_code_is_one() {
    // Clip c two frames late, the box 3 px to the right, and another pixel format: one failure in each of three groups.
    let other = drawn(
        "everything-off",
        &Drawn {
            late: [0, 0, 2, 0],
            mover_shift: 3,
            pix_fmt: "yuv444p",
            ..Drawn::default()
        },
    );
    let mut args = clips_and_mover();
    args.extend(strings(&["--frames", "0,30,150,269"]));
    let said = run("compare", &other, &args);
    assert_eq!(said.code, 1, "{}", said.out);
    assert!(said.out.trim_end().ends_with("result: failed"), "{}", said.out);
    let (_, closing) = said
        .out
        .split_once("failed checks (the check, the measured value, the threshold):\n")
        .expect("the report closes with the failed checks");
    let closing = closing.split("information only").next().unwrap();
    assert!(
        closing.contains("container: pixel format\n    measured:  ours yuv420p, reference yuv444p\n    threshold: identical\n"),
        "{closing}"
    );
    assert!(
        closing.contains("timing: clip c appears\n    measured:  ours frame 150, reference frame 152, 2 frame(s) apart (limit 1)\n    threshold: at most 1 frame apart\n"),
        "{closing}"
    );
    let at = closing.find("position: moving element\n").expect("the position check is repeated");
    assert!(
        (2.5..3.5).contains(&number_after(&closing[at..], "largest difference ")),
        "{closing}"
    );
    assert!(
        closing[at..].contains("    threshold: at most 1 px apart on every frame\n"),
        "{closing}"
    );
    // The checks that passed are not repeated there.
    for passed in ["container: size", "timing: clip a appears", "timing: clip b appears"] {
        assert!(!closing.contains(passed), "{passed} in {closing}");
    }
}
