//! Footage in a frame (`docs/spec/render-command.md` section 4.8): a `<video>` shows, in each frame of its clip window, the
//! picture of its source with that frame's number counted from the window's first frame, stretched to its box, and nothing
//! outside the window. The clips are made at test time by frx7's writer, from flat colours of our own, and drawn on the
//! processor.

mod common;

use std::path::Path;

use common::project_with;
use draw_cpu::CpuRenderer;
use measure::Frame;
use project::Project;
use render::{Scene, Sources};

const CANVAS: [u8; 4] = [0x10, 0x20, 0x40, 255];

/// A flat picture of `width` by `height` in one colour.
fn flat(width: u32, height: u32, rgb: [u8; 3]) -> Frame {
    Frame {
        width,
        height,
        rgb: rgb.iter().copied().cycle().take((width * height * 3) as usize).collect(),
    }
}

/// The colours of the numbered pictures: picture `k` is told apart from every other by its red and blue, far enough apart
/// that the encoder's rounding cannot take one for another.
fn numbered(k: u32) -> [u8; 3] {
    [((k % 5) * 50 + 20) as u8, 128, ((k / 5) * 50 + 20) as u8]
}

/// The number of the numbered picture nearest to `pixel`.
fn number_of(pixel: [u8; 4]) -> u32 {
    (0..25)
        .min_by_key(|&k| {
            let c = numbered(k);
            (0..3).map(|i| (c[i] as i32 - pixel[i] as i32).abs()).sum::<i32>()
        })
        .unwrap()
}

fn write(folder: &Path, name: &str, frames: &[Frame]) {
    measure::write_video(&folder.join(name), (10, 1), frames).unwrap_or_else(|error| panic!("{error}"));
}

fn scene_of(folder: &Path) -> Scene {
    let project = Project::open(folder).unwrap_or_else(|error| panic!("{error}"));
    Scene::from_project(&project, &render::system_codecs()).unwrap_or_else(|reports| panic!("{reports:?}"))
}

fn composition(seconds: u32, inside: &str) -> String {
    format!(
        "<composition version=\"1\" width=\"96\" height=\"54\" fps=\"10\" duration=\"{seconds}\" background=\"#102040\">\n{inside}\n</composition>\n"
    )
}

