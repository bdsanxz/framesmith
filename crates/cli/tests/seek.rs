//! T047: the seek measurement, run on real projects through the
//! `framesmith` command.

mod common;

use common::*;
use measure::{Frame, Rgb};
use std::path::Path;

/// Runs `bench seek` on a project and returns what it prints and its exit code.
fn seek(test: &str, project: &Path) -> (String, i32) {
    let work = scratch(&format!("{test}_work"));
    let args = ["seek", text(project), "--framesmith", BIN, "--work", text(&work)].map(String::from);
    bench::run(&args)
}

fn title_card() -> &'static (String, i32) {
    static ONCE: std::sync::OnceLock<(String, i32)> = std::sync::OnceLock::new();
    ONCE.get_or_init(|| seek("t047_title_card", &fixture("t047_title_card", "title-card")))
}

/// The time of each frame line of a report, in frame order.
fn times(report: &str) -> Vec<f64> {
    report
        .lines()
        .filter_map(|line| line.strip_prefix("  frame "))
        .enumerate()
        .map(|(frame, rest)| {
            let mut words = rest.split_whitespace();
            assert_eq!(words.next(), Some(frame.to_string().as_str()), "{rest}");
            words.next().unwrap().parse().unwrap()
        })
        .collect()
}

#[test]
fn t047_c1_the_measurement_reports_every_frame_of_the_title_card_and_a_summary() {
    let (report, _) = title_card();
    println!("{report}");
    let times = times(report);
    assert_eq!(times.len(), 120, "one time for each of the 120 frames");
    assert!(times.iter().all(|&ms| ms.is_finite() && ms >= 0.0));
    assert!(times.iter().any(|&ms| ms > 0.0), "a real timer was read: {times:?}");
    let largest = times.iter().copied().fold(0.0, f64::max);
    let mut sorted = times.clone();
    sorted.sort_by(f64::total_cmp);
    let summary = report.lines().find(|line| line.starts_with("summary: ")).expect("a summary line");
    assert!(summary.starts_with("summary: 120 frames, largest "), "{summary}");
    assert!(summary.contains(&format!("largest {largest:.3} ms (frame ")), "{summary}");
    assert!(summary.contains(&format!("typical {:.3} ms", sorted[60])), "{summary}");
}

#[test]
fn t047_c2_the_title_card_passes_and_a_page_with_a_slow_seek_fails() {
    let (report, code) = title_card();
    let largest = times(report).into_iter().fold(0.0, f64::max);
    assert!(largest < 2.0, "the largest seek took {largest} ms");
    assert!(report.ends_with("result: pass\n"), "{report}");
    assert_eq!(*code, 0);
    // A page whose script does long work on every frame: its seeks take far more than 2 ms.
    let slow = "<script>\n\
        function work() {\n\
          var sum = 0;\n\
          for (var i = 0; i < 60000000; i += 1) { sum += i % 7; }\n\
          document.getElementById(\"a\").style.width = (20 + sum % 3) + \"px\";\n\
          requestAnimationFrame(work);\n\
        }\n\
        requestAnimationFrame(work);\n\
        </script>";
    let project = project(
        "t047_c2_slow",
        "slow",
        &page(
            "data-fs-size=\"64x36\" data-fs-fps=\"5\" data-fs-duration=\"1\"",
            "#a { position: absolute; left: 0; top: 0; width: 20px; height: 20px; background: #ff0000; }",
            "<div id=\"a\"></div>",
            slow,
        ),
    );
    let (report, code) = seek("t047_c2_slow", &project);
    println!("{report}");
    let times = times(&report);
    assert_eq!(times.len(), 5);
    assert!(times.iter().any(|&ms| ms >= 2.0), "{times:?}");
    assert!(
        report.ends_with("result: fail (a seek took 2 milliseconds or longer)\n"),
        "{report}"
    );
    assert_eq!(code, 1);
}

const RED: Rgb = [255, 0, 0];
const GREEN: Rgb = [0, 255, 0];

fn width(frame: &Frame, colour: Rgb) -> u32 {
    frame.bounds(colour, 90).map_or(0, |bar| bar.width())
}

#[test]
fn t047_c3_a_script_that_reads_the_clock_during_the_measurement_reads_frame_time() {
    // Two bars whose widths are the date and the high-resolution timer:
    // 4 pixels, plus 1 pixel for every 10 milliseconds. At 10 frames a
    // second frame `i` is at `100 × i` milliseconds, so both are `10 × i + 4` wide.
    let script = "<script>\n\
        function draw() {\n\
          document.getElementById(\"red\").style.width = (Date.now() / 10 + 4) + \"px\";\n\
          document.getElementById(\"green\").style.width = (performance.now() / 10 + 4) + \"px\";\n\
          requestAnimationFrame(draw);\n\
        }\n\
        requestAnimationFrame(draw);\n\
        </script>";
    let style = ".bar { position: absolute; left: 0; width: 0; height: 20px; }\n\
        #red { top: 10px; background: #ff0000; } #green { top: 50px; background: #00ff00; }";
    let project = project(
        "t047_c3",
        "clock",
        &page(
            "data-fs-size=\"400x80\" data-fs-fps=\"10\" data-fs-duration=\"3\"",
            style,
            "<div id=\"red\" class=\"bar\"></div><div id=\"green\" class=\"bar\"></div>",
            script,
        ),
    );
    let work = scratch("t047_c3_work");
    // Real time passes before and during the run; the page must not show it.
    let costs = bench::seek::measure(Path::new(BIN), &project, &work).unwrap();
    assert_eq!(costs.len(), 30, "every seek was timed with the real timer");
    assert!(costs.iter().any(|&ms| ms > 0.0), "{costs:?}");
    // The video of the measured run itself: every frame shows its own frame time.
    let frames = measure::frames_from_video(&ffmpeg(), &work.join("seek.mp4"), 400, 80).unwrap();
    assert_eq!(frames.len(), 30);
    for (number, frame) in frames.iter().enumerate() {
        let wanted = 10 * number as u32 + 4;
        for (name, colour) in [("date", RED), ("timer", GREEN)] {
            let seen = width(frame, colour);
            assert!(
                seen.abs_diff(wanted) <= 1,
                "frame {number}: the {name} bar is {seen} wide, not {wanted}"
            );
        }
    }
}

#[test]
fn t047_c4_the_measurement_gives_the_same_result_when_it_is_repeated() {
    let (first, first_code) = title_card();
    let project = fixture("t047_c4", "title-card");
    let (second, second_code) = seek("t047_c4_second", &project);
    let (third, third_code) = seek("t047_c4_third", &project);
    let result = |report: &str| report.lines().last().unwrap().to_string();
    assert_eq!((result(first), *first_code), ("result: pass".to_string(), 0));
    assert_eq!((result(&second), second_code), (result(first), *first_code));
    assert_eq!((result(&third), third_code), (result(first), *first_code));
    assert_eq!((times(&second).len(), times(&third).len()), (120, 120));
}
