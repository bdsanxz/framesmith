//! What is done to the finished picture of an element: a torn edge, uneven ink and a shadow
//! that follows it, as `docs/spec/native-notation.md`, section 17, says, on both renderers.

use poc::cpu::Cpu;
use poc::gpu::Gpu;
use poc::model::{Composition, NoiseField};
use poc::noise::Noise;
use poc::scene::Evaluator;
use poc::text::Shaper;
use poc::{layout, xml, Backend};
use std::path::Path;

const WIDTH: usize = 200;
const HEIGHT: usize = 140;

/// A white frame with `inside` on it.
fn scene(inside: &str) -> Composition {
    let source = format!(
        r##"<composition width="{WIDTH}" height="{HEIGHT}" fps="10" duration="1" background="#ffffff">
{inside}
</composition>"##
    );
    let mut composition = xml::parse(&source, Path::new(".")).expect("the scene is valid");
    layout::lay_out(&mut composition, &mut Shaper::new(), &[]);
    composition
}

fn drawn(backend: &mut dyn Backend, composition: &Composition) -> Vec<[u8; 3]> {
    backend
        .draw(0, &Evaluator::new(composition).frame(0).ops)
        .expect("the frame is drawn");
    let pixels = backend.pixels().expect("the frame is read");
    pixels
        .as_chunks::<4>()
        .0
        .iter()
        .map(|pixel| [pixel[0], pixel[1], pixel[2]])
        .collect()
}

fn on_cpu(inside: &str) -> Vec<[u8; 3]> {
    let composition = scene(inside);
    drawn(&mut Cpu::new(&composition, &[]), &composition)
}

fn problems(inside: &str) -> Vec<String> {
    let source = format!("<composition width=\"64\" height=\"64\" fps=\"10\" duration=\"1\">\n{inside}\n</composition>");
    match xml::parse(&source, Path::new(".")) {
        Ok(_) => Vec::new(),
        Err(problems) => problems
            .iter()
            .map(|problem| format!("{}: {}", problem.line, problem.message))
            .collect(),
    }
}

#[test]
fn the_noise_is_the_standards_number_for_number() {
    // Taken from the prototype whose picture a browser's noise matched pixel for pixel.
    let points = [(0.5, 0.5), (17.5, 3.5), (120.5, 64.5), (301.25, 7.75), (999.5, 1500.5)];
    let known: [(i32, f64, u32, [u8; 5]); 4] = [
        (9, 0.035, 3, [118, 77, 131, 150, 144]),
        (5, 0.9, 2, [100, 114, 106, 105, 71]),
        (12, 0.03, 3, [134, 118, 158, 112, 125]),
        (0, 0.05, 1, [132, 102, 149, 176, 123]),
    ];
    for (seed, frequency, octaves, expected) in known {
        let (noise, field) = (Noise::new(seed), NoiseField { frequency, octaves, seed });
        let found = points.map(|(x, y)| (noise.at(&field, x, y) * 255.0).round() as u8);
        assert_eq!(found, expected, "seed {seed}, frequency {frequency}, {octaves} octaves");
    }
    // A seed that is not above 0 is, by the standard, the seed one above its size.
    let field = NoiseField {
        frequency: 0.11,
        octaves: 4,
        seed: 0,
    };
    let (below, above) = (Noise::new(-7), Noise::new(8));
    assert!(points.iter().all(|(x, y)| below.at(&field, *x, *y) == above.at(&field, *x, *y)));
    assert!(points
        .iter()
        .any(|(x, y)| below.at(&field, *x, *y) != Noise::new(7).at(&field, *x, *y)));
}

/// Whether the pixel of the frame at (x, y) lies in the box of 100 by 60 at (40, 30).
fn in_box(x: i64, y: i64) -> bool {
    (40..140).contains(&x) && (30..90).contains(&y)
}

