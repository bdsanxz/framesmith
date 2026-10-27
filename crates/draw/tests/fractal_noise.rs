//! The fractal noise of `docs/spec/finishes.md`, section 1: worked by hand on the lattice, its seeds as the standard takes
//! them, and its values at chosen points equal to a second working of the standard's procedure (`reference_noise`).

mod reference_noise;

use draw::Noise;
use reference_noise::Reference;

fn noise(frequency: f64, octaves: u32, seed: i64) -> Noise {
    Noise { frequency, octaves, seed }
}

/// Points of the box, with fractions of every kind, near 0 and far from it.
fn points() -> Vec<(f64, f64)> {
    let mut points = Vec::new();
    for j in 0..23 {
        for i in 0..31 {
            points.push((f64::from(i) * 3.7 + 0.5, f64::from(j) * 5.3 + 0.5));
        }
    }
    points.extend([(0.5, 0.5), (1000.25, 77.75), (12345.5, 999.5), (-3.25, 8.5)]);
    points
}

#[test]
fn on_the_lattice_every_octave_is_zero_and_the_value_is_the_middle_step() {
    // Where frequency times the point is whole, the corner's offsets are 0 and the smooth steps are 0, so each octave gives
    // g · (0, 0) = 0, the sum is 0, and n = (0 · 255 + 255) / 2 / 255 = 0.5, kept as floor(127.5 + 0.5) / 255 = 128 / 255.
    for seed in [1, 9, 12345] {
        for octaves in [1, 3, 8] {
            let field = noise(2.0, octaves, seed).field();
            for (x, y) in [(0.0, 0.0), (0.5, 1.5), (3.0, 7.5), (40.5, 0.0)] {
                assert_eq!(field.value(x, y), 128.0 / 255.0, "seed {seed}, {octaves} octaves, at ({x}, {y})");
            }
        }
    }
}

#[test]
fn the_values_equal_the_second_working_of_the_standard() {
    for (frequency, octaves, seed) in [(0.035, 3, 9), (0.9, 2, 5), (0.2, 1, 1), (0.05, 8, 424242), (1.3, 4, 2147483646)] {
        let field = noise(frequency, octaves, seed).field();
        let reference = Reference::new(frequency, octaves, seed);
        let mut seen = std::collections::BTreeSet::new();
        for (x, y) in points() {
            let wanted = reference.value([x, y]);
            assert_eq!(field.value(x, y), wanted, "noise({frequency} {octaves} {seed}) at ({x}, {y})");
            seen.insert(reference.level([x, y]));
        }
        // The points are not all on one value: the comparison covers a spread of the field.
        assert!(
            seen.len() > 20,
            "noise({frequency} {octaves} {seed}) gives only {} levels",
            seen.len()
        );
    }
}

#[test]
fn a_seed_not_above_zero_stands_for_the_seed_one_above_its_size() {
    for (written, meant) in [(-7, 8), (0, 1), (-1, 2)] {
        let (a, b) = (noise(0.1, 3, written).field(), noise(0.1, 3, meant).field());
        for (x, y) in points() {
            assert_eq!(a.value(x, y), b.value(x, y), "seed {written} against {meant} at ({x}, {y})");
        }
    }
    // A seed above m - 1 is m - 1.
    let (a, b) = (noise(0.1, 3, 2147483647 + 5).field(), noise(0.1, 3, 2147483646).field());
    for (x, y) in points() {
        assert_eq!(a.value(x, y), b.value(x, y));
    }
}

#[test]
fn another_seed_gives_another_field() {
    let (a, b) = (noise(0.1, 3, 9).field(), noise(0.1, 3, 10).field());
    let differing = points().into_iter().filter(|(x, y)| a.value(*x, *y) != b.value(*x, *y)).count();
    assert!(differing > points().len() / 2, "{differing} of {} points differ", points().len());
}

#[test]
fn the_same_seed_gives_the_same_field_every_time() {
    let (a, b) = (noise(0.1, 3, 9).field(), noise(0.1, 3, 9).field());
    for (x, y) in points() {
        assert_eq!(a.value(x, y), b.value(x, y), "at ({x}, {y})");
    }
}
