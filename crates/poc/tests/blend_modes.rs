//! How an element is mixed with what is under it: every blend of
//! `docs/spec/native-notation.md`, section 16, against its formula, and on both renderers.

use poc::cpu::Cpu;
use poc::gpu::Gpu;
use poc::model::{Blend, Composition};
use poc::scene::Evaluator;
use poc::text::Shaper;
use poc::{layout, xml, Backend};
use std::path::Path;

type Colour = [f64; 3];

/// What is under the element, and the element.
const UNDER: Colour = [64.0 / 255.0, 96.0 / 255.0, 128.0 / 255.0];
const OVER: Colour = [192.0 / 255.0, 128.0 / 255.0, 32.0 / 255.0];

fn lightness(c: Colour) -> f64 {
    0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2]
}

fn saturation(c: Colour) -> f64 {
    c.iter().copied().fold(0.0, f64::max) - c.iter().copied().fold(1.0, f64::min)
}

/// The colour `c` with the lightness `l`, brought back between 0 and 1 without changing it.
fn with_lightness(c: Colour, l: f64) -> Colour {
    let shift = l - lightness(c);
    let c = c.map(|channel| channel + shift);
    let (l, least, most) = (
        lightness(c),
        c.iter().copied().fold(f64::MAX, f64::min),
        c.iter().copied().fold(f64::MIN, f64::max),
    );
    let c = if least < 0.0 {
        c.map(|channel| l + (channel - l) * l / (l - least))
    } else {
        c
    };
    if most > 1.0 {
        c.map(|channel| l + (channel - l) * (1.0 - l) / (most - l))
    } else {
        c
    }
}

/// The colour `c` with the saturation `s`: its least channel 0, its greatest `s`.
fn with_saturation(c: Colour, s: f64) -> Colour {
    let (least, most) = (
        c.iter().copied().fold(f64::MAX, f64::min),
        c.iter().copied().fold(f64::MIN, f64::max),
    );
    if most == least {
        return [0.0; 3];
    }
    c.map(|channel| (channel - least) * s / (most - least))
}

fn hard_light(s: f64, b: f64) -> f64 {
    if s <= 0.5 {
        b * 2.0 * s
    } else {
        let s = 2.0 * s - 1.0;
        s + b - s * b
    }
}

/// The standard's formula for one blend, from the element's colour and the colour under it.
fn expected(blend: Blend, s: Colour, b: Colour) -> Colour {
    let each = |mix: fn(f64, f64) -> f64| [mix(s[0], b[0]), mix(s[1], b[1]), mix(s[2], b[2])];
    match blend {
        Blend::Normal => s,
        Blend::Multiply => each(|s, b| s * b),
        Blend::Screen => each(|s, b| s + b - s * b),
        Blend::Overlay => each(|s, b| hard_light(b, s)),
        Blend::Darken => each(f64::min),
        Blend::Lighten => each(f64::max),
        Blend::ColorDodge => each(|s, b| {
            if b == 0.0 {
                0.0
            } else if s == 1.0 {
                1.0
            } else {
                (b / (1.0 - s)).min(1.0)
            }
        }),
        Blend::ColorBurn => each(|s, b| {
            if b == 1.0 {
                1.0
            } else if s == 0.0 {
                0.0
            } else {
                1.0 - ((1.0 - b) / s).min(1.0)
            }
        }),
        Blend::HardLight => each(hard_light),
        Blend::SoftLight => each(|s, b| {
            if s <= 0.5 {
                b - (1.0 - 2.0 * s) * b * (1.0 - b)
            } else {
                let d = if b <= 0.25 { ((16.0 * b - 12.0) * b + 4.0) * b } else { b.sqrt() };
                b + (2.0 * s - 1.0) * (d - b)
            }
        }),
        Blend::Difference => each(|s, b| (s - b).abs()),
        Blend::Exclusion => each(|s, b| s + b - 2.0 * s * b),
        Blend::Hue => with_lightness(with_saturation(s, saturation(b)), lightness(b)),
        Blend::Saturation => with_lightness(with_saturation(b, saturation(s)), lightness(b)),
        Blend::Color => with_lightness(s, lightness(b)),
        Blend::Luminosity => with_lightness(b, lightness(s)),
    }
}

