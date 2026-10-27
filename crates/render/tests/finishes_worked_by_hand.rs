//! A torn edge and grain (`docs/spec/finishes.md`, section 3) drawn through the scene on both renderers. Every expected
//! pixel is worked from the spec with the noise of a second working of the standard (`crates/draw/tests/reference_noise`),
//! read at the pixel's centre taken back to the element's box, or worked by hand on the lattice.

mod common;
#[path = "../../draw/tests/reference_noise/mod.rs"]
mod reference_noise;

use common::project_with;
use draw::Picture;
use draw_cpu::CpuRenderer;
use draw_gpu::CardRenderer;
use project::Project;
use reference_noise::Reference;
use render::Scene;

const RED: [u8; 3] = [255, 0, 0];
const BLUE: [u8; 3] = [0, 0, 255];

enum On {
    Processor,
    Card,
}

fn composition(width: u32, height: u32, background: &str, inside: &str) -> String {
    format!(
        "<composition version=\"1\" width=\"{width}\" height=\"{height}\" fps=\"10\" duration=\"0.1\" background=\"{background}\">\n{inside}\n</composition>\n"
    )
}

fn drawn(name: &str, composition: &str, on: &On) -> Picture {
    // Each renderer has a folder of its own: the two tests of one name run side by side and must not clear each other's.
    let renderer = match on {
        On::Processor => "processor",
        On::Card => "card",
    };
    let folder = project_with("finishes-worked-by-hand", &format!("{name}-{renderer}"), composition);
    let scene = Scene::from_project(&Project::open(&folder).unwrap()).unwrap_or_else(|reports| panic!("{name}: {reports:?}"));
    match on {
        On::Processor => scene.picture(0, &mut CpuRenderer).unwrap(),
        On::Card => {
            let mut card = CardRenderer::new().unwrap_or_else(|error| panic!("the card could not be started: {error}"));
            scene.picture(0, &mut card).unwrap()
        }
    }
}

/// A red box 40 by 40 at (10, 10) over a blue frame of 60 by 60, torn by `14 noise(0.035 3 seed)`.
fn torn(seed: i64) -> String {
    composition(
        60,
        60,
        "#0000ff",
        &format!("<box left=\"10\" top=\"10\" width=\"40\" height=\"40\" background=\"#ff0000\" displace=\"14 noise(0.035 3 {seed})\"/>"),
    )
}

/// Whether the pixel (`column`, `row`) of the torn box shows red, by section 3, rule 1: the noise at the centre less the
/// box's corner, d = 14 (n - 0.5), the place (column + round(d), row + round(d)), red when that lies in the box.
fn red_by_the_spec(reference: &Reference, column: i64, row: i64) -> bool {
    let n = reference.value([column as f64 + 0.5 - 10.0, row as f64 + 0.5 - 10.0]);
    let step = (14.0 * (n - 0.5)).round() as i64;
    let (x, y) = (column + step, row + step);
    (10..50).contains(&x) && (10..50).contains(&y)
}

fn holds_the_torn_edge(on: &On) {
    let picture = drawn("torn-9", &torn(9), on);
    let reference = Reference::new(0.035, 3, 9);
    let mut torn_rows = 0;
    for row in 0..60 {
        for column in 0..60 {
            let got = picture.pixel(column, row).unwrap();
            let wanted = if red_by_the_spec(&reference, i64::from(column), i64::from(row)) {
                RED
            } else {
                BLUE
            };
            // A hard edge: every pixel is the fill or what is under it, never a mix.
            assert_eq!(got[..3], wanted, "({column}, {row})");
            // Away from the straight edge by more than half the amount, the picture is unchanged.
            let inside = (17..43).contains(&column) && (17..43).contains(&row);
            let outside = !(3..57).contains(&column) || !(3..57).contains(&row);
            if inside {
                assert_eq!(got[..3], RED, "inside, ({column}, {row})");
            }
            if outside {
                assert_eq!(got[..3], BLUE, "outside, ({column}, {row})");
            }
        }
    }
    // Along the top edge, the red starts at a row that leaves the straight line at y = 10 by at most 7, and not everywhere at
    // 10: the edge is torn.
    for column in 20..40 {
        let first = (0..60).find(|row| picture.pixel(column, *row).unwrap()[..3] == RED).unwrap();
        assert!((3..=17).contains(&first), "column {column}: the red starts at row {first}");
        if first != 10 {
            torn_rows += 1;
        }
    }
    assert!(torn_rows > 5, "only {torn_rows} columns of the top edge leave the straight line");
}

