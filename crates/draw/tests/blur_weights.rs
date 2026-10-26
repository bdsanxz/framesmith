//! The weights of a blur (`docs/spec/render-command.md`, section 4.5, rule 11) against the shares of a Gaussian worked
//! here by numerical integration of its density, and the standard deviation in the frame under a map.

use draw::{blur_weights, Filter, Map};

/// The share of a Gaussian of standard deviation `sigma` between `first` and `last`: its density integrated by Simpson's
/// rule over 2000 steps.
fn share(first: f64, last: f64, sigma: f64) -> f64 {
    let density = |t: f64| (-(t * t) / (2.0 * sigma * sigma)).exp() / (sigma * (2.0 * std::f64::consts::PI).sqrt());
    let steps = 2000;
    let h = (last - first) / f64::from(steps);
    let mut sum = density(first) + density(last);
    for i in 1..steps {
        sum += density(first + f64::from(i) * h) * if i % 2 == 1 { 4.0 } else { 2.0 };
    }
    sum * h / 3.0
}

#[test]
fn each_weight_is_the_share_of_a_gaussian_over_its_pixel() {
    for sigma in [0.3, 1.0, 2.5, 4.0, 11.0] {
        let weights = blur_weights(sigma, 1000);
        let reach = (4.0 * sigma).ceil() as i64;
        assert_eq!(weights.len() as i64, 2 * reach + 1, "σ {sigma}: from −⌈4σ⌉ to ⌈4σ⌉");
        let total = share(-(reach as f64) - 0.5, reach as f64 + 0.5, sigma);
        for (index, weight) in weights.iter().enumerate() {
            let k = index as f64 - reach as f64;
            let wanted = share(k - 0.5, k + 0.5, sigma) / total;
            assert!((weight - wanted).abs() < 1e-9, "σ {sigma}, k {k}: {weight} against {wanted}");
        }
        assert!((weights.iter().sum::<f64>() - 1.0).abs() < 1e-12);
    }
}

#[test]
fn the_weights_reach_no_further_than_asked() {
    assert_eq!(blur_weights(10.0, 7).len(), 15);
    assert_eq!(blur_weights(0.0, 100), vec![1.0]);
    assert_eq!(blur_weights(-2.0, 100), vec![1.0]);
}

#[test]
fn the_blur_in_the_frame_is_scaled_by_the_square_root_of_the_area_the_map_scales() {
    let filter = Filter {
        blur: 3.0,
        brightness: 1.0,
    };
    assert_eq!(filter.sigma(Map::IDENTITY), 3.0);
    let doubled = Map {
        a: 2.0,
        b: 0.0,
        c: 0.0,
        d: 2.0,
        e: 5.0,
        f: 7.0,
    };
    assert!((filter.sigma(doubled) - 6.0).abs() < 1e-12);
    let wide = Map {
        a: 4.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };
    assert!((filter.sigma(wide) - 6.0).abs() < 1e-12);
    assert!(
        Filter::NONE.is_none()
            && !filter.is_none()
            && !Filter {
                blur: 0.0,
                brightness: 0.5
            }
            .is_none()
    );
}