#[test]
fn a_displaced_box_takes_every_pixel_from_as_far_on_as_its_noise_says() {
    let found = on_cpu(r##"<box left="40" top="30" width="100" height="60" background="#ff0000" displace="14 noise(0.035 3 9)"/>"##);
    let (noise, field) = (
        Noise::new(9),
        NoiseField {
            frequency: 0.035,
            octaves: 3,
            seed: 9,
        },
    );
    let (mut moved, mut wrong) = (0, Vec::new());
    for y in 0..HEIGHT as i64 {
        for x in 0..WIDTH as i64 {
            let n = noise.at(&field, x as f64 + 0.5 - 40.0 + 0.5, y as f64 + 0.5 - 30.0 + 0.5);
            let by = (14.0 * (n - 0.5) + 0.5).floor() as i64;
            let red = in_box(x + by, y + by);
            moved += usize::from(red != in_box(x, y));
            let expected = if red { [255, 0, 0] } else { [255, 255, 255] };
            if found[y as usize * WIDTH + x as usize] != expected {
                wrong.push((x, y));
            }
        }
    }
    assert!(moved > 200, "the edge is torn: {moved} pixels are not where the plain box has them");
    assert!(
        wrong.is_empty(),
        "{} pixels are not the torn box's, the first at {:?}",
        wrong.len(),
        wrong.first()
    );
}

#[test]
fn the_tear_is_scaled_and_moves_with_the_element() {
    // The box of 50 by 30 at (65, 45), twice its size about its middle, covers (40, 30) to (140, 90).
    let found =
        on_cpu(r##"<box left="65" top="45" width="50" height="30" scale="2" background="#ff0000" displace="14 noise(0.035 3 9)"/>"##);
    let (noise, field) = (
        Noise::new(9),
        NoiseField {
            frequency: 0.035,
            octaves: 3,
            seed: 9,
        },
    );
    let mut wrong = Vec::new();
    for y in 0..HEIGHT as i64 {
        for x in 0..WIDTH as i64 {
            let own = ((x as f64 + 0.5 - 40.0) / 2.0, (y as f64 + 0.5 - 30.0) / 2.0);
            let n = noise.at(&field, own.0 + 0.5, own.1 + 0.5);
            let by = (2.0 * 14.0 * (n - 0.5) + 0.5).floor() as i64;
            let expected = if in_box(x + by, y + by) { [255, 0, 0] } else { [255, 255, 255] };
            if found[y as usize * WIDTH + x as usize] != expected {
                wrong.push((x, y));
            }
        }
    }
    assert!(
        wrong.is_empty(),
        "{} pixels are not the torn box's, the first at {:?}",
        wrong.len(),
        wrong.first()
    );
}

#[test]
fn grain_takes_ink_away_where_the_noise_is_high() {
    let found = on_cpu(r##"<box left="40" top="30" width="100" height="60" background="#000000" grain="noise(0.9 2 5) -1.7 1.75"/>"##);
    let (noise, field) = (
        Noise::new(5),
        NoiseField {
            frequency: 0.9,
            octaves: 2,
            seed: 5,
        },
    );
    let (mut thin, mut wrong) = (0, Vec::new());
    for y in 0..HEIGHT as i64 {
        for x in 0..WIDTH as i64 {
            let n = noise.at(&field, x as f64 + 0.5 - 40.0 + 0.5, y as f64 + 0.5 - 30.0 + 0.5);
            let kept = if in_box(x, y) { (-1.7 * n + 1.75).clamp(0.0, 1.0) } else { 0.0 };
            thin += usize::from(in_box(x, y) && kept < 0.9);
            let expected = 255.0 * (1.0 - kept);
            let pixel = found[y as usize * WIDTH + x as usize];
            if pixel.iter().any(|channel| (f64::from(*channel) - expected).abs() > 1.0) {
                wrong.push((x, y, pixel, expected));
            }
        }
    }
    assert!(thin > 300, "the ink is uneven: {thin} pixels kept less than nine tenths of it");
    assert!(
        wrong.is_empty(),
        "{} pixels have another ink, the first {:?}",
        wrong.len(),
        wrong.first()
    );
}

#[test]
fn a_shadow_lies_under_the_picture_of_an_element_and_its_children() {
    // A clear box with a red child: the shadow is the child's, not the box's.
    let inside = |blur: u32| {
        format!(
            r##"<box left="40" top="30" width="100" height="60" drop-shadow="10 14 {blur} #000000">
  <box left="20" top="10" width="40" height="30" background="#ff0000"/>
</box>"##
        )
    };
    let at = |pixels: &[[u8; 3]], x: usize, y: usize| pixels[y * WIDTH + x];
    let hard = on_cpu(&inside(0));
    // The child covers (60, 40) to (100, 70); its shadow (70, 54) to (110, 84).
    assert_eq!(at(&hard, 80, 55), [255, 0, 0], "the picture lies over its shadow");
    assert_eq!(at(&hard, 105, 75), [0, 0, 0], "the shadow, where the picture is not");
    assert_eq!(at(&hard, 75, 80), [0, 0, 0]);
    assert_eq!(at(&hard, 111, 75), [255, 255, 255], "past the shadow's edge");
    assert_eq!(at(&hard, 65, 75), [255, 255, 255], "left of the shadow, under the picture");
    assert_eq!(at(&hard, 50, 35), [255, 255, 255], "the clear box has no shadow of its own");

    let soft = on_cpu(&inside(12));
    let grey = |x: usize, y: usize| at(&soft, x, y)[0];
    assert_eq!(at(&soft, 80, 55), [255, 0, 0]);
    // Across the shadow's right edge at x = 110: half the ink at the edge, more inside, less outside.
    assert!(
        (110..=145).contains(&grey(109, 72)) && (110..=145).contains(&grey(110, 72)),
        "{} {}",
        grey(109, 72),
        grey(110, 72)
    );
    let across: Vec<u8> = (98..124).map(|x| grey(x, 72)).collect();
    assert!(across.windows(2).all(|pair| pair[0] <= pair[1]), "lighter outwards: {across:?}");
    assert!(across[0] < 40 && across[across.len() - 1] > 240, "{across:?}");
}

#[test]
fn the_opacity_of_an_element_comes_after_its_shadow() {
    let found =
        on_cpu(r##"<box left="40" top="30" width="60" height="40" background="#ff0000" opacity="0.5" drop-shadow="20 20 0 #000000"/>"##);
    let at = |x: usize, y: usize| found[y * WIDTH + x];
    // Under the picture the shadow does not show through.
    assert!(at(80, 60)[0] == 255 && at(80, 60)[1].abs_diff(128) <= 1, "{:?}", at(80, 60));
    assert!(
        at(110, 80).iter().all(|channel| channel.abs_diff(128) <= 1),
        "the shadow, half clear: {:?}",
        at(110, 80)
    );
}

#[test]
fn the_card_shows_the_finishes_as_the_cpu_renderer_draws_them() {
    let composition = scene(
        r##"<box left="20" top="20" width="70" height="50" background="#c02020" rotation="8" displace="10 noise(0.05 2 3)" drop-shadow="6 8 6 #00000080"/>
<box left="110" top="60" width="70" height="50" background="#102060" grain="noise(0.4 2 7) -1.2 1.4" opacity="0.8"/>"##,
    );
    let on_cpu = drawn(&mut Cpu::new(&composition, &[]), &composition);
    let mut gpu = Gpu::new(&composition, &[]).expect("a graphics adapter is there");
    let on_gpu = drawn(&mut gpu, &composition);
    let apart = on_cpu
        .iter()
        .zip(&on_gpu)
        .filter(|(cpu, gpu)| (0..3).any(|channel| cpu[channel].abs_diff(gpu[channel]) > 3))
        .count();
    assert_eq!(apart, 0, "pixels that differ between the two renderers");
    assert!(on_cpu.iter().any(|pixel| *pixel != [255, 255, 255]), "something was drawn");
}

#[test]
fn a_finish_that_cannot_be_read_is_refused_with_its_line() {
    let found = problems(
        r##"<box width="8" height="8" displace="0 noise(0.035 3 9)"/>
<box width="8" height="8" displace="14 noise(0.035 9 9)"/>
<box width="8" height="8" displace="14 0.035"/>
<box width="8" height="8" grain="noise(0.9 2 5) -1.7"/>
<box width="8" height="8" grain="noise(0 2 5) -1.7 1.75"/>
<box width="8" height="8" grain="noise(0.9 2 5.5) -1.7 1.75"/>
<box width="8" height="8" drop-shadow="10 14 #000000"/>
<box width="8" height="8" drop-shadow="1 2 3 #000000, 4 5 6 #000000"/>"##,
    );
    let displace = |written: &str| format!("`displace=\"{written}\"` is not an amount above 0 and a noise: `14 noise(0.035 3 9)`");
    let grain = |written: &str| format!("`grain=\"{written}\"` is not a noise, a slope and an offset: `noise(0.9 2 5) -1.7 1.75`");
    let shadow = |written: &str| format!("`drop-shadow=\"{written}\"` is not one shadow: x, y, blur and a colour");
    let expected = vec![
        format!("2: {}", displace("0 noise(0.035 3 9)")),
        format!("3: {}", displace("14 noise(0.035 9 9)")),
        format!("4: {}", displace("14 0.035")),
        format!("5: {}", grain("noise(0.9 2 5) -1.7")),
        format!("6: {}", grain("noise(0 2 5) -1.7 1.75")),
        format!("7: {}", grain("noise(0.9 2 5.5) -1.7 1.75")),
        format!("8: {}", shadow("10 14 #000000")),
        format!("9: {}", shadow("1 2 3 #000000, 4 5 6 #000000")),
    ];
    assert_eq!(found, expected);
}

#[test]
fn a_shape_keeps_the_shadow_of_its_outlines() {
    let composition = scene(
        r##"<shape left="40" top="30" width="60" height="40" view-box="0 0 60 40" d="M0 0H60V40H0Z" color="#ff0000" drop-shadow="20 20 0 #000000"/>"##,
    );
    assert!(
        !composition.root.children[0].look.finish.any(),
        "the shadow is the shape's own, not a finish of its picture"
    );
}
