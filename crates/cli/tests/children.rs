//! T039: when the browser or `ffmpeg` stops during a render, the command
//! says which one, where, and why, exits 1, and leaves nothing behind.

mod common;

use common::live::{child_of, descendants, running_with, script, signal, still_alive, Running};
use common::*;
use measure::Ran;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const FRAMES: u64 = 300;
const EARLIER: &[u8] = b"an earlier video: these bytes must not change";

struct Place {
    root: PathBuf,
    project: PathBuf,
    tmp: PathBuf,
    saved: PathBuf,
}

/// A render long enough to be caught in the middle: 300 frames.
fn place(test: &str) -> Place {
    let project = project(
        test,
        "demo",
        &page(
            "data-fs-size=\"320x180\" data-fs-fps=\"30\" data-fs-duration=\"10\"",
            "",
            "<p>x</p>",
            "",
        ),
    );
    let root = project.parent().unwrap().to_path_buf();
    let (tmp, saved) = (root.join("tmp"), root.join("saved"));
    std::fs::create_dir_all(&tmp).unwrap();
    Place { root, project, tmp, saved }
}

impl Place {
    fn start(&self, env: &[(&str, &OsStr)]) -> Running {
        let mut all: Vec<(&str, &OsStr)> = vec![
            ("TMPDIR", self.tmp.as_os_str()),
            ("FRAMESMITH_DEBUG_FRAMES", self.saved.as_os_str()),
        ];
        all.extend_from_slice(env);
        Running::start(&self.root, &["render", "demo"], &all)
    }

