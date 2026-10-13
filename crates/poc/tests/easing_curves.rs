//! The easing curves the proof of concept adds to the motion engine's ten: fifth power, sine
//! and exponential, each slow at its start, at its end, or at both.

use poc::anim::Curve;

fn curve(name: &str) -> Curve {
    Curve::from_name(name).unwrap_or_else(|| panic!("`{name}` is a curve"))
}

const NAMES: [&str; 9] = [
    "in-quint",
    "out-quint",
    "in-out-quint",
    "in-sine",
    "out-sine",
    "in-out-sine",
    "in-expo",
    "out-expo",
    "in-out-expo",
];

#[test]
fn every_curve_is_exact_at_its_ends_and_never_goes_back() {
    for name in NAMES {
        let curve = curve(name);
        assert_eq!(curve.apply(0.0), 0.0, "{name} at 0");
        assert_eq!(curve.apply(1.0), 1.0, "{name} at 1");
        let mut before = 0.0;
        for step in 1..=1000 {
            let value = curve.apply(f64::from(step) / 1000.0);
            assert!(
                value >= before && value <= 1.0,
                "{name} at {step}/1000 gives {value} after {before}"
            );
            before = value;
        }
        // Outside its range a tween is at one of its ends.
        assert_eq!(curve.apply(-0.5), 0.0, "{name} before its start");
        assert_eq!(curve.apply(1.5), 1.0, "{name} after its end");
    }
}

#[test]
fn the_curves_have_the_values_their_definitions_give() {
    let close = |name: &str, p: f64, expected: f64| {
        let found = curve(name).apply(p);
        assert!((found - expected).abs() < 1e-12, "{name} at {p} gives {found}, not {expected}");
    };
    close("in-quint", 0.5, 0.03125);
    close("out-quint", 0.5, 0.96875);
    close("in-out-quint", 0.25, 16.0 * 0.25_f64.powi(5));
    close("in-out-quint", 0.5, 0.5);
    close("in-out-quint", 0.75, 1.0 - 0.5_f64.powi(5) / 2.0);
    close("in-sine", 0.5, 1.0 - std::f64::consts::FRAC_PI_4.cos());
    close("out-sine", 0.5, std::f64::consts::FRAC_PI_4.sin());
    close("in-out-sine", 0.25, (1.0 - std::f64::consts::FRAC_PI_4.cos()) / 2.0);
    close("in-out-sine", 0.5, 0.5);
    close("in-expo", 0.5, 2.0_f64.powi(-5));
    close("out-expo", 0.5, 1.0 - 2.0_f64.powi(-5));
    close("out-expo", 0.1, 0.5);
    close("in-out-expo", 0.25, 2.0_f64.powi(-5) / 2.0);
    close("in-out-expo", 0.5, 0.5);
    close("in-out-expo", 0.75, (2.0 - 2.0_f64.powi(-5)) / 2.0);
}

#[test]
fn a_curve_slow_at_its_end_is_the_one_slow_at_its_start_turned_round() {
    for shape in ["quint", "sine", "expo"] {
        let (slow_start, slow_end) = (curve(&format!("in-{shape}")), curve(&format!("out-{shape}")));
        for step in 0..=20 {
            let p = f64::from(step) / 20.0;
            assert!(
                (slow_end.apply(p) - (1.0 - slow_start.apply(1.0 - p))).abs() < 1e-12,
                "{shape} at {p}"
            );
        }
    }
}

#[test]
fn a_name_that_is_not_a_curve_is_not_taken() {
    for name in ["in-quintic", "sine", "out-", "expo-out", "in-out-back(2)", "in-out-"] {
        assert!(Curve::from_name(name).is_none(), "`{name}` is taken");
    }
}
