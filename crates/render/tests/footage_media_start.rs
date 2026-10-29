//! A clip's start in its source (`docs/spec/notation.md` section 5.3, N4; `docs/spec/render-command.md` section 4.8,
//! rule 2): the first frame shows the picture at the `media-start` on the source's grid, each later frame follows the
//! at-or-before rule from there, and past the source's end the last picture holds. The clips are made at test time by
//! frx7's writer, from numbered flat colours of our own; a frame that shows another picture than the one worked by hand
//! below fails, naming the frame, the picture expected and the picture found.

mod common;

use std::path::Path;

use common::agreement::figures;
use common::project_with;
use draw_cpu::CpuRenderer;
use draw_gpu::CardRenderer;
use measure::Frame;
use project::Project;
use render::{Scene, Sources};

/// A flat picture of `width` by `height` in one colour.
fn flat(width: u32, height: u32, rgb: [u8; 3]) -> Frame {
    Frame {
        width,
        height,
        rgb: rgb.iter().copied().cycle().take((width * height * 3) as usize).collect(),
    }
}

/// The colours of the numbered pictures: picture `k` is told apart from every other by its red, green and blue, far
/// enough apart that the encoder's rounding cannot take one for another. Up to 128 pictures: the red and blue cycle
/// every 64, and the green names the page.
fn numbered(k: u32) -> [u8; 3] {
    [
        ((k % 8) * 32 + 4) as u8,
        (96 + (k / 64) % 2 * 64) as u8,
        ((((k / 8) % 8) * 32) + 4) as u8,
    ]
}

/// The number of the numbered picture nearest to `pixel`.
fn number_of(pixel: [u8; 4]) -> u32 {
    (0..128)
        .min_by_key(|&k| {
            let c = numbered(k);
            (0..3).map(|i| (c[i] as i32 - pixel[i] as i32).abs()).sum::<i32>()
        })
        .unwrap()
}

fn composition(fps: u32, seconds: u32, inside: &str) -> String {
    format!(
        "<composition version=\"1\" width=\"96\" height=\"54\" fps=\"{fps}\" duration=\"{seconds}\" background=\"#102040\">\n{inside}\n</composition>\n"
    )
}

fn video(media_start: &str) -> String {
    format!("<video src=\"source.mp4\" width=\"32\" height=\"18\" left=\"8\" top=\"4\" media-start=\"{media_start}\"/>")
}

fn scene_of(folder: &Path) -> Scene {
    let project = Project::open(folder).unwrap_or_else(|error| panic!("{error}"));
    Scene::from_project(&project, &render::system_codecs()).unwrap_or_else(|reports| panic!("{reports:?}"))
}

/// The number the drawn frame `number` of `scene` shows, read at the middle of its box, with `sources`.
fn shown(scene: &Scene, sources: &mut Sources, number: u64) -> u32 {
    let picture = scene.picture_with(number, &mut CpuRenderer, sources).unwrap();
    number_of(picture.pixel(24, 13).unwrap())
}

#[test]
fn the_first_frame_shows_the_grid_picture_of_the_media_start_and_later_frames_the_next_ones() {
    // A 30 pictures-a-second source in a 30 frames-a-second composition: with `media-start="2"` the first frame
    // shows picture 60 and each later frame the next, so frame `e` shows `60 + e`; with `media-start="1"` the
    // first frame shows picture 30; with `media-start="0"` the clip starts at the source's first picture, as
    // without the attribute. The source holds 90 pictures, so no frame reaches past its end.
    for (media_start, first) in [("2", 60), ("1", 30), ("0", 0)] {
        let folder = project_with(
            "media-start",
            &format!("first-{media_start}"),
            &composition(30, 1, &video(media_start)),
        );
        let pictures: Vec<Frame> = (0..90).map(|k| flat(32, 18, numbered(k))).collect();
        measure::write_video(&folder.join("source.mp4"), (30, 1), &pictures).unwrap();
        let scene = scene_of(&folder);
        assert_eq!(scene.frames(), 30);
        let mut sources = Sources::open(&scene).unwrap();
        for e in 0..30 {
            let found = shown(&scene, &mut sources, e);
            assert_eq!(
                found,
                first + e as u32,
                "media-start={media_start}, frame {e} shows picture {found}"
            );
        }
    }
}

