//! The GPU renderer against the CPU renderer, which defines the correct picture. These tests
//! need a graphics adapter.
#![cfg(feature = "gpu")]

use poc::cpu::Cpu;
use poc::gpu::Gpu;
use poc::model::{Composition, Kind, Node};
use poc::scene::Evaluator;
use poc::style::{ColorOp, Grade};
use poc::text::Shaper;
use poc::{layout, xml, yuv, Backend};
use std::path::Path;

fn load(source: &str) -> Composition {
    let mut composition = xml::parse(source, Path::new(".")).expect("the scene is valid");
    layout::lay_out(&mut composition, &mut Shaper::new(), &[]);
    composition
}

fn drawn(backend: &mut dyn Backend, composition: &Composition, frame: u64) -> Vec<u8> {
    let evaluator = Evaluator::new(composition);
    backend.draw(frame, &evaluator.frame(frame).ops).expect("the frame is drawn");
    backend.pixels().expect("the frame is read").to_vec()
}

/// The mean difference of the colour channels, and the share of pixels with a channel that
/// differs by more than 48 of 255.
fn difference(a: &[u8], b: &[u8]) -> (f64, f64) {
    assert_eq!(a.len(), b.len());
    let mut total = 0_u64;
    let mut far = 0_usize;
    for (one, other) in a.chunks(4).zip(b.chunks(4)) {
        let apart = [0, 1, 2].map(|channel| u64::from(one[channel].abs_diff(other[channel])));
        total += apart.iter().sum::<u64>();
        far += usize::from(apart.iter().any(|apart| *apart > 48));
    }
    let pixels = a.len() / 4;
    (total as f64 / (pixels * 3) as f64, far as f64 / pixels as f64)
}

/// Draws `frame` of `source` on both renderers and checks that the pictures agree.
fn agree(source: &str, frame: u64) {
    let composition = load(source);
    let on_cpu = drawn(&mut Cpu::new(&composition, &[]), &composition, frame);
    let mut gpu = Gpu::new(&composition, &[]).expect("a graphics adapter is there");
    let on_gpu = drawn(&mut gpu, &composition, frame);
    assert!(on_cpu.chunks(4).any(|pixel| pixel[..3] != [0, 0, 0]), "the frame is not empty");
    let (mean, far) = difference(&on_cpu, &on_gpu);
    assert!(
        mean < 1.0 && far < 0.005,
        "mean difference {mean:.3} of 255, {:.3}% of the pixels far apart",
        far * 100.0
    );
}

#[test]
fn fills_gradients_layers_and_corners_agree() {
    agree(
        r##"<composition width="256" height="128" fps="10" duration="1" background="#101418">
  <box left="0" top="0" right="0" bottom="0" background="radial-gradient(ellipse 62% 78% at 50% 42%, #06050400 38%, #ffffff80 72%, #ffffffdb 100%)"/>
  <box left="8" top="8" width="80" height="48" background="linear-gradient(90deg, #ff0000 0%, #0000ff 100%)" border-radius="12"
       border="3 #ffffff" box-shadow="4 6 12 #000000cc"/>
  <box left="100" top="8" width="64" height="48" background="radial-gradient(circle, #ffffffcc 3, #00000000 5), #203040" background-size="16 16, auto"/>
  <box left="176" top="8" width="64" height="48" background="linear-gradient(#00ff00 0%, #00ff00 100%), #302010"
       background-size="8 100%, auto" background-position="50% 0, 0 0" background-repeat="no-repeat, repeat"/>
  <box left="8" top="70" width="64" height="48" overflow="hidden" border-radius="8">
    <rect left="16" top="16" width="96" height="96" background="#ffcc00"/>
  </box>
  <box left="90" top="70" width="64" height="48" background="#808080"/>
  <box left="110" top="80" width="64" height="30" background="#ff8000" mix-blend-mode="multiply"/>
  <box left="180" top="70" width="64" height="48" opacity="0.5">
    <rect left="0" top="0" width="40" height="40" background="#00c0ff"/>
    <rect left="20" top="10" width="40" height="38" background="#ff00c0"/>
  </box>
</composition>"##,
        0,
    );
}

