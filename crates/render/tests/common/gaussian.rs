//! A Gaussian of our own for the tests of blur (`docs/spec/render-command.md`, section 4.5, rule 11): its density integrated
//! numerically, so that the values the tests expect come from no code of the renderers.

/// The share of a Gaussian of standard deviation `sigma` that lies below `x`: its density integrated from `-12σ` by
/// Simpson's rule over 4000 steps.
pub fn below(x: f64, sigma: f64) -> f64 {
    let (from, to) = (-12.0 * sigma, x.clamp(-12.0 * sigma, 12.0 * sigma));
    if to <= from {
        return 0.0;
    }
    let density = |t: f64| (-(t * t) / (2.0 * sigma * sigma)).exp() / (sigma * (2.0 * std::f64::consts::PI).sqrt());
    let steps = 4000;
    let h = (to - from) / f64::from(steps);
    let mut sum = density(from) + density(to);
    for i in 1..steps {
        sum += density(from + f64::from(i) * h) * if i % 2 == 1 { 4.0 } else { 2.0 };
    }
    sum * h / 3.0
}

/// The share of the span from `first` to `last` that a Gaussian of `sigma` centred at `centre` lays there.
pub fn share(first: f64, last: f64, centre: f64, sigma: f64) -> f64 {
    below(last - centre, sigma) - below(first - centre, sigma)
}
