//! T015: the render stops when the page does not become ready, and only
//! the wait for readiness has a time limit.

mod common;

use common::*;
use std::path::{Path, PathBuf};
use std::time::Instant;

const SMALL: &str = "data-fs-size=\"64x36\" data-fs-duration=\"0.2\"";

/// Renders a project that already has a video at its output path, and
/// checks what every failure must leave behind: exit 1, the old video
/// untouched, nothing new beside it, and no browser.
fn stopped(dir: &Path) -> (String, serde_json::Value) {
    let name = dir.file_name().unwrap().to_string_lossy().into_owned();
    write(dir, &format!("out/{name}.mp4"), "an earlier video");
    let ran = framesmith(&["render", text(dir), "--json"]);
    assert_eq!(ran.code, 1, "{}", ran.all());
    let document = document(&ran);
    assert_eq!(document["outcome"], "failed");
    // c5: the video already there is unchanged, and no staging file is left.
    assert_eq!(
        std::fs::read_to_string(dir.join(format!("out/{name}.mp4"))).unwrap(),
        "an earlier video"
    );
    assert_eq!(std::fs::read_dir(dir.join("out")).unwrap().count(), 1);
    // c5: no browser started by the run is left.
    assert!(!browser_left(document["run_id"].as_str().unwrap()));
    (ran.stderr, document)
}

#[test]
fn t015_c1_c5_a_corrupt_image_stops_the_render_and_is_named() {
    let dir = project(
        "t015_c1",
        "card",
        &page(SMALL, "", "<img src=\"pictures/broken.png\" alt=\"\">", ""),
    );
    write(&dir, "pictures/broken.png", "this is not a picture");
    let (words, document) = stopped(&dir);
    assert!(
        words.contains("error FS-R006\n  The image pictures/broken.png did not load in the page.\n"),
        "{words}"
    );
    assert_eq!(document["error"]["code"], "FS-R006");
    assert_eq!(document["error"]["stage"], "probe");
}

#[test]
fn t015_c1_a_corrupt_image_writes_no_output_where_there_was_none() {
    let dir = project("t015_c1_fresh", "card", &page(SMALL, "", "<img src=\"broken.png\" alt=\"\">", ""));
    write(&dir, "broken.png", "this is not a picture");
    let ran = framesmith(&["render", text(&dir)]);
    assert_eq!(ran.code, 1, "{}", ran.all());
    assert!(
        ran.stderr.contains("The image broken.png did not load in the page."),
        "{}",
        ran.stderr
    );
    assert!(
        ran.stdout
            .contains("render stopped in probe: The image broken.png did not load in the page.\n"),
        "{}",
        ran.stdout
    );
    assert!(!dir.join("out/card.mp4").exists());
    assert_eq!(std::fs::read_dir(dir.join("out")).map(Iterator::count).unwrap_or(0), 0);
}

#[test]
fn t015_c2_c5_a_font_file_that_fails_to_load_stops_the_render_and_is_named() {
    let style = "@font-face { font-family: \"Ours\"; src: url(\"fonts/broken.ttf\"); }\np { font-family: \"Ours\", sans-serif; }";
    let dir = project("t015_c2", "card", &page(SMALL, style, "<p>Hello</p>", ""));
    write(&dir, "fonts/broken.ttf", "this is not a font");
    let (words, document) = stopped(&dir);
    assert!(
        words.contains("error FS-R006\n  The font fonts/broken.ttf did not load in the page.\n"),
        "{words}"
    );
    assert_eq!(document["error"]["code"], "FS-R006");
}

#[test]
fn t015_c3_c5_a_script_error_while_the_page_loads_stops_the_render_and_is_shown() {
    let script = "<script>\nthrow new Error(\"the counter is not set up\");\n</script>";
    let dir = project("t015_c3", "card", &page(SMALL, "", "<p>Hello</p>", script));
    let (words, document) = stopped(&dir);
    assert!(
        words.contains("error FS-R007\n  A page script failed: Uncaught Error: the counter is not set up.\n"),
        "{words}"
    );
    assert_eq!(document["error"]["code"], "FS-R007");
}

#[test]
fn t015_c4_c5_a_page_that_never_reports_ready_stops_after_30_seconds() {
    let dir = project(
        "t015_c4",
        "card",
        &page(SMALL, "", "<p>Hello</p>", "<script>\nfor (;;) {}\n</script>"),
    );
    let began = Instant::now();
    let (words, document) = stopped(&dir);
    let took = began.elapsed().as_secs_f64();
    assert!(
        words.contains("error FS-R008\n  The page did not become ready within 30 seconds.\n"),
        "{words}"
    );
    assert_eq!(document["error"]["code"], "FS-R008");
    assert!((30.0..36.0).contains(&took), "the run took {took} s");
}

/// A full-size picture that takes the browser a while to paint.
fn slow(test: &str, frames: u32) -> PathBuf {
    let style = "[data-fs-composition] { background: linear-gradient(120deg, #123456, #fedcba 40%, #0f8f6f); }";
    let attrs = format!(
        "data-fs-size=\"1280x720\" data-fs-fps=\"10\" data-fs-duration=\"{}\"",
        f64::from(frames) / 10.0
    );
    project(test, "long", &page(&attrs, style, "<p>Hello</p>", ""))
}

fn timed(dir: &Path) -> (f64, measure::Ran) {
    let began = Instant::now();
    let ran = framesmith(&["render", text(dir)]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    (began.elapsed().as_secs_f64(), ran)
}

#[test]
fn t015_c6_a_capture_longer_than_the_readiness_limit_is_not_stopped() {
    // How long a frame takes here, from two short runs.
    let (ten, _) = timed(&slow("t015_c6_ten", 10));
    let (forty, _) = timed(&slow("t015_c6_forty", 40));
    let a_frame = ((forty - ten) / 30.0).max(0.01);
    // Enough frames for 50 seconds of capture: well past the 30 seconds allowed for readiness,
    // with room for the machine being quicker now than in the two short runs.
    let frames = (50.0 / a_frame).ceil() as u32;
    let dir = slow("t015_c6", frames);
    let (took, ran) = timed(&dir);
    let facts = facts(&dir.join("out/long.mp4"));
    assert_eq!(facts.frames, u64::from(frames));
    // Away from a terminal a capture line is printed about once a second, so
    // more than 32 of them means the capture itself ran past the limit.
    let lines = ran.stdout.lines().filter(|line| line.starts_with("capture ")).count();
    assert!(took > 32.0 && lines > 32, "the run took {took} s and printed {lines} capture lines");
    assert!(ran.stdout.contains(&format!("capture   {frames}/{frames}\n")), "{}", ran.stdout);
}