#[test]
fn text_outlines_and_shapes_agree() {
    agree(
        r##"<composition width="256" height="128" fps="10" duration="1" background="#101418">
  <text left="8" top="4" font-family="Arial" font-weight="700" font-size="40" color="#ffffff" text-shadow="0 2 6 #000000cc">Frame</text>
  <text left="140" top="4" font-family="Arial" font-size="40" color="#203040" text-stroke="2 #ffcc00">by</text>
  <shape left="8" top="64" width="56" height="56" d="M 4 4 L 20 4 L 20 20 L 4 20 Z" view-box="0 0 24 24" color="#00c0ff"/>
  <shape left="80" top="64" width="56" height="56" d="M 4 12 C 4 2, 20 2, 20 12 L 20 20" view-box="0 0 24 24" color="#ffffff" stroke-width="3"/>
  <shape left="150" top="64" width="56" height="56" d="M 6 6 L 18 6 L 18 18 L 6 18 Z" view-box="0 0 24 24" color="#ffffff"
         drop-shadow="2 3 3 #000000cc"/>
</composition>"##,
        0,
    );
}

#[test]
fn a_counting_number_and_a_line_drawn_in_part_and_in_dashes_agree() {
    let source = r##"<composition width="256" height="128" fps="10" duration="1" background="#101418">
  <text left="8" top="4" font-family="Arial" font-weight="700" font-size="40" color="#ffffff" count="0" count-format="0,0.0">
    <tween at="0" duration="1" ease="linear" to-count="2500"/>
  </text>
  <shape left="8" top="64" width="56" height="56" d="M 12 2 A 10 10 0 1 1 11.99 2" view-box="0 0 24 24" color="#00c0ff" stroke-width="3">
    <tween at="0" duration="1" ease="linear" from-draw="0" to-draw="1"/>
  </shape>
  <shape left="80" top="64" width="160" height="56" d="M 4 12 C 40 2, 60 22, 96 12" view-box="0 0 100 24" color="#ffffff" stroke-width="2"
         dash="6 5" draw="0.7"/>
</composition>"##;
    agree(source, 3);
    agree(source, 8);
}

#[test]
fn a_blurred_group_and_motion_agree() {
    let source = r##"<composition width="256" height="128" fps="10" duration="2" background="#101418">
  <box left="20" top="20" width="100" height="80" blur="4">
    <rect left="0" top="0" width="60" height="50" background="#ff4040"/>
    <text left="10" top="30" font-family="Arial" font-size="32" color="#ffffff">blur</text>
  </box>
  <rect left="140" top="20" width="40" height="40" background="#40ff80">
    <tween at="0" duration="1" ease="out-cubic" from-x="-20" from-opacity="0" from-scale="0.5" to-x="40" to-opacity="1" to-scale="1.2"/>
    <tween at="0" duration="1" from-rotation="0" to-rotation="30"/>
  </rect>
</composition>"##;
    agree(source, 0);
    agree(source, 5);
    agree(source, 12);
}

#[test]
fn a_brightened_group_agrees() {
    agree(
        r##"<composition width="128" height="64" fps="10" duration="1" background="#101418">
  <box left="8" top="8" width="64" height="48" background="linear-gradient(90deg, #ffffff30 2, #00000000 2), #203040"
       background-size="8 100%, auto" brightness="2.5" border-radius="6">
    <rect left="20" top="12" width="24" height="24" background="#402010"/>
  </box>
  <box left="80" top="8" width="40" height="48" background="#c08040" brightness="0.4" mix-blend-mode="screen"/>
</composition>"##,
        0,
    );
}

#[test]
fn a_slant_the_font_does_not_have_leans_to_the_right_on_the_gpu_too() {
    let mut composition = load(
        r##"<composition width="64" height="64" fps="10" duration="1" background="#000000">
  <text left="24" top="0" font-family="Arial" font-size="48" color="#ffffff">l</text>
</composition>"##,
    );
    for run in &mut composition.root.children[0].runs {
        run.skew = Some(0.25);
    }
    let mut gpu = Gpu::new(&composition, &[]).expect("a graphics adapter is there");
    let pixels = drawn(&mut gpu, &composition, 0);
    let centre = |y: usize| -> Option<f64> {
        let row = &pixels[y * 64 * 4..(y + 1) * 64 * 4];
        let ink: f64 = row.chunks(4).map(|pixel| f64::from(pixel[0])).sum();
        let weighted: f64 = row.chunks(4).enumerate().map(|(x, pixel)| x as f64 * f64::from(pixel[0])).sum();
        (ink > 255.0).then(|| weighted / ink)
    };
    let inked: Vec<(usize, f64)> = (0..64).filter_map(|y| centre(y).map(|x| (y, x))).collect();
    let (top, bottom) = (
        inked.first().expect("the letter is drawn"),
        inked.last().expect("the letter is drawn"),
    );
    let lean = (top.1 - bottom.1) / (bottom.0 - top.0) as f64;
    assert!(
        (0.2..=0.3).contains(&lean),
        "the top leans right by a quarter of the height: {lean}"
    );
}

