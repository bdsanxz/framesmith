//! Sprint 7 motion (CAP-002/CAP-004): stagger delay distribution
//! across origins and grids, exact repeat pass layout, and reversed
//! (yoyo) evaluation through the same easing.

use motion::*;
use timebase::Time;

const NO_VALUES: Values = [None; PROP_COUNT];

fn values(list: &[(Prop, f64)]) -> Values {
    let mut out = NO_VALUES;
    for &(prop, value) in list {
        out[prop.index()] = Some(value);
    }
    out
}

fn secs(text: &str) -> Time {
    Time::parse(text).unwrap()
}

fn from_to_reversed(start: f64, end: f64, at: &str, duration: &str, ease: Ease) -> Tween {
    Tween {
        target: "#a".into(),
        kind: Kind::FromTo,
        from: values(&[(Prop::X, start)]),
        to: values(&[(Prop::X, end)]),
        at: secs(at),
        duration: secs(duration),
        ease,
        reversed: true,
    }
}

fn timeline(tweens: &[Tween], fps: u32) -> Timeline {
    let targets: Vec<Vec<usize>> = tweens.iter().map(|_| vec![0]).collect();
    Timeline::build(tweens, &targets, fps)
}

fn at(timeline: &Timeline, prop: Prop, secs: &str) -> f64 {
    timeline.elements[0].tracks[prop.index()]
        .as_ref()
        .expect("the property is animated")
        .value(Time::parse(secs).unwrap())
}

#[test]
fn stagger_ranks_run_from_each_origin() {
    // Four pieces: start counts up, end counts down, center measures
    // the distance from the middle (half ranks when even).
    assert_eq!(
        (0..4)
            .map(|i| stagger_rank(i, 4, StaggerOrigin::Start))
            .collect::<Vec<_>>(),
        vec![0.0, 1.0, 2.0, 3.0]
    );
    assert_eq!(
        (0..4)
            .map(|i| stagger_rank(i, 4, StaggerOrigin::End))
            .collect::<Vec<_>>(),
        vec![3.0, 2.0, 1.0, 0.0]
    );
    assert_eq!(
        (0..4)
            .map(|i| stagger_rank(i, 4, StaggerOrigin::Center))
            .collect::<Vec<_>>(),
        vec![1.5, 0.5, 0.5, 1.5]
    );
    // Five pieces center on the middle piece exactly.
    assert_eq!(
        (0..5)
            .map(|i| stagger_rank(i, 5, StaggerOrigin::Center))
            .collect::<Vec<_>>(),
        vec![2.0, 1.0, 0.0, 1.0, 2.0]
    );
    // A single piece never shifts, from any origin.
    for origin in [
        StaggerOrigin::Start,
        StaggerOrigin::Center,
        StaggerOrigin::End,
    ] {
        assert_eq!(stagger_rank(0, 1, origin), 0.0);
    }
}

#[test]
fn stagger_delays_scale_ranks_by_the_amount() {
    // stagger 0.5 across four pieces from the start: 0, 0.5, 1, 1.5.
    assert_eq!(
        (0..4)
            .map(|i| stagger_delay(i, 4, 0.5, StaggerOrigin::Start))
            .collect::<Vec<_>>(),
        vec![0.0, 0.5, 1.0, 1.5]
    );
    // A non-positive stagger delays nothing, never shifts backward.
    assert_eq!(stagger_delay(3, 4, 0.0, StaggerOrigin::Start), 0.0);
    assert_eq!(stagger_delay(3, 4, -0.5, StaggerOrigin::Start), 0.0);
    // Out-of-range pieces and empty layouts delay nothing.
    assert_eq!(stagger_delay(4, 4, 0.5, StaggerOrigin::Start), 0.0);
    assert_eq!(stagger_delay(0, 0, 0.5, StaggerOrigin::Start), 0.0);
}

