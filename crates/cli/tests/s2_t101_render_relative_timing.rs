//! Sprint 2, ticket T101: the Framesmith half of the relative-timing pair. The scene is one chain of four
//! clips (two of them with an offset, one earlier and one later than the end of the clip they follow) and
//! one box that moves steadily; the second half of the pair is written for the other program of the
//! comparison and lives outside this repository (see `crates/parity/tests/s2_t101_pair_relative_timing.rs`).
//!
//! Notes for maintainers
//! ---------------------
//! * The expected frames are written out here from the way the fixture `pair-relative-timing` is written
//!   (30 frames a second; `a` at 1 s for 2 s; `b` after `a` plus 0.5 s for 2 s; `c` after `b` minus 0.5 s
//!   for 2 s; `d` after `c` for 1 s; the white box moves 2 px a frame from frame 15 to frame 255), not taken
//!   from the code that works the starts out.
//! * Only the two render tests need a browser and `ffmpeg`; the other reads files and runs `check`.
//! * Each test names its scratch folders with a key of its own (the tests of a file run in parallel).

mod common;

use common::*;
use measure::{Frame, Rgb};
use std::path::Path;

const RED: Rgb = [0xff, 0x5a, 0x36];
const AMBER: Rgb = [0xff, 0xb0, 0x00];
const BLUE: Rgb = [0x34, 0x98, 0xdb];
const GREEN: Rgb = [0x2e, 0xcc, 0x71];
const WHITE: Rgb = [0xff, 0xff, 0xff];

/// How far a channel may be from the colour written in the fixture and still be that colour.
const NEAR: u8 = 24;
/// One tile is 120 by 120 pixels.
const TILE: usize = 14_400;

/// One clip of the fixture as the tests expect to see it.
struct Expected {
    id: &'static str,
    colour: Rgb,
    /// The frames it shows on: the first, and one past the last.
    window: (usize, usize),
    /// The place its box fills: left, top, right, bottom, as pixels counted from 0.
    place: (u32, u32, u32, u32),
}

const CLIPS: [Expected; 4] = [
    Expected {
        id: "a",
        colour: RED,
        window: (30, 90),
        place: (40, 40, 159, 159),
    },
    Expected {
        id: "b",
        colour: AMBER,
        window: (105, 165),
        place: (190, 40, 309, 159),
    },
    Expected {
        id: "c",
        colour: BLUE,
        window: (150, 210),
        place: (340, 40, 459, 159),
    },
    Expected {
        id: "d",
        colour: GREEN,
        window: (210, 240),
        place: (490, 40, 609, 159),
    },
];

fn fixture_text() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/pair-relative-timing/index.html");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The value of attribute `name` on one line of the fixture.
fn attribute(line: &str, name: &str) -> Option<String> {
    let at = line.find(&format!(" {name}=\""))? + name.len() + 3;
    let rest = &line[at..];
    Some(rest[..rest.find('"')?].to_string())
}

/// One clip as the fixture writes it: id, written start, written duration.
fn written_clips() -> Vec<(String, String, String)> {
    fixture_text()
        .lines()
        .filter(|line| line.contains("data-fs-clip"))
        .map(|line| {
            (
                attribute(line, "id").expect("a clip has an id"),
                attribute(line, "data-fs-start").expect("a clip has a start"),
                attribute(line, "data-fs-duration").expect("a clip has a duration"),
            )
        })
        .collect()
}

/// A relative start written `after:<id>`, `after:<id>+<n>` or `after:<id>-<n>`: the id and the signed offset.
fn relative(start: &str) -> Option<(String, f64)> {
    let rest = start.strip_prefix("after:")?;
    match rest.find(['+', '-']) {
        Some(at) => {
            let offset: f64 = rest[at + 1..].parse().expect("the offset is a number");
            Some((rest[..at].to_string(), if rest[at..].starts_with('-') { -offset } else { offset }))
        }
        None => Some((rest.to_string(), 0.0)),
    }
}

/// How many clips the chain that ends at `id` holds, counting `id` and every clip it follows.
fn chain_length(id: &str, clips: &[(String, String, String)]) -> usize {
    let (_, start, _) = clips.iter().find(|(name, _, _)| name == id).expect("the clip exists");
    match relative(start) {
        Some((base, _)) => 1 + chain_length(&base, clips),
        None => 1,
    }
}

fn shows(frame: &Frame, colour: Rgb) -> bool {
    frame.count(colour, NEAR) >= TILE * 97 / 100
}

fn wording(frames: &[Frame], number: usize) -> String {
    CLIPS
        .iter()
        .filter(|clip| shows(&frames[number], clip.colour))
        .map(|clip| clip.id)
        .collect::<Vec<_>>()
        .join(",")
}

