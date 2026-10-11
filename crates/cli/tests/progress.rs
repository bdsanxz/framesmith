//! T037: following a render by stage and frame count, in a terminal and
//! when the output is not a terminal.

mod common;

use common::*;
use measure::Ran;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

const STAGES: [&str; 5] = ["compile", "probe", "capture", "encode", "commit"];

fn composition(test: &str, size: &str, fps: u32, seconds: u32) -> PathBuf {
    project(
        test,
        "demo",
        &page(
            &format!("data-fs-size=\"{size}\" data-fs-fps=\"{fps}\" data-fs-duration=\"{seconds}\""),
            "",
            "<p>x</p>",
            "",
        ),
    )
}

/// Runs `framesmith render demo ...` from the test's scratch folder, so the
/// project is written the short way, as a user would type it.
fn render(dir: &Path, extra: &[&str], env: &[(&str, &OsStr)]) -> Ran {
    let mut args = vec!["render", "demo"];
    args.extend_from_slice(extra);
    run_in(dir.parent().unwrap(), &args, env)
}

/// The same command given a pseudo-terminal for standard output and standard
/// error, so the command sees a terminal.
fn render_in_terminal(dir: &Path, extra: &[&str], env: &[(&str, &OsStr)]) -> Ran {
    let mut args = vec!["-q", "/dev/null", BIN, "render", "demo"];
    args.extend_from_slice(extra);
    measure::run(Path::new("/usr/bin/script"), &args, dir.parent().unwrap(), env)
}

/// Every state the display went through: the text between line breaks and
/// carriage returns, without terminal control characters. The terminal helper
/// itself echoes the end-of-input key as `^D` when it starts; that is not ours.
fn states(out: &str) -> Vec<String> {
    out.replacen("^D", "", 1)
        .split(['\r', '\n'])
        .map(|part| part.chars().filter(|c| !c.is_control()).collect::<String>().trim().to_string())
        .filter(|part| !part.is_empty())
        .collect()
}

/// The `done/total` pairs of every capture state.
fn counters(states: &[String]) -> Vec<(u64, u64)> {
    states
        .iter()
        .filter_map(|state| state.strip_prefix("capture"))
        .filter_map(|rest| rest.trim().split_once('/'))
        .map(|(done, total)| (done.parse().unwrap(), total.parse().unwrap()))
        .collect()
}

fn lines(out: &str) -> Vec<&str> {
    out.lines().filter(|line| !line.trim().is_empty()).collect()
}

