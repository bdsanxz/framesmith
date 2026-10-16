//! The arithmetic of the comparison of the card's pictures with the processor's: the least
//! similarity, the share of values counted, and the lines and report that tell the figures of each
//! frame (`docs/spec/card-renderer.md` sections 4 and 4.1). No card is used here.

mod common;

use common::agreement::{figures, raise_red, Comparison, Figures, DOCTORED_LEVELS, LEAST_SIMILARITY, MOST_LEVELS_APART, ONE_IN};
use draw::Picture;

/// Pixels of a frame of 96 by 54: 15552 values of red, green and blue.
const VALUES: u64 = 96 * 54 * 3;

/// A picture of 96 by 54 pixels, all of one colour.
fn flat(red: u8, green: u8, blue: u8) -> Picture {
    Picture::new(96, 54, [red, green, blue, 255].repeat(96 * 54)).unwrap()
}

fn figures_of(number: u64, similarity: Option<f64>, further_apart: u64) -> Figures {
    Figures {
        number,
        similarity,
        further_apart,
        values: VALUES,
    }
}

#[test]
fn identical_pictures_have_similarity_one_and_no_value_counted() {
    let figures = figures(0, &flat(16, 32, 64), &flat(16, 32, 64)).unwrap();
    assert_eq!(figures.similarity.map(|s| format!("{s:.6}")), Some("1.000000".to_string()));
    assert_eq!((figures.further_apart, figures.values), (0, VALUES));
    assert!(figures.within());
    assert_eq!(
        figures.line(),
        "frame 0000: similarity 1.000000, values further apart than 4 levels 0.0000 % (0 of 15552), within the tolerance"
    );
}

#[test]
fn a_value_four_levels_apart_is_not_counted_and_a_value_five_levels_apart_is_in_either_direction() {
    let base = flat(100, 100, 100);
    assert_eq!(figures(0, &base, &flat(104, 96, 104)).unwrap().further_apart, 0);
    // Five levels apart in red, green or blue: every red value of the frame is counted.
    assert_eq!(figures(0, &base, &flat(105, 100, 100)).unwrap().further_apart, 5184);
    assert_eq!(figures(0, &base, &flat(95, 100, 100)).unwrap().further_apart, 5184);
    assert_eq!(figures(0, &base, &flat(100, 105, 100)).unwrap().further_apart, 5184);
    assert_eq!(figures(0, &base, &flat(100, 100, 95)).unwrap().further_apart, 5184);
    assert_eq!(MOST_LEVELS_APART, 4);
}

#[test]
fn seven_values_in_fifteen_thousand_five_hundred_fifty_two_are_within_and_eight_are_not() {
    assert!(figures_of(0, Some(1.0), 7).within());
    assert!(!figures_of(0, Some(1.0), 8).within());
    // The share is in whole numbers, so a frame of 2000 values allows one value.
    let small = |further_apart| Figures {
        number: 0,
        similarity: Some(1.0),
        further_apart,
        values: 2000,
    };
    assert!(small(1).share_within());
    assert!(!small(2).share_within());
    assert_eq!(ONE_IN, 2000, "0.05 % is one value in 2000");
}

#[test]
fn the_share_is_written_to_four_places_of_per_cent_in_the_line() {
    let seven = figures_of(2, Some(1.0), 7);
    assert_eq!(
        seven.line(),
        "frame 0002: similarity 1.000000, values further apart than 4 levels 0.0450 % (7 of 15552), within the tolerance"
    );
    let eight = figures_of(3, Some(1.0), 8);
    assert_eq!(
        eight.line(),
        "frame 0003: similarity 1.000000, values further apart than 4 levels 0.0514 % (8 of 15552), outside the tolerance"
    );
}

