//! T016, T017, T018, T019: the four tween kinds, the five properties and
//! the ten easings, measured on the painted frames.

mod common;

use common::*;
use measure::{Frame, Rgb};
use std::ffi::OsStr;

const BLACK: Rgb = [0, 0, 0];
const RED: Rgb = [255, 0, 0];
const GREEN: Rgb = [0, 255, 0];
const BLUE: Rgb = [0, 0, 255];
/// A red box, 20 by 20, laid out at 20, 20.
const BOX: &str = "#a { position: absolute; left: 20px; top: 20px; width: 20px; height: 20px; background: #ff0000; }";

struct Scene<'a> {
    test: &'a str,
    size: &'a str,
    fps: u32,
    length: &'a str,
    style: &'a str,
    body: &'a str,
    tweens: &'a str,
}

impl Scene<'_> {
    fn render(&self, folder: &str, env: &[(&str, &OsStr)]) -> Vec<Frame> {
        let attrs = format!(
            "data-fs-size=\"{}\" data-fs-fps=\"{}\" data-fs-duration=\"{}\"",
            self.size, self.fps, self.length
        );
        let dir = project(
            &format!("{}_{folder}", self.test),
            "motion",
            &page(&attrs, self.style, self.body, &motion(self.tweens)),
        );
        let (ran, frames) = render_frames(&dir, &[], env);
        assert_eq!(ran.code, 0, "{}", ran.all());
        frames
    }

    fn frames(&self) -> Vec<Frame> {
        self.render("forward", &[])
    }

    /// The frames from forward capture, after showing that capture from
    /// the last frame to the first paints every frame the same.
    fn frames_in_both_orders(&self) -> Vec<Frame> {
        let forward = self.render("forward", &[]);
        let reverse = self.render("reverse", &[("FRAMESMITH_DEBUG_ORDER", OsStr::new("reverse"))]);
        assert_eq!(
            measure::first_difference(&hashes(&forward), &hashes(&reverse)),
            None,
            "{}",
            self.test
        );
        forward
    }
}

fn hashes(frames: &[Frame]) -> Vec<u64> {
    frames.iter().map(Frame::hash).collect()
}

/// The left and top edge of the box of a colour, which must be whole.
fn corner(frame: &Frame, colour: Rgb, width: u32, height: u32) -> (u32, u32) {
    let found = frame.bounds(colour, 0).expect("the box is painted");
    assert_eq!((found.width(), found.height()), (width, height), "{found:?}");
    (found.left, found.top)
}

fn lefts(frames: &[Frame], colour: Rgb) -> Vec<u32> {
    frames.iter().map(|frame| corner(frame, colour, 20, 20).0).collect()
}

fn tops(frames: &[Frame], colour: Rgb) -> Vec<u32> {
    frames.iter().map(|frame| corner(frame, colour, 20, 20).1).collect()
}

#[test]
fn t016_c1_a_to_tween_moves_300_pixels_between_1_and_2_seconds() {
    let frames = Scene {
        test: "t016_c1",
        size: "400x60",
        fps: 10,
        length: "3",
        style: BOX,
        body: "<div id=\"a\"></div>",
        tweens: "{ \"target\": \"#a\", \"to\": { \"x\": 300 }, \"at\": 1, \"duration\": 1, \"ease\": \"linear\" }",
    }
    .frames();
    let lefts = lefts(&frames, RED);
    assert_eq!(lefts.len(), 30);
    // Not moved up to and at 1 second.
    assert!(lefts[..=10].iter().all(|&left| left == 20), "{lefts:?}");
    // 150 pixels to the right at 1.5 seconds, and 30 pixels more on each frame between.
    assert_eq!(lefts[15], 170);
    assert_eq!(lefts[10..=20], [20, 50, 80, 110, 140, 170, 200, 230, 260, 290, 320]);
    // 300 pixels to the right from 2 seconds to the end.
    assert!(lefts[20..].iter().all(|&left| left == 320), "{lefts:?}");
    assert!(tops(&frames, RED).iter().all(|&top| top == 20));
}

