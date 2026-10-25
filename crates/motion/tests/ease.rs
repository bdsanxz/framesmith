//! The easing curves against their worked values and their shape (`docs/spec/motion.md`, "Easing names").

use motion::*;

/// The tolerance of the curves' values (`docs/spec/motion.md`, "Easing names").
const TOLERANCE: f64 = 1e-12;

fn root2() -> f64 {
    2f64.sqrt()
}

/// Holds `ease` to exact ends and to its worked values at 1/4, 1/2 and 3/4.
fn holds_worked_values(ease: Ease, quarter: f64, half: f64, three_quarters: f64) {
    assert_eq!(ease.apply(0.0), 0.0, "{} at 0", ease.name());
    assert_eq!(ease.apply(1.0), 1.0, "{} at 1", ease.name());
    for (p, expected) in [(0.25, quarter), (0.5, half), (0.75, three_quarters)] {
        let value = ease.apply(p);
        assert!(
            (value - expected).abs() <= TOLERANCE,
            "{} at {p}: {value}, expected {expected}",
            ease.name()
        );
    }
}

macro_rules! worked_values {
    ($($test:ident: $ease:expr => $quarter:expr, $half:expr, $three_quarters:expr;)*) => {
        $(
            #[test]
            fn $test() {
                holds_worked_values($ease, $quarter, $half, $three_quarters);
            }
        )*
    };
}

worked_values! {
    linear_has_its_worked_values: Ease::Linear => 1.0 / 4.0, 1.0 / 2.0, 3.0 / 4.0;
    in_quad_has_its_worked_values: Ease::InQuad => 1.0 / 16.0, 1.0 / 4.0, 9.0 / 16.0;
    out_quad_has_its_worked_values: Ease::OutQuad => 7.0 / 16.0, 3.0 / 4.0, 15.0 / 16.0;
    in_out_quad_has_its_worked_values: Ease::InOutQuad => 1.0 / 8.0, 1.0 / 2.0, 7.0 / 8.0;
    in_cubic_has_its_worked_values: Ease::InCubic => 1.0 / 64.0, 1.0 / 8.0, 27.0 / 64.0;
    out_cubic_has_its_worked_values: Ease::OutCubic => 37.0 / 64.0, 7.0 / 8.0, 63.0 / 64.0;
    in_out_cubic_has_its_worked_values: Ease::InOutCubic => 1.0 / 16.0, 1.0 / 2.0, 15.0 / 16.0;
    in_quart_has_its_worked_values: Ease::InQuart => 1.0 / 256.0, 1.0 / 16.0, 81.0 / 256.0;
    out_quart_has_its_worked_values: Ease::OutQuart => 175.0 / 256.0, 15.0 / 16.0, 255.0 / 256.0;
    in_out_quart_has_its_worked_values: Ease::InOutQuart => 1.0 / 32.0, 1.0 / 2.0, 31.0 / 32.0;
    in_quint_has_its_worked_values: Ease::InQuint => 1.0 / 1024.0, 1.0 / 32.0, 243.0 / 1024.0;
    out_quint_has_its_worked_values: Ease::OutQuint => 781.0 / 1024.0, 31.0 / 32.0, 1023.0 / 1024.0;
    in_out_quint_has_its_worked_values: Ease::InOutQuint => 1.0 / 64.0, 1.0 / 2.0, 63.0 / 64.0;
    in_sine_has_its_worked_values: Ease::InSine =>
        1.0 - (2.0 + root2()).sqrt() / 2.0, 1.0 - root2() / 2.0, 1.0 - (2.0 - root2()).sqrt() / 2.0;
    out_sine_has_its_worked_values: Ease::OutSine =>
        (2.0 - root2()).sqrt() / 2.0, root2() / 2.0, (2.0 + root2()).sqrt() / 2.0;
    in_out_sine_has_its_worked_values: Ease::InOutSine => (2.0 - root2()) / 4.0, 1.0 / 2.0, (2.0 + root2()) / 4.0;
    in_expo_has_its_worked_values: Ease::InExpo => root2() / 256.0, 1.0 / 32.0, root2() / 8.0;
    out_expo_has_its_worked_values: Ease::OutExpo => 1.0 - root2() / 8.0, 31.0 / 32.0, 1.0 - root2() / 256.0;
    in_out_expo_has_its_worked_values: Ease::InOutExpo => 1.0 / 64.0, 1.0 / 2.0, 63.0 / 64.0;
}

