//! T011, T021, T022: a clip shows only inside its window, worked-out
//! durations, and the worked-out length of the video.

mod common;

use common::*;
use measure::{Frame, Rgb};

const RED: Rgb = [255, 0, 0];
const GREEN: Rgb = [0, 255, 0];
const BLUE: Rgb = [0, 0, 255];
const STYLE: &str = ".box { position: absolute; width: 20px; height: 20px; }\n\
#red { left: 10px; top: 10px; background: #ff0000; }\n\
#green { left: 40px; top: 10px; background: #00ff00; }\n\
#blue { left: 70px; top: 10px; background: #0000ff; }";

/// The frames on which a box of the colour is painted, whole.
fn shown(frames: &[Frame], colour: Rgb) -> Vec<usize> {
    let mut found = Vec::new();
    for (number, frame) in frames.iter().enumerate() {
        match frame.count(colour, 0) {
            0 => {}
            400 => found.push(number),
            part => panic!("frame {number} shows {part} pixels of the box, neither none nor all"),
        }
    }
    found
}

fn frames_of(test: &str, attrs: &str, body: &str, rest: &str) -> Vec<Frame> {
    let dir = project(test, "clips", &page(&format!("data-fs-size=\"100x40\" {attrs}"), STYLE, body, rest));
    let (ran, frames) = render_frames(&dir, &[], &[]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    frames
}

fn range(from: usize, to: usize) -> Vec<usize> {
    (from..to).collect()
}

#[test]
fn t011_c1_c6_a_clip_from_1_for_2_seconds_shows_on_frames_30_to_89() {
    let frames = frames_of(
        "t011_c1",
        "data-fs-fps=\"30\" data-fs-duration=\"3.2\"",
        "<div id=\"red\" class=\"box\" data-fs-clip data-fs-start=\"1\" data-fs-duration=\"2\"></div>",
        "",
    );
    assert_eq!(frames.len(), 96);
    assert_eq!(shown(&frames, RED), range(30, 90));
    // c6: the very first captured frame is all background.
    assert!(frames[0].is_all([0, 0, 0], 0));
    assert!(frames[29].is_all([0, 0, 0], 0) && frames[90].is_all([0, 0, 0], 0));
}

#[test]
fn t011_c2_a_window_between_frame_times_shows_on_the_frames_whose_time_is_inside() {
    // At 10 frames per second the window 0.25 s to 0.55 s holds the times 0.3, 0.4 and 0.5.
    let frames = frames_of(
        "t011_c2",
        "data-fs-fps=\"10\" data-fs-duration=\"1\"",
        "<div id=\"red\" class=\"box\" data-fs-clip data-fs-start=\"0.25\" data-fs-duration=\"0.3\"></div>\n\
         <div id=\"green\" class=\"box\" data-fs-clip data-fs-start=\"0.2\" data-fs-duration=\"0.31\"></div>",
        "",
    );
    assert_eq!(shown(&frames, RED), vec![3, 4, 5]);
    // 0.2 s to 0.51 s holds the times 0.2, 0.3, 0.4 and 0.5.
    assert_eq!(shown(&frames, GREEN), vec![2, 3, 4, 5]);
}

#[test]
fn t011_c3_a_start_of_seven_sixtieths_first_shows_on_frame_7() {
    let frames = frames_of(
        "t011_c3",
        "data-fs-fps=\"60\" data-fs-duration=\"0.2\"",
        "<div id=\"red\" class=\"box\" data-fs-clip data-fs-start=\"0.116667\" data-fs-duration=\"0.05\"></div>",
        "",
    );
    assert_eq!(frames.len(), 12);
    // 0.116667 s + 0.05 s is 10 sixtieths, so the end frame is frame 10.
    assert_eq!(shown(&frames, RED), vec![7, 8, 9]);
}

#[test]
fn t011_c4_an_inner_clip_shows_only_while_both_windows_hold_the_time() {
    let frames = frames_of(
        "t011_c4",
        "data-fs-fps=\"10\" data-fs-duration=\"1.2\"",
        "<div data-fs-clip data-fs-start=\"0.2\" data-fs-duration=\"0.5\">\n\
           <div id=\"red\" class=\"box\"></div>\n\
           <div id=\"green\" class=\"box\" data-fs-clip data-fs-start=\"0.4\" data-fs-duration=\"0.6\"></div>\n\
           <div id=\"blue\" class=\"box\" data-fs-clip data-fs-start=\"0\" data-fs-duration=\"0.4\"></div>\n\
         </div>",
        "",
    );
    // The outer window is frames 2 to 6.
    assert_eq!(shown(&frames, RED), vec![2, 3, 4, 5, 6]);
    // The inner window is frames 4 to 9; both hold frames 4, 5 and 6.
    assert_eq!(shown(&frames, GREEN), vec![4, 5, 6]);
    // An inner window that begins before the outer one: frames 0 to 3 and 2 to 6 share 2 and 3.
    assert_eq!(shown(&frames, BLUE), vec![2, 3]);
}

#[test]
fn t011_c5_a_window_past_the_end_shows_to_the_last_frame_and_adds_none() {
    let frames = frames_of(
        "t011_c5",
        "data-fs-fps=\"10\" data-fs-duration=\"1\"",
        "<div id=\"red\" class=\"box\" data-fs-clip data-fs-start=\"0.5\" data-fs-duration=\"5\"></div>",
        "",
    );
    assert_eq!(frames.len(), 10);
    assert_eq!(shown(&frames, RED), vec![5, 6, 7, 8, 9]);
}

#[test]
fn t011_c7_a_track_number_does_not_change_the_picture() {
    let body = |track: &str| {
        format!(
            "<div id=\"red\" class=\"box\" data-fs-clip data-fs-start=\"0.2\" data-fs-duration=\"0.5\"{track}></div>\n\
             <div id=\"green\" class=\"box\" data-fs-clip data-fs-start=\"0\" data-fs-duration=\"0.4\"{track}></div>"
        )
    };
    let attrs = "data-fs-fps=\"10\" data-fs-duration=\"1\"";
    let without = frames_of("t011_c7_without", attrs, &body(""), "");
    let with = frames_of("t011_c7_with", attrs, &body(" data-fs-track=\"3\""), "");
    let hashes = |frames: &[Frame]| frames.iter().map(Frame::hash).collect::<Vec<u64>>();
    assert_eq!(measure::first_difference(&hashes(&without), &hashes(&with)), None);
    assert_eq!(shown(&with, RED), vec![2, 3, 4, 5, 6]);
}

#[test]
fn t011_a_hidden_clip_keeps_its_place_in_the_layout() {
    // The green box follows the red one in the flow; it stays put while the red one is hidden.
    let style = "<div style=\"display: flex\">\n\
         <div data-fs-clip data-fs-start=\"0.5\" data-fs-duration=\"0.5\" style=\"width: 30px; height: 20px; background: #ff0000\"></div>\n\
         <div style=\"width: 20px; height: 20px; background: #00ff00\"></div>\n\
         </div>";
    let frames = frames_of("t011_layout", "data-fs-fps=\"10\" data-fs-duration=\"1\"", style, "");
    let before = frames[0].bounds(GREEN, 0).unwrap();
    let during = frames[5].bounds(GREEN, 0).unwrap();
    assert_eq!(before, during);
    assert_eq!(before.left, 30);
    assert_eq!(frames[0].count(RED, 0), 0);
    assert_eq!(frames[5].count(RED, 0), 600);
}

const DOT: &str =
    "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"20\" height=\"20\"><rect width=\"20\" height=\"20\" fill=\"#ff0000\"/></svg>\n";

#[test]
fn t021_c1_c2_worked_out_durations_decide_the_frames_a_clip_shows_on() {
    // The same page with a written length of 6 s, and with the length worked out (the motion ends at 5 s).
    for (test, length, frame_count) in [("t021_c1", " data-fs-duration=\"6\"", 60), ("t021_c2", "", 50)] {
        let index = page(
            &format!("data-fs-size=\"100x40\" data-fs-fps=\"10\"{length}"),
            STYLE,
            "<img id=\"red\" class=\"box\" data-fs-clip data-fs-start=\"1\" src=\"dot.svg\" alt=\"\">\n\
             <p id=\"green\" class=\"box\" data-fs-clip data-fs-start=\"1\" style=\"margin: 0\"></p>\n\
             <div id=\"blue\" class=\"box\"></div>",
            &motion("{ \"target\": \"#blue\", \"to\": { \"opacity\": 1 }, \"at\": 4, \"duration\": 1 }"),
        );
        let dir = project(test, "clips", &index);
        write(&dir, "dot.svg", DOT);
        let (ran, frames) = render_frames(&dir, &[], &[]);
        assert_eq!(ran.code, 0, "{}", ran.all());
        assert_eq!(frames.len(), frame_count);
        // c1: the image clip lasts 3 seconds: from 1 s up to, not including, 4 s.
        assert_eq!(shown(&frames, RED), range(10, 40));
        // c2: the other clip lasts from 1 s through the last frame of the composition.
        assert_eq!(shown(&frames, GREEN), range(10, frame_count));
    }
}

#[test]
fn t022_c1_a_worked_out_length_of_5_seconds_renders_150_frames() {
    let index = page(
        "data-fs-size=\"64x36\" data-fs-fps=\"30\"",
        STYLE,
        "<div id=\"red\" class=\"box\" data-fs-clip data-fs-start=\"0\" data-fs-duration=\"4\"></div>",
        &motion("{ \"target\": \"#red\", \"to\": { \"x\": 10 }, \"at\": 0, \"duration\": 5 }"),
    );
    let dir = project("t022_c1_render", "clips", &index);
    let checked = framesmith(&["check", text(&dir)]);
    assert!(
        checked.stdout.contains("  length       5 s (filled in)\n  frames       150\n"),
        "{}",
        checked.stdout
    );
    let ran = framesmith(&["render", text(&dir)]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    let facts = facts(&dir.join("out/clips.mp4"));
    assert_eq!(facts.frames, 150);
    assert!((facts.duration - 5.0).abs() < 0.001);
}

#[test]
fn t022_c2_a_clip_that_ends_after_the_motion_does_not_lengthen_the_video() {
    // The clip is written to end at 6.5 s; the motion ends at 5 s.
    let index = page(
        "data-fs-size=\"100x40\" data-fs-fps=\"10\"",
        STYLE,
        "<div id=\"red\" class=\"box\" data-fs-clip data-fs-start=\"1\" data-fs-duration=\"5.5\"></div>\n\
         <div id=\"blue\" class=\"box\"></div>",
        &motion("{ \"target\": \"#blue\", \"to\": { \"opacity\": 1 }, \"at\": 0, \"duration\": 5 }"),
    );
    let dir = project("t022_c2_render", "clips", &index);
    let (ran, frames) = render_frames(&dir, &[], &[]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert_eq!(frames.len(), 50);
    assert_eq!(shown(&frames, RED), range(10, 50));
}
