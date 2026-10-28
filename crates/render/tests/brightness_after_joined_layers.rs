//! Layers that are partly clear are joined before the brightness counts (`docs/spec/notation.md`, section 7.6; the layer of
//! an element with a brightness, `docs/spec/render-command.md`, section 4.5, rule 11). The expected values are worked by
//! hand here, not taken from the renderer: a white of alpha 64 over `#202020` gives
//! 64 + (1 - 64/255) x 32 = 87.97, shown as 88 (`#585858`); brightened by 2 that is 175.9, shown as 176 (`#b0b0b0`). A
//! brightness applied to each child before the join would give 64 + (1 - 64/255) x 64 = 112 instead.
//! The card's test needs a graphics card that frx7 can draw with: reference machine only.

mod common;

use common::project_with;
use draw::{Picture, Renderer};
use draw_cpu::CpuRenderer;
use draw_gpu::CardRenderer;
use project::Project;
use render::Scene;

/// Frame 0 of a composition of 200 by 100 pixels on black holding `inside`, drawn by `renderer`.
fn drawn(name: &str, inside: &str, renderer: &mut dyn Renderer) -> Picture {
    let text = format!(
        "<composition version=\"1\" width=\"200\" height=\"100\" fps=\"10\" duration=\"1\" background=\"#000000\">\n{inside}\n</composition>\n"
    );
    let folder = project_with("brightness-after-joined-layers", name, &text);
    let scene = Scene::from_project(&Project::open(&folder).unwrap()).unwrap_or_else(|reports| panic!("{reports:?}"));
    scene.picture(0, renderer).unwrap()
}

/// A dark grey and a quarter of white over it, in one box, with the brightness the test gives.
fn partly_clear_layers(brightness: &str) -> String {
    format!(
        r##"<box left="0" top="0" width="200" height="100" brightness="{brightness}">
<box left="0" top="0" width="200" height="100" background="#202020"/>
<box left="0" top="0" width="200" height="100" background="#ffffff40"/>
</box>"##
    )
}

fn joined_then_brightened(renderer: &mut dyn Renderer, within: f64) {
    // Joined: 88 at every pixel of the box (`#585858`). Brightness 2 after the join: 176 (`#b0b0b0`).
    let plain = drawn("joined", &partly_clear_layers("1"), renderer);
    let brightened = drawn("brightened", &partly_clear_layers("2"), renderer);
    for (x, y) in [(0, 0), (100, 50), (199, 99)] {
        for channel in 0..3 {
            let joined = f64::from(plain.pixel(x, y).unwrap()[channel]);
            assert!((joined - 88.0).abs() <= within, "({x}, {y}) joined: {joined}");
            let after = f64::from(brightened.pixel(x, y).unwrap()[channel]);
            assert!((after - 176.0).abs() <= within, "({x}, {y}) brightened: {after}");
        }
        assert_eq!(brightened.pixel(x, y).unwrap()[3], 255);
    }
}

#[test]
fn partly_clear_layers_are_joined_before_the_brightness_counts_on_the_processor() {
    let mut renderer = CpuRenderer;
    // The processor's picture is held to the byte: the values are the worked ones, with no tolerance.
    let brightened = drawn("processor-exact", &partly_clear_layers("2"), &mut renderer);
    assert_eq!(brightened.pixel(100, 50).unwrap(), [176, 176, 176, 255]);
    let plain = drawn("processor-exact-plain", &partly_clear_layers("1"), &mut renderer);
    assert_eq!(plain.pixel(100, 50).unwrap(), [88, 88, 88, 255]);
    joined_then_brightened(&mut renderer, 0.0);
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_partly_clear_layers_are_joined_before_the_brightness_counts_on_the_card() {
    let mut card = CardRenderer::new().unwrap_or_else(|error| panic!("the card could not be started: {error}"));
    // The card agrees with the processor within 4 levels of 255 (`PLAN.md`, section 5.2).
    joined_then_brightened(&mut card, 4.0);
}
