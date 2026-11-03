//! The blur and the brightness of a picture move as the other properties do, and an easing that carries either below 0 holds
//! it at 0 (`docs/spec/motion.md`, Properties and The value of a property at a time). The expected values below are worked by
//! hand from the easing's formula, not taken from the engine.

use motion::*;
use timebase::Time;

const NO_VALUES: Values = [None; PROP_COUNT];

fn values(list: &[(Prop, f64)]) -> Values {
    let mut values = NO_VALUES;
    for &(prop, value) in list {
        values[prop.index()] = Some(value);
    }
    values
}

fn secs(text: &str) -> Time {
    Time::parse(text).unwrap()
}

fn from_to(start: &[(Prop, f64)], end: &[(Prop, f64)], at: &str, duration: &str, ease: Ease) -> Tween {
    Tween {
        target: "#a".into(),
        kind: Kind::FromTo,
        from: values(start),
        to: values(end),
        at: secs(at),
        duration: secs(duration),
        ease,
    }
}

fn set(list: &[(Prop, f64)], at: &str) -> Tween {
    Tween {
        target: "#a".into(),
        kind: Kind::Set,
        from: NO_VALUES,
        to: values(list),
        at: secs(at),
        duration: Time::ZERO,
        ease: Ease::Linear,
    }
}

fn timeline(tweens: &[Tween]) -> Timeline {
    let targets: Vec<Vec<usize>> = tweens.iter().map(|_| vec![0]).collect();
    Timeline::build(tweens, &targets, 30)
}

fn at(timeline: &Timeline, prop: Prop, secs_text: &str) -> f64 {
    timeline.elements[0].tracks[prop.index()]
        .as_ref()
        .expect("the property is animated")
        .value(secs(secs_text))
}

/// The out-back easing of the motion spec with overshoot `n`, worked by hand: `q = p - 1`, `q² ((n + 1) q + n) + 1`.
fn out_back(p: f64, n: f64) -> f64 {
    let q = p - 1.0;
    q * q * ((n + 1.0) * q + n) + 1.0
}

#[test]
fn a_blur_tween_from_20_to_0_is_10_at_its_middle() {
    let tl = timeline(&[from_to(&[(Prop::Blur, 20.0)], &[(Prop::Blur, 0.0)], "0", "1", Ease::Linear)]);
    assert_eq!(at(&tl, Prop::Blur, "0"), 20.0);
    assert_eq!(at(&tl, Prop::Blur, "0.5"), 10.0);
    assert_eq!(at(&tl, Prop::Blur, "1"), 0.0);
    assert!(tl.elements[0].tracks[Prop::Opacity.index()].is_none());
}

#[test]
fn a_brightness_tween_from_1_to_0_is_half_at_its_middle() {
    let tl = timeline(&[from_to(
        &[(Prop::Brightness, 1.0)],
        &[(Prop::Brightness, 0.0)],
        "0",
        "1",
        Ease::Linear,
    )]);
    assert_eq!(at(&tl, Prop::Brightness, "0"), 1.0);
    assert_eq!(at(&tl, Prop::Brightness, "0.5"), 0.5);
    assert_eq!(at(&tl, Prop::Brightness, "1"), 0.0);
}

#[test]
fn a_set_on_blur_is_sharp_before_its_time_and_blurred_from_it() {
    let tl = timeline(&[set(&[(Prop::Blur, 10.0)], "0.5")]);
    assert_eq!(at(&tl, Prop::Blur, "0.4"), 0.0);
    assert_eq!(at(&tl, Prop::Blur, "0.5"), 10.0);
    assert_eq!(at(&tl, Prop::Blur, "2"), 10.0);
}

#[test]
fn an_out_back_easing_holds_blur_at_0_where_it_carries_the_value_below_0() {
    // From 4 to 0 over one second with out-back(1.7): the value is 4 - 4 e(p). It is below 0 from about p = 0.370 on, and it is
    // held at 0 there; before that it is the eased value.
    let tl = timeline(&[from_to(&[(Prop::Blur, 4.0)], &[(Prop::Blur, 0.0)], "0", "1", Ease::OutBack(1.7))]);
    let before = 11.0 / 30.0;
    let expected_before = 4.0 - 4.0 * out_back(before, 1.7);
    assert!((expected_before - 0.016).abs() < 1e-3, "worked value {expected_before}");
    assert!((at(&tl, Prop::Blur, "0.3667") - expected_before).abs() < 1e-3);
    for time in ["0.5", "0.5667", "0.9667", "1"] {
        assert_eq!(at(&tl, Prop::Blur, time), 0.0, "held at 0 at {time} s");
    }
    assert!(4.0 - 4.0 * out_back(17.0 / 30.0, 1.7) < 0.0, "the easing is below 0 here");
}

#[test]
fn an_out_back_easing_holds_brightness_at_0_where_it_carries_the_value_below_0() {
    let tl = timeline(&[from_to(
        &[(Prop::Brightness, 1.0)],
        &[(Prop::Brightness, 0.0)],
        "0",
        "1",
        Ease::OutBack(1.7),
    )]);
    assert!((at(&tl, Prop::Brightness, "0.3667") - (1.0 - out_back(11.0 / 30.0, 1.7))).abs() < 1e-3);
    for time in ["0.5", "0.5667", "0.9667", "1"] {
        assert_eq!(at(&tl, Prop::Brightness, time), 0.0, "held at 0 at {time} s");
    }
}

#[test]
fn a_property_with_no_tween_keeps_its_own_value() {
    let tl = timeline(&[from_to(&[(Prop::X, 0.0)], &[(Prop::X, 10.0)], "0", "1", Ease::Linear)]);
    assert!(tl.elements[0].tracks[Prop::Blur.index()].is_none());
    assert!(tl.elements[0].tracks[Prop::Brightness.index()].is_none());
    assert_eq!(Prop::Blur.own_value(), 0.0);
    assert_eq!(Prop::Brightness.own_value(), 1.0);
    assert_eq!(Prop::from_name("blur"), Some(Prop::Blur));
    assert_eq!(Prop::from_name("brightness"), Some(Prop::Brightness));
}