/// The planes of a picture of our own making: brightness rises to the right in bands, and
/// the colour turns from the top left corner to the bottom right one.
fn planes(width: usize, height: usize) -> Vec<u8> {
    let mut planes = vec![0_u8; yuv::size(width, height)];
    let (luma, chroma) = planes.split_at_mut(width * height);
    let (blue, red) = chroma.split_at_mut(width / 2 * (height / 2));
    for (place, luma) in luma.iter_mut().enumerate() {
        let (x, y) = (place % width, place / width);
        *luma = (40 + (x / 8) * 180 / (width / 8) + (y % 2) * 6) as u8;
    }
    for (place, (blue, red)) in blue.iter_mut().zip(red.iter_mut()).enumerate() {
        let (x, y) = (place % (width / 2), place / (width / 2));
        *blue = (60 + x * 140 / (width / 2)) as u8;
        *red = (200 - y * 140 / (height / 2)) as u8;
    }
    planes
}

/// Every video of a composition, with its number, its size and its colour filter.
fn videos(node: &Node, found: &mut Vec<(usize, (u32, u32), Vec<ColorOp>)>) {
    if let Kind::Video { video, filter, .. } = &node.kind {
        found.push((*video, (node.rect.width() as u32, node.rect.height() as u32), filter.clone()));
    }
    for child in &node.children {
        videos(child, found);
    }
}

#[test]
fn footage_given_as_planes_agrees_with_footage_made_rgba_in_memory() {
    // A plain video, a small one with a filter that begins by mixing the channels, and one
    // inside a blurred group, which the painter draws from memory on both renderers.
    let composition = load(
        r##"<composition width="256" height="128" fps="10" duration="1" background="#101418">
  <video src="a.mp4" left="0" top="0" width="128" height="64" filter="brightness(0.8) contrast(1.2) saturate(0.7)"/>
  <video src="b.mp4" left="136" top="8" width="96" height="48" border-radius="8" filter="grayscale(1) brightness(1.1)"/>
  <box left="8" top="70" width="120" height="52" blur="2">
    <video src="c.mp4" left="4" top="4" width="64" height="32"/>
  </box>
  <video src="d.mp4" left="150" top="70" width="64" height="32"/>
</composition>"##,
    );
    let mut found = Vec::new();
    videos(&composition.root, &mut found);
    assert_eq!(found.len(), 4);
    let mut cpu = Cpu::new(&composition, &[]);
    let mut gpu = Gpu::new(&composition, &[]).expect("a graphics adapter is there");
    for (video, (width, height), filter) in &found {
        let picture = planes(*width as usize, *height as usize);
        let grade = Grade::new(filter);
        let buffer = cpu.video_buffer(*video, *width, *height);
        yuv::to_rgba(&picture, *width as usize, *height as usize, buffer, |row| grade.apply(row));
        assert!(gpu.takes_planes(filter), "the card takes a filter of {} steps", filter.len());
        gpu.video_planes(*video, *width, *height, filter).copy_from_slice(&picture);
    }
    let on_cpu = drawn(&mut cpu, &composition, 0);
    let on_gpu = drawn(&mut gpu, &composition, 0);
    // The plain video shows its colours, not the canvas.
    let at = |pixels: &[u8], x: usize, y: usize| -> [u8; 3] { [0, 1, 2].map(|channel| pixels[(y * 256 + x) * 4 + channel]) };
    assert_ne!(at(&on_cpu, 180, 86), [0x10, 0x14, 0x18]);
    for (x, y) in [(20, 20), (100, 50), (160, 30), (200, 40), (30, 90), (170, 80), (200, 95)] {
        let (cpu, gpu) = (at(&on_cpu, x, y), at(&on_gpu, x, y));
        let apart = (0..3).map(|channel| cpu[channel].abs_diff(gpu[channel])).max();
        assert!(apart <= Some(3), "at ({x}, {y}) the CPU has {cpu:?} and the GPU {gpu:?}");
    }
    let (mean, far) = difference(&on_cpu, &on_gpu);
    assert!(
        mean < 1.0 && far < 0.005,
        "mean difference {mean:.3} of 255, {:.3}% of the pixels far apart",
        far * 100.0
    );
}

