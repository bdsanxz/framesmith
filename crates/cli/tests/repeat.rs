//! T025, T026: the same project gives the same frames, whenever it is
//! rendered and in whatever order its frames are captured.

mod common;

use common::*;
use measure::{same_frames, Frame, Rgb};
use std::ffi::OsStr;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;

const GROUND: Rgb = [0x14, 0x21, 0x3d];

/// Renders a copy of the repeat fixture: clips, an image, text and tweens.
fn render(test: &str, env: &[(&str, &OsStr)]) -> (PathBuf, Vec<Frame>) {
    let project = fixture(test, "repeat");
    let (ran, frames) = render_frames(&project, &[], env);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert_eq!(frames.len(), 60, "2 seconds at 30 frames per second");
    (project, frames)
}

/// One forward render, shared by the comparisons.
fn forward() -> &'static [Frame] {
    static ONCE: OnceLock<Vec<Frame>> = OnceLock::new();
    ONCE.get_or_init(|| render("repeat_forward", &[]).1)
}

fn order(written: &'static str) -> [(&'static str, &'static OsStr); 1] {
    [("FRAMESMITH_DEBUG_ORDER", OsStr::new(written))]
}

/// When a frame's picture was saved: later for a frame captured later.
fn saved(project: &Path, frame: u32) -> std::time::SystemTime {
    let file = project.parent().unwrap().join(format!("repeat-frames/frame-{frame:05}.png"));
    std::fs::metadata(&file)
        .unwrap_or_else(|e| panic!("{}: {e}", file.display()))
        .modified()
        .unwrap()
}

fn word_shows(frame: &Frame) -> bool {
    !frame.crop(0, 0, 190, 60).is_all(GROUND, 0)
}

fn picture_shows(frame: &Frame) -> bool {
    !frame.crop(180, 0, 140, 96).is_all(GROUND, 0)
}

#[test]
fn the_fixture_has_what_the_comparison_needs() {
    let frames = forward();
    // Text and an image inside clips, and something that moves on most frames.
    assert!(!word_shows(&frames[5]) && word_shows(&frames[6]) && word_shows(&frames[41]) && !word_shows(&frames[42]));
    assert!(!picture_shows(&frames[14]) && picture_shows(&frames[15]) && picture_shows(&frames[44]) && !picture_shows(&frames[45]));
    let mut distinct: Vec<u64> = frames.iter().map(Frame::hash).collect();
    distinct.sort_unstable();
    distinct.dedup();
    assert!(distinct.len() >= 40, "only {} different pictures", distinct.len());
    assert!(frames.iter().all(|frame| (frame.width, frame.height) == (320, 180)));
}

#[test]
fn t025_c1_two_renders_give_the_same_hash_for_every_frame() {
    let (_, second) = render("t025_c1", &[]);
    assert_eq!(same_frames(forward(), &second), Ok(()));
    let (first, second): (Vec<u64>, Vec<u64>) = (
        forward().iter().map(Frame::hash).collect(),
        second.iter().map(Frame::hash).collect(),
    );
    assert_eq!(first, second);
}

/// Other work loading the machine: one busy program for each processor, twice over.
struct Load(Vec<Child>);

impl Load {
    fn start() -> Load {
        let busy = 2 * std::thread::available_parallelism().map_or(4, usize::from);
        Load(
            (0..busy)
                .map(|_| Command::new("/usr/bin/yes").stdout(Stdio::null()).spawn().expect("yes starts"))
                .collect(),
        )
    }
}