#[test]
fn a_torn_edge_takes_each_pixel_whole_from_where_the_noise_says_on_the_processor() {
    holds_the_torn_edge(&On::Processor);
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_a_torn_edge_takes_each_pixel_whole_from_where_the_noise_says_on_the_card() {
    holds_the_torn_edge(&On::Card);
}

/// A red box 20 by 20 at (20, 20) scaled by 2 about its centre (30, 30), torn by `6 noise(0.11 2 4)`: the pixel's centre is
/// taken back by x = (X - 30) / 2 + 30 to the box, and the distance d = 6 (n - 0.5) is scaled by 2 before it is rounded.
fn holds_a_scaled_torn_edge(on: &On) {
    let picture = drawn(
        "torn-scaled",
        &composition(
            60,
            60,
            "#0000ff",
            "<box left=\"20\" top=\"20\" width=\"20\" height=\"20\" background=\"#ff0000\" scale=\"2\" displace=\"6 noise(0.11 2 4)\"/>",
        ),
        on,
    );
    let reference = Reference::new(0.11, 2, 4);
    let mut moved = 0;
    for row in 0..60_i64 {
        for column in 0..60_i64 {
            let back = |value: i64| (value as f64 + 0.5 - 30.0) / 2.0 + 30.0 - 20.0;
            let n = reference.value([back(column), back(row)]);
            let step = (2.0 * 6.0 * (n - 0.5)).round() as i64;
            let red = (10..50).contains(&(column + step)) && (10..50).contains(&(row + step));
            moved += usize::from(step != 0);
            let got = picture.pixel(column as u32, row as u32).unwrap();
            assert_eq!(got[..3], if red { RED } else { BLUE }, "({column}, {row})");
        }
    }
    assert!(moved > 600, "only {moved} pixels are moved");
}

#[test]
fn a_torn_edge_is_scaled_with_its_element_on_the_processor() {
    holds_a_scaled_torn_edge(&On::Processor);
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_a_torn_edge_is_scaled_with_its_element_on_the_card() {
    holds_a_scaled_torn_edge(&On::Card);
}

/// A grey box (128, 128, 128) 30 by 20 at (5, 5) over a black frame of 40 by 30 with `grain`: each premultiplied value is
/// times g = clamp(SLOPE n + OFFSET, 0, 1), so over black the colour is floor(128 g + 0.5).
fn grained(name: &str, grain: &str, extra: &str, on: &On) -> Picture {
    drawn(
        name,
        &composition(
            40,
            30,
            "#000000",
            &format!("<box left=\"5\" top=\"5\" width=\"30\" height=\"20\" background=\"#808080\" grain=\"{grain}\"{extra}/>"),
        ),
        on,
    )
}

fn near(got: u8, wanted: u8, on: &On) -> bool {
    match on {
        On::Processor => got == wanted,
        On::Card => got.abs_diff(wanted) <= 1,
    }
}

fn holds_grain_on_the_lattice(on: &On) {
    // With a frequency of 2 every centre (i + 0.5, j + 0.5) is on the lattice, so n = 128 / 255 and g = n: the grey is
    // floor(128 · 128 / 255 + 0.5) = floor(64.75) = 64 everywhere in the box. At an opacity of 0.5, floor(64 · 0.5 + 0.5) = 32.
    for (name, extra, wanted) in [("lattice", "", 64), ("lattice-faded", " opacity=\"0.5\"", 32)] {
        let picture = grained(name, "noise(2 3 7) 1 0", extra, on);
        for row in 0..30 {
            for column in 0..40 {
                let inside = (5..35).contains(&column) && (5..25).contains(&row);
                let got = picture.pixel(column, row).unwrap();
                let level = if inside { wanted } else { 0 };
                assert!(
                    got[..3].iter().all(|value| near(*value, level, on)),
                    "{name}: ({column}, {row}): {got:?}"
                );
            }
        }
    }
}

#[test]
fn grain_on_the_lattice_keeps_half_the_alpha_on_the_processor() {
    holds_grain_on_the_lattice(&On::Processor);
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_grain_on_the_lattice_keeps_half_the_alpha_on_the_card() {
    holds_grain_on_the_lattice(&On::Card);
}

fn holds_grain_at_worked_places(on: &On) {
    // A slope of 4 and an offset of -1.5 take the share below 0 where n < 0.375 and above 1 where n > 0.625, so both ends of
    // the hold are met.
    let picture = grained("slope-4", "noise(0.21 2 5) 4 -1.5", "", on);
    let reference = Reference::new(0.21, 2, 5);
    let (mut clear, mut full) = (0, 0);
    for row in 5..25_u32 {
        for column in 5..35_u32 {
            let n = reference.value([f64::from(column) + 0.5 - 5.0, f64::from(row) + 0.5 - 5.0]);
            let share = (4.0 * n - 1.5).clamp(0.0, 1.0);
            let wanted = (128.0 * share + 0.5).floor() as u8;
            clear += usize::from(wanted == 0);
            full += usize::from(wanted == 128);
            let got = picture.pixel(column, row).unwrap();
            assert!(
                got[..3].iter().all(|value| near(*value, wanted, on)),
                "({column}, {row}): {got:?} against {wanted}"
            );
        }
    }
    assert!(clear > 20 && full > 20, "{clear} clear and {full} full pixels");
}

#[test]
fn grain_multiplies_the_alpha_by_the_held_share_on_the_processor() {
    holds_grain_at_worked_places(&On::Processor);
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_grain_multiplies_the_alpha_by_the_held_share_on_the_card() {
    holds_grain_at_worked_places(&On::Card);
}

fn holds_the_seed(on: &On) {
    let first = drawn("seed-9-first", &torn(9), on);
    let again = drawn("seed-9-again", &torn(9), on);
    let other = drawn("seed-10", &torn(10), on);
    assert_eq!(first.pixels(), again.pixels(), "the same seed gives the same picture");
    assert_ne!(first.pixels(), other.pixels(), "another seed gives another picture");
}

#[test]
fn the_same_seed_gives_the_same_picture_and_another_seed_another_on_the_processor() {
    holds_the_seed(&On::Processor);
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_the_same_seed_gives_the_same_picture_and_another_seed_another_on_the_card() {
    holds_the_seed(&On::Card);
}
