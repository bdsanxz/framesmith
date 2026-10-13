//! An opening written by the author as a shader: what the card draws with it, and what is said
//! of one it cannot take. These tests need a graphics adapter.
#![cfg(feature = "gpu")]

use poc::cpu::Cpu;
use poc::gpu::Gpu;
use poc::model::Composition;
use poc::scene::Evaluator;
use poc::text::Shaper;
use poc::{layout, xml, Backend};
use std::path::{Path, PathBuf};

const WIDTH: usize = 200;
const RED: [u8; 3] = [255, 0, 0];
const BLUE: [u8; 3] = [0, 0, 255];
const GREEN: [u8; 3] = [0, 255, 0];

/// A new, empty folder for one test, under the build's own temporary folder.
fn folder(name: &str) -> PathBuf {
    let folder = Path::new(env!("CARGO_TARGET_TMPDIR")).join("shader-opening").join(name);
    if folder.exists() {
        std::fs::remove_dir_all(&folder).expect("the old test folder is removed");
    }
    std::fs::create_dir_all(&folder).expect("the test folder is made");
    folder
}

/// A blue sheet with a green strip at its left end, over a red frame; the sheet has these
/// attributes. The scene is read from `folder`.
fn sheet(folder: &Path, attributes: &str) -> Composition {
    let source = format!(
        r##"<composition width="200" height="100" fps="10" duration="1" background="#ff0000">
  <box left="0" top="0" width="200" height="100" background="#0000ff" {attributes}>
    <box position="absolute" left="0" top="0" width="40" height="100" background="#00ff00"/>
  </box>
</composition>"##
    );
    let mut composition = xml::parse(&source, folder).expect("the scene is valid");
    layout::lay_out(&mut composition, &mut Shaper::new(), &[]);
    composition
}

fn drawn(backend: &mut dyn Backend, composition: &Composition) -> Vec<u8> {
    backend
        .draw(0, &Evaluator::new(composition).frame(0).ops)
        .expect("the frame is drawn");
    backend.pixels().expect("the frame is read").to_vec()
}

fn on_card(composition: &Composition) -> Vec<u8> {
    drawn(&mut Gpu::new(composition, &[]).expect("a graphics adapter is there"), composition)
}

fn at(pixels: &[u8], x: usize, y: usize) -> [u8; 3] {
    let place = (y * WIDTH + x) * 4;
    [pixels[place], pixels[place + 1], pixels[place + 2]]
}