impl Drop for Load {
    fn drop(&mut self) {
        for child in &mut self.0 {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[test]
fn t025_c2_a_render_made_while_the_machine_is_loaded_gives_the_same_hashes() {
    let first = forward();
    let load = Load::start();
    let started = std::time::Instant::now();
    let (_, second) = render("t025_c2", &[]);
    let loaded = started.elapsed();
    assert!(load.0.len() >= 2);
    drop(load);
    println!(
        "render under load took {:.1} s with the busy programs running",
        loaded.as_secs_f64()
    );
    assert_eq!(same_frames(first, &second), Ok(()));
}

#[test]
fn t025_c3_one_differing_pixel_of_one_frame_is_reported_with_its_frame_number() {
    let mut second = forward().to_vec();
    let at = ((100 * 320 + 17) * 3) as usize;
    second[41].rgb[at] ^= 1;
    assert_eq!(
        same_frames(forward(), &second),
        Err("frame 41 differs: its first differing pixel is at (17, 100)".to_string())
    );
}

#[test]
fn t025_c4_a_display_scale_factor_of_two_leaves_the_hashes_and_the_size_as_they_are() {
    // A browser that is told its display has a scale factor of 2, as on a
    // high-density screen: the last such switch on its command line wins.
    let dir = scratch("t025_c4_browser");
    let script = dir.join("scaled-browser");
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\necho \"$@\" >> '{}'\nexec '{}' \"$@\" --force-device-scale-factor=2\n",
            dir.join("started").display(),
            browser().display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let env = [("FRAMESMITH_BROWSER", script.as_os_str())];
    let (_, first) = render("t025_c4_first", &env);
    let (_, second) = render("t025_c4_second", &env);
    assert!(dir.join("started").exists(), "the scaled browser was the one used");
    assert!(first.iter().all(|frame| (frame.width, frame.height) == (320, 180)));
    assert_eq!(same_frames(&first, &second), Ok(()));
    // And the same as on a display with a scale factor of 1.
    assert_eq!(same_frames(forward(), &first), Ok(()));
    // The video keeps the declared size too.
    let project = fixture("t025_c4_video", "repeat");
    let ran = run_in(&project, &["render", "."], &env);
    assert_eq!(ran.code, 0, "{}", ran.all());
    let video = facts(&project.join("out").join("repeat.mp4"));
    assert_eq!((video.width, video.height, video.frames), (320, 180, 60));
}

#[test]
fn t026_c1_capture_from_the_last_frame_to_the_first_gives_the_same_hashes() {
    let (project, reverse) = render("t026_c1", &order("reverse"));
    assert!(
        saved(&project, 59) < saved(&project, 58) && saved(&project, 1) < saved(&project, 0),
        "captured last to first"
    );
    assert_eq!(same_frames(forward(), &reverse), Ok(()));
}

#[test]
fn t026_c2_a_frame_inside_a_tween_captured_directly_after_the_last_frame_is_the_same() {
    // Frame 25: inside the tweens of the first box (0.3 to 1.2 s) and of the picture (0.5 to 1 s).
    // Frame 50: inside the tween that runs past the end of the video (1.5 to 3 s).
    let (project, jumped) = render("t026_c2", &order("59,25,59,50"));
    assert!(saved(&project, 59) < saved(&project, 25) && saved(&project, 25) < saved(&project, 50));
    assert!(saved(&project, 50) < saved(&project, 0), "the listed frames came first");
    assert_ne!(forward()[25].hash(), forward()[24].hash(), "frame 25 is inside a tween");
    assert_ne!(forward()[50].hash(), forward()[49].hash(), "frame 50 is inside a tween");
    assert_eq!(jumped[25].hash(), forward()[25].hash());
    assert_eq!(same_frames(forward(), &jumped), Ok(()));
}

#[test]
fn t026_c3_frames_at_the_edges_of_a_clip_captured_right_after_the_other_side_are_the_same() {
    // The word's window is frames 6 to 41, the picture's is frames 15 to 44.
    // Shown directly after hidden, entering from both ends of each window.
    let (project, entering) = render("t026_c3_entering", &order("5,6,42,41,14,15,45,44"));
    for pair in [[5, 6], [6, 42], [42, 41], [41, 14], [14, 15], [15, 45], [45, 44]] {
        assert!(saved(&project, pair[0]) < saved(&project, pair[1]), "{pair:?}");
    }
    assert_eq!(same_frames(forward(), &entering), Ok(()));
    // Hidden directly after shown.
    let (project, leaving) = render("t026_c3_leaving", &order("6,5,41,42,15,14,44,45"));
    for pair in [[6, 5], [5, 41], [41, 42], [42, 15], [15, 14], [14, 44], [44, 45]] {
        assert!(saved(&project, pair[0]) < saved(&project, pair[1]), "{pair:?}");
    }
    assert_eq!(same_frames(forward(), &leaving), Ok(()));
    for frames in [&entering, &leaving] {
        assert!(!word_shows(&frames[5]) && word_shows(&frames[6]) && word_shows(&frames[41]) && !word_shows(&frames[42]));
        assert!(!picture_shows(&frames[14]) && picture_shows(&frames[15]) && picture_shows(&frames[44]) && !picture_shows(&frames[45]));
    }
}

#[test]
fn t026_c4_a_differing_hash_between_the_two_orders_is_reported_with_its_frame_number() {
    let (_, mut reverse) = render("t026_c4", &order("reverse"));
    assert_eq!(same_frames(forward(), &reverse), Ok(()));
    reverse[17].rgb[0] ^= 1;
    assert_eq!(
        same_frames(forward(), &reverse),
        Err("frame 17 differs: its first differing pixel is at (0, 0)".to_string())
    );
}
