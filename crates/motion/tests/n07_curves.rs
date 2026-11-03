//! Sprint 7 motion (T007): the full curve set against independently
//! worked values. Reference numbers come from closed-form math and an
//! independent bisection solver, not from the implementation itself.

use motion::*;

/// Tolerance for closed-form values.
const TOL: f64 = 1e-9;
/// Looser tolerance for the iterative cubic-bezier solver.
const BEZ_TOL: f64 = 1e-6;

fn ends_exact(ease: Ease) {
    assert_eq!(ease.apply(0.0), 0.0, "{} at 0", ease.name());
    assert_eq!(ease.apply(1.0), 1.0, "{} at 1", ease.name());
    assert_eq!(ease.apply(-0.5), 0.0, "{} below 0", ease.name());
    assert_eq!(ease.apply(1.5), 1.0, "{} above 1", ease.name());
}

#[test]
fn every_new_curve_is_exact_at_both_ends_and_clamps_outside() {
    for ease in [
        Ease::CubicBezier(0.25, 0.1, 0.25, 1.0),
        Ease::Steps(4),
        Ease::Spring(100.0, 5.0),
        Ease::Bounce,
        Ease::Elastic,
        Ease::Wiggle(3),
        Ease::Hold,
    ] {
        ends_exact(ease);
    }
}

#[test]
fn curve_names() {
    assert_eq!(
        Ease::CubicBezier(0.25, 0.1, 0.25, 1.0).name(),
        "cubic-bezier"
    );
    assert_eq!(Ease::Steps(4).name(), "steps");
    assert_eq!(Ease::Spring(100.0, 5.0).name(), "spring");
    assert_eq!(Ease::Bounce.name(), "bounce");
    assert_eq!(Ease::Elastic.name(), "elastic");
    assert_eq!(Ease::Wiggle(2).name(), "wiggle");
    assert_eq!(Ease::Hold.name(), "hold");
}

#[test]
fn default_counts_match_the_documented_values() {
    assert_eq!(SPRING_DEFAULT_STIFFNESS, 100.0);
    assert_eq!(SPRING_DEFAULT_DAMPING, 5.0);
    assert_eq!(WIGGLE_DEFAULT_COUNT, 3);
}

#[test]
fn cubic_bezier_linear_params_are_the_identity() {
    let ease = Ease::CubicBezier(0.0, 0.0, 1.0, 1.0);
    for (p, expected) in [(0.25, 0.25), (0.5, 0.5), (0.75, 0.75)] {
        assert!(
            (ease.apply(p) - expected).abs() <= BEZ_TOL,
            "identity bezier at {p}: {}",
            ease.apply(p)
        );
    }
}

#[test]
fn cubic_bezier_matches_the_independent_solver() {
    // CSS `ease` (0.25, 0.1, 0.25, 1): values from a 200-pass bisection
    // solver, independent of the Newton iteration in the implementation.
    let ease = Ease::CubicBezier(0.25, 0.1, 0.25, 1.0);
    for (p, expected) in [(0.25, 0.408510591355), (0.5, 0.802403387585)] {
        assert!(
            (ease.apply(p) - expected).abs() <= BEZ_TOL,
            "css ease at {p}: {}",
            ease.apply(p)
        );
    }
}

#[test]
fn cubic_bezier_symmetric_params_pass_through_the_middle() {
    // x1 + x2 = 1 with mirrored y values is point-symmetric: p = 0.5
    // must ease to exactly 0.5.
    let ease = Ease::CubicBezier(0.42, 0.0, 0.58, 1.0);
    assert!((ease.apply(0.5) - 0.5).abs() <= BEZ_TOL);
}

#[test]
fn steps_holds_each_interval_until_its_end() {
    let ease = Ease::Steps(4);
    assert_eq!(ease.apply(0.0), 0.0);
    assert_eq!(ease.apply(0.24), 0.0);
    assert_eq!(ease.apply(0.25), 0.25);
    assert_eq!(ease.apply(0.3), 0.25);
    assert_eq!(ease.apply(0.5), 0.5);
    assert_eq!(ease.apply(0.99), 0.75);
    assert_eq!(Ease::Steps(1).apply(0.5), 0.0);
    assert_eq!(Ease::Steps(1).apply(0.999), 0.0);
}

#[test]
fn spring_matches_the_damped_closed_form() {
    // 1 - e^(-5p)(cos(10p) + 0.5 sin(10p)), worked by hand.
    let ease = Ease::Spring(100.0, 5.0);
    for (p, expected) in [
        (0.25, 1.143798918792),
        (0.5, 1.01607223878),
        (0.75, 0.980818096157),
    ] {
        assert!(
            (ease.apply(p) - expected).abs() <= TOL,
            "spring at {p}: {}",
            ease.apply(p)
        );
    }
    // A real spring overshoots before it settles.
    assert!(ease.apply(0.25) > 1.0);
}

#[test]
fn bounce_matches_the_four_segment_closed_form() {
    // 7.5625p^2 at 1/4; second parabola at 1/2: both exact rationals.
    assert_eq!(Ease::Bounce.apply(0.25), 0.47265625);
    assert_eq!(Ease::Bounce.apply(0.5), 0.765625);
    // The first fall touches down exactly at 1/2.75.
    assert!((Ease::Bounce.apply(1.0 / 2.75) - 1.0).abs() <= TOL);
}

#[test]
fn elastic_matches_the_decaying_sine_closed_form() {
    // sin(-pi/6)...: at 1/4 the sine is exactly -1/2, at 1/2 exactly 1/2.
    assert!((Ease::Elastic.apply(0.25) - 0.911611652352).abs() <= TOL);
    assert!((Ease::Elastic.apply(0.5) - 1.015625).abs() <= TOL);
    assert!(Ease::Elastic.apply(0.5) > 1.0);
}

#[test]
fn wiggle_oscillates_about_the_diagonal_and_fades() {
    // p + sin(2 pi n p)(1 - p)/4 with n = 1: exact at quarters.
    let ease = Ease::Wiggle(1);
    assert_eq!(ease.apply(0.25), 0.4375);
    assert_eq!(ease.apply(0.5), 0.5);
    assert_eq!(ease.apply(0.75), 0.6875);
    // Three default wiggles cross the diagonal at p = 1/6 .. 5/6.
    // (sin(6 pi p) = 0 five times inside (0, 1); exact zeros never
    // double-count: only nonzero sign changes move the count).
    let w3 = Ease::Wiggle(3);
    let mut crossings = 0;
    let mut sign = 1.0f64;
    for i in 1..1000 {
        let p = i as f64 / 1000.0;
        let d = w3.apply(p) - p;
        if d != 0.0 && d.signum() != sign {
            crossings += 1;
            sign = d.signum();
        }
    }
    assert_eq!(crossings, 5);
}

#[test]
fn hold_keeps_zero_until_the_exact_end() {
    for p in [0.0, 0.25, 0.5, 0.99, 0.999999] {
        assert_eq!(Ease::Hold.apply(p), 0.0, "hold at {p}");
    }
}
