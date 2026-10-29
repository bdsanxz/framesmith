//! The source picture a frame shows (`docs/spec/render-command.md` section 4.8, rule 2): the latest picture of the
//! source whose time is at or before the frame's time, never a later one, counted from the clip's first frame. The clips
//! are made at test time by frx7's writer, from numbered flat colours of our own; a frame that shows another picture
//! than the one worked by hand below fails, naming the frame, the picture expected and the picture found.

mod common;

use std::path::Path;
use std::sync::Arc;

use common::agreement::figures;
use common::project_with;
use draw_cpu::CpuRenderer;
use draw_gpu::CardRenderer;
use measure::Frame;
use project::Project;
use render::{Footage, Scene, Sources};

/// A flat picture of `width` by `height` in one colour.
fn flat(width: u32, height: u32, rgb: [u8; 3]) -> Frame {
    Frame {
        width,
        height,
        rgb: rgb.iter().copied().cycle().take((width * height * 3) as usize).collect(),
    }
}

/// The colours of the numbered pictures: picture `k` is told apart from every other by its red and blue, far enough
/// apart that the encoder's rounding cannot take one for another. Up to 64 pictures.
fn numbered(k: u32) -> [u8; 3] {
    [((k % 8) * 32 + 4) as u8, 128, (((k / 8) % 8) * 32 + 4) as u8]
}