#[test]
fn a_media_start_between_two_pictures_shows_the_grid_picture_first() {
    // A 30 pictures-a-second source with `media-start="0.5"`: `F(0.5)` is picture 15, so frames show 15, 16, ....
    let folder = project_with("media-start", "between", &composition(30, 1, &video("0.5")));
    let pictures: Vec<Frame> = (0..45).map(|k| flat(32, 18, numbered(k))).collect();
    measure::write_video(&folder.join("source.mp4"), (30, 1), &pictures).unwrap();
    let scene = scene_of(&folder);
    let mut sources = Sources::open(&scene).unwrap();
    for e in 0..30 {
        let found = shown(&scene, &mut sources, e);
        assert_eq!(found, 15 + e as u32, "frame {e} shows picture {found}, expected {}", 15 + e);
    }
}

#[test]
fn a_media_start_with_a_source_at_another_rate_follows_the_rule_from_there() {
    // A 25 pictures-a-second source with `media-start="1"`: frame `e` is at `1 + e/30` seconds, so it shows
    // `floor(25 * (1 + e/30))`: frames 0 to 5 show 25, 25, 26, 27, 28, 29 and frame 29 shows 49.
    let want = [
        25, 25, 26, 27, 28, 29, 30, 30, 31, 32, 33, 34, 35, 35, 36, 37, 38, 39, 40, 40, 41, 42, 43, 44, 45, 45, 46, 47, 48, 49,
    ];
    let folder = project_with("media-start", "other-rate", &composition(30, 1, &video("1")));
    let pictures: Vec<Frame> = (0..50).map(|k| flat(32, 18, numbered(k))).collect();
    measure::write_video(&folder.join("source.mp4"), (25, 1), &pictures).unwrap();
    let scene = scene_of(&folder);
    assert_eq!(scene.frames(), 30);
    let mut sources = Sources::open(&scene).unwrap();
    for (e, &w) in want.iter().enumerate() {
        let found = shown(&scene, &mut sources, e as u64);
        assert_eq!(found, w, "frame {e} shows picture {found}, expected {w}");
    }
}

#[test]
fn past_the_end_of_the_source_each_later_frame_holds_its_last_picture() {
    // A 20-picture source at 30 pictures a second in a 30-frame clip with no `media-start`: frames 0 to 19 show
    // pictures 0 to 19, frames 20 to 29 hold picture 19.
    // With `media-start="1.5"` on a 60-picture source: frames show 45 to 59, then hold 59 for frames 15 to 29.
    let folder = project_with("media-start", "short-plain", &composition(30, 1, &video("0")));
    let pictures: Vec<Frame> = (0..20).map(|k| flat(32, 18, numbered(k))).collect();
    measure::write_video(&folder.join("source.mp4"), (30, 1), &pictures).unwrap();
    let scene = scene_of(&folder);
    let mut sources = Sources::open(&scene).unwrap();
    for e in 0..30 {
        let found = shown(&scene, &mut sources, e);
        let want = e.min(19) as u32;
        assert_eq!(found, want, "frame {e} shows picture {found}, expected {want}");
    }

    let folder = project_with("media-start", "short-shifted", &composition(30, 1, &video("1.5")));
    let pictures: Vec<Frame> = (0..60).map(|k| flat(32, 18, numbered(k))).collect();
    measure::write_video(&folder.join("source.mp4"), (30, 1), &pictures).unwrap();
    let scene = scene_of(&folder);
    let mut sources = Sources::open(&scene).unwrap();
    for e in 0..30 {
        let found = shown(&scene, &mut sources, e);
        let want = (45 + e).min(59) as u32;
        assert_eq!(found, want, "media-start=1.5, frame {e} shows picture {found}, expected {want}");
    }
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_the_card_holds_the_same_pictures_from_the_media_start() {
    // A 30 pictures-a-second source with `media-start="1"` in a 30 frames-a-second composition, every frame on
    // both renderers: the card's picture agrees with the processor's within the tolerance, and shows picture 30 + e.
    let folder = project_with("media-start", "card", &composition(30, 1, &video("1")));
    let pictures: Vec<Frame> = (0..60).map(|k| flat(32, 18, numbered(k))).collect();
    measure::write_video(&folder.join("source.mp4"), (30, 1), &pictures).unwrap();
    let scene = scene_of(&folder);
    let mut sources = Sources::open(&scene).unwrap();
    let mut card = CardRenderer::new().unwrap_or_else(|error| panic!("the card could not be started: {error}"));
    for e in 0..scene.frames() {
        let processor = scene.picture_with(e, &mut CpuRenderer, &mut sources).unwrap();
        let carded = scene.picture_with(e, &mut card, &mut sources).unwrap();
        let measured = figures(e, &processor, &carded).unwrap_or_else(|error| panic!("{error}"));
        assert!(measured.within(), "frame {e}: {}", measured.misses().join("; "));
        let found = number_of(carded.pixel(24, 13).unwrap());
        assert_eq!(found, 30 + e as u32, "frame {e} shows picture {found} on the card");
    }
}