#[test]
fn t037_c1_in_a_terminal_the_five_stage_names_appear_in_order() {
    let dir = composition("t037_c1", "64x36", 10, 1);
    let ran = render_in_terminal(&dir, &[], &[]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    let seen = states(&ran.stdout);
    // The first state that starts with each stage name is in the order of the stages.
    let first: Vec<usize> = STAGES
        .iter()
        .map(|stage| {
            seen.iter()
                .position(|state| state.starts_with(stage))
                .unwrap_or_else(|| panic!("no {stage} in {seen:?}"))
        })
        .collect();
    assert!(first.windows(2).all(|pair| pair[0] < pair[1]), "{first:?} in {seen:?}");
    // Each name is shown on its own as soon as its stage begins, before the stage reports.
    for stage in STAGES {
        assert!(seen.iter().any(|state| state == stage), "{stage} was not shown alone in {seen:?}");
    }
    assert!(ran.stdout.contains('\r'), "the display rewrites the line in a terminal");
}

#[test]
fn t037_c1_when_the_output_is_not_a_terminal_the_stages_are_lines_in_order() {
    let dir = composition("t037_c1_plain", "64x36", 10, 1);
    let ran = render(&dir, &[], &[]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert!(!ran.stdout.contains('\r'));
    let shown = lines(&ran.stdout);
    let stage_lines: Vec<&str> = shown
        .iter()
        .filter(|line| STAGES.iter().any(|s| line.starts_with(s)))
        .copied()
        .collect();
    let order: Vec<&str> = stage_lines.iter().map(|line| line.split_whitespace().next().unwrap()).collect();
    let mut once = order.clone();
    once.dedup();
    assert_eq!(once, STAGES, "{shown:?}");
}

#[test]
fn t037_c2_the_probe_line_shows_size_rate_length_and_frame_count() {
    // 100 frames: 5 s at 20 fps, 96x54.
    let dir = composition("t037_c2", "96x54", 20, 5);
    let ran = render_in_terminal(&dir, &[], &[]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    let seen = states(&ran.stdout);
    let probe = seen
        .iter()
        .find(|state| state.starts_with("probe") && state.contains("done"))
        .expect("a finished probe line");
    let words: Vec<&str> = probe.split_whitespace().collect();
    assert_eq!(
        words,
        ["probe", "done", "96x54,", "20", "fps,", "5", "s,", "100", "frames"],
        "{probe}"
    );
    // The values are the ones the render used.
    let video = facts(&dir.join("out/demo.mp4"));
    assert_eq!(
        (video.width, video.height, video.frame_rate.as_str(), video.frames),
        (96, 54, "20/1", 100)
    );
    // The same line when the output is not a terminal.
    let plain = render(&dir, &[], &[]);
    assert!(
        plain
            .stdout
            .lines()
            .any(|line| line == "probe     done      96x54, 20 fps, 5 s, 100 frames"),
        "{}",
        plain.stdout
    );
}

/// A 180 frame composition: 64x36, 30 fps, 6 s.
fn long_run(test: &str) -> PathBuf {
    composition(test, "64x36", 30, 6)
}

#[test]
fn t037_c3_in_a_terminal_the_counter_advances_from_0_to_180_and_never_passes_it() {
    let dir = long_run("t037_c3_terminal");
    let ran = render_in_terminal(&dir, &[], &[]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    let counts = counters(&states(&ran.stdout));
    assert_eq!(counts.first(), Some(&(0, 180)), "{counts:?}");
    assert_eq!(counts.last(), Some(&(180, 180)), "{counts:?}");
    assert!(counts.iter().all(|&(done, total)| total == 180 && done <= 180), "{counts:?}");
    assert!(
        counts.windows(2).all(|pair| pair[0].0 <= pair[1].0),
        "the counter went back: {counts:?}"
    );
    // Every number from 0 to 180 is shown in a terminal: the line is rewritten for each frame.
    let shown: Vec<u64> = counts.iter().map(|&(done, _)| done).collect();
    assert!((0..=180).all(|n| shown.contains(&n)), "{shown:?}");
}

#[test]
fn t037_c3_when_the_output_is_not_a_terminal_the_counter_is_printed_at_the_start_about_each_second_and_the_end() {
    let dir = long_run("t037_c3_plain");
    let ran = render(&dir, &[], &[]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    let counts = counters(&states(&ran.stdout));
    assert_eq!(counts.first(), Some(&(0, 180)), "{counts:?}");
    assert_eq!(counts.last(), Some(&(180, 180)), "{counts:?}");
    assert!(counts.iter().all(|&(done, total)| total == 180 && done <= 180), "{counts:?}");
    assert!(counts.windows(2).all(|pair| pair[0].0 < pair[1].0), "{counts:?}");
    assert!(counts.len() >= 2, "{counts:?}");
}

#[test]
fn t037_c4_the_summary_shows_the_output_path_180_frames_size_rate_and_elapsed_time() {
    let dir = long_run("t037_c4");
    for terminal in [false, true] {
        let ran = if terminal {
            render_in_terminal(&dir, &["--output", "result/video.mp4"], &[])
        } else {
            render(&dir, &["--output", "result/video.mp4"], &[])
        };
        assert_eq!(ran.code, 0, "{}", ran.all());
        let seen = states(&ran.stdout);
        // The output path is the one the author typed.
        assert!(seen.iter().any(|state| state == "commit    result/video.mp4"), "{seen:?}");
        let last = seen.last().unwrap();
        let words: Vec<&str> = last.split_whitespace().collect();
        assert_eq!(words.len(), 9, "{last}");
        assert_eq!(
            (words[..7].to_vec(), words[8]),
            (vec!["Rendered", "180", "frames,", "64x36,", "30", "fps,", "in"], "s"),
            "{last}"
        );
        let elapsed: f64 = words[7].parse().unwrap_or_else(|_| panic!("elapsed time in {last}"));
        assert!(elapsed > 0.0 && elapsed < 120.0, "{elapsed}");
        // The file is the one the summary talks about.
        let video = facts(&dir.parent().unwrap().join("result/video.mp4"));
        assert_eq!(
            (video.frames, video.width, video.height, video.frame_rate.as_str()),
            (180, 64, 36, "30/1")
        );
    }
}

#[test]
fn t037_c5_with_fps_24_the_summary_shows_24_as_the_rate_used() {
    // Written at 30 fps; the run asks for 24, so 6 s is 144 frames.
    let dir = long_run("t037_c5");
    let ran = render(&dir, &["--fps", "24"], &[]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    let last = lines(&ran.stdout).last().copied().unwrap().to_string();
    assert!(last.starts_with("Rendered 144 frames, 64x36, 24 fps, in "), "{last}");
    assert!(
        ran.stdout
            .lines()
            .any(|line| line == "probe     done      64x36, 24 fps, 6 s, 144 frames"),
        "{}",
        ran.stdout
    );
    assert_eq!(facts(&dir.join("out/demo.mp4")).frame_rate, "24/1");
}

/// A page whose script calls the script interface, which stops the capture.
fn capture_stops(test: &str) -> PathBuf {
    let script = "<script>setTimeout(function () { window[[\"frame\", \"smith\"].join(\"\")].timeline(); }, 500);</script>";
    project(
        test,
        "demo",
        &page(
            "data-fs-size=\"64x36\" data-fs-fps=\"10\" data-fs-duration=\"5\"",
            "",
            "<p>x</p>",
            script,
        ),
    )
}

/// A page with an image that is not a picture, which stops the probe.
fn probe_stops(test: &str) -> PathBuf {
    let dir = project(
        test,
        "demo",
        &page(
            "data-fs-size=\"64x36\" data-fs-fps=\"10\" data-fs-duration=\"1\"",
            "",
            "<img src=\"broken.png\">",
            "",
        ),
    );
    write(&dir, "broken.png", "this is not a picture");
    dir
}

#[test]
fn t037_c6_a_run_that_stops_early_ends_with_the_stage_and_the_cause() {
    // In the probe.
    let dir = probe_stops("t037_c6_probe");
    let ran = render(&dir, &[], &[]);
    assert_eq!(ran.code, 1, "{}", ran.all());
    let last = lines(&ran.stdout).last().copied().unwrap().to_string();
    assert_eq!(last, "render stopped in probe: The image broken.png did not load in the page.");
    assert!(!ran.stdout.contains("Rendered"));

    // In the capture, after the counter began.
    let dir = capture_stops("t037_c6_capture");
    let ran = render(&dir, &[], &[]);
    assert_eq!(ran.code, 2, "{}", ran.all());
    let shown = lines(&ran.stdout);
    assert!(shown.iter().any(|line| line.starts_with("capture   0/")), "{shown:?}");
    let last = shown.last().unwrap();
    assert!(last.starts_with("render stopped in capture: "), "{last}");
    assert!(last.len() > "render stopped in capture: ".len() + 10, "the cause is stated: {last}");
    assert!(!ran.stdout.contains("Rendered"));
}

#[test]
fn t037_c6_in_a_terminal_the_last_line_is_on_its_own_line_after_the_stage_it_stopped_in() {
    let dir = probe_stops("t037_c6_terminal");
    let ran = render_in_terminal(&dir, &[], &[]);
    assert_eq!(ran.code, 1, "{}", ran.all());
    let seen = states(&ran.stdout);
    assert_eq!(
        seen.last().unwrap(),
        "render stopped in probe: The image broken.png did not load in the page.",
        "{seen:?}"
    );
    assert!(
        seen.iter().any(|state| state == "probe"),
        "the stage it stopped in was shown: {seen:?}"
    );
}

fn forbidden_in_colourless_output(out: &str) {
    assert!(!out.contains('\u{1b}'), "an escape sequence is in the output: {out:?}");
}

#[test]
fn t037_c7_without_colour_every_stage_the_counter_and_the_result_are_still_text() {
    let dir = composition("t037_c7", "64x36", 10, 1);
    let no_colour: [(&str, &OsStr); 2] = [("NO_COLOR", OsStr::new("1")), ("TERM", OsStr::new("dumb"))];
    for terminal in [false, true] {
        let ran = if terminal {
            render_in_terminal(&dir, &[], &no_colour)
        } else {
            render(&dir, &[], &no_colour)
        };
        assert_eq!(ran.code, 0, "{}", ran.all());
        forbidden_in_colourless_output(&ran.all());
        let seen = states(&ran.stdout);
        for stage in STAGES {
            assert!(seen.iter().any(|state| state.starts_with(stage)), "{stage} missing in {seen:?}");
        }
        assert!(seen.contains(&"compile   done".to_string()));
        assert!(seen.contains(&"encode    done".to_string()));
        assert!(seen
            .iter()
            .any(|state| state == "probe     done      64x36, 10 fps, 1 s, 10 frames"));
        assert_eq!(counters(&seen).last(), Some(&(10, 10)));
        assert!(seen.iter().any(|state| state == "commit    demo/out/demo.mp4"));
        assert!(seen.last().unwrap().starts_with("Rendered 10 frames, 64x36, 10 fps, in "));
    }
    // With the colour setting left alone, and on a stop, there is still none.
    let ran = render_in_terminal(&composition("t037_c7_default", "64x36", 10, 1), &[], &[]);
    forbidden_in_colourless_output(&ran.all());
    let ran = render_in_terminal(&probe_stops("t037_c7_stop"), &[], &[]);
    forbidden_in_colourless_output(&ran.all());
}

#[test]
fn t037_c8_no_remaining_time_and_no_percentage_is_shown() {
    let dir = long_run("t037_c8");
    let words = [
        "remaining",
        "left",
        "eta",
        "estimate",
        "estimated",
        "to go",
        "countdown",
        "%",
        "percent",
    ];
    for terminal in [false, true] {
        let ran = if terminal {
            render_in_terminal(&dir, &[], &[])
        } else {
            render(&dir, &[], &[])
        };
        assert_eq!(ran.code, 0, "{}", ran.all());
        for state in states(&ran.stdout) {
            let lower = state.to_lowercase();
            for word in words {
                assert!(!lower.contains(word), "{word:?} in {state:?}");
            }
        }
    }
    // The only time stated is the elapsed time of the finished run, on the last line.
    let ran = render(&dir, &[], &[]);
    let timed: Vec<&str> = ran.stdout.lines().filter(|line| line.contains(" s")).collect();
    assert!(
        timed.iter().all(|line| line.starts_with("probe ") || line.starts_with("Rendered ")),
        "{timed:?}"
    );
}
