//! The facts and the pictures frx7 gives of a video, held to FFmpeg's. Yardstick tests: they start
//! `ffprobe` and `ffmpeg` from outside, to compare (`docs/dev/gates.md`, section 5). The comparison
//! tool itself starts neither.

mod common;

use common::folder;
use common::pictures::{moving, with_block, HEIGHT, WIDTH};
use parity::difference;

fn yardstick_the_facts_frx7_reads_are_the_facts_ffprobe_reads() {
    let dir = folder("yardstick_video_facts");
    let video = dir.join("video.mp4");
    measure::write_video(&video, (30, 1), &moving(24, WIDTH, HEIGHT)).unwrap();
    let ours = measure::video_facts(&video).unwrap();
    let ffprobe = measure::tool("ffprobe").expect("ffprobe");
    let theirs = measure::facts(&ffprobe, &video).unwrap();
    assert_eq!(ours.frames, theirs.frames, "frame count");
    assert_eq!((ours.width, ours.height), (theirs.width, theirs.height), "size");
    assert_eq!(theirs.frame_rate, format!("{}/{}", ours.rate.0, ours.rate.1), "frame rate");
    assert!(
        (ours.seconds() - theirs.duration).abs() < 1e-6,
        "{} against {}",
        ours.seconds(),
        theirs.duration
    );
    assert_eq!(theirs.codec, ours.codec);
}

fn yardstick_the_pictures_frx7_decodes_are_the_pictures_ffmpeg_decodes() {
    let dir = folder("yardstick_video_pictures");
    let video = dir.join("video.mp4");
    // Picture 6 has a block of green over a fifth of its rows: the pictures are not all alike.
    measure::write_video(&video, (30, 1), &with_block(&moving(24, WIDTH, HEIGHT), 6, 20)).unwrap();
    let ffmpeg = measure::tool("ffmpeg").expect("ffmpeg");
    let theirs = measure::frames_from_video(&ffmpeg, &video, WIDTH, HEIGHT).unwrap();
    let mut reader = measure::open_pictures(&video).unwrap();
    assert_eq!(reader.count(), theirs.len() as u64);
    for (number, other) in theirs.iter().enumerate() {
        let frame = reader.next_frame().unwrap().expect("a picture");
        let found = difference::compare(&frame, other).unwrap();
        // The two converters round the chroma of hard edges differently: on 24 pictures of squares
        // the most that differ by the measure is 30 of 14,400 pixels (0.2 %). Half a percent is the bound.
        assert!(
            found.differing * 200 <= found.pixels,
            "picture {number}: {} of {} pixels differ by the measure",
            found.differing,
            found.pixels
        );
        let total: u64 = frame.rgb.iter().zip(&other.rgb).map(|(a, b)| u64::from(a.abs_diff(*b))).sum();
        let mean = total as f64 / frame.rgb.len() as f64;
        assert!(mean < 2.0, "picture {number}: mean difference of {mean} levels");
    }
    assert!(reader.next_frame().unwrap().is_none());
}

measure::yardstick_tests!(
    measure::yardstick::Needs::ffmpeg();
    yardstick_the_facts_frx7_reads_are_the_facts_ffprobe_reads,
    yardstick_the_pictures_frx7_decodes_are_the_pictures_ffmpeg_decodes,
);