#[test]
fn t016_c2_a_to_tween_moves_on_both_axes_at_once() {
    let frames = Scene {
        test: "t016_c2",
        size: "200x80",
        fps: 10,
        length: "1.5",
        style: BOX,
        body: "<div id=\"a\"></div>",
        tweens: "{ \"target\": \"#a\", \"to\": { \"x\": 100, \"y\": 30 }, \"at\": 0, \"duration\": 1 }",
    }
    .frames();
    assert_eq!(corner(&frames[0], RED, 20, 20), (20, 20));
    assert_eq!(corner(&frames[5], RED, 20, 20), (70, 35));
    assert_eq!(corner(&frames[10], RED, 20, 20), (120, 50));
    assert_eq!(corner(&frames[14], RED, 20, 20), (120, 50));
}

#[test]
fn t016_c3_a_target_that_matches_three_elements_moves_all_three() {
    let frames = Scene {
        test: "t016_c3",
        size: "200x100",
        fps: 10,
        length: "1",
        style: ".m { position: absolute; left: 20px; width: 20px; height: 20px; }\n\
                #r { top: 5px; background: #ff0000; } #g { top: 40px; background: #00ff00; } #b { top: 75px; background: #0000ff; }",
        body: "<div id=\"r\" class=\"m\"></div><div id=\"g\" class=\"m\"></div><div id=\"b\" class=\"m\"></div>",
        tweens: "{ \"target\": \".m\", \"to\": { \"x\": 100 }, \"at\": 0, \"duration\": 1 }",
    }
    .frames();
    for (colour, top) in [(RED, 5), (GREEN, 40), (BLUE, 75)] {
        assert_eq!(corner(&frames[0], colour, 20, 20), (20, top));
        assert_eq!(corner(&frames[5], colour, 20, 20), (70, top));
        assert_eq!(corner(&frames[9], colour, 20, 20), (110, top));
    }
}

#[test]
fn t016_c4_a_tween_that_ends_after_the_composition_adds_no_frames() {
    let frames = Scene {
        test: "t016_c4",
        size: "400x60",
        fps: 10,
        length: "1",
        style: BOX,
        body: "<div id=\"a\"></div>",
        tweens: "{ \"target\": \"#a\", \"to\": { \"x\": 300 }, \"at\": 0, \"duration\": 2 }",
    }
    .frames();
    assert_eq!(frames.len(), 10);
    // The last frame is at 0.9 s: 45 hundredths of the way, 135 pixels.
    assert_eq!(corner(&frames[9], RED, 20, 20), (155, 20));
}

#[test]
fn t016_c5_an_element_in_a_hidden_clip_is_where_the_time_says_when_the_clip_shows() {
    let frames = Scene {
        test: "t016_c5",
        size: "400x60",
        fps: 10,
        length: "1.5",
        style: BOX,
        body: "<div data-fs-clip data-fs-start=\"0.5\" data-fs-duration=\"0.7\"><div id=\"a\"></div></div>",
        tweens: "{ \"target\": \"#a\", \"to\": { \"x\": 300 }, \"at\": 0, \"duration\": 1 }",
    }
    .frames_in_both_orders();
    for frame in &frames[..5] {
        assert!(frame.is_all(BLACK, 0));
    }
    // The clip shows from 0.5 s; the tween has been running since 0.
    assert_eq!(corner(&frames[5], RED, 20, 20), (170, 20));
    assert_eq!(corner(&frames[9], RED, 20, 20), (290, 20));
    assert_eq!(corner(&frames[11], RED, 20, 20), (320, 20));
    assert!(frames[12].is_all(BLACK, 0));
}

#[test]
fn t017_c1_c5_a_from_tween_holds_its_starting_value_before_it_starts() {
    let frames = Scene {
        test: "t017_c1",
        size: "60x120",
        fps: 20,
        length: "2",
        style: BOX,
        body: "<div id=\"a\"></div>",
        tweens: "{ \"target\": \"#a\", \"from\": { \"y\": 60 }, \"at\": 1, \"duration\": 0.5 }",
    }
    .frames_in_both_orders();
    let tops = tops(&frames, RED);
    // 60 pixels below at every frame before 1 second, and still on the frame at 1 second.
    assert!(tops[..=20].iter().all(|&top| top == 80), "{tops:?}");
    // Moving between 1 and 1.5 seconds: 6 pixels a frame.
    assert_eq!(tops[20..=30], [80, 74, 68, 62, 56, 50, 44, 38, 32, 26, 20]);
    // At its own position afterwards.
    assert!(tops[30..].iter().all(|&top| top == 20), "{tops:?}");
}

