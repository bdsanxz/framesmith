//! The picture check on counts alone: what is a low frame, what a cause does, and when a scene and
//! the whole check pass (`docs/dev/comparison.md`, section 4).

use parity::picture::{check_excuses, is_low, judge, parse_excuse, parse_scene, percent, scenes_for, score_milli, Excuse, Scene, State};

fn scene(name: &str, first: u64, last: u64) -> Scene {
    Scene {
        name: name.into(),
        first,
        last,
    }
}

fn cause(first: u64, last: u64, text: &str) -> Excuse {
    Excuse {
        first,
        last,
        cause: text.into(),
    }
}

/// `frames` frames of 10,000 pixels; `low` gives the differing pixels of chosen frames.
fn counts(frames: usize, low: &[(usize, u64)]) -> Vec<(u64, u64)> {
    (0..frames)
        .map(|at| (10_000, low.iter().find(|(n, _)| *n == at).map_or(0, |(_, d)| *d)))
        .collect()
}

#[test]
fn a_frame_is_low_only_when_more_than_one_percent_of_its_pixels_differ() {
    assert!(!is_low(10_000, 0));
    assert!(!is_low(10_000, 100), "exactly 99 % is not low");
    assert!(is_low(10_000, 101));
    assert!(!is_low(57_600, 576));
    assert!(is_low(57_600, 577));
}

#[test]
fn a_score_is_the_share_of_equal_pixels_rounded_down_in_thousandths_of_a_percent() {
    assert_eq!(percent(10_000, 0), "100.000");
    assert_eq!(percent(10_000, 100), "99.000");
    assert_eq!(percent(10_000, 101), "98.990");
    assert_eq!(percent(57_600, 1), "99.998");
    assert_eq!(score_milli(3, 1), 66_666);
    assert_eq!(percent(10_000, 10_000), "0.000");
}

#[test]
fn frames_at_99_percent_or_better_are_ok_and_the_whole_check_passes() {
    let judged = judge(&counts(20, &[(4, 100), (5, 50)]), &[scene("all", 0, 19)], &[]);
    assert!(judged.frames.iter().all(|f| f.state == State::Ok));
    assert!(judged.average_passes() && judged.passes());
}

#[test]
fn a_low_frame_without_a_cause_fails_its_scene_even_when_the_scene_average_is_high() {
    // One frame at 98 % in a scene of 100: the average is 99.98 %.
    let judged = judge(&counts(100, &[(40, 200)]), &[scene("a", 0, 49), scene("b", 50, 99)], &[]);
    assert_eq!(judged.frames[40].state, State::Fail);
    assert!(judged.average_passes());
    assert!(!judged.scenes[0].passes(), "the scene of the low frame fails");
    assert!(judged.scenes[1].passes(), "the other scene passes");
    assert!(!judged.passes());
    assert_eq!(judged.scenes[0].failed, 1);
    assert_eq!(judged.lowest().unwrap().number, 40);
}

#[test]
fn a_low_frame_with_a_cause_is_excused_and_keeps_the_score_that_was_measured() {
    let excuses = [cause(40, 40, "the reference draws this still late")];
    let judged = judge(&counts(100, &[(40, 200)]), &[scene("all", 0, 99)], &excuses);
    assert_eq!(
        judged.frames[40].state,
        State::Excused("the reference draws this still late".into())
    );
    assert_eq!(judged.frames[40].differing, 200, "the score is not changed by the cause");
    assert_eq!(judged.differing, 200, "and the average counts it");
    assert_eq!(judged.scenes[0].excused, 1);
    assert_eq!(judged.scenes[0].failed, 0);
    assert!(judged.passes(), "an excused frame does not fail a scene whose average holds");
    assert_eq!(judged.low().count(), 1, "it is still listed");
}

