//! The footage element: how it is read, and which picture of the source a frame needs.

use poc::model::{Composition, Kind};
use poc::scene::Evaluator;
use poc::text::Shaper;
use poc::{layout, xml};
use std::path::Path;

fn load(source: &str) -> Composition {
    let mut composition = xml::parse(source, Path::new(".")).expect("the composition is valid");
    layout::lay_out(&mut composition, &mut Shaper::new(), &[]);
    composition
}

/// The videos a frame needs, each with the picture of its source it shows.
fn shown(composition: &Composition, frame: u64) -> Vec<(String, u64)> {
    let described = Evaluator::new(composition).frame(frame);
    described
        .footage
        .iter()
        .map(|footage| (footage.node.id.clone(), footage.source_frame))
        .collect()
}

#[test]
fn a_video_is_read_with_its_source_and_its_place_among_the_videos() {
    let composition = load(
        r#"<composition width="64" height="32" fps="30" duration="4">
  <video id="a" src="clips/a.mp4" width="64" height="32"/>
  <video id="b" src="b.mp4" left="8" top="4" width="32" height="16" start="1" duration="2"/>
</composition>"#,
    );
    let read: Vec<(&Path, usize)> = composition
        .root
        .children
        .iter()
        .map(|element| match &element.kind {
            Kind::Video { source, video, .. } => (source.as_path(), *video),
            _ => panic!("every element is a video"),
        })
        .collect();
    assert_eq!(read, [(Path::new("clips/a.mp4"), 0), (Path::new("b.mp4"), 1)]);
    let second = composition.root.children[1].rect;
    assert_eq!((second.x0, second.y0, second.width(), second.height()), (8.0, 4.0, 32.0, 16.0));
}

#[test]
fn the_picture_shown_counts_from_the_start_of_the_clip() {
    let composition = load(
        r#"<composition width="64" height="32" fps="30" duration="4">
  <video id="a" src="a.mp4" width="32" height="32"/>
  <video id="b" src="b.mp4" left="32" width="32" height="32" start="1" duration="2"/>
</composition>"#,
    );
    assert_eq!(shown(&composition, 0), [("a".to_string(), 0)]);
    assert_eq!(shown(&composition, 30), [("a".to_string(), 30), ("b".to_string(), 0)]);
    assert_eq!(shown(&composition, 40), [("a".to_string(), 40), ("b".to_string(), 10)]);
    assert_eq!(shown(&composition, 90), [("a".to_string(), 90)]);
}

#[test]
fn a_clip_that_starts_inside_its_source_shows_the_picture_at_that_place() {
    let composition = load(
        r#"<composition width="64" height="32" fps="30" duration="4">
  <video id="cut" src="a.mp4" width="64" height="32" start="1" duration="2" media-start="45.6667"/>
</composition>"#,
    );
    // 45.6667 s is picture 1370 of a source of 30 pictures a second.
    assert_eq!(shown(&composition, 30), [("cut".to_string(), 1370)]);
    assert_eq!(shown(&composition, 45), [("cut".to_string(), 1385)]);
}

#[test]
fn footage_wholly_under_an_opaque_box_that_fills_the_frame_is_not_asked_for() {
    let composition = load(
        r##"<composition width="64" height="32" fps="30" duration="4">
  <video id="under" src="a.mp4" width="64" height="32"/>
  <box id="scene" background="#101010" start="1" duration="1">
    <rect left="4" top="4" width="8" height="8" background="#ffffff"/>
  </box>
  <video id="over" src="b.mp4" left="16" width="32" height="16" start="1.5" duration="1"/>
</composition>"##,
    );
    assert_eq!(shown(&composition, 0), [("under".to_string(), 0)]);
    assert_eq!(shown(&composition, 30), []);
    assert_eq!(shown(&composition, 50), [("over".to_string(), 5)]);
    assert_eq!(shown(&composition, 60), [("under".to_string(), 60), ("over".to_string(), 15)]);
    // A scene that lets the footage through is no cover.
    let faded = load(
        r##"<composition width="64" height="32" fps="30" duration="4">
  <video id="under" src="a.mp4" width="64" height="32"/>
  <box id="scene" background="#101010" opacity="0.5"/>
</composition>"##,
    );
    assert_eq!(shown(&faded, 0), [("under".to_string(), 0)]);
}

#[test]
fn a_video_without_a_source_or_with_a_part_pixel_size_is_a_problem() {
    let source = "<composition width=\"64\" height=\"32\" duration=\"1\">\n  <video width=\"64\" height=\"32\"/>\n  <video src=\"a.mp4\" width=\"10.5\" height=\"32\"/>\n</composition>";
    let problems = xml::parse(source, Path::new(".")).err().expect("the file has problems");
    let found: Vec<(u32, &str)> = problems.iter().map(|p| (p.line, p.message.as_str())).collect();
    assert_eq!(
        found,
        [
            (2, "<video> needs the attribute `src`"),
            (3, "`width` of a <video> must be a whole number of 2 or more, not 10.5"),
        ]
    );
}

#[test]
fn times_written_a_hair_off_a_frame_land_on_the_frame_they_mean() {
    // At 30 frames a second: 1.6667 s is a hair over frame 50, 3.0333 s a hair under frame 91,
    // and 1.68 s is truly between frames 50 and 51.
    let composition = load(
        r#"<composition width="64" height="32" fps="30" duration="6">
  <video id="over" src="a.mp4" width="32" height="32" start="1.6667" duration="1" media-start="0.6667"/>
  <video id="under" src="b.mp4" left="32" width="32" height="32" start="3.0333" duration="1" media-start="2.0333"/>
  <box start="4" duration="2">
    <video id="between" src="c.mp4" width="32" height="32" start="0.68" duration="1"/>
  </box>
</composition>"#,
    );
    // Frame 50 is the first of `over`, and shows source picture 20 (0.6667 s is a hair over it).
    assert_eq!(shown(&composition, 49), []);
    assert_eq!(shown(&composition, 50), [("over".to_string(), 20)]);
    assert_eq!(shown(&composition, 79), [("over".to_string(), 49)]);
    assert_eq!(shown(&composition, 80), []);
    // Frame 91 is the first of `under`, and shows source picture 61.
    assert_eq!(shown(&composition, 90), []);
    assert_eq!(shown(&composition, 91), [("under".to_string(), 61)]);
    // 4 + 0.68 s is 140.4 frames: the clip starts on the next frame, 141, with its first picture.
    assert_eq!(shown(&composition, 140), []);
    assert_eq!(shown(&composition, 141), [("between".to_string(), 0)]);
}
