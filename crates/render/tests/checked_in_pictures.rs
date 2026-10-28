//! The frames drawn for the fixtures equal their checked-in pictures to the byte, and a changed pixel
//! fails with the frame's number (`docs/spec/render-command.md` sections 4 and 9; `PLAN.md`
//! section 5.2).

mod common;

use common::{compare, expected, numbers_with_a_picture, project_from_fixture, with_pixel};
use draw::Renderer;
use draw_cpu::CpuRenderer;
use project::Project;
use render::Scene;

fn scene_of(fixture: &str) -> Scene {
    let folder = project_from_fixture("checked-in-pictures", fixture, fixture);
    let project = Project::open(&folder).unwrap();
    Scene::from_project(&project, &render::system_codecs()).unwrap_or_else(|reports| panic!("{reports:?}"))
}

#[test]
fn the_frames_drawn_for_the_first_fixture_equal_their_checked_in_pictures() {
    let scene = scene_of("first-render");
    let numbers = numbers_with_a_picture("first-render");
    assert_eq!(numbers, vec![0, 4, 9], "the first, a middle and the last frame have a picture");
    for number in numbers {
        let drawn = scene.picture(number, &mut CpuRenderer).unwrap();
        if let Err(difference) = compare(number, &drawn, &expected("first-render", number)) {
            panic!("{difference}");
        }
    }
}

#[test]
fn the_frames_drawn_for_the_overlapping_boxes_equal_their_checked_in_pictures() {
    let scene = scene_of("overlapping-boxes");
    let numbers = numbers_with_a_picture("overlapping-boxes");
    assert_eq!(numbers, vec![0, 4]);
    for number in numbers {
        let drawn = scene.picture(number, &mut CpuRenderer).unwrap();
        if let Err(difference) = compare(number, &drawn, &expected("overlapping-boxes", number)) {
            panic!("{difference}");
        }
    }
}

#[test]
fn the_checked_in_pictures_show_the_box_where_it_was_written() {
    // The first fixture: a box at left 12, top 8, 40 wide and 30 high on the canvas #102040.
    let picture = expected("first-render", 4);
    assert_eq!((picture.width(), picture.height()), (96, 54));
    for y in 0..54 {
        for x in 0..96 {
            let inside = (12..52).contains(&x) && (8..38).contains(&y);
            let want = if inside { [0xe0, 0xa0, 0x30, 255] } else { [0x10, 0x20, 0x40, 255] };
            assert_eq!(picture.pixel(x, y), Some(want), "({x}, {y})");
        }
    }
    // The second: the later box (#2266dd) over the earlier (#cc2222) where they meet.
    let picture = expected("overlapping-boxes", 0);
    assert_eq!(picture.pixel(10, 10), Some([0xcc, 0x22, 0x22, 255]), "only the earlier box");
    assert_eq!(
        picture.pixel(40, 30),
        Some([0x22, 0x66, 0xdd, 255]),
        "where they overlap the later box is seen"
    );
    assert_eq!(picture.pixel(70, 45), Some([0x22, 0x66, 0xdd, 255]), "only the later box");
    assert_eq!(picture.pixel(90, 2), Some([0, 0, 0, 255]), "the canvas");
}

#[test]
fn a_changed_pixel_fails_the_comparison_with_the_frames_number_and_the_pixel() {
    let scene = scene_of("first-render");
    let drawn = scene.picture(4, &mut CpuRenderer).unwrap();
    let want = expected("first-render", 4);
    assert_eq!(compare(4, &drawn, &want), Ok(()));

    // One byte of one pixel, one level off, in the box.
    let changed = with_pixel(&drawn, 20, 15, [0xe0, 0xa0, 0x31, 255]);
    let difference = compare(4, &changed, &want).unwrap_err();
    assert!(difference.starts_with("frame 4: "), "{difference}");
    assert!(difference.contains("pixel (20, 15)"), "{difference}");
    assert!(
        difference.contains("[224, 160, 49, 255]") && difference.contains("[224, 160, 48, 255]"),
        "{difference}"
    );
    assert!(difference.contains("1 pixels differ"), "{difference}");

    // The alpha of one pixel counts too.
    let translucent = with_pixel(&drawn, 0, 0, [0x10, 0x20, 0x40, 254]);
    let difference = compare(9, &translucent, &want).unwrap_err();
    assert!(
        difference.starts_with("frame 9: ") && difference.contains("pixel (0, 0)"),
        "{difference}"
    );

    // The same picture drawn after a change to the composition is not the picture checked in.
    let other = scene_of("overlapping-boxes").picture(0, &mut CpuRenderer).unwrap();
    assert!(compare(0, &other, &want).is_err());
}