/// The number of the numbered picture nearest to `pixel`.
fn number_of(pixel: [u8; 4]) -> u32 {
    (0..64)
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

fn scene_of(folder: &Path) -> Scene {
    let project = Project::open(folder).unwrap_or_else(|error| panic!("{error}"));
    Scene::from_project(&project, &render::system_codecs()).unwrap_or_else(|reports| panic!("{reports:?}"))
}

/// The number the drawn frame `number` of `scene` shows, read at the middle of its box, with `sources`.
fn shown(scene: &Scene, sources: &mut Sources, number: u64) -> u32 {
    let picture = scene.picture_with(number, &mut CpuRenderer, sources).unwrap();
    number_of(picture.pixel(24, 13).unwrap())
}

/// Draws `scene` with a 32 by 18 box at (8, 4): the middle of the footage is (24, 13).

#[test]
fn a_25_pictures_a_second_source_shows_the_latest_picture_at_or_before_each_frame() {
    // A source at 25 pictures a second in a composition at 30 frames a second. Frame `e` is at `e/30` seconds and
    // picture `k` at `k/25`, so the frame shows `floor(25 * e / 30)`: frames 0 to 5 show 0, 0, 1, 2, 3, 4, and frame 29
    // shows 24.
    let want = [
        0, 0, 1, 2, 3, 4, 5, 5, 6, 7, 8, 9, 10, 10, 11, 12, 13, 14, 15, 15, 16, 17, 18, 19, 20, 20, 21, 22, 23, 24,
    ];
    let folder = project_with(
        "source-picture",
        "twenty-five",
        &composition(30, 1, r#"<video src="source.mp4" width="32" height="18" left="8" top="4"/>"#),
    );
    let pictures: Vec<Frame> = (0..25).map(|k| flat(32, 18, numbered(k))).collect();
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
fn a_23_and_976_pictures_a_second_source_shows_the_latest_picture_at_or_before_each_frame() {
    // Pictures 1001 ticks apart at a timescale of 24000 (23.976 pictures a second) in a composition at 30 frames a
    // second. Frame `e` shows `floor(e * 24000 / (30 * 1001))`: frames 0 and 1 show 0, frame 2 shows 1, frames 5 and 10
    // show 3 and 7, and frame 29 shows 23.
    let want = [
        0, 0, 1, 2, 3, 3, 4, 5, 6, 7, 7, 8, 9, 10, 11, 11, 12, 13, 14, 15, 15, 16, 17, 18, 19, 19, 20, 21, 22, 23,
    ];
    let folder = project_with(
        "source-picture",
        "twenty-three-976",
        &composition(30, 1, r#"<video src="source.mp4" width="32" height="18" left="8" top="4"/>"#),
    );
    let at: Vec<i64> = (0..24).map(|k| k * 1001).collect();
    let pictures: Vec<Frame> = (0..24).map(|k| flat(32, 18, numbered(k as u32))).collect();
    measure::write_video_with_times(&folder.join("source.mp4"), (24000, 1001), true, 24000, &at, &pictures).unwrap();
    let scene = scene_of(&folder);
    assert_eq!(scene.frames(), 30);
    let mut sources = Sources::open(&scene).unwrap();
    for (e, &w) in want.iter().enumerate() {
        let found = shown(&scene, &mut sources, e as u64);
        assert_eq!(found, w, "frame {e} shows picture {found}, expected {w}");
    }
}

#[test]
fn a_30_pictures_a_second_source_shows_the_picture_of_the_frame_number() {
    // The source at the composition's own rate: frame `e` shows picture `e`.
    let folder = project_with(
        "source-picture",
        "thirty",
        &composition(30, 1, r#"<video src="source.mp4" width="32" height="18" left="8" top="4"/>"#),
    );
    let pictures: Vec<Frame> = (0..30).map(|k| flat(32, 18, numbered(k))).collect();
    measure::write_video(&folder.join("source.mp4"), (30, 1), &pictures).unwrap();
    let scene = scene_of(&folder);
    assert_eq!(scene.frames(), 30);
    let mut sources = Sources::open(&scene).unwrap();
    for e in 0..30 {
        let found = shown(&scene, &mut sources, e);
        assert_eq!(found, e as u32, "frame {e} shows picture {found}, expected {e}");
    }
}

#[test]
fn a_60_pictures_a_second_source_passes_over_every_second_picture() {
    // Twice the composition's rate: frame `e` shows picture `2 * e`, the odd pictures passed over.
    let folder = project_with(
        "source-picture",
        "sixty",
        &composition(30, 1, r#"<video src="source.mp4" width="32" height="18" left="8" top="4"/>"#),
    );
    let pictures: Vec<Frame> = (0..60).map(|k| flat(32, 18, numbered(k))).collect();
    measure::write_video(&folder.join("source.mp4"), (60, 1), &pictures).unwrap();
    let scene = scene_of(&folder);
    assert_eq!(scene.frames(), 30);
    let mut sources = Sources::open(&scene).unwrap();
    for e in 0..30 {
        let found = shown(&scene, &mut sources, e);
        assert_eq!(found, 2 * e as u32, "frame {e} shows picture {found}, expected {}", 2 * e);
    }
}

#[test]
fn a_source_whose_rate_changes_part_way_repeats_then_passes_over() {
    // Ten pictures two ticks apart then twenty one tick apart, at a timescale of 60: 30 pictures a second, then 60.
    // In a composition at 30 frames a second frame `e` is at `2 * e` ticks: frames 0 to 9 show 0 to 9, frame 10 shows
    // 11, frame 11 shows 13, frame 15 shows 21, frame 19 shows 29, and the last picture holds after it.
    let want = [
        0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 11, 13, 15, 17, 19, 21, 23, 25, 27, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29,
    ];
    let folder = project_with(
        "source-picture",
        "changing",
        &composition(30, 1, r#"<video src="source.mp4" width="32" height="18" left="8" top="4"/>"#),
    );
    let mut at = vec![0i64];
    for _ in 1..10 {
        at.push(at.last().unwrap() + 2);
    }
    for _ in 10..30 {
        at.push(at.last().unwrap() + 1);
    }
    let pictures: Vec<Frame> = (0..30).map(|k| flat(32, 18, numbered(k as u32))).collect();
    measure::write_video_with_times(&folder.join("source.mp4"), (30, 1), false, 60, &at, &pictures).unwrap();
    let scene = scene_of(&folder);
    assert_eq!(scene.frames(), 30);
    let mut sources = Sources::open(&scene).unwrap();
    for (e, &w) in want.iter().enumerate() {
        let found = shown(&scene, &mut sources, e as u64);
        assert_eq!(found, w, "frame {e} shows picture {found}, expected {w}");
    }
}

#[test]
fn no_sampled_frame_shows_a_picture_after_its_time() {
    // For every frame of a 25 pictures-a-second source and of the changing-rate source, the picture shown has its file
    // time at or before the frame's time, and the next picture's after it (or it is the last picture, which holds).
    let folder = project_with(
        "source-picture",
        "never-after-25",
        &composition(30, 1, r#"<video src="source.mp4" width="32" height="18" left="8" top="4"/>"#),
    );
    let pictures: Vec<Frame> = (0..25).map(|k| flat(32, 18, numbered(k))).collect();
    measure::write_video(&folder.join("source.mp4"), (25, 1), &pictures).unwrap();
    let scene = scene_of(&folder);
    let mut sources = Sources::open(&scene).unwrap();
    let (scale, times) = sources.picture_times(0).unwrap();
    assert_eq!(times.len(), 25);
    for e in 0..scene.frames() {
        let found = shown(&scene, &mut sources, e);
        assert!(
            (times[found as usize] as i128) * 30 <= (e as i128) * (scale as i128),
            "frame {e} shows picture {found} after its time"
        );
        let last = found as usize + 1 == times.len();
        assert!(
            last || (times[found as usize + 1] as i128) * 30 > (e as i128) * (scale as i128),
            "frame {e} shows picture {found}, but picture {} is also at or before its time",
            found + 1
        );
    }

    let folder = project_with(
        "source-picture",
        "never-after-changing",
        &composition(30, 1, r#"<video src="source.mp4" width="32" height="18" left="8" top="4"/>"#),
    );
    let mut at = vec![0i64];
    for _ in 1..10 {
        at.push(at.last().unwrap() + 2);
    }
    for _ in 10..30 {
        at.push(at.last().unwrap() + 1);
    }
    let pictures: Vec<Frame> = (0..30).map(|k| flat(32, 18, numbered(k as u32))).collect();
    measure::write_video_with_times(&folder.join("source.mp4"), (30, 1), false, 60, &at, &pictures).unwrap();
    let scene = scene_of(&folder);
    let mut sources = Sources::open(&scene).unwrap();
    let (scale, times) = sources.picture_times(0).unwrap();
    assert_eq!(times.len(), 30);
    for e in 0..scene.frames() {
        let found = shown(&scene, &mut sources, e);
        assert!(
            (times[found as usize] as i128) * 30 <= (e as i128) * (scale as i128),
            "frame {e} shows picture {found} after its time"
        );
        let last = found as usize + 1 == times.len();
        assert!(
            last || (times[found as usize + 1] as i128) * 30 > (e as i128) * (scale as i128),
            "frame {e} shows picture {found}, but picture {} is also at or before its time",
            found + 1
        );
    }
}

#[test]
fn frames_asked_out_of_order_show_the_same_pictures_as_in_order() {
    // The picture a frame shows is a function of the frame's number alone: asked for in a mixed order across the clip,
    // and twice, the frames show the same pictures as in order.
    let folder = project_with(
        "source-picture",
        "mixed-order",
        &composition(30, 1, r#"<video src="source.mp4" width="32" height="18" left="8" top="4"/>"#),
    );
    let pictures: Vec<Frame> = (0..25).map(|k| flat(32, 18, numbered(k))).collect();
    measure::write_video(&folder.join("source.mp4"), (25, 1), &pictures).unwrap();
    let scene = scene_of(&folder);
    let mut sources = Sources::open(&scene).unwrap();
    let mut inorder = Vec::new();
    for e in 0..scene.frames() {
        inorder.push(shown(&scene, &mut sources, e));
    }
    // A mixed order across the clip, each frame twice in a row.
    let mut mixed = vec![
        29, 0, 15, 7, 22, 3, 11, 19, 26, 5, 13, 28, 1, 17, 9, 24, 6, 20, 12, 27, 2, 18, 8, 23, 4, 16, 10, 21, 14, 25,
    ];
    mixed.extend_from_within(..);
    for &e in &mixed {
        let found = shown(&scene, &mut sources, e);
        assert_eq!(
            found, inorder[e as usize],
            "frame {e} shows picture {found} out of order, expected {}",
            inorder[e as usize]
        );
    }
}

/// A source whose pictures start after 0: the presentation times are scripted, since the test-only writers rebase the
/// first picture to 0. The stub stands in for the file, naming the source, the picture and the size it is asked for.
struct Scripted {
    scale: u32,
    times: Vec<i64>,
}

impl Footage for Scripted {
    fn count(&mut self, _source: usize) -> Result<u64, String> {
        Ok(self.times.len() as u64)
    }

    fn picture_times(&mut self, _source: usize) -> Result<(u32, Vec<i64>), String> {
        Ok((self.scale, self.times.clone()))
    }

    fn picture(&mut self, _source: usize, number: u64, size: (u32, u32)) -> Result<Arc<Vec<u8>>, String> {
        let pixels = vec![(number as u8).wrapping_mul(40), 0, 0, 255]
            .into_iter()
            .cycle()
            .take(size.0 as usize * size.1 as usize * 4)
            .collect();
        Ok(Arc::new(pixels))
    }
}

/// The red of the footage pixel at the box's corner in `frame`.
fn footage_red(frame: &draw::Frame) -> u8 {
    use draw::Mark;
    for mark in &frame.marks {
        if let Mark::Picture(picture) = mark {
            return picture.pixels[0];
        }
    }
    panic!("the frame shows no footage");
}

#[test]
fn a_frame_before_the_first_picture_shows_the_first_picture() {
    // Three pictures at 1, 2 and 3 seconds (timescale 30) in a composition at 30 frames a second. Frames before 1
    // second show the first picture; frame 30, at exactly 1 second, shows it too; frames 60 and 90 show the second and
    // third.
    let folder = project_with(
        "source-picture",
        "late-start",
        &composition(30, 4, r#"<video src="source.mp4" width="32" height="18" left="8" top="4"/>"#),
    );
    let pictures: Vec<Frame> = (0..30).map(|k| flat(32, 18, numbered(k as u32))).collect();
    measure::write_video(&folder.join("source.mp4"), (30, 1), &pictures).unwrap();
    let scene = scene_of(&folder);
    assert_eq!(scene.frames(), 120);
    let mut scripted = Scripted {
        scale: 30,
        times: vec![30, 60, 90],
    };
    for (e, want) in [(0, 0), (29, 0), (30, 0), (59, 0), (60, 1), (89, 1), (90, 2), (119, 2)] {
        let frame = scene.frame_with(e, &mut scripted).unwrap();
        let found = footage_red(&frame) / 40;
        assert_eq!(found, want, "frame {e} shows picture {found}, expected {want}");
    }
}

#[test]
#[ignore = "reference machine only: needs a graphics card that frx7 can draw with"]
fn reference_machine_only_the_card_shows_the_same_source_pictures() {
    // A 25 pictures-a-second source in a 30 frames-a-second composition, every frame on both renderers: the card's
    // picture agrees with the processor's within the tolerance, and shows the same worked picture number.
    let want = [
        0, 0, 1, 2, 3, 4, 5, 5, 6, 7, 8, 9, 10, 10, 11, 12, 13, 14, 15, 15, 16, 17, 18, 19, 20, 20, 21, 22, 23, 24,
    ];
    let folder = project_with(
        "source-picture",
        "card",
        &composition(30, 1, r#"<video src="source.mp4" width="32" height="18" left="8" top="4"/>"#),
    );
    let pictures: Vec<Frame> = (0..25).map(|k| flat(32, 18, numbered(k))).collect();
    measure::write_video(&folder.join("source.mp4"), (25, 1), &pictures).unwrap();
    let scene = scene_of(&folder);
    let mut sources = Sources::open(&scene).unwrap();
    let mut card = CardRenderer::new().unwrap_or_else(|error| panic!("the card could not be started: {error}"));
    for (e, &w) in want.iter().enumerate() {
        let processor = scene.picture_with(e as u64, &mut CpuRenderer, &mut sources).unwrap();
        let carded = scene.picture_with(e as u64, &mut card, &mut sources).unwrap();
        let measured = figures(e as u64, &processor, &carded).unwrap_or_else(|error| panic!("{error}"));
        assert!(measured.within(), "frame {e}: {}", measured.misses().join("; "));
        let found = number_of(carded.pixel(24, 13).unwrap());
        assert_eq!(found, w, "frame {e} shows picture {found} on the card, expected {w}");
    }
}