#[test]
fn a_shader_that_says_what_the_wipe_says_draws_the_wipe() {
    let folder = folder("as-wipe");
    let text = "fn effect(place: vec2<f32>, size: vec2<f32>, progress: f32) -> vec4<f32> {\n    return picture(place) * open_at(progress, place.x);\n}\n";
    std::fs::write(folder.join("mine.wgsl"), text).expect("the shader is written");
    let written = on_card(&sheet(&folder, r#"transition="mine.wgsl" reveal="0.5""#));
    let built_in = sheet(&folder, r#"transition="wipe" reveal="0.5""#);
    assert_eq!(written, on_card(&built_in), "the same picture as the wipe that is built in");
    // And that picture is the CPU renderer's wipe, to a level.
    let on_cpu = drawn(&mut Cpu::new(&built_in, &[]), &built_in);
    let most = written.iter().zip(&on_cpu).map(|(card, cpu)| card.abs_diff(*cpu)).max();
    assert!(most <= Some(1), "the card's wipe is {most:?} levels from the CPU renderer's");
    assert_eq!(at(&written, 20, 50), GREEN);
    assert_eq!(at(&written, 80, 50), BLUE);
    assert_eq!(at(&written, 160, 50), RED);
}

#[test]
fn a_shader_may_move_the_picture_and_is_given_the_place_the_size_and_the_progress() {
    let folder = folder("moved");
    // The picture comes in from the right: it is shown a share of its width to the right of
    // its place, the share falling with the progress. Below the middle nothing is shown, and
    // the size is checked by making the top left 50 pixels square yellow.
    let text = r"
fn effect(place: vec2<f32>, size: vec2<f32>, progress: f32) -> vec4<f32> {
    if place.y > 0.5 {
        return vec4<f32>(0.0);
    }
    if place.x * size.x < 50.0 && place.y * size.y < 50.0 && size.x == 200.0 && size.y == 100.0 {
        return vec4<f32>(1.0, 1.0, 0.0, 1.0);
    }
    return picture(place - vec2<f32>(1.0 - progress, 0.0));
}
";
    std::fs::write(folder.join("slide.wgsl"), text).expect("the shader is written");
    let pixels = on_card(&sheet(&folder, r#"transition="slide.wgsl" reveal="0.25""#));
    // Three quarters of the width to the right: the green strip, flat from 0 to 40, is seen
    // from 150 to 190, with blue after it and the frame's red before it.
    assert_eq!(at(&pixels, 25, 25), [255, 255, 0]);
    assert_eq!(at(&pixels, 100, 25), RED);
    assert_eq!(at(&pixels, 170, 25), GREEN);
    assert_eq!(at(&pixels, 195, 25), BLUE);
    assert_eq!(at(&pixels, 170, 75), RED);
    // Fully open, the element is drawn without the shader.
    let open = on_card(&sheet(&folder, r#"transition="slide.wgsl" reveal="1""#));
    assert_eq!(at(&open, 25, 25), GREEN);
    assert_eq!(at(&open, 170, 75), BLUE);
}

#[test]
fn a_shader_the_card_does_not_take_stops_the_renderer_with_its_file_and_line() {
    let folder = folder("faulty");
    let text = "fn effect(place: vec2<f32>, size: vec2<f32>, progress: f32) -> vec4<f32> {\n    return picture(place) * nothing_of_that_name;\n}\n";
    std::fs::write(folder.join("broken.wgsl"), text).expect("the shader is written");
    let composition = sheet(&folder, r#"transition="broken.wgsl" reveal="0.5""#);
    let reason = Gpu::new(&composition, &[]).err().expect("the renderer does not start");
    assert!(
        reason.contains("the transition `broken.wgsl` is not a shader the card takes") && reason.contains("nothing_of_that_name"),
        "{reason}"
    );
    // The compiler counts the lines as the file does: the fault is on its second line.
    assert!(reason.contains("wgsl:2:"), "{reason}");
    // A file with no function of the name that is asked for.
    std::fs::write(folder.join("empty.wgsl"), "// nothing here\n").expect("the shader is written");
    let composition = sheet(&folder, r#"transition="empty.wgsl" reveal="0.5""#);
    let reason = Gpu::new(&composition, &[]).err().expect("the renderer does not start");
    assert!(
        reason.contains("the transition `empty.wgsl`") && reason.contains("effect"),
        "{reason}"
    );
}

#[test]
fn a_shader_on_an_element_that_is_itself_blurred_is_refused_when_it_is_drawn() {
    let folder = folder("blurred");
    let text = "fn effect(place: vec2<f32>, size: vec2<f32>, progress: f32) -> vec4<f32> {\n    return picture(place);\n}\n";
    std::fs::write(folder.join("plain.wgsl"), text).expect("the shader is written");
    let composition = sheet(&folder, r#"transition="plain.wgsl" reveal="0.5" blur="3""#);
    let mut gpu = Gpu::new(&composition, &[]).expect("a graphics adapter is there");
    let reason = gpu
        .draw(0, &Evaluator::new(&composition).frame(0).ops)
        .expect_err("the frame is refused");
    assert!(reason.contains("a transition written as a shader"), "{reason}");
}

/// Runs `poc check` on a scene of `folder` with these attributes on its sheet, and gives
/// whether it succeeded, what it printed and what it complained of.
fn checked(folder: &Path, attributes: &str) -> (bool, String, String) {
    let scene = format!(
        "<composition width=\"200\" height=\"100\" fps=\"10\" duration=\"1\" background=\"#ff0000\">\n  <box left=\"0\" top=\"0\" width=\"100\" height=\"100\" background=\"#00ff00\"/>\n  <box left=\"0\" top=\"0\" width=\"200\" height=\"100\" background=\"#0000ff\" {attributes}/>\n</composition>\n"
    );
    std::fs::write(folder.join("composition.xml"), scene).expect("the composition is written");
    let run = std::process::Command::new(env!("CARGO_BIN_EXE_poc"))
        .arg("check")
        .arg(folder.join("composition.xml"))
        .output()
        .expect("poc runs");
    (
        run.status.success(),
        String::from_utf8_lossy(&run.stdout).to_string(),
        String::from_utf8_lossy(&run.stderr).to_string(),
    )
}

#[test]
fn check_says_that_a_composition_with_a_shader_opening_needs_the_gpu_renderer() {
    let folder = folder("check");
    let text = "fn effect(place: vec2<f32>, size: vec2<f32>, progress: f32) -> vec4<f32> {\n    return picture(place);\n}\n";
    std::fs::write(folder.join("mine.wgsl"), text).expect("the shader is written");
    let (passed, said, complained) = checked(&folder, r#"transition="mine.wgsl" reveal="0.5""#);
    assert!(passed, "{complained}");
    assert!(said.contains("is a valid composition: 10 frames, 200x100, 10 fps"), "{said}");
    // The sheet is on the third line of the file.
    assert!(
        said.contains("line 3: the transition `mine.wgsl` is a shader: this composition can be rendered only with the GPU renderer, and its picture is not checked against the CPU renderer"),
        "{said}"
    );
    // A composition without one is not said to need anything.
    let (passed, said, _) = checked(&folder, r#"transition="wipe" reveal="0.5""#);
    assert!(passed);
    assert!(said.contains("is a valid composition") && !said.contains("GPU renderer"), "{said}");
}

#[test]
fn check_fails_on_a_shader_the_card_does_not_take_and_on_a_composition_that_is_not_valid() {
    let folder = folder("check-faulty");
    std::fs::write(folder.join("broken.wgsl"), "fn effect() -> f32 {\n    return 1.0;\n}\n").expect("the shader is written");
    let (passed, said, complained) = checked(&folder, r#"transition="broken.wgsl" reveal="0.5""#);
    assert!(!passed, "{said}");
    assert!(
        complained.contains("this composition cannot be rendered on this machine")
            && complained.contains("the transition `broken.wgsl` is not a shader the card takes"),
        "{complained}"
    );
    let (passed, _, complained) = checked(&folder, r#"transition="missing.wgsl""#);
    assert!(!passed);
    assert!(
        complained.contains("is not a valid composition") && complained.contains("line 3") && complained.contains("missing.wgsl"),
        "{complained}"
    );
}