#[test]
fn a_picture_of_another_size_fails_the_comparison_with_the_frames_number() {
    let want = expected("first-render", 0);
    let smaller = draw::Picture::new(2, 2, vec![0; 16]).unwrap();
    let difference = compare(0, &smaller, &want).unwrap_err();
    assert!(difference.starts_with("frame 0: the picture drawn is 2x2"), "{difference}");
}

#[test]
fn the_checked_in_pictures_are_read_from_png_files_of_any_filter_and_block_kind() {
    // The reader is held to a file made by another program: a 2x2 picture of four colours, filtered
    // with "None" and stored in one uncompressed block.
    let raw: Vec<u8> = vec![
        0, 255, 0, 0, 255, 0, 255, 0, 255, //
        0, 0, 0, 255, 255, 10, 20, 30, 40,
    ];
    let mut zlib = vec![0x78, 0x01, 0x01];
    zlib.extend_from_slice(&(raw.len() as u16).to_le_bytes());
    zlib.extend_from_slice(&(!(raw.len() as u16)).to_le_bytes());
    zlib.extend_from_slice(&raw);
    let (a, b) = raw.iter().fold((1u32, 0u32), |(a, b), &byte| {
        let a = (a + u32::from(byte)) % 65521;
        (a, (b + a) % 65521)
    });
    zlib.extend_from_slice(&((b << 16) | a).to_be_bytes());
    let crc = |parts: &[&[u8]]| {
        let mut crc = 0xffff_ffffu32;
        for part in parts {
            for &byte in *part {
                crc ^= u32::from(byte);
                for _ in 0..8 {
                    crc = if crc & 1 == 1 { 0xedb8_8320 ^ (crc >> 1) } else { crc >> 1 };
                }
            }
        }
        !crc
    };
    let mut png = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    for (kind, data) in [
        (&b"IHDR"[..], [0, 0, 0, 2, 0, 0, 0, 2, 8, 6, 0, 0, 0].to_vec()),
        (&b"IDAT"[..], zlib),
        (&b"IEND"[..], Vec::new()),
    ] {
        png.extend_from_slice(&(data.len() as u32).to_be_bytes());
        png.extend_from_slice(kind);
        png.extend_from_slice(&data);
        png.extend_from_slice(&crc(&[kind, &data]).to_be_bytes());
    }
    let (width, height, pixels) = common::png::decode(&png).unwrap();
    assert_eq!((width, height), (2, 2));
    assert_eq!(pixels, vec![255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 10, 20, 30, 40]);
}

#[test]
fn what_the_writer_writes_the_reader_reads_back_for_flat_and_noisy_pictures() {
    let mut noisy = Vec::new();
    let mut seed = 12345u32;
    for _ in 0..(37 * 23 * 4) {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        noisy.push((seed >> 24) as u8);
    }
    for (width, height, pixels) in [(37u32, 23u32, noisy), (64, 64, vec![7u8; 64 * 64 * 4]), (1, 1, vec![1, 2, 3, 4])] {
        let png = common::png::encode(width, height, &pixels);
        assert_eq!(common::png::decode(&png).unwrap(), (width, height, pixels));
    }
}

#[test]
fn a_renderer_that_draws_a_different_picture_is_caught_by_the_checked_in_pictures() {
    struct Blank;
    impl Renderer for Blank {
        fn name(&self) -> &'static str {
            "blank"
        }
        fn draw(&mut self, list: &draw::DisplayList) -> Result<draw::Picture, draw::Error> {
            Ok(draw::Picture::new(list.width, list.height, vec![0; list.width as usize * list.height as usize * 4]).unwrap())
        }
    }
    let scene = scene_of("first-render");
    let drawn = scene.picture(0, &mut Blank).unwrap();
    let difference = compare(0, &drawn, &expected("first-render", 0)).unwrap_err();
    assert!(difference.starts_with("frame 0: pixel (0, 0)"), "{difference}");
}