#[test]
fn grid_stagger_spreads_across_rows_and_columns() {
    // Seven pieces in three columns (rows: 0,0,0,1,1,1,2 in
    // row-major): the start origin counts along the layout order.
    assert_eq!(
        (0..7)
            .map(|i| stagger_grid_rank(i, 7, 3, 3, StaggerOrigin::Start))
            .collect::<Vec<_>>(),
        vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0]
    );
    assert_eq!(
        (0..7)
            .map(|i| stagger_grid_rank(i, 7, 3, 3, StaggerOrigin::End))
            .collect::<Vec<_>>(),
        vec![6.0, 5.0, 4.0, 3.0, 2.0, 1.0, 0.0]
    );
    // The center origin measures Manhattan distance from the grid
    // center (column 1, row 1): corners sit 2 away, the middle 0.
    assert_eq!(
        (0..7)
            .map(|i| stagger_grid_rank(i, 7, 3, 3, StaggerOrigin::Center))
            .collect::<Vec<_>>(),
        vec![2.0, 1.0, 2.0, 1.0, 0.0, 1.0, 2.0]
    );
    // Grid delays scale the grid ranks by the stagger amount.
    assert_eq!(stagger_grid_delay(4, 7, 0.5, StaggerOrigin::Center, 3), 0.0);
    assert_eq!(stagger_grid_delay(0, 7, 0.5, StaggerOrigin::Center, 3), 1.0);
    // The declared-rows form rows the same grid: 7 pieces in 3
    // columns need 3 rows, so "3x3" matches the derived shape.
    assert_eq!(
        stagger_grid_delay_rows(2, 7, 0.5, StaggerOrigin::Center, 3, 3),
        stagger_grid_delay(2, 7, 0.5, StaggerOrigin::Center, 3)
    );
    // Fewer declared rows than needed still place every piece.
    assert_eq!(stagger_grid_rows(7, 3), 3);
    assert_eq!(stagger_grid_rows(6, 3), 2);
}

#[test]
fn max_delay_covers_the_slowest_piece() {
    assert_eq!(stagger_max_delay(4, 0.5, StaggerOrigin::Start), 1.5);
    assert_eq!(stagger_max_delay(4, 0.5, StaggerOrigin::End), 1.5);
    assert_eq!(stagger_max_delay(4, 0.5, StaggerOrigin::Center), 0.75);
    assert_eq!(stagger_max_delay(1, 0.5, StaggerOrigin::End), 0.0);
    assert_eq!(
        stagger_max_delay_grid(7, 0.5, StaggerOrigin::Center, Some((3, 3))),
        1.0
    );
    assert_eq!(
        stagger_max_delay_grid(7, 0.0, StaggerOrigin::Start, None),
        0.0
    );
}

#[test]
fn repeat_passes_space_windows_with_gaps_and_yoyo() {
    // Three 1s passes with 0.25s gaps: offsets 0, 1.25, 2.5.
    let passes = repeat_passes(1.0, 3, 0.25, false);
    assert_eq!(passes.len(), 3);
    assert_eq!(
        passes.iter().map(|p| p.offset).collect::<Vec<_>>(),
        vec![0.0, 1.25, 2.5]
    );
    assert!(passes.iter().all(|p| !p.reversed));
    // With yoyo the middle pass runs backward; without gaps the
    // passes sit end to end.
    let yoyo = repeat_passes(1.0, 3, 0.0, true);
    assert_eq!(
        yoyo.iter()
            .map(|p| (p.offset, p.reversed))
            .collect::<Vec<_>>(),
        vec![(0.0, false), (1.0, true), (2.0, false)]
    );
    // A single pass is never reversed, even with yoyo.
    assert_eq!(
        repeat_passes(1.0, 1, 0.25, true)
            .iter()
            .map(|p| (p.offset, p.reversed))
            .collect::<Vec<_>>(),
        vec![(0.0, false)]
    );
    // The latest animated instant is the last pass end.
    assert_eq!(repeat_motion_end(2.0, 1.0, 3, 0.5), 6.0);
    assert_eq!(repeat_motion_end(0.0, 1.0, 1, 0.0), 1.0);
}

#[test]
fn reversed_tween_traces_forward_values_backward() {
    // in-quad from 0 to 100 over [1, 2): forward at progress 0.25
    // reads 6.25; reversed at the same instant reads in-quad(0.75).
    let tl = timeline(&[from_to_reversed(0.0, 100.0, "1", "1", Ease::InQuad)], 30);
    assert_eq!(at(&tl, Prop::X, "1"), 100.0);
    assert!((at(&tl, Prop::X, "1.25") - 56.25).abs() < 1e-9);
    assert!((at(&tl, Prop::X, "1.5") - 25.0).abs() < 1e-9);
    assert!((at(&tl, Prop::X, "1.75") - 6.25).abs() < 1e-9);
    // A reversed pass ends where the forward motion begins.
    assert_eq!(at(&tl, Prop::X, "2"), 0.0);
    assert_eq!(at(&tl, Prop::X, "5"), 0.0);
}

#[test]
fn forward_tween_still_evaluates_exactly() {
    // The `reversed: false` default changes nothing: linear from 0
    // to 100 over [1, 2) reads the midpoint at 1.5 exactly.
    let tl = timeline(
        &[Tween {
            target: "#a".into(),
            kind: Kind::FromTo,
            from: values(&[(Prop::X, 0.0)]),
            to: values(&[(Prop::X, 100.0)]),
            at: secs("1"),
            duration: secs("1"),
            ease: Ease::Linear,
            reversed: false,
        }],
        30,
    );
    assert_eq!(at(&tl, Prop::X, "1.5"), 50.0);
}
