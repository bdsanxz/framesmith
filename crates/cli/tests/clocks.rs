//! T023: page scripts see frame time on every clock, and callbacks run
//! only when the frame time moves.

mod common;

use common::*;
use measure::{Frame, Rgb};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const RED: Rgb = [255, 0, 0];
const GREEN: Rgb = [0, 255, 0];
const BLUE: Rgb = [0, 0, 255];
const BARS: &str = ".bar { position: absolute; left: 0; width: 0; height: 10px; }\n\
#red { top: 5px; background: #ff0000; } #green { top: 25px; background: #00ff00; } #blue { top: 45px; background: #0000ff; }";
const THREE: &str = "<div id=\"red\" class=\"bar\"></div><div id=\"green\" class=\"bar\"></div><div id=\"blue\" class=\"bar\"></div>";

/// On every animation frame the page draws three bars whose widths are
/// the date, the high-resolution timer and the callback's own time stamp:
/// 2 pixels, plus 1 pixel for every 20 milliseconds.
const DRAW: &str = "<script>\n\
function width(ms) { return (ms / 20 + 2) + \"px\"; }\n\
function draw(stamp) {\n\
  document.getElementById(\"red\").style.width = width(Date.now());\n\
  document.getElementById(\"green\").style.width = width(performance.now());\n\
  document.getElementById(\"blue\").style.width = width(new Date().getTime() === stamp ? stamp : 1e9);\n\
  requestAnimationFrame(draw);\n\
}\n\
requestAnimationFrame(draw);\n\
</script>";

fn width(frame: &Frame, colour: Rgb) -> u32 {
    frame.bounds(colour, 0).map_or(0, |bar| bar.width())
}

/// 400 by 60 at 10 frames a second: frame `i` is at `100 × i` milliseconds.
fn clock_page(test: &str, seconds: u32) -> PathBuf {
    let attrs = format!("data-fs-size=\"400x60\" data-fs-fps=\"10\" data-fs-duration=\"{seconds}\"");
    project(test, "clocks", &page(&attrs, BARS, THREE, DRAW))
}

fn assert_frame_time(frames: &[Frame]) {
    for (number, frame) in frames.iter().enumerate() {
        let wanted = 5 * number as u32 + 2;
        let widths = (width(frame, RED), width(frame, GREEN), width(frame, BLUE));
        assert_eq!(widths, (wanted, wanted, wanted), "frame {number}");
    }
}

