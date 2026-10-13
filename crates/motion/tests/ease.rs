use motion::*;

#[test]
fn every_curve_is_exact_at_both_ends() {
    for ease in Ease::ALL {
        assert_eq!(ease.apply(0.0), 0.0, "{}", ease.name());
        assert_eq!(ease.apply(1.0), 1.0, "{}", ease.name());
    }
}

#[test]
fn in_out_curves_are_half_way_at_half_time() {
    for ease in [Ease::InOutQuad, Ease::InOutCubic, Ease::InOutQuart] {
        assert_eq!(ease.apply(0.5), 0.5, "{}", ease.name());
    }
}

#[test]
fn curves_follow_their_formulas() {
    assert_eq!(Ease::InQuad.apply(0.25), 0.0625);
    assert_eq!(Ease::OutQuad.apply(0.25), 0.4375);
    assert_eq!(Ease::InCubic.apply(0.5), 0.125);
    assert_eq!(Ease::OutCubic.apply(0.5), 0.875);
    assert_eq!(Ease::InQuart.apply(0.5), 0.0625);
    assert_eq!(Ease::OutQuart.apply(0.5), 0.9375);
    assert_eq!(Ease::InOutQuad.apply(0.25), 0.125);
    assert_eq!(Ease::InOutCubic.apply(0.75), 0.9375);
    assert_eq!(Ease::InOutQuart.apply(0.25), 0.03125);
}

#[test]
fn curves_never_go_backwards() {
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
fn names_round_trip() {
    for ease in Ease::ALL {
        assert_eq!(Ease::from_name(ease.name()), Some(ease));
    }
    assert_eq!(Ease::from_name("out-bounce"), None);
}