#[test]
fn t017_c2_c5_a_from_to_tween_runs_between_its_two_values() {
    let frames = Scene {
        test: "t017_c2",
        size: "400x60",
        fps: 10,
        length: "2",
        style: BOX,
        body: "<div id=\"a\"></div>",
        tweens: "{ \"target\": \"#a\", \"from\": { \"x\": 50 }, \"to\": { \"x\": 250 }, \"at\": 0.5, \"duration\": 1 }",
    }
    .frames_in_both_orders();
    let lefts = lefts(&frames, RED);
    assert!(lefts[..=5].iter().all(|&left| left == 70), "{lefts:?}");
    assert_eq!(lefts[5..=15], [70, 90, 110, 130, 150, 170, 190, 210, 230, 250, 270]);
    assert!(lefts[15..].iter().all(|&left| left == 270), "{lefts:?}");
}

#[test]
fn t017_c3_c5_a_set_tween_applies_from_the_frame_at_its_time() {
    let frames = Scene {
        test: "t017_c3",
        size: "200x60",
        fps: 10,
        length: "3",
        style: BOX,
        body: "<div id=\"a\"></div>",
        tweens: "{ \"target\": \"#a\", \"to\": { \"x\": 40 }, \"at\": 0, \"duration\": 1 },\n\
                 { \"target\": \"#a\", \"set\": { \"x\": 100 }, \"at\": 2 }",
    }
    .frames_in_both_orders();
    let lefts = lefts(&frames, RED);
    // The earlier value holds up to the frame before 2 seconds.
    assert!(lefts[10..20].iter().all(|&left| left == 60), "{lefts:?}");
    // The set value from the frame at 2 seconds on.
    assert!(lefts[20..].iter().all(|&left| left == 120), "{lefts:?}");
}

#[test]
fn t017_c4_c5_an_earlier_tween_holds_until_the_next_one_on_the_property_starts() {
    let frames = Scene {
        test: "t017_c4",
        size: "400x60",
        fps: 10,
        length: "4.5",
        style: BOX,
        body: "<div id=\"a\"></div>",
        tweens: "{ \"target\": \"#a\", \"to\": { \"x\": 100 }, \"at\": 0, \"duration\": 1 },\n\
                 { \"target\": \"#a\", \"from\": { \"x\": 200 }, \"to\": { \"x\": 300 }, \"at\": 3, \"duration\": 1 }",
    }
    .frames_in_both_orders();
    let lefts = lefts(&frames, RED);
    assert_eq!(lefts[0], 20);
    // The first tween's end value holds until 3 seconds; the second tween's start does not show early.
    assert!(lefts[10..30].iter().all(|&left| left == 120), "{lefts:?}");
    assert_eq!((lefts[30], lefts[35]), (220, 270));
    assert!(lefts[40..].iter().all(|&left| left == 320), "{lefts:?}");
}

#[test]
fn t018_c1_scale_from_1_to_2_paints_one_and_a_half_times_then_twice_the_size() {
    let frames = Scene {
        test: "t018_c1",
        size: "200x160",
        fps: 10,
        length: "1.5",
        style: "#a { position: absolute; left: 80px; top: 60px; width: 40px; height: 40px; background: #ff0000; }",
        body: "<div id=\"a\"></div>",
        tweens: "{ \"target\": \"#a\", \"to\": { \"scale\": 2 }, \"at\": 0, \"duration\": 1 }",
    }
    .frames();
    // The box grows about its centre at 100, 80.
    assert_eq!(corner(&frames[0], RED, 40, 40), (80, 60));
    assert_eq!(corner(&frames[5], RED, 60, 60), (70, 50));
    assert_eq!(corner(&frames[10], RED, 80, 80), (60, 40));
    assert_eq!(corner(&frames[14], RED, 80, 80), (60, 40));
}

const LONG: &str = "#a { position: absolute; left: 60px; top: 70px; width: 80px; height: 20px; background: #ff0000; }";