#[test]
fn t023_c1_every_clock_shows_the_time_of_the_frame_in_two_renders() {
    let dir = clock_page("t023_c1", 3);
    let (ran, first) = render_frames(&dir, &[], &[]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert_eq!(first.len(), 30);
    assert_frame_time(&first);
    // Real time has moved on; the second render shows the same times.
    std::thread::sleep(Duration::from_secs(3));
    let (ran, second) = render_frames(&dir, &[], &[]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert_frame_time(&second);
    let hashes = |frames: &[Frame]| frames.iter().map(Frame::hash).collect::<Vec<u64>>();
    assert_eq!(measure::first_difference(&hashes(&first), &hashes(&second)), None);
}

#[test]
fn t023_c2_c3_timeouts_and_intervals_follow_frame_time() {
    let script = "<script>\n\
        setTimeout(function () { document.getElementById(\"red\").style.width = \"50px\"; }, 1000);\n\
        var ticks = 0;\n\
        setInterval(function () { ticks += 1; document.getElementById(\"green\").style.width = (20 * ticks) + \"px\"; }, 500);\n\
        var gone = setTimeout(function () { document.getElementById(\"blue\").style.width = \"50px\"; }, 300);\n\
        clearTimeout(gone);\n\
        </script>";
    let dir = project(
        "t023_c2",
        "clocks",
        &page(
            "data-fs-size=\"400x60\" data-fs-fps=\"10\" data-fs-duration=\"2.2\"",
            BARS,
            THREE,
            script,
        ),
    );
    let (ran, frames) = render_frames(&dir, &[], &[]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert_eq!(frames.len(), 22);
    for (number, frame) in frames.iter().enumerate() {
        // c2: the timeout of 1 second first shows on the frame at 1 second.
        assert_eq!(width(frame, RED), if number >= 10 { 50 } else { 0 }, "frame {number}");
        // c3: the interval of half a second has run once by 0.5 s, twice by 1.0 s, and so on.
        assert_eq!(width(frame, GREEN), 20 * (number as u32 / 5), "frame {number}");
        // A cleared timeout never runs.
        assert_eq!(width(frame, BLUE), 0, "frame {number}");
    }
}

#[test]
fn t023_c4_the_first_script_in_the_page_reads_composition_time_zero() {
    // The script is the first thing in the page, before any other element.
    let index = "<!doctype html>\n<script>\n\
        var atLoad = [Date.now(), performance.now(), new Date().getTime(), new Date().getUTCFullYear()].join();\n\
        </script>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<title>First</title>\n<style>\n\
        .bar { position: absolute; left: 0; top: 5px; width: 0; height: 10px; background: #ff0000; }\n</style>\n</head>\n<body>\n\
        <div data-fs-composition=\"main\" data-fs-size=\"400x60\" data-fs-fps=\"10\" data-fs-duration=\"0.3\">\n\
        <div id=\"red\" class=\"bar\"></div>\n</div>\n<script>\n\
        document.getElementById(\"red\").style.width = (atLoad === \"0,0,0,1970\" ? 30 : 300) + \"px\";\n\
        </script>\n</body>\n</html>\n";
    let dir = project("t023_c4", "clocks", index);
    let (ran, frames) = render_frames(&dir, &[], &[]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert!(
        frames.iter().all(|frame| width(frame, RED) == 30),
        "the first script did not read time zero"
    );
}

fn signal(name: &str, pids: &[String]) {
    for pid in pids {
        measure::run(Path::new("/bin/kill"), &[name, pid], Path::new("/"), &[]);
    }
}

#[test]
fn t023_c5_a_pause_of_several_seconds_between_two_frames_moves_the_clocks_by_one_frame() {
    let dir = clock_page("t023_c5", 6);
    let frames_dir = dir.parent().unwrap().join("frames");
    let mut child = Command::new(BIN)
        .args(["render", text(&dir)])
        .env("FRAMESMITH_DEBUG_FRAMES", &frames_dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let captured = || std::fs::read_dir(&frames_dir).map(Iterator::count).unwrap_or(0);
    let began = Instant::now();
    while captured() < 5 {
        assert!(began.elapsed() < Duration::from_secs(60), "no frame was captured");
        std::thread::sleep(Duration::from_millis(2));
    }
    // Hold the command, its browser and its encoder still for three seconds in the middle of the capture.
    let own = child.id().to_string();
    let children = measure::run(Path::new("/usr/bin/pgrep"), &["-P", &own], Path::new("/"), &[]);
    let mut pids: Vec<String> = children.stdout.split_whitespace().map(str::to_string).collect();
    pids.push(own);
    assert!(pids.len() >= 2, "the browser is a child of the command");
    signal("-STOP", &pids);
    std::thread::sleep(Duration::from_millis(200));
    let at_pause = captured();
    std::thread::sleep(Duration::from_secs(3));
    assert_eq!(captured(), at_pause, "nothing was captured during the pause");
    assert!((5..60).contains(&at_pause), "the pause fell between two frames (after {at_pause})");
    signal("-CONT", &pids);
    assert!(child.wait().unwrap().success());
    let frames = measure::frames_from_pngs(&ffmpeg(), &frames_dir).unwrap();
    assert_eq!(frames.len(), 60);
    assert_frame_time(&frames);
}

#[test]
fn t023_c6_when_the_clocks_cannot_be_replaced_the_render_stops() {
    let said = "error FS-R009\n  The page's clocks could not be replaced, so its frames would not be repeatable.\n";
    // The replacement is switched off from outside.
    let dir = clock_page("t023_c6_fault", 1);
    let ran = run_in(
        dir.parent().unwrap(),
        &["render", text(&dir)],
        &[("FRAMESMITH_DEBUG_FAULT", OsStr::new("clocks"))],
    );
    assert_eq!(ran.code, 1, "{}", ran.all());
    assert!(ran.stderr.contains(said), "{}", ran.stderr);
    assert!(!dir.join("out").exists());
    // A page script takes a clock back.
    let script = "<script>\nwindow.Date = function () { return { getTime: function () { return 42; } }; };\n</script>";
    let dir = project(
        "t023_c6_script",
        "clocks",
        &page("data-fs-size=\"64x36\" data-fs-duration=\"0.2\"", "", "<p>x</p>", script),
    );
    let ran = framesmith(&["render", text(&dir)]);
    assert_eq!(ran.code, 1, "{}", ran.all());
    assert!(ran.stderr.contains(said), "{}", ran.stderr);
    assert!(!dir.join("out").exists());
}