#[test]
fn an_excused_frame_does_not_lift_the_average_or_a_scene_below_99_percent() {
    // Three frames in a scene, one of them at 50 %: the scene average is 83 %, whatever the cause.
    let excuses = [cause(1, 1, "a different still")];
    let judged = judge(&counts(3, &[(1, 5_000)]), &[scene("short", 0, 2)], &excuses);
    assert!(!judged.average_passes());
    assert!(!judged.scenes[0].passes());
    assert!(!judged.passes());
}

#[test]
fn the_check_passes_only_when_the_average_and_every_scene_are_at_99_percent_or_better() {
    // Every frame at 98.9 %: each is low, each is excused, and the average and the scene are under 99 %.
    let judged = judge(
        &counts(10, &(0..10).map(|n| (n, 110)).collect::<Vec<_>>()),
        &[scene("all", 0, 9)],
        &[cause(0, 9, "grain")],
    );
    assert!(!judged.passes());
    // Every frame at exactly 99 %: nothing is low, and the average and the scene are at 99 %.
    let judged = judge(
        &counts(10, &(0..10).map(|n| (n, 100)).collect::<Vec<_>>()),
        &[scene("all", 0, 9)],
        &[],
    );
    assert!(judged.passes());
    // One scene under 99 % on average with every other scene at 100 %.
    let judged = judge(
        &counts(100, &(0..10).map(|n| (n, 90)).collect::<Vec<_>>()),
        &[scene("a", 0, 9), scene("b", 10, 99)],
        &[],
    );
    assert!(judged.average_passes());
    assert!(
        judged.scenes[0].passes(),
        "90 of 10,000 is above 99 % in each frame and in the scene"
    );
    let judged = judge(
        &counts(100, &(0..10).map(|n| (n, 150)).collect::<Vec<_>>()),
        &[scene("a", 0, 9), scene("b", 10, 99)],
        &[cause(0, 9, "x")],
    );
    assert!(judged.average_passes(), "98.5 % over 10 frames of 100 is 99.85 %");
    assert!(!judged.scenes[0].passes(), "the scene of ten frames at 98.5 % is under 99 %");
    assert!(!judged.passes());
}

#[test]
fn scenes_written_on_the_command_line_are_read_and_wrong_ones_are_refused() {
    assert_eq!(parse_scene("intro=0-29").unwrap(), scene("intro", 0, 29));
    assert_eq!(parse_scene("hold=30-30").unwrap(), scene("hold", 30, 30));
    assert_eq!(parse_scene("one=7").unwrap(), scene("one", 7, 7));
    assert!(parse_scene("0-29").is_err());
    assert!(parse_scene("=0-29").is_err());
    assert!(parse_scene("a=9-3").unwrap_err().contains("ends before it begins"));
    assert!(parse_scene("a=x-3").is_err());
}

#[test]
fn causes_written_on_the_command_line_are_read_and_empty_or_overlapping_ones_are_refused() {
    assert_eq!(parse_excuse("12=a late still").unwrap(), cause(12, 12, "a late still"));
    assert_eq!(
        parse_excuse("3-9=fonts differ = known").unwrap(),
        cause(3, 9, "fonts differ = known")
    );
    assert!(parse_excuse("3-9=").unwrap_err().contains("is empty"));
    assert!(parse_excuse("3-9").is_err());
    assert!(check_excuses(&[cause(0, 3, "a"), cause(3, 5, "b")], 10)
        .unwrap_err()
        .contains("two causes"));
    assert!(check_excuses(&[cause(0, 3, "a")], 3).unwrap_err().contains("recorded for frame 3"));
    assert!(check_excuses(&[cause(0, 3, "a"), cause(4, 5, "b")], 10).is_ok());
}

#[test]
fn with_no_scene_given_one_scene_called_all_holds_every_frame() {
    assert_eq!(scenes_for(&[], 24).unwrap(), vec![scene("all", 0, 23)]);
    assert!(scenes_for(&[scene("a", 0, 3), scene("a", 4, 5)], 6)
        .unwrap_err()
        .contains("used twice"));
}
