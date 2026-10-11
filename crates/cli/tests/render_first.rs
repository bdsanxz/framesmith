//! T004, T005, T006, T008: the first render, the frame count, the frame
//! rate of a run, and the default rate.

mod common;

use common::*;
use std::ffi::OsStr;

const TINY: &str = "data-fs-size=\"64x36\"";

fn tiny(test: &str, attrs: &str) -> std::path::PathBuf {
    project(test, "tiny", &page(&format!("{TINY} {attrs}"), "", "<p>x</p>", ""))
}

fn rendered_frames(test: &str, attrs: &str, args: &[&str]) -> (u64, String) {
    let dir = tiny(test, attrs);
    let mut all = vec!["render", text(&dir)];
    all.extend_from_slice(args);
    let ran = framesmith(&all);
    assert_eq!(ran.code, 0, "{}", ran.all());
    let facts = facts(&dir.join("out/tiny.mp4"));
    (facts.frames, facts.frame_rate)
}

#[test]
fn t004_c1_c2_c3_c4_a_still_composition_renders_to_a_playable_mp4() {
    let dir = fixture("t004_still", "still");
    let ran = framesmith(&["render", text(&dir)]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    let video = dir.join("out/still.mp4");
    assert!(video.is_file(), "the video is at <project>/out/<project folder name>.mp4");

    // c2: one H.264 video stream, 8-bit 4:2:0, 1280x720, 30 fps, 60 frames, no audio.
    let facts = facts(&video);
    assert_eq!((facts.video_streams, facts.audio_streams), (1, 0));
    assert_eq!((facts.codec.as_str(), facts.pixel_format.as_str()), ("h264", "yuv420p"));
    assert_eq!(
        (facts.width, facts.height, facts.frame_rate.as_str(), facts.frames),
        (1280, 720, "30/1", 60)
    );

    // c4: the index comes before the picture data.
    assert!(facts.index_first);

    // c3: every frame decodes from start to end without a complaint.
    let decoded = measure::run(&ffmpeg(), &["-v", "error", "-i", text(&video), "-f", "null", "-"], &dir, &[]);
    assert_eq!((decoded.code, decoded.stderr.trim()), (0, ""));
    assert!((facts.duration - 2.0).abs() < 0.001, "the stream lasts {} s", facts.duration);
}

#[test]
fn t004_c5_a_frame_shows_the_shapes_at_the_declared_size() {
    let dir = fixture("t004_pixels", "still");
    let (ran, frames) = render_frames(&dir, &[], &[]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert_eq!(frames.len(), 60);
    for frame in [&frames[0], &frames[59]] {
        assert_eq!((frame.width, frame.height), (1280, 720));
        // One output pixel per CSS pixel: the block is at 100,100 and is 400 by 200.
        let block = frame.bounds([0xe6, 0x39, 0x46], 0).unwrap();
        assert_eq!((block.left, block.top, block.width(), block.height()), (100, 100, 400, 200));
        let disc = frame.bounds([0x2a, 0x9d, 0x8f], 0).unwrap();
        // The edge pixels of a circle are blends, so the solid part may stop one pixel short.
        assert!(disc.left.abs_diff(800) <= 1 && disc.top.abs_diff(360) <= 1, "{disc:?}");
        assert!(disc.width().abs_diff(240) <= 2 && disc.height().abs_diff(240) <= 2, "{disc:?}");
        // The text is painted in its own colour, left edge near 100.
        let word = frame.bounds([0xf1, 0xfa, 0xee], 0).unwrap();
        assert!(word.left >= 100 && word.left <= 110 && word.top >= 400, "{word:?}");
    }
    // The same holds in the encoded file, within what the encoder changes.
    let video = measure::frames_from_video(&ffmpeg(), &dir.join("out/still.mp4"), 1280, 720).unwrap();
    assert_eq!(video.len(), 60);
    let block = video[30].bounds([0xe6, 0x39, 0x46], 24).unwrap();
    assert!(block.left.abs_diff(100) <= 1 && block.top.abs_diff(100) <= 1, "{block:?}");
    assert!(block.width().abs_diff(400) <= 2 && block.height().abs_diff(200) <= 2, "{block:?}");
}

#[test]
fn t004_c6_a_composition_with_no_content_shows_its_background_on_every_frame() {
    let dir = fixture("t004_empty", "empty");
    let (ran, frames) = render_frames(&dir, &[], &[]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert_eq!(frames.len(), 30);
    for (number, frame) in frames.iter().enumerate() {
        assert!(frame.is_all([0x3a, 0x6e, 0xa5], 0), "frame {number} is not all background");
    }
    // With no background of its own the picture is black.
    let plain = project(
        "t004_empty_black",
        "plain",
        &page("data-fs-size=\"64x36\" data-fs-duration=\"0.2\"", "", "", ""),
    );
    let (ran, frames) = render_frames(&plain, &[], &[]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert!(frames.iter().all(|frame| frame.is_all([0, 0, 0], 0)));
}

#[test]
fn t004_c7_a_project_path_with_spaces_and_other_scripts_renders() {
    let dir = project(
        "t004 path with spaces",
        "été 日本 folder",
        &page(&format!("{TINY} data-fs-duration=\"0.5\""), "", "<p>x</p>", ""),
    );
    let ran = framesmith(&["render", text(&dir)]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    let facts = facts(&dir.join("out/été 日本 folder.mp4"));
    assert_eq!((facts.frames, facts.width, facts.height), (15, 64, 36));
}

#[test]
fn t004_c8_a_browser_the_user_has_open_is_unchanged_after_a_render() {
    let dir = tiny("t004_c8", "data-fs-duration=\"0.5\"");
    let root = dir.parent().unwrap().to_path_buf();
    // The user's own browser: open before the render, with its own profile and one page.
    let user = common::live::UserBrowser::start(&root);
    let windows = user.windows().expect("the user's browser answers");
    assert_eq!(windows.len(), 1, "{windows:?}");
    // A home folder with nothing in it: a browser started without a profile
    // folder of its own would write the user's default profile under it.
    let (home, tmp) = (root.join("home"), root.join("tmp"));
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&tmp).unwrap();
    let browser = browser();
    let ran = run_in(
        &root,
        &["render", "tiny"],
        &[
            ("HOME", home.as_os_str()),
            ("TMPDIR", tmp.as_os_str()),
            ("FRAMESMITH_BROWSER", browser.as_os_str()),
        ],
    );
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert_eq!(facts(&dir.join("out/tiny.mp4")).frames, 15);
    // Windows and tabs: the user's browser is the same process, with the same pages open.
    assert!(common::live::alive(user.pid()), "the user's browser was stopped");
    assert_eq!(user.windows(), Some(windows));
    // Profile: nothing was written under the user's home, and the profile
    // folder the run made for its own browser is gone again.
    assert_eq!(tree(&home).len(), 0, "{:?}", tree(&home));
    assert_eq!(std::fs::read_dir(&home).unwrap().count(), 0);
    assert_eq!(std::fs::read_dir(&tmp).unwrap().count(), 0);
    // The only program left from this test's folder is the user's browser.
    let left = common::live::running_with(&format!("{}/", root.display()));
    assert!(
        !left.is_empty() && left.iter().all(|line| line.contains("the-users-own-profile")),
        "{left:?}"
    );
}

#[test]
fn t004_c9_the_only_change_in_the_project_is_the_output_file() {
    let dir = fixture("t004_tree", "empty");
    let before = tree(&dir);
    let ran = framesmith(&["render", text(&dir)]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    let mut after = tree(&dir);
    assert!(after.remove(std::path::Path::new("out/empty.mp4")).is_some());
    assert_eq!(before, after);
}

#[test]
fn t004_c10_t005_c5_the_specs_for_stages_protocol_and_frame_time_exist() {
    for (name, about) in [
        ("render-stages.md", "# Render stages"),
        ("page-protocol.md", "# Page protocol"),
        ("frame-time.md", "# Frame"),
    ] {
        let spec = spec(name);
        assert!(spec.starts_with(about), "{name} starts with {:?}", spec.lines().next());
        assert!(spec.contains("Status: v"), "{name} states its version");
    }
}

#[test]
fn t005_c1_six_seconds_gives_144_180_and_360_frames() {
    for (fps, frames) in [(24, 144), (30, 180), (60, 360)] {
        let got = rendered_frames(
            &format!("t005_c1_{fps}"),
            &format!("data-fs-fps=\"{fps}\" data-fs-duration=\"6\""),
            &[],
        );
        assert_eq!(got, (frames, format!("{fps}/1")));
    }
}

#[test]
fn t005_c2_c3_a_part_frame_adds_one_frame_unless_it_is_within_a_thousandth() {
    let attrs = |length: &str| format!("data-fs-fps=\"30\" data-fs-duration=\"{length}\"");
    assert_eq!(rendered_frames("t005_c2", &attrs("6.01"), &[]).0, 181);
    // 6.00001 s is 180.0003 frames: within 0.001 of 180.
    assert_eq!(rendered_frames("t005_c3_near", &attrs("6.00001"), &[]).0, 180);
    // 6.0001 s is 180.003 frames: past the tolerance, so one more frame.
    assert_eq!(rendered_frames("t005_c3_past", &attrs("6.0001"), &[]).0, 181);
}

#[test]
fn t005_c4_a_length_of_zero_or_less_makes_no_video() {
    for (name, length) in [("zero", "0"), ("negative", "-2")] {
        let dir = tiny(&format!("t005_c4_{name}"), &format!("data-fs-duration=\"{length}\""));
        let ran = framesmith(&["render", text(&dir)]);
        assert_eq!(ran.code, 1, "{}", ran.all());
        assert!(ran.stderr.contains("error FS-E010 index.html:11"), "{}", ran.stderr);
        assert!(
            ran.stderr.contains(&format!("The composition length is {length} seconds")),
            "{}",
            ran.stderr
        );
        assert!(!dir.join("out").exists());
    }
}

#[test]
fn t006_c1_c2_c3_fps_changes_the_rate_of_one_run_only() {
    let dir = tiny("t006_fps", "data-fs-fps=\"30\" data-fs-duration=\"6\"");
    let before = tree(&dir);
    let ran = framesmith(&["render", text(&dir), "--fps", "24"]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    let video = dir.join("out/tiny.mp4");
    let with = facts(&video);
    assert_eq!((with.frames, with.frame_rate.as_str()), (144, "24/1"));
    assert!((with.duration - 6.0).abs() < 0.001);

    // c3: no source file changed.
    let mut after = tree(&dir);
    after.remove(std::path::Path::new("out/tiny.mp4"));
    assert_eq!(before, after);

    // c2: without the option the written rate is used again.
    let ran = framesmith(&["render", text(&dir)]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    let without = facts(&video);
    assert_eq!((without.frames, without.frame_rate.as_str()), (180, "30/1"));
}

#[test]
fn t006_c4_a_bad_fps_value_stops_before_anything_starts() {
    let dir = tiny("t006_bad", "data-fs-fps=\"30\" data-fs-duration=\"1\"");
    let (spy_browser, started) = spy(dir.parent().unwrap(), "browser-spy", &browser());
    for value in ["0", "-5", "23.976", "fast", "241", ""] {
        let ran = run_in(
            &dir,
            &["render", ".", "--fps", value],
            &[("FRAMESMITH_BROWSER", spy_browser.as_os_str())],
        );
        assert_eq!(ran.code, 1, "--fps {value}: {}", ran.all());
        assert!(ran.stderr.contains("error FS-R004"), "{}", ran.stderr);
        assert!(ran.stderr.contains("a whole number from 1 to 240"), "{}", ran.stderr);
        assert!(!started.exists(), "no browser is started for --fps {value}");
        assert!(!dir.join("out").exists());
    }
    // A good value with the same stand-in does start it, so the check above means something.
    let ran = run_in(
        &dir,
        &["render", ".", "--fps", "5"],
        &[("FRAMESMITH_BROWSER", OsStr::new(&spy_browser))],
    );
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert!(started.exists());
}

#[test]
fn t008_c3_no_written_rate_renders_at_30() {
    assert_eq!(rendered_frames("t008_c3", "data-fs-duration=\"2\"", &[]), (60, "30/1".to_string()));
}

#[test]
fn a_composition_far_smaller_than_a_browser_window_renders_at_its_exact_size() {
    // A full browser build keeps a strip of its window for itself; a window as short as these
    // compositions has no room left for the page (see `render-stages.md`). The left half of the
    // page is red and the rest blue, so a page laid out at any other size shows at once.
    let style = "[data-fs-composition] { background: rgb(0, 0, 255); }\n\
                 #left { position: fixed; left: 0; top: 0; width: 50vw; height: 100vh; background: rgb(255, 0, 0); }";
    for (name, width, height) in [("dot", 16_u32, 16_u32), ("strip", 320, 50)] {
        let attrs = format!("data-fs-size=\"{width}x{height}\" data-fs-duration=\"0.1\"");
        let dir = project(&format!("small_{name}"), name, &page(&attrs, style, "<div id=\"left\"></div>", ""));
        let (ran, frames) = render_frames(&dir, &[], &[]);
        assert_eq!(ran.code, 0, "{}", ran.all());
        assert_eq!(frames.len(), 3, "{}", ran.all());
        for frame in &frames {
            assert_eq!((frame.width, frame.height), (width, height));
            for (at, pixel) in frame.rgb.as_chunks::<3>().0.iter().enumerate() {
                let x = at as u32 % width;
                let expected = if x < width / 2 { [255, 0, 0] } else { [0, 0, 255] };
                assert_eq!(*pixel, expected, "{name}: the pixel at column {x}, row {}", at as u32 / width);
            }
        }
        let facts = facts(&dir.join(format!("out/{name}.mp4")));
        assert_eq!((facts.width, facts.height, facts.frames), (width, height, 3));
    }
}