#[test]
fn the_planes_the_card_makes_are_those_made_from_its_rgba_in_memory() {
    let composition = load(
        r##"<composition width="256" height="128" fps="10" duration="1" background="#101418">
  <box left="0" top="0" right="0" bottom="0" background="linear-gradient(90deg, #ff2000 0%, #00c0ff 100%)"/>
  <box left="20" top="16" width="120" height="60" background="#f0e020" border-radius="14" border="3 #ffffff"/>
  <box left="150" top="40" width="80" height="70" background="#20a040"/>
  <text left="24" top="84" font-size="30" color="#ffffff">Planes</text>
</composition>"##,
    );
    let mut plain = Gpu::new(&composition, &[]).expect("a graphics adapter is there");
    let rgba = drawn(&mut plain, &composition, 0);
    let mut expected = vec![0; yuv::size(256, 128)];
    yuv::from_rgba(&rgba, 256, 128, &mut expected);

    let mut gpu = Gpu::new(&composition, &[]).expect("a graphics adapter is there");
    assert!(gpu.hand_over_planes(), "a frame with even sides can be handed over as planes");
    let planes = drawn(&mut gpu, &composition, 0);
    assert_eq!(planes.len(), expected.len());
    let apart: Vec<u8> = planes
        .iter()
        .zip(&expected)
        .map(|(found, expected)| found.abs_diff(*expected))
        .collect();
    let most = apart.iter().copied().max().unwrap_or(0);
    let mean = apart.iter().map(|apart| f64::from(*apart)).sum::<f64>() / apart.len() as f64;
    assert!(
        most <= 1 && mean < 0.1,
        "the planes differ by up to {most} levels, {mean:.4} on average"
    );
    // Luma is not flat, and the chroma planes hold colour.
    let luma = &planes[..256 * 128];
    let (darkest, lightest) = (luma.iter().min().copied().unwrap_or(0), luma.iter().max().copied().unwrap_or(0));
    assert!(lightest - darkest > 100, "luma runs from {darkest} to {lightest}");
    assert!(planes[256 * 128..].iter().any(|c| c.abs_diff(128) > 60));
}