#[test]
fn t018_c2_rotation_to_90_degrees_is_45_degrees_half_way() {
    let frames = Scene {
        test: "t018_c2",
        size: "200x160",
        fps: 10,
        length: "1.5",
        style: LONG,
        body: "<div id=\"a\"></div>",
        tweens: "{ \"target\": \"#a\", \"to\": { \"rotation\": 90 }, \"at\": 0, \"duration\": 1 }",
    }
    .frames();
    let half = measure::shape(&frames[5], BLACK, RED).unwrap();
    assert!((half.angle - 45.0).abs() < 0.5, "{half:?}");
    assert!((half.length - 80.0).abs() < 1.0 && (half.breadth - 20.0).abs() < 1.0, "{half:?}");
    let expected = measure::expected_corners(60.0, 70.0, 80.0, 20.0, [0.0, 0.0, 1.0, 45.0]);
    assert!(measure::corner_distance(&expected, &half.corners) <= 1.0, "{half:?}");
    // At the end the box stands upright about its centre at 100, 80.
    assert_eq!(corner(&frames[10], RED, 20, 80), (90, 40));
    assert_eq!(corner(&frames[14], RED, 20, 80), (90, 40));
}

#[test]
fn t018_c3_opacity_from_1_to_0_is_an_even_blend_half_way_and_gone_at_the_end() {
    let frames = Scene {
        test: "t018_c3",
        size: "200x160",
        fps: 10,
        length: "1.5",
        style: "[data-fs-composition] { background: #204060; }\n\
                #a { position: absolute; left: 60px; top: 40px; width: 80px; height: 80px; background: #e0a020; }",
        body: "<div id=\"a\"></div>",
        tweens: "{ \"target\": \"#a\", \"to\": { \"opacity\": 0 }, \"at\": 0, \"duration\": 1 }",
    }
    .frames();
    let (colour, ground): (Rgb, Rgb) = ([0xe0, 0xa0, 0x20], [0x20, 0x40, 0x60]);
    assert_eq!(frames[0].pixel(100, 80), colour);
    // Half-way every pixel inside the box is the mean of the two colours, to the nearest level.
    let inside = frames[5].crop(60, 40, 80, 80);
    let blend = [0x80, 0x70, 0x40];
    assert!(inside.is_all(blend, 1), "the middle pixel is {:?}", frames[5].pixel(100, 80));
    assert_eq!(frames[5].pixel(10, 10), ground);
    assert!(frames[10].is_all(ground, 0) && frames[14].is_all(ground, 0));
}

#[test]
fn t018_c4_four_properties_in_one_tween_are_all_half_way_at_the_half_way_frame() {
    let frames = Scene {
        test: "t018_c4",
        size: "320x240",
        fps: 10,
        length: "1.5",
        style: LONG,
        body: "<div id=\"a\"></div>",
        tweens: "{ \"target\": \"#a\", \"to\": { \"x\": 80, \"y\": 40, \"scale\": 2, \"rotation\": 90, \"opacity\": 0.5 }, \"at\": 0, \"duration\": 1 }",
    }
    .frames();
    for (frame, values, opacity) in [(5, [40.0, 20.0, 1.5, 45.0], 0.75), (10, [80.0, 40.0, 2.0, 90.0], 0.5)] {
        let shape = measure::shape(&frames[frame], BLACK, RED).unwrap();
        let expected = measure::expected_corners(60.0, 70.0, 80.0, 20.0, values);
        let distance = measure::corner_distance(&expected, &shape.corners);
        assert!(distance <= 1.0, "frame {frame}: {distance} from {expected:?}: {shape:?}");
        assert!((shape.length - 80.0 * values[2]).abs() < 1.0, "frame {frame}: {shape:?}");
        assert!((shape.opacity - opacity).abs() <= 2.0 / 255.0, "frame {frame}: {shape:?}");
    }
    let half = measure::shape(&frames[5], BLACK, RED).unwrap();
    assert!((half.angle - 45.0).abs() < 0.5, "{half:?}");
    assert!(
        (half.centre.0 - 140.0).abs() < 0.5 && (half.centre.1 - 100.0).abs() < 0.5,
        "{half:?}"
    );
}

const EASINGS: [&str; 10] = [
    "linear",
    "in-quad",
    "out-quad",
    "in-out-quad",
    "in-cubic",
    "out-cubic",
    "in-out-cubic",
    "in-quart",
    "out-quart",
    "in-out-quart",
];