/// One square of 16 by 16 for each blend, side by side, over the colour under them.
fn squares(opacity: &str) -> Composition {
    let mut source = format!(
        r##"<composition width="{}" height="16" fps="10" duration="1" background="#406080">"##,
        16 * Blend::NAMED.len()
    );
    for (index, (name, _)) in Blend::NAMED.iter().enumerate() {
        source += &format!(
            r##"<box left="{}" top="0" width="16" height="16" background="#c08020" mix-blend-mode="{name}" {opacity}/>"##,
            16 * index
        );
    }
    source += "</composition>";
    let mut composition = xml::parse(&source, Path::new(".")).expect("the scene is valid");
    layout::lay_out(&mut composition, &mut Shaper::new(), &[]);
    composition
}

/// The colour in the middle of each square.
fn drawn(backend: &mut dyn Backend, composition: &Composition) -> Vec<[u8; 3]> {
    backend
        .draw(0, &Evaluator::new(composition).frame(0).ops)
        .expect("the frame is drawn");
    let pixels = backend.pixels().expect("the frame is read");
    let width = 16 * Blend::NAMED.len();
    (0..Blend::NAMED.len())
        .map(|index| {
            let at = (8 * width + 16 * index + 8) * 4;
            [pixels[at], pixels[at + 1], pixels[at + 2]]
        })
        .collect()
}

fn near(found: [u8; 3], expected: Colour, by: f64) -> bool {
    (0..3).all(|channel| (f64::from(found[channel]) - expected[channel] * 255.0).abs() <= by)
}

#[test]
fn every_blend_gives_the_colour_of_its_formula() {
    let composition = squares("");
    let found = drawn(&mut Cpu::new(&composition, &[]), &composition);
    for ((name, blend), found) in Blend::NAMED.iter().zip(found) {
        let expected = expected(*blend, OVER, UNDER);
        assert!(
            near(found, expected, 2.0),
            "{name}: {found:?} and not {:?}",
            expected.map(|channel| channel * 255.0)
        );
    }
}

#[test]
fn a_partly_clear_element_is_mixed_by_its_alpha() {
    let composition = squares(r#"opacity="0.35""#);
    let found = drawn(&mut Cpu::new(&composition, &[]), &composition);
    for ((name, blend), found) in Blend::NAMED.iter().zip(found) {
        let whole = expected(*blend, OVER, UNDER);
        let expected: Colour = std::array::from_fn(|channel| UNDER[channel] + 0.35 * (whole[channel] - UNDER[channel]));
        assert!(
            near(found, expected, 2.0),
            "{name}: {found:?} and not {:?}",
            expected.map(|channel| channel * 255.0)
        );
    }
}

#[test]
fn the_card_draws_every_blend_as_the_cpu_renderer_does() {
    for opacity in ["", r#"opacity="0.35""#] {
        let composition = squares(opacity);
        let on_cpu = drawn(&mut Cpu::new(&composition, &[]), &composition);
        let mut gpu = Gpu::new(&composition, &[]).expect("a graphics adapter is there");
        let on_gpu = drawn(&mut gpu, &composition);
        for (((name, _), cpu), gpu) in Blend::NAMED.iter().zip(on_cpu).zip(on_gpu) {
            assert!(
                (0..3).all(|channel| cpu[channel].abs_diff(gpu[channel]) <= 3),
                "{name} {opacity}: {cpu:?} on the CPU, {gpu:?} on the card"
            );
        }
    }
}

#[test]
fn the_colour_blend_tints_grey_without_changing_how_light_it_is() {
    let source = r##"<composition width="32" height="16" fps="10" duration="1" background="#808080">
  <box left="0" top="0" width="16" height="16" background="#6f7458" mix-blend-mode="color"/>
</composition>"##;
    let mut composition = xml::parse(source, Path::new(".")).expect("the scene is valid");
    layout::lay_out(&mut composition, &mut Shaper::new(), &[]);
    let mut cpu = Cpu::new(&composition, &[]);
    cpu.draw(0, &Evaluator::new(&composition).frame(0).ops).expect("the frame is drawn");
    let pixels = cpu.pixels().expect("the frame is read");
    let tinted = [pixels[8 * 32 * 4 + 32], pixels[8 * 32 * 4 + 33], pixels[8 * 32 * 4 + 34]];
    let light = lightness(tinted.map(|channel| f64::from(channel) / 255.0));
    assert!((light - 128.0 / 255.0).abs() < 0.01, "as light as the grey under it: {tinted:?}");
    assert!(tinted[1] > tinted[0] && tinted[0] > tinted[2], "olive: {tinted:?}");
}
