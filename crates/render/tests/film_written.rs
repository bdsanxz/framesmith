//! The film written for a fixture is an H.264 MP4 whose size, count and rate are the composition's and
//! whose pictures, decoded again by frx7, show the colours and the boxes where they were written
//! (`docs/spec/render-command.md` section 5).

mod common;

use common::{project_from_fixture, scratch};
use draw::{DisplayList, Error, Picture, Renderer};
use draw_cpu::CpuRenderer;
use measure::{open_pictures, video_facts, Frame};
use project::Project;
use render::{write_film, Scene};
use std::fs;

fn scene_of(name: &str, fixture: &str) -> Scene {
    let folder = project_from_fixture("film-written", name, fixture);
    Scene::from_project(&Project::open(&folder).unwrap()).unwrap_or_else(|reports| panic!("{reports:?}"))
}

fn frames_of(path: &std::path::Path) -> Vec<Frame> {
    let mut reader = open_pictures(path).unwrap();
    let mut frames = Vec::new();
    while let Some(frame) = reader.next_frame().unwrap() {
        frames.push(frame);
    }
    frames
}

/// How far a decoded colour may be from the colour drawn: the encoder is lossy.
const NEAR: u8 = 12;

#[test]
fn the_film_is_an_h264_mp4_with_the_size_the_frame_count_and_the_rate_of_the_composition() {
    let scene = scene_of("facts-out-in", "first-render");
    let output = scratch("film-written", "facts-out").join("first.mp4");
    let written = write_film(&scene, &mut CpuRenderer, &output, 192).unwrap();
    assert_eq!(written.frames, 10);
    assert_eq!(written.bytes, fs::metadata(&output).unwrap().len());
    assert!(written.bytes > 0 && written.seconds >= 0.0);

    let facts = video_facts(&output).unwrap();
    assert_eq!(facts.container, "mp4");
    assert_eq!(facts.codec, "h264");
    assert_eq!((facts.width, facts.height), (96, 54));
    assert_eq!(facts.frames, 10);
    assert_eq!(facts.rate, (10, 1));
    assert!((facts.seconds() - 1.0).abs() < 1e-9, "{}", facts.seconds());
}

#[test]
fn the_pictures_at_three_times_show_the_canvas_and_the_box_where_they_were_written() {
    let scene = scene_of("first-out-in", "first-render");
    let output = scratch("film-written", "first-out").join("first.mp4");
    write_film(&scene, &mut CpuRenderer, &output, 192).unwrap();
    let frames = frames_of(&output);
    assert_eq!(frames.len(), 10);
    for number in [0usize, 4, 9] {
        let frame = &frames[number];
        assert_eq!((frame.width, frame.height), (96, 54));
        // The canvas at the corners, the box in its middle.
        for (x, y) in [(0, 0), (95, 0), (0, 53), (95, 53), (70, 30)] {
            let [r, g, b] = frame.pixel(x, y);
            assert!(
                r.abs_diff(0x10) <= NEAR && g.abs_diff(0x20) <= NEAR && b.abs_diff(0x40) <= NEAR,
                "frame {number} ({x}, {y}): {:?}",
                [r, g, b]
            );
        }
        let [r, g, b] = frame.pixel(32, 23);
        assert!(
            r.abs_diff(0xe0) <= NEAR && g.abs_diff(0xa0) <= NEAR && b.abs_diff(0x30) <= NEAR,
            "frame {number} box: {:?}",
            [r, g, b]
        );
        // The box's edges: left 12, top 8, 40 wide, 30 high, so the last pixels are 51 and 37.
        let bounds = frame
            .bounds([0xe0, 0xa0, 0x30], 20)
            .unwrap_or_else(|| panic!("frame {number}: no box"));
        for (name, found, want) in [
            ("left", bounds.left, 12u32),
            ("top", bounds.top, 8),
            ("right", bounds.right, 51),
            ("bottom", bounds.bottom, 37),
        ] {
            assert!(found.abs_diff(want) <= 1, "frame {number}: box {name} is {found}, written {want}");
        }
    }
}