#[test]
fn a_similarity_of_least_the_plan_value_is_within_and_one_below_it_is_not() {
    assert_eq!(LEAST_SIMILARITY, 0.995);
    assert!(figures_of(0, Some(0.995), 0).within());
    assert!(figures_of(0, Some(0.996), 0).within());
    assert!(!figures_of(0, Some(0.994_999), 0).within());
    assert!(
        !figures_of(0, None, 0).within(),
        "a similarity that cannot be measured is not within"
    );
}

#[test]
fn a_failing_frame_names_both_figures_it_misses() {
    let failing = figures_of(7, Some(0.812_345), 100);
    assert!(!failing.within());
    assert_eq!(
        failing.misses(),
        vec![
            "similarity 0.812345 is below 0.995".to_string(),
            "0.6430 % of values are further apart than 4 levels, above 0.05 %".to_string(),
        ]
    );
    assert_eq!(
        failing.line(),
        "frame 0007: similarity 0.812345, values further apart than 4 levels 0.6430 % (100 of 15552), outside the tolerance"
    );
}

#[test]
fn a_similarity_that_cannot_be_measured_is_named_as_such() {
    let unmeasured = figures_of(1, None, 0);
    assert_eq!(unmeasured.misses(), vec!["similarity could not be measured".to_string()]);
    assert_eq!(
        unmeasured.line(),
        "frame 0001: similarity not measured, values further apart than 4 levels 0.0000 % (0 of 15552), outside the tolerance"
    );
}

#[test]
fn pictures_of_different_sizes_are_an_error_that_names_both_sizes() {
    let small = Picture::new(2, 2, [0; 16].to_vec()).unwrap();
    let large = Picture::new(3, 3, vec![0; 36]).unwrap();
    assert_eq!(
        figures(3, &small, &large).unwrap_err(),
        "frame 0003: the card's picture is 3x3 and the processor's is 2x2"
    );
}

#[test]
fn the_test_device_raises_only_red_and_stops_at_255() {
    assert_eq!(DOCTORED_LEVELS, 40);
    let picture = Picture::new(2, 1, vec![10, 20, 30, 255, 250, 20, 30, 255]).unwrap();
    let raised = raise_red(&picture, DOCTORED_LEVELS);
    assert_eq!(raised.pixels(), &[50, 20, 30, 255, 255, 20, 30, 255]);
}

#[test]
fn the_report_gives_a_line_for_each_frame_and_then_the_verdict() {
    let ok = |number| figures_of(number, Some(1.0), 0);
    let comparison = Comparison {
        frames: vec![Ok(ok(0)), Ok(ok(1))],
    };
    assert!(comparison.passed());
    assert_eq!(
        comparison.report(),
        vec![
            "frame 0000: similarity 1.000000, values further apart than 4 levels 0.0000 % (0 of 15552), within the tolerance".to_string(),
            "frame 0001: similarity 1.000000, values further apart than 4 levels 0.0000 % (0 of 15552), within the tolerance".to_string(),
            "the comparison passed: all 2 frames are within the tolerance".to_string(),
        ]
    );
}

#[test]
fn a_failing_report_names_each_frame_that_misses_and_counts_them() {
    let comparison = Comparison {
        frames: vec![
            Ok(figures_of(0, Some(1.0), 0)),
            Ok(figures_of(1, Some(0.9), 0)),
            Err("frame 0002: the card's picture is 1x1 and the processor's is 2x2".to_string()),
        ],
    };
    assert!(!comparison.passed());
    assert_eq!(
        comparison.report(),
        vec![
            "frame 0000: similarity 1.000000, values further apart than 4 levels 0.0000 % (0 of 15552), within the tolerance".to_string(),
            "frame 0001: similarity 0.900000, values further apart than 4 levels 0.0000 % (0 of 15552), outside the tolerance".to_string(),
            "frame 0002: the card's picture is 1x1 and the processor's is 2x2; it is outside the tolerance".to_string(),
            "frame 0001 is outside the tolerance: similarity 0.900000 is below 0.995".to_string(),
            "the comparison failed: 2 of 3 frames are outside the tolerance".to_string(),
        ]
    );
}