/// The curve as the spec writes it.
fn curve(name: &str, p: f64) -> f64 {
    let power = match name.rsplit('-').next().unwrap() {
        "quad" => 2,
        "cubic" => 3,
        "quart" => 4,
        _ => return p,
    };
    if name.starts_with("in-out") {
        if p < 0.5 {
            2f64.powi(power - 1) * p.powi(power)
        } else {
            1.0 - (2.0 - 2.0 * p).powi(power) / 2.0
        }
    } else if name.starts_with("in") {
        p.powi(power)
    } else {
        1.0 - (1.0 - p).powi(power)
    }
}

/// Ten boxes, one to a row of 18 pixels, each with its own easing on a
/// 300 pixel tween from 0.5 s to 1.5 s.
fn eased(test: &str) -> Vec<Frame> {
    let mut body = String::new();
    let mut tweens = Vec::new();
    for (row, name) in EASINGS.iter().enumerate() {
        body.push_str(&format!(
            "<div id=\"e{row}\" class=\"e\" style=\"top: {}px\"></div>\n",
            4 + 18 * row
        ));
        tweens.push(format!(
            "{{ \"target\": \"#e{row}\", \"to\": {{ \"x\": 300 }}, \"at\": 0.5, \"duration\": 1, \"ease\": \"{name}\" }}"
        ));
    }
    Scene {
        test,
        size: "360x180",
        fps: 20,
        length: "2",
        style: ".e { position: absolute; left: 20px; width: 20px; height: 10px; background: #ff0000; }",
        body: &body,
        tweens: &tweens.join(",\n"),
    }
    .frames()
}

/// How far each box has moved on each frame, to a fraction of a pixel.
fn travelled(frames: &[Frame]) -> Vec<[f64; 10]> {
    frames
        .iter()
        .map(|frame| {
            let mut moved = [0.0; 10];
            for (row, moved) in moved.iter_mut().enumerate() {
                let band = frame.crop(0, 18 * row as u32, 360, 18);
                *moved = measure::shape(&band, BLACK, RED).unwrap().centre.0 - 30.0;
            }
            moved
        })
        .collect()
}

#[test]
fn t019_c1_c2_c3_each_of_the_ten_easings_follows_its_curve() {
    let frames = eased("t019_c1");
    assert_eq!(frames.len(), 40);
    let travelled = travelled(&frames);
    for (row, name) in EASINGS.iter().enumerate() {
        // c1: a quarter, a half and three quarters of the way through the tween.
        for (frame, p) in [(15, 0.25), (20, 0.5), (25, 0.75)] {
            let (measured, computed) = (travelled[frame][row], 300.0 * curve(name, p));
            assert!(
                (measured - computed).abs() <= 1.0,
                "{name} at {p}: measured {measured}, computed {computed}"
            );
        }
        // c2: exactly at the start value up to the tween's first frame, exactly at the end value from its end.
        let band = |frame: usize| frames[frame].crop(0, 18 * row as u32, 360, 18).bounds(RED, 0).unwrap();
        for frame in 0..=10 {
            assert_eq!((band(frame).left, band(frame).width()), (20, 20), "{name}, frame {frame}");
        }
        for frame in 30..40 {
            assert_eq!((band(frame).left, band(frame).width()), (320, 20), "{name}, frame {frame}");
        }
        // c3: an in-out curve is half-way at the half-way frame.
        if name.starts_with("in-out") {
            assert_eq!((band(20).left, band(20).width()), (170, 20), "{name}");
        }
        // Each curve only ever moves forward.
        assert!((10..30).all(|frame| travelled[frame + 1][row] >= travelled[frame][row]), "{name}");
    }
    // The curves differ from one another where they should: in is behind linear, out is ahead.
    assert!(travelled[20][1] < travelled[20][0] && travelled[20][0] < travelled[20][2]);
}

#[test]
fn t019_c4_an_eased_tween_measures_the_same_in_two_renders() {
    let first = eased("t019_c4_first");
    let second = eased("t019_c4_second");
    assert_eq!(travelled(&first), travelled(&second));
    assert_eq!(measure::first_difference(&hashes(&first), &hashes(&second)), None);
}