#[test]
fn s2_t101_c1_the_pair_project_has_a_chain_with_a_positive_and_a_negative_offset_and_passes_check() {
    let clips = written_clips();
    assert!(clips.len() >= 3, "at least three clips: {clips:?}");

    // A chain of at least three clips: some clip ends a line of relative starts that holds three clips.
    let longest = clips.iter().map(|(id, _, _)| chain_length(id, &clips)).max().unwrap();
    assert!(longest >= 3, "the longest chain holds {longest} clips: {clips:?}");

    // One relative start with a positive offset and one with a negative offset, both written out.
    let offsets: Vec<f64> = clips
        .iter()
        .filter_map(|(_, start, _)| relative(start))
        .map(|(_, offset)| offset)
        .collect();
    assert!(offsets.iter().any(|offset| *offset > 0.0), "a positive offset: {offsets:?}");
    assert!(offsets.iter().any(|offset| *offset < 0.0), "a negative offset: {offsets:?}");

    // The project passes `check`, and `check` works the starts out as the fixture's text says.
    let project = fixture("s2_t101_c1", "pair-relative-timing");
    let ran = framesmith(&["check", text(&project)]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert_eq!(ran.stdout.lines().next(), Some("check passed"), "{}", ran.stdout);
    for line in [
        "  composition  pair-relative-timing",
        "  size         640x360",
        "  rate         30 fps",
        "  length       9 s",
        "  frames       270",
        "  clips        4",
        "  clip #b  start 3.5 s (after:a+0.5)",
        "  clip #c  start 5 s (after:b-0.5)",
        "  clip #d  start 7 s (after:c)",
    ] {
        assert!(
            ran.stdout.lines().any(|printed| printed == line),
            "no line {line:?} in:\n{}",
            ran.stdout
        );
    }
    assert!(!ran.stdout.contains("warning"), "{}", ran.stdout);
}

#[test]
fn s2_t101_c2_each_clip_is_told_apart_in_a_frame_and_one_box_moves_so_events_and_position_can_be_measured() {
    let project = fixture("s2_t101_c2", "pair-relative-timing");
    let (ran, frames) = render_frames(&project, &[], &[]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert_eq!(frames.len(), 270);

    // Events: every clip shows on exactly the frames of its window, and on no other frame.
    let counts: Vec<[usize; 4]> = frames
        .iter()
        .map(|frame| CLIPS.each_ref().map(|clip| frame.count(clip.colour, NEAR)))
        .collect();
    for (
        row,
        Expected {
            id, window: (first, end), ..
        },
    ) in CLIPS.iter().enumerate()
    {
        for (n, count) in counts.iter().enumerate() {
            if (*first..*end).contains(&n) {
                assert!(count[row] >= TILE * 97 / 100, "clip #{id}, frame {n}: {} pixels", count[row]);
            } else {
                assert_eq!(count[row], 0, "clip #{id}, frame {n}");
            }
        }
    }

    // Told apart: in one frame the clips are different colours in places of their own, and the frames
    // where two clips overlap in time (150 to 164) show both, each whole, in its own place.
    for Expected {
        id,
        colour,
        window: (first, _),
        place: (left, top, right, bottom),
    } in CLIPS
    {
        let bounds = frames[first].bounds(colour, NEAR).expect("the clip is in its first frame");
        assert_eq!(
            (bounds.left, bounds.top, bounds.right, bounds.bottom),
            (left, top, right, bottom),
            "clip #{id}"
        );
    }
    assert_eq!(wording(&frames, 29), "");
    assert_eq!(wording(&frames, 30), "a");
    assert_eq!(wording(&frames, 89), "a");
    assert_eq!(wording(&frames, 90), "", "a gap follows a: b starts 0.5 s after it ends");
    assert_eq!(wording(&frames, 104), "");
    assert_eq!(wording(&frames, 105), "b");
    assert_eq!(wording(&frames, 149), "b");
    assert_eq!(wording(&frames, 150), "b,c", "c starts 0.5 s before b ends");
    assert_eq!(wording(&frames, 164), "b,c");
    assert_eq!(wording(&frames, 165), "c");
    assert_eq!(wording(&frames, 209), "c");
    assert_eq!(wording(&frames, 210), "d", "d starts as c ends");
    assert_eq!(wording(&frames, 239), "d");
    assert_eq!(wording(&frames, 240), "");

    // Position: the white box waits at 40 px until frame 15, moves 2 px a frame to frame 255, and rests.
    let strip = |number: usize| frames[number].crop(0, 200, 640, 160);
    let mut lefts = Vec::new();
    for number in 0..frames.len() {
        let bounds = strip(number).bounds(WHITE, NEAR).expect("the white box is in every frame");
        assert_eq!(
            (bounds.right - bounds.left + 1, bounds.bottom - bounds.top + 1),
            (60, 60),
            "frame {number}"
        );
        assert_eq!(bounds.top + 200, 240, "frame {number}");
        lefts.push(bounds.left);
    }
    for (number, left) in lefts.iter().enumerate() {
        let expected = 40 + 2 * (number.clamp(15, 255) - 15) as u32;
        assert_eq!(*left, expected, "frame {number}");
    }
    assert!(
        lefts[15] < lefts[16] && lefts[254] < lefts[255],
        "the box moves while it is tweened"
    );
    assert_eq!((lefts[0], lefts[255], lefts[269]), (40, 520, 520));
}

#[test]
fn s2_t101_c3_our_render_exists_at_30_frames_a_second_made_by_the_render_command() {
    let project = fixture("s2_t101_c3", "pair-relative-timing");
    let video = project.parent().unwrap().join("pair-relative-timing.ours.mp4");
    let ran = framesmith(&["render", text(&project), "--output", text(&video)]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert!(video.is_file(), "no video at {}", video.display());

    let facts = facts(&video);
    assert_eq!(facts.frame_rate, "30/1", "{facts:?}");
    assert_eq!(facts.frames, 270, "{facts:?}");
    assert_eq!((facts.width, facts.height), (640, 360), "{facts:?}");

    // The frame rate of our half is the one the fixture writes, and the other half is asked for the same
    // rate by the comparison tool (`s2_t101_c3_the_other_half_is_asked_for_the_rate_our_half_is_written_with`).
    assert!(fixture_text().contains("data-fs-fps=\"30\""));
    assert!(ran.stdout.contains("30 fps"), "{}", ran.stdout);
}