#[test]
fn a_group_turned_in_space_and_one_that_opens_agree() {
    let scene = |effect: &str| {
        format!(
            r##"<composition width="256" height="128" fps="10" duration="1" background="#101418">
  <box left="0" top="0" right="0" bottom="0" background="linear-gradient(90deg, #203060 0%, #602030 100%)"/>
  <box left="48" top="24" width="160" height="80" background="linear-gradient(180deg, #f0e8d0 0%, #c0b090 100%)" border-radius="12" overflow="hidden" {effect}>
    <box position="absolute" left="12" top="12" width="56" height="56" background="#e04a3a" border-radius="8"/>
    <text position="absolute" left="80" top="20" font-size="24" color="#102040">Turned</text>
  </box>
</composition>"##
        )
    };
    agree(&scene(r#"perspective="300" rotate-y="50" rotate-x="15""#), 0);
    agree(&scene(r#"transition="iris" reveal="0.5""#), 0);
    agree(&scene(r#"transition="wipe" reveal="0.4""#), 0);
    // The hard-edged opening is the element's one cut, so the card has square corners here.
    let square = scene(r#"transition="cut" reveal="0.4""#).replace(r#" border-radius="12" overflow="hidden""#, "");
    agree(&square, 0);
}

#[test]
fn footage_through_a_colour_table_agrees_to_two_levels() {
    // A table of our own with a bend in every channel, so a wrong place between points shows.
    let folder = Path::new(env!("CARGO_TARGET_TMPDIR")).join("gpu-colour-table");
    std::fs::create_dir_all(&folder).expect("the test folder is made");
    let size = 9;
    let mut table = format!("LUT_3D_SIZE {size}\n");
    let at = |point: usize| point as f32 / (size - 1) as f32;
    for blue in 0..size {
        for green in 0..size {
            for red in 0..size {
                let (r, g, b) = (at(red), at(green), at(blue));
                table.push_str(&format!("{} {} {}\n", r * r, (g * 0.6 + b * 0.4).sqrt(), 1.0 - b * (0.5 + 0.5 * r)));
            }
        }
    }
    std::fs::write(folder.join("bent.cube"), table).expect("the table is written");
    let mut composition = xml::parse(
        r##"<composition width="256" height="128" fps="10" duration="1" background="#101418">
  <video src="a.mp4" left="0" top="0" width="128" height="128" lut="bent.cube"/>
  <video src="b.mp4" left="128" top="0" width="128" height="128" lut="bent.cube" filter="contrast(1.2) saturate(0.8)"/>
</composition>"##,
        &folder,
    )
    .expect("the scene is valid");
    layout::lay_out(&mut composition, &mut Shaper::new(), &[]);
    let mut found = Vec::new();
    videos(&composition.root, &mut found);
    assert_eq!(found.len(), 2);
    let mut cpu = Cpu::new(&composition, &[]);
    let mut gpu = Gpu::new(&composition, &[]).expect("a graphics adapter is there");
    let picture = planes(128, 128);
    let mut plain = vec![0; 128 * 128 * 4];
    yuv::to_rgba(&picture, 128, 128, &mut plain, |_| {});
    for (video, (width, height), filter) in &found {
        let grade = Grade::new(filter);
        let buffer = cpu.video_buffer(*video, *width, *height);
        yuv::to_rgba(&picture, 128, 128, buffer, |row| grade.apply(row));
        assert!(gpu.takes_planes(filter), "the card takes a filter that begins with a table");
        gpu.video_planes(*video, *width, *height, filter).copy_from_slice(&picture);
    }
    let on_cpu = drawn(&mut cpu, &composition, 0);
    let on_gpu = drawn(&mut gpu, &composition, 0);
    // The table changed the picture.
    let changed = (0..128 * 128)
        .filter(|place| on_cpu[(place / 128 * 256 + place % 128) * 4..][..3] != plain[place * 4..][..3])
        .count();
    assert!(changed > 128 * 128 / 2, "the table changed only {changed} pixels");
    let most = on_cpu.iter().zip(&on_gpu).map(|(cpu, gpu)| cpu.abs_diff(*gpu)).max();
    assert!(most <= Some(2), "the renderers differ by up to {most:?} levels");
}

#[test]
fn a_half_clear_group_keeps_its_colours_through_the_cards_pass() {
    // A group that lets the frame show through: were its alpha taken the wrong way on the
    // card, the whole of it would be darker or lighter than the CPU renderer's.
    let scene = |effect: &str| {
        format!(
            r##"<composition width="256" height="128" fps="10" duration="1" background="#f0f0f0">
  <box left="48" top="24" width="160" height="80" background="#ff200080" opacity="0.7" {effect}>
    <box position="absolute" left="16" top="16" width="48" height="48" background="#0040ffc0"/>
  </box>
</composition>"##
        )
    };
    for effect in [
        r#"perspective="300" rotate-y="35""#,
        r#"transition="wipe" reveal="0.8""#,
        r#"transition="iris" reveal="0.9""#,
    ] {
        let composition = load(&scene(effect));
        let on_cpu = drawn(&mut Cpu::new(&composition, &[]), &composition, 0);
        let mut gpu = Gpu::new(&composition, &[]).expect("a graphics adapter is there");
        let on_gpu = drawn(&mut gpu, &composition, 0);
        let at = |pixels: &[u8], x: usize, y: usize| -> [u8; 3] { [0, 1, 2].map(|channel| pixels[(y * 256 + x) * 4 + channel]) };
        // On the red sheet, on the blue square, and on the frame beside the group.
        for (x, y) in [(110, 64), (130, 90), (84, 62), (10, 10)] {
            let (cpu, gpu) = (at(&on_cpu, x, y), at(&on_gpu, x, y));
            let apart = (0..3).map(|channel| cpu[channel].abs_diff(gpu[channel])).max();
            assert!(apart <= Some(3), "{effect}: at ({x}, {y}) the CPU has {cpu:?} and the GPU {gpu:?}");
        }
        // The sheet is seen and is neither the frame's colour nor its own full colour.
        let sheet = at(&on_cpu, 110, 64);
        assert!(
            sheet != [0xf0, 0xf0, 0xf0] && sheet[1] > 0x30 && sheet[1] < 0xe0,
            "{effect}: the sheet is {sheet:?}"
        );
    }
}