#[test]
fn every_named_curve_is_exact_at_both_ends_and_out_of_range_takes_the_nearer_end() {
    for ease in Ease::ALL.into_iter().chain([Ease::OutBack(1.7), Ease::OutBack(0.0)]) {
        assert_eq!(ease.apply(0.0), 0.0, "{}", ease.name());
        assert_eq!(ease.apply(1.0), 1.0, "{}", ease.name());
        assert_eq!(ease.apply(-0.5), 0.0, "{}", ease.name());
        assert_eq!(ease.apply(1.5), 1.0, "{}", ease.name());
    }
}

#[test]
fn each_in_curve_mirrors_its_out_curve() {
    let pairs = [
        (Ease::InQuad, Ease::OutQuad),
        (Ease::InCubic, Ease::OutCubic),
        (Ease::InQuart, Ease::OutQuart),
        (Ease::InQuint, Ease::OutQuint),
        (Ease::InSine, Ease::OutSine),
        (Ease::InExpo, Ease::OutExpo),
    ];
    for (ease_in, ease_out) in pairs {
        for i in 0..=1000 {
            let p = i as f64 / 1000.0;
            let mirrored = 1.0 - ease_out.apply(1.0 - p);
            assert!(
                (ease_in.apply(p) - mirrored).abs() <= TOLERANCE,
                "{} at {p} against {}",
                ease_in.name(),
                ease_out.name()
            );
        }
    }
}

#[test]
fn each_in_out_curve_passes_through_one_half_at_the_middle() {
    for ease in [
        Ease::InOutQuad,
        Ease::InOutCubic,
        Ease::InOutQuart,
        Ease::InOutQuint,
        Ease::InOutSine,
        Ease::InOutExpo,
    ] {
        assert!((ease.apply(0.5) - 0.5).abs() <= TOLERANCE, "{}", ease.name());
    }
}

#[test]
fn named_curves_never_go_backwards() {
    for ease in Ease::ALL {
        let mut last = 0.0;
        for i in 0..=1000 {
            let v = ease.apply(i as f64 / 1000.0);
            assert!(v >= last, "{} at {i}", ease.name());
            last = v;
        }
    }
}

#[test]
fn out_back_of_1_7_rises_above_1_before_the_end_and_is_exactly_1_at_the_end() {
    let ease = Ease::OutBack(1.7);
    assert!((1..1000).any(|i| ease.apply(i as f64 / 1000.0) > 1.0));
    assert_eq!(ease.apply(1.0), 1.0);
}

#[test]
fn out_back_of_0_never_rises_above_1_and_is_out_cubic() {
    let ease = Ease::OutBack(0.0);
    for i in 0..=10000 {
        let p = i as f64 / 10000.0;
        assert!(ease.apply(p) <= 1.0, "at {p}");
        assert!((ease.apply(p) - Ease::OutCubic.apply(p)).abs() <= TOLERANCE, "at {p}");
    }
}

#[test]
fn out_back_of_1_7_peaks_at_its_worked_value_and_place() {
    let ease = Ease::OutBack(1.7);
    // 1 + 4n³ ÷ (27 (n + 1)²) at p = 1 − 2n ÷ (3 (n + 1)): 1 + 19.652 ÷ 196.83 at p = 47/81.
    let largest = 1.0 + 19.652 / 196.83;
    assert!((ease.apply(47.0 / 81.0) - largest).abs() <= TOLERANCE);
    for i in 0..=100000 {
        let p = i as f64 / 100000.0;
        assert!(ease.apply(p) <= largest + TOLERANCE, "at {p}");
    }
}

#[test]
fn out_back_peaks_where_the_spec_works_it_out_for_other_n() {
    for n in [0.5, 1.0, 2.5, 4.0] {
        let ease = Ease::OutBack(n);
        let place = 1.0 - 2.0 * n / (3.0 * (n + 1.0));
        let largest = 1.0 + 4.0 * n * n * n / (27.0 * (n + 1.0) * (n + 1.0));
        assert!((ease.apply(place) - largest).abs() <= TOLERANCE, "n = {n}");
        assert!(ease.apply(place - 1e-3) < ease.apply(place) && ease.apply(place + 1e-3) < ease.apply(place));
    }
}

#[test]
fn names_round_trip() {
    for ease in Ease::ALL {
        assert_eq!(Ease::from_name(ease.name()), Some(ease));
    }
    assert_eq!(Ease::from_name("bounce"), None);
    assert_eq!(Ease::from_name("out-back"), None);
}