#[test]
fn each_frame_shows_the_picture_of_its_number() {
    let folder = project_with(
        "footage",
        "numbered",
        &composition(2, r#"<video src="numbered.mp4" width="32" height="18" left="8" top="4"/>"#),
    );
    let pictures: Vec<Frame> = (0..20).map(|k| flat(32, 18, numbered(k))).collect();
    write(&folder, "numbered.mp4", &pictures);
    let scene = scene_of(&folder);
    let mut sources = Sources::open(&scene).unwrap();
    for number in 0..scene.frames() {
        let picture = scene.picture_with(number, &mut CpuRenderer, &mut sources).unwrap();
        let middle = picture.pixel(24, 13).unwrap();
        assert_eq!(number_of(middle), number as u32, "frame {number} shows {middle:?}");
        assert_eq!(picture.pixel(4, 2), Some(CANVAS));
    }
}

/// A picture whose left half is one colour and right half another, so that a stretch shows where the halves meet.
fn split(width: u32, height: u32, left: [u8; 3], right: [u8; 3]) -> Frame {
    let mut rgb = Vec::with_capacity((width * height * 3) as usize);
    for _ in 0..height {
        for x in 0..width {
            rgb.extend_from_slice(if x < width / 2 { &left } else { &right });
        }
    }
    Frame { width, height, rgb }
}

fn near(pixel: [u8; 4], rgb: [u8; 3], by: i32) -> bool {
    (0..3).all(|i| (pixel[i] as i32 - rgb[i] as i32).abs() <= by)
}

const RED: [u8; 3] = [220, 20, 20];
const BLUE: [u8; 3] = [20, 20, 220];

#[test]
fn the_footage_is_stretched_to_fill_its_box_at_the_written_size() {
    // A 16 by 18 source in a box of 64 by 36 at (8, 6): stretched four times across and twice down, the halves meet at
    // 8 + 32 = 40, and the box ends at 72 across and 42 down. The halves are read a source pixel and more from where they
    // meet: the encoder keeps colour at half the size, so the meeting is soft across one source pixel.
    let folder = project_with(
        "footage",
        "stretched",
        &composition(1, r#"<video src="split.mp4" width="64" height="36" left="8" top="6"/>"#),
    );
    write(&folder, "split.mp4", &vec![split(16, 18, RED, BLUE); 10]);
    let scene = scene_of(&folder);
    let mut sources = Sources::open(&scene).unwrap();
    let picture = scene.picture_with(0, &mut CpuRenderer, &mut sources).unwrap();
    let at = |x, y| picture.pixel(x, y).unwrap();
    for y in [6, 24, 41] {
        assert!(near(at(8, y), RED, 6), "({}, {y}) {:?}", 8, at(8, y));
        assert!(near(at(28, y), RED, 6), "(28, {y}) {:?}", at(28, y));
        assert!(near(at(52, y), BLUE, 6), "(52, {y}) {:?}", at(52, y));
        assert!(near(at(71, y), BLUE, 6), "(71, {y}) {:?}", at(71, y));
        assert_eq!(at(7, y), CANVAS);
        assert_eq!(at(72, y), CANVAS);
    }
    for x in [8, 40, 71] {
        assert_eq!(at(x, 5), CANVAS);
        assert_eq!(at(x, 42), CANVAS);
    }
}

#[test]
fn the_footage_is_shown_only_inside_its_clip_window() {
    // A window from 3 s for 3 s at 10 frames a second: frames 30 to 59. Frame 30 shows picture 0, frame 59 picture 29.
    let folder = project_with(
        "footage",
        "window",
        &composition(
            7,
            r#"<video src="numbered.mp4" width="32" height="18" left="8" top="4" start="3" duration="3"/>"#,
        ),
    );
    let pictures: Vec<Frame> = (0..30).map(|k| flat(32, 18, numbered(k % 25))).collect();
    write(&folder, "numbered.mp4", &pictures);
    let scene = scene_of(&folder);
    let mut sources = Sources::open(&scene).unwrap();
    let mut shown = |number| {
        scene
            .picture_with(number, &mut CpuRenderer, &mut sources)
            .unwrap()
            .pixel(24, 13)
            .unwrap()
    };
    assert_eq!(shown(29), CANVAS);
    assert_eq!(number_of(shown(30)), 0);
    assert_eq!(number_of(shown(45)), 15);
    assert_eq!(number_of(shown(59)), 29 % 25);
    assert_eq!(shown(60), CANVAS);
}

#[test]
fn white_black_and_grey_take_the_values_of_frx7() {
    // frx7's pixel spec 12.1: narrow-range white, black and mid grey of BT.709 come back as 255, 0 and 128.
    let folder = project_with(
        "footage",
        "greys",
        &composition(1, r#"<video src="greys.mp4" width="48" height="16" left="0" top="0"/>"#),
    );
    let mut rgb = Vec::new();
    for _ in 0..16 {
        for x in 0..48 {
            rgb.extend_from_slice(match x / 16 {
                0 => &[255, 255, 255],
                1 => &[0, 0, 0],
                _ => &[128, 128, 128],
            });
        }
    }
    write(
        &folder,
        "greys.mp4",
        &vec![
            Frame {
                width: 48,
                height: 16,
                rgb
            };
            10
        ],
    );
    let scene = scene_of(&folder);
    let mut sources = Sources::open(&scene).unwrap();
    let picture = scene.picture_with(0, &mut CpuRenderer, &mut sources).unwrap();
    assert_eq!(picture.pixel(8, 8), Some([255, 255, 255, 255]));
    assert_eq!(picture.pixel(24, 8), Some([0, 0, 0, 255]));
    assert_eq!(picture.pixel(40, 8), Some([128, 128, 128, 255]));
}

#[test]
fn the_sound_of_the_source_is_not_heard() {
    let folder = project_with(
        "footage",
        "sound",
        &composition(1, r#"<video src="loud.mp4" width="32" height="18" left="8" top="4"/>"#),
    );
    let source = folder.join("loud.mp4");
    let loud = measure::made_sound(48_000, 2, 48_000, 0.8, 0);
    measure::write_video_with_sound(&source, (10, 1), &vec![flat(32, 18, RED); 10], Some(&loud)).unwrap();
    assert!(
        measure::has_sound(&source).unwrap(),
        "the source must carry sound for this test to mean anything"
    );
    let scene = scene_of(&folder);
    let output = common::scratch("footage", "sound-out").join("film.mp4");
    render::write_film(&scene, &mut CpuRenderer, &output, 192).unwrap();
    match measure::read_sound(&output).unwrap() {
        None => {}
        Some(sound) => {
            let loudest = sound.samples.iter().fold(0f32, |m, s| m.max(s.abs()));
            assert!(loudest < 1e-4, "the film plays sound of {loudest}");
        }
    }
    let mut reader = measure::open_pictures(&output).unwrap();
    let first = reader.next_frame().unwrap().unwrap();
    let p = first.pixel(24, 13);
    assert!(near([p[0], p[1], p[2], 255], RED, 12), "{p:?}");
}

#[test]
fn a_source_in_mov_gives_the_same_frames_as_in_mp4() {
    // The same flat pictures written once as `.mp4` and once as `.mov`: the frames drawn from each are the same picture.
    let pictures: Vec<Frame> = (0..10).map(|k| flat(32, 18, numbered(k))).collect();
    let mut drawn = Vec::new();
    for (index, name) in ["same.mp4", "same.mov"].into_iter().enumerate() {
        let folder = project_with(
            "footage",
            &format!("container-{index}"),
            &composition(
                1,
                &format!("<video src=\"{name}\" width=\"32\" height=\"18\" left=\"8\" top=\"4\"/>"),
            ),
        );
        write(&folder, name, &pictures);
        let scene = scene_of(&folder);
        let mut sources = Sources::open(&scene).unwrap();
        let picture = scene.picture_with(4, &mut CpuRenderer, &mut sources).unwrap();
        let at = |x, y| picture.pixel(x, y).unwrap();
        assert_eq!(number_of(at(24, 13)), 4, "frame 4 of {name}");
        assert_eq!(at(4, 2), CANVAS);
        let mut frame = Vec::new();
        for y in 0..54 {
            for x in 0..96 {
                frame.push(picture.pixel(x, y).unwrap());
            }
        }
        drawn.push(frame);
    }
    assert_eq!(drawn[0], drawn[1], "MP4 and MOV give the same frame");
}

#[test]
fn opacity_brightness_and_blur_on_a_video_are_drawn_as_on_any_element() {
    // White footage, exact through the encoder (narrow-range white comes back as 255): at an opacity of 0 nothing of it
    // shows; with a brightness of 0 it is black; with a brightness of 2 mid grey stops at white. A blur of a flat field
    // changes no interior pixel. The composition's background is #102040.
    let pictures: Vec<Frame> = vec![flat(32, 18, [255, 255, 255]); 10];
    let shown = |name: &str, look: &str| {
        let folder = project_with(
            "footage",
            name,
            &composition(
                1,
                &format!("<video src=\"white.mp4\" width=\"32\" height=\"18\" left=\"8\" top=\"4\" {look}/>"),
            ),
        );
        write(&folder, "white.mp4", &pictures);
        let scene = scene_of(&folder);
        let mut sources = Sources::open(&scene).unwrap();
        scene.picture_with(0, &mut CpuRenderer, &mut sources).unwrap()
    };
    let picture = shown("look-clear", "opacity=\"0\"");
    assert_eq!(picture.pixel(24, 13), Some(CANVAS), "an opacity of 0 draws nothing");
    let picture = shown("look-dark", "brightness=\"0\"");
    assert_eq!(picture.pixel(24, 13), Some([0, 0, 0, 255]), "a brightness of 0 is black");
    let picture = shown("look-full", "brightness=\"2\" opacity=\"0.5\"");
    // The faded white is 128; laid over the background by src-over: 128 + each background value times 127 of 255.
    assert_eq!(picture.pixel(24, 13), Some([136, 144, 160, 255]));
    // A blur mixes the footage with what is transparent around it, so the field must fill the frame for its middle to
    // stay flat: mid grey stays 128 there, while the frame's edge is darkened by the clear outside it.
    let pictures: Vec<Frame> = vec![flat(96, 54, [128, 128, 128]); 10];
    let folder = project_with(
        "footage",
        "look-blurred",
        &composition(1, "<video src=\"grey.mp4\" width=\"96\" height=\"54\" blur=\"4\"/>"),
    );
    write(&folder, "grey.mp4", &pictures);
    let scene = scene_of(&folder);
    let mut sources = Sources::open(&scene).unwrap();
    let picture = scene.picture_with(0, &mut CpuRenderer, &mut sources).unwrap();
    assert_eq!(picture.pixel(48, 27), Some([128, 128, 128, 255]), "a blur keeps a flat field");
    assert!(picture.pixel(0, 27).unwrap()[0] < 128, "the blur reaches the frame's edge");
}

#[test]
fn footage_follows_the_rounded_corners_of_its_box() {
    // A box of 32 by 18 at (8, 4) with round corners of radius 8: the corner pixel shows the background, the middle the
    // footage. Centres of the corners are 8 from the edges; a pixel 11 or more from a centre is outside the round shape.
    let pictures: Vec<Frame> = vec![flat(32, 18, [255, 255, 255]); 10];
    let folder = project_with(
        "footage",
        "corners",
        &composition(
            1,
            "<video src=\"white.mp4\" width=\"32\" height=\"18\" left=\"8\" top=\"4\" border-radius=\"8\"/>",
        ),
    );
    write(&folder, "white.mp4", &pictures);
    let scene = scene_of(&folder);
    let mut sources = Sources::open(&scene).unwrap();
    let picture = scene.picture_with(0, &mut CpuRenderer, &mut sources).unwrap();
    let at = |x, y| picture.pixel(x, y).unwrap();
    assert_eq!(at(8, 4), CANVAS, "the corner is cut away");
    assert_eq!(at(39, 21), CANVAS, "the far corner is cut away");
    assert_eq!(at(24, 13), [255, 255, 255, 255], "the middle shows the footage");
    assert_eq!(at(8, 13), [255, 255, 255, 255], "the straight edge shows the footage");
}