#[test]
fn where_boxes_overlap_the_one_written_later_is_seen() {
    let scene = scene_of("overlap-out-in", "overlapping-boxes");
    let output = scratch("film-written", "overlap-out").join("overlap.mp4");
    let written = write_film(&scene, &mut CpuRenderer, &output, 192).unwrap();
    assert_eq!(written.frames, 5);
    let frames = frames_of(&output);
    assert_eq!(frames.len(), 5);
    let near = |pixel: [u8; 3], want: [u8; 3]| pixel.iter().zip(want).all(|(a, b)| a.abs_diff(b) <= NEAR);
    for (number, frame) in frames.iter().enumerate() {
        assert!(
            near(frame.pixel(12, 10), [0xcc, 0x22, 0x22]),
            "frame {number}: earlier box only: {:?}",
            frame.pixel(12, 10)
        );
        assert!(
            near(frame.pixel(40, 30), [0x22, 0x66, 0xdd]),
            "frame {number}: overlap: {:?}",
            frame.pixel(40, 30)
        );
        assert!(
            near(frame.pixel(75, 40), [0x22, 0x66, 0xdd]),
            "frame {number}: later box only: {:?}",
            frame.pixel(75, 40)
        );
        assert!(
            near(frame.pixel(90, 2), [0, 0, 0]),
            "frame {number}: canvas: {:?}",
            frame.pixel(90, 2)
        );
        // The earlier box is not drawn over the later one: the red does not reach the overlap.
        let red = frame.bounds([0xcc, 0x22, 0x22], 24).unwrap();
        assert!(red.left.abs_diff(8) <= 1 && red.top.abs_diff(6) <= 1, "frame {number}: {red:?}");
    }
}

#[test]
fn the_folder_for_the_file_is_made_when_it_is_missing() {
    let scene = scene_of("folder-out-in", "first-render");
    let output = scratch("film-written", "folder-out").join("a/b/c/first.mp4");
    assert!(!output.parent().unwrap().exists());
    write_film(&scene, &mut CpuRenderer, &output, 192).unwrap();
    assert!(output.is_file());
}

#[test]
fn a_film_that_cannot_be_started_leaves_no_file() {
    let scene = scene_of("blocked-out-in", "first-render");
    let blocker = scratch("film-written", "blocked-out").join("not-a-folder");
    fs::write(&blocker, b"a file").unwrap();
    let output = blocker.join("first.mp4");
    let failure = write_film(&scene, &mut CpuRenderer, &output, 192).unwrap_err();
    assert!(
        failure.to_string().starts_with("the folder for the file could not be made: "),
        "{failure}"
    );
    assert!(!output.exists());
    assert_eq!(fs::read(&blocker).unwrap(), b"a file", "the file that was in the way is untouched");
}

/// Draws as the processor does until it is asked for its `fail_at`th picture.
struct FailsAt {
    drawn: usize,
    fail_at: usize,
}

impl Renderer for FailsAt {
    fn name(&self) -> &'static str {
        "fails"
    }
    fn draw(&mut self, list: &DisplayList) -> Result<Picture, Error> {
        if self.drawn == self.fail_at {
            return Err(Error::new("the picture could not be drawn"));
        }
        self.drawn += 1;
        CpuRenderer.draw(list)
    }
}

#[test]
fn a_film_that_fails_after_it_was_started_leaves_no_file_and_says_why() {
    let scene = scene_of("failed-out-in", "first-render");
    let output = scratch("film-written", "failed-out").join("first.mp4");
    let failure = write_film(&scene, &mut FailsAt { drawn: 0, fail_at: 5 }, &output, 192).unwrap_err();
    assert!(failure.to_string().contains("the picture could not be drawn"), "{failure}");
    assert!(!output.exists(), "a film that was not finished is removed");
    assert!(output.parent().unwrap().is_dir(), "the folder stays");
}

#[test]
fn an_earlier_file_is_overwritten_by_a_film_that_succeeds() {
    let scene = scene_of("again-out-in", "first-render");
    let output = scratch("film-written", "again-out").join("first.mp4");
    fs::write(&output, b"old").unwrap();
    write_film(&scene, &mut CpuRenderer, &output, 192).unwrap();
    assert_eq!(video_facts(&output).unwrap().frames, 10);
}

#[test]
fn two_films_of_the_same_composition_show_the_same_pictures() {
    let scene = scene_of("twice-out-in", "first-render");
    let folder = scratch("film-written", "twice-out");
    let (one, two) = (folder.join("one.mp4"), folder.join("two.mp4"));
    write_film(&scene, &mut CpuRenderer, &one, 192).unwrap();
    write_film(&scene, &mut CpuRenderer, &two, 192).unwrap();
    let (a, b) = (frames_of(&one), frames_of(&two));
    assert_eq!(a.len(), b.len());
    assert!(
        a.iter().zip(&b).all(|(a, b)| a == b),
        "the decoded pictures of the two films are the same"
    );
}