    /// Waits until the capture has saved at least `count` pictures.
    fn wait_for_frames(&self, count: usize) {
        let began = Instant::now();
        while std::fs::read_dir(&self.saved).map(Iterator::count).unwrap_or(0) < count {
            assert!(
                began.elapsed() < Duration::from_secs(60),
                "the capture did not reach {count} frames"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn saved(&self) -> u64 {
        std::fs::read_dir(&self.saved).map(Iterator::count).unwrap_or(0) as u64
    }

    fn left_running(&self) -> Vec<String> {
        running_with(&format!("{}/", self.root.display()))
    }

    /// c4: no output, no staging file, no output folder, no child, no profile folder.
    fn nothing_is_left(&self, what: &str) {
        assert!(!self.project.join("out").exists(), "{what}: {:?}", tree(&self.project));
        assert_eq!(self.left_running(), Vec::<String>::new(), "{what}");
        assert_eq!(
            std::fs::read_dir(&self.tmp).unwrap().count(),
            0,
            "{what}: the profile folder was left"
        );
    }
}

/// The frame number the message says the capture had reached.
fn frame_named(ran: &Ran) -> u64 {
    let after = ran
        .stderr
        .split("at frame ")
        .nth(1)
        .unwrap_or_else(|| panic!("no frame is named: {}", ran.all()));
    after.split(' ').next().unwrap().parse().unwrap()
}

fn kill_the_browser(place: &Place, env: &[(&str, &OsStr)]) -> Ran {
    let running = place.start(env);
    place.wait_for_frames(20);
    let browser = child_of(running.pid(), "remote-debugging-pipe").expect("the run's browser");
    signal(browser, "KILL");
    running.finish()
}

fn kill_ffmpeg(place: &Place) -> Ran {
    let running = place.start(&[]);
    place.wait_for_frames(20);
    let ffmpeg = child_of(running.pid(), "ffmpeg").expect("the run's ffmpeg");
    signal(ffmpeg, "KILL");
    running.finish()
}

/// A stand-in `ffmpeg`, first on the search path, that is the real one
/// without its H.264 encoder: it asks the real ffmpeg for an encoder it does not have.
fn without_the_encoder(place: &Place) -> String {
    let tools = place.root.join("tools");
    std::fs::create_dir_all(&tools).unwrap();
    script(
        &tools.join("ffmpeg"),
        &format!(
            "for word in \"$@\"; do\n  shift\n  [ \"$word\" = libx264 ] && word=libx264_is_not_in_this_build\n  set -- \"$@\" \"$word\"\ndone\nexec \"{}\" \"$@\"",
            ffmpeg().display()
        ),
    );
    format!("{}:{}", tools.display(), std::env::var("PATH").unwrap())
}

fn render_without_the_encoder(place: &Place) -> Ran {
    let path = without_the_encoder(place);
    place.start(&[("PATH", OsStr::new(&path))]).finish()
}

#[test]
fn t039_c1_a_killed_browser_exits_1_and_names_the_browser_the_capture_stage_and_the_frame_reached() {
    let place = place("t039_c1");
    let ran = kill_the_browser(&place, &[]);
    assert_eq!(ran.code, 1, "{}", ran.all());
    assert!(ran.stdout.contains("render stopped in capture:"), "{}", ran.all());
    assert!(ran.stderr.contains("error FS-R011"), "{}", ran.all());
    assert!(
        ran.stderr
            .contains("The browser stopped or reported an error during capture, at frame "),
        "{}",
        ran.all()
    );
    assert!(ran.stderr.contains(&format!(" of {FRAMES}: ")), "{}", ran.all());
    // The frame named is the one the capture had reached: it was killed
    // after 20 pictures were saved, and no picture past the named frame exists.
    let (named, saved) = (frame_named(&ran), place.saved());
    assert!((20..FRAMES).contains(&named), "frame {named}: {}", ran.all());
    assert!(
        saved == named || saved == named + 1,
        "frame {named} is named and {saved} pictures were saved"
    );
    assert!(!Path::new(&place.saved).join(format!("frame-{:05}.png", named + 1)).exists());
    println!("{}{}", ran.stdout.lines().last().unwrap_or_default(), ran.stderr);
}

#[test]
fn t039_c2_a_killed_ffmpeg_exits_1_and_names_ffmpeg_and_the_stage() {
    let place = place("t039_c2");
    let ran = kill_ffmpeg(&place);
    assert_eq!(ran.code, 1, "{}", ran.all());
    assert!(ran.stdout.contains("render stopped in capture:"), "{}", ran.all());
    assert!(ran.stderr.contains("error FS-R012"), "{}", ran.all());
    assert!(
        ran.stderr.contains("ffmpeg stopped or reported an error during capture, at frame "),
        "{}",
        ran.all()
    );
    assert!(ran.stderr.contains("signal: 9"), "the way it ended is given: {}", ran.all());
    assert!((20..FRAMES).contains(&frame_named(&ran)), "{}", ran.all());
    println!("{}{}", ran.stdout.lines().last().unwrap_or_default(), ran.stderr);
}

#[test]
fn t039_c3_an_ffmpeg_without_the_h264_encoder_exits_1_and_gives_ffmpegs_own_reason() {
    let place = place("t039_c3");
    // What the real ffmpeg itself says when asked for that encoder.
    let asked = measure::run(
        &ffmpeg(),
        &[
            "-hide_banner",
            "-f",
            "lavfi",
            "-i",
            "color=c=black:s=16x16:d=0.1",
            "-c:v",
            "libx264_is_not_in_this_build",
            "-f",
            "null",
            "-",
        ],
        &place.root,
        &[],
    );
    let reason = "Unknown encoder 'libx264_is_not_in_this_build'";
    assert!(asked.code != 0 && asked.stderr.contains(reason), "{}", asked.all());

    let ran = render_without_the_encoder(&place);
    assert_eq!(ran.code, 1, "{}", ran.all());
    assert!(ran.stdout.contains("render stopped in capture:"), "{}", ran.all());
    assert!(ran.stderr.contains("error FS-R012"), "{}", ran.all());
    assert!(
        ran.stderr
            .contains("The installed ffmpeg cannot encode H.264; it stopped during capture"),
        "{}",
        ran.all()
    );
    assert!(ran.stderr.contains(reason), "{}", ran.all());
    assert!(
        ran.stderr
            .contains("fix: Install an ffmpeg build that includes the libx264 encoder."),
        "{}",
        ran.all()
    );
    println!("{}{}", ran.stdout.lines().last().unwrap_or_default(), ran.stderr);
}

#[test]
fn t039_c4_after_each_failure_no_output_no_staging_file_and_no_child_is_left() {
    // The browser is killed: the ffmpeg that survives it is stopped.
    let first = place("t039_c4_browser");
    let running = first.start(&[]);
    first.wait_for_frames(20);
    let staged: Vec<String> = std::fs::read_dir(first.project.join("out"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert!(staged.len() == 1 && staged[0].ends_with(".part"), "{staged:?}");
    let (browser, ffmpeg) = (
        child_of(running.pid(), "remote-debugging-pipe").unwrap(),
        child_of(running.pid(), "ffmpeg").unwrap(),
    );
    let started = descendants(running.pid());
    signal(browser, "KILL");
    assert_eq!(running.finish().code, 1);
    assert!(!common::live::alive(ffmpeg), "the surviving ffmpeg is still running");
    assert_eq!(still_alive(&started), Vec::<u32>::new(), "of {started:?}");
    first.nothing_is_left("browser killed");

    // ffmpeg is killed: the browser that survives it is stopped.
    let second = place("t039_c4_ffmpeg");
    let running = second.start(&[]);
    second.wait_for_frames(20);
    let (browser, ffmpeg) = (
        child_of(running.pid(), "remote-debugging-pipe").unwrap(),
        child_of(running.pid(), "ffmpeg").unwrap(),
    );
    let started = descendants(running.pid());
    assert!(started.len() > 2, "a browser, its helpers and ffmpeg: {started:?}");
    signal(ffmpeg, "KILL");
    assert_eq!(running.finish().code, 1);
    assert!(!common::live::alive(browser), "the surviving browser is still running");
    assert_eq!(still_alive(&started), Vec::<u32>::new(), "of {started:?}");
    second.nothing_is_left("ffmpeg killed");

    // ffmpeg cannot encode: the browser is stopped.
    let third = place("t039_c4_encoder");
    assert_eq!(render_without_the_encoder(&third).code, 1);
    third.nothing_is_left("no encoder");
}

/// One way a render fails mid-run.
type Failure = fn(&Place) -> Ran;

fn failures() -> [(&'static str, Failure); 3] {
    [
        ("browser", |place| kill_the_browser(place, &[])),
        ("ffmpeg", kill_ffmpeg),
        ("encoder", render_without_the_encoder),
    ]
}

#[test]
fn t039_c5_an_earlier_video_is_unchanged_after_each_failure() {
    for (name, fail) in failures() {
        let place = place(&format!("t039_c5_{name}"));
        write(&place.project, "out/demo.mp4", EARLIER);
        let ran = fail(&place);
        assert_eq!(ran.code, 1, "{name}: {}", ran.all());
        assert_eq!(std::fs::read(place.project.join("out/demo.mp4")).unwrap(), EARLIER, "{name}");
        assert_eq!(
            std::fs::read_dir(place.project.join("out")).unwrap().count(),
            1,
            "{name}: something was left beside it"
        );
        assert_eq!(place.left_running(), Vec::<String>::new(), "{name}");
    }
}

#[test]
fn t039_c6_with_both_programs_healthy_the_next_render_completes_from_the_first_frame() {
    for (name, fail) in failures() {
        let place = place(&format!("t039_c6_{name}"));
        assert_eq!(fail(&place).code, 1, "{name}");
        let again = run_in(&place.root, &["render", "demo"], &[("TMPDIR", place.tmp.as_os_str())]);
        assert_eq!(again.code, 0, "{name}: {}", again.all());
        let captures: Vec<&str> = again.stdout.lines().filter(|line| line.starts_with("capture")).collect();
        assert_eq!(captures.first(), Some(&"capture   0/300"), "{name}: {}", again.stdout);
        assert_eq!(captures.last(), Some(&"capture   300/300"), "{name}: {}", again.stdout);
        assert_eq!(facts(&place.project.join("out/demo.mp4")).frames, FRAMES, "{name}");
        assert_eq!(std::fs::read_dir(place.project.join("out")).unwrap().count(), 1, "{name}");
    }
}
