//! T052: our side of the variants that are compared with the reference. The comparison itself needs the
//! reference and is run by the maintainer; these tests pin what our fixtures do on their own.

mod common;

use common::*;
use measure::{Frame, Rgb};

const RED: Rgb = [0xff, 0x5a, 0x36];
const GREEN: Rgb = [0x2e, 0xcc, 0x71];
const BLUE: Rgb = [0x34, 0x98, 0xdb];
const AMBER: Rgb = [0xff, 0xb0, 0x00];

/// An easing as a curve: progress in, eased progress out.
type Curve = fn(f64) -> f64;

/// The horizontal centre of everything near `colour`, in pixels from the left edge.
fn centre(frame: &Frame, colour: Rgb) -> f64 {
    let found = frame.bounds(colour, 60).expect("the colour is in the picture");
    f64::from(found.left + found.right + 1) / 2.0
}

#[test]
fn t052_c3_the_variant_with_values_left_out_is_filled_in_by_our_rules() {
    let project = fixture("t052_c3", "title-card-unwritten");
    let ran = framesmith(&["check", text(&project)]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    for line in [
        "  rate         30 fps (filled in)",
        "  length       4 s (filled in)",
        "  frames       120",
        "  clip #title  duration 3.2 s (filled in)",
    ] {
        assert!(
            ran.stdout.lines().any(|printed| printed == line),
            "no line {line:?} in:\n{}",
            ran.stdout
        );
    }
}

#[test]
fn t052_c4_before_a_late_from_tween_starts_its_element_waits_at_the_from_value() {
    let project = fixture("t052_c4", "late-from");
    let (ran, frames) = render_frames(&project, &[], &[]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert_eq!(frames.len(), 90);

    // Every frame before 1 second: the red box (from x 400, starting at 1.2 s) waits 400 px to the right
    // of its own place, the blue box (a to tween from 1.1 s) has not moved, and the green box (from
    // opacity 0, starting at 1.5 s) is not in the picture.
    for (number, frame) in frames.iter().enumerate().take(30) {
        assert_eq!(centre(frame, RED), 480.0, "frame {number}");
        assert_eq!(centre(frame, BLUE), 80.0, "frame {number}");
        assert_eq!(frame.count(GREEN, 60), 0, "frame {number}");
    }
    // The red box still waits on the last frame before its tween (1.2 s is frame 36).
    assert_eq!(centre(&frames[35], RED), 480.0);
    // At the end each box is at its own place, the blue one at the end of its move.
    let last = &frames[89];
    assert_eq!((centre(last, RED), centre(last, GREEN), centre(last, BLUE)), (80.0, 80.0, 280.0));
}

#[test]
fn t052_c5_each_box_of_the_easings_fixture_follows_the_curve_of_its_own_easing() {
    let curves: [(&str, Curve); 10] = [
        ("linear", |p| p),
        ("in-quad", |p| p * p),
        ("out-quad", |p| 1.0 - (1.0 - p).powi(2)),
        ("in-out-quad", |p| {
            if p < 0.5 {
                2.0 * p * p
            } else {
                1.0 - (2.0 - 2.0 * p).powi(2) / 2.0
            }
        }),
        ("in-cubic", |p| p.powi(3)),
        ("out-cubic", |p| 1.0 - (1.0 - p).powi(3)),
        ("in-out-cubic", |p| {
            if p < 0.5 {
                4.0 * p.powi(3)
            } else {
                1.0 - (2.0 - 2.0 * p).powi(3) / 2.0
            }
        }),
        ("in-quart", |p| p.powi(4)),
        ("out-quart", |p| 1.0 - (1.0 - p).powi(4)),
        ("in-out-quart", |p| {
            if p < 0.5 {
                8.0 * p.powi(4)
            } else {
                1.0 - (2.0 - 2.0 * p).powi(4) / 2.0
            }
        }),
    ];
    let project = fixture("t052_c5", "easings");
    // The fixture gives box `#e<i>` the i-th easing of the list above.
    let index = std::fs::read_to_string(project.join("index.html")).unwrap();
    for (row, (name, _)) in curves.iter().enumerate() {
        let tween = index
            .lines()
            .find(|line| line.contains(&format!("\"#e{row}\"")))
            .unwrap_or_else(|| panic!("no tween for #e{row}"));
        assert!(tween.contains(&format!("\"ease\": \"{name}\"")), "{tween}");
    }

    let (ran, frames) = render_frames(&project, &[], &[]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert_eq!(frames.len(), 90);
    // Each box is 40 px wide at left 40 and moves 1000 px between 0.5 s and 2.5 s; rows are 68 px apart.
    for number in [0usize, 15, 22, 30, 45, 60, 68, 75, 89] {
        let progress = ((number as f64 / 30.0 - 0.5) / 2.0).clamp(0.0, 1.0);
        for (row, (name, curve)) in curves.iter().enumerate() {
            let strip = frames[number].crop(0, 24 + 68 * row as u32, 1280, 42);
            let expected = 60.0 + 1000.0 * curve(progress);
            let measured = centre(&strip, AMBER);
            assert!(
                (measured - expected).abs() <= 1.0,
                "frame {number}, {name}: centre at {measured}, expected {expected}"
            );
        }
    }
}
