//! Sprint 2, ticket T011: a clip whose start is written `after:<clip id>` followed by `+` or `-` and a
//! time starts at the end of the named clip, moved by the offset, with exact arithmetic. `check` prints
//! each resolved start beside the written form, the render shows the clip from the frame the frame grid
//! rule gives for the exact sum, an offset that is not a time and a start before zero are errors with
//! codes of their own, `render` refuses both and starts nothing, and the page resolves the same start.
//!
//! Notes for maintainers
//! ---------------------
//! * The times are read from the text of the built binary and from the pictures of real renders; the
//!   expected values are written out here from the way each project is written, not taken from the code.
//!   In c2 the expected frame is worked out in this file from integers: every written time is turned into
//!   a whole number of nano-seconds (nine decimals at most), the two are added, and the frame grid rule
//!   (`frame-time.md`) is applied to that integer: multiply by the rate; when what is left over after the
//!   whole frames is at most one thousandth of a frame take the whole number, otherwise the next one. No
//!   floating point is used on the way.
//! * The fixtures are `fixtures/relative-offset*`; the other projects are written out in this file.
//! * Only the render tests (c1, c2, and the page tests at the end) need a browser and `ffmpeg`; the others
//!   read files only. The refusing tests use stand-ins that note being started.
//! * Scratch folders: every test names its projects with a scratch key of its own (the tests of a file run
//!   in parallel, and a key names a folder that is emptied when it is made).

mod common;

use common::*;
use measure::{Frame, Ran, Rgb};
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

const RED: Rgb = [255, 0, 0];
const GREEN: Rgb = [0, 255, 0];
const BLUE: Rgb = [0, 0, 255];
const YELLOW: Rgb = [255, 255, 0];
/// The pixels of one box of the fixtures (60 by 60).
const BOX: usize = 3600;

// ---------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------

fn check(dir: &Path) -> Ran {
    framesmith(&["check", text(dir)])
}

/// One printed diagnostic header: kind word, code, file and line.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Listed {
    kind: String,
    code: String,
    file: String,
    line: u32,
}

fn listed(output: &str) -> Vec<Listed> {
    output
        .lines()
        .filter(|line| !line.starts_with(' '))
        .filter_map(|line| {
            let mut words = line.split_whitespace();
            let (kind, code) = (words.next()?, words.next()?);
            if !matches!(kind, "error" | "unsupported" | "unknown" | "warning") || !code.starts_with("FS-") {
                return None;
            }
            let (file, digits) = words.next()?.rsplit_once(':')?;
            Some(Listed {
                kind: kind.to_string(),
                code: code.to_string(),
                file: file.to_string(),
                line: digits.parse().ok()?,
            })
        })
        .collect()
}

fn codes(output: &str) -> Vec<String> {
    listed(output).into_iter().map(|item| item.code).collect()
}

/// The line numbers (from 1) of the lines of a file of a fixture that hold `needle`.
fn lines_with(folder: &str, needle: &str) -> Vec<u32> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(folder)
        .join("index.html");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        .lines()
        .enumerate()
        .filter(|(_, line)| line.contains(needle))
        .map(|(index, _)| index as u32 + 1)
        .collect()
}

/// Stand-ins for the browser and for `ffmpeg` that note being started.
struct Spies {
    dir: PathBuf,
    browser: PathBuf,
    browser_marker: PathBuf,
    ffmpeg_marker: PathBuf,
}

impl Spies {
    fn new(key: &str) -> Spies {
        let dir = scratch(&format!("{key}_spies"));
        let (browser, browser_marker) = spy(&dir, "browser", &common::browser());
        let (_, ffmpeg_marker) = spy(&dir, "ffmpeg", &common::ffmpeg());
        Spies {
            dir,
            browser,
            browser_marker,
            ffmpeg_marker,
        }
    }

    fn run(&self, cwd: &Path, args: &[&str]) -> Ran {
        let path: OsString = format!("{}:{}", self.dir.display(), std::env::var("PATH").unwrap_or_default()).into();
        run_in(
            cwd,
            args,
            &[("FRAMESMITH_BROWSER", self.browser.as_os_str()), ("PATH", path.as_os_str())],
        )
    }

    fn started(&self) -> (bool, bool) {
        (self.browser_marker.exists(), self.ffmpeg_marker.exists())
    }
}

const BOX_STYLE: &str = ".box { position: absolute; top: 60px; width: 60px; height: 60px; }\n\
#a { left: 20px; background: #ff0000; }\n\
#b { left: 100px; background: #00ff00; }\n\
#c { left: 180px; background: #0000ff; }\n\
#d { left: 250px; background: #ffff00; }";

/// A 320 by 180 project at `fps` frames a second, `length` seconds long, with the given clips in the body.
fn boxes_for(test: &str, fps: u32, length: &str, clips: &str) -> PathBuf {
    let attrs = format!("data-fs-size=\"320x180\" data-fs-fps=\"{fps}\" data-fs-duration=\"{length}\"");
    project(test, "boxes", &page(&attrs, BOX_STYLE, clips, ""))
}

fn boxes(test: &str, fps: u32, clips: &str) -> PathBuf {
    boxes_for(test, fps, "5", clips)
}

/// A clip element that is a box of its own colour.
fn clip(id: &str, start: &str, duration: Option<&str>) -> String {
    let duration = duration.map_or(String::new(), |d| format!(" data-fs-duration=\"{d}\""));
    format!("<div id=\"{id}\" class=\"box\" data-fs-clip data-fs-start=\"{start}\"{duration}></div>\n")
}

/// The frames on which a whole box of the colour is painted; a part of a box is a failure.
fn shown(frames: &[Frame], colour: Rgb) -> Vec<usize> {
    let mut found = Vec::new();
    for (number, frame) in frames.iter().enumerate() {
        match frame.count(colour, 0) {
            0 => {}
            BOX => found.push(number),
            part => panic!("frame {number} shows {part} pixels of the box, neither none nor all"),
        }
    }
    found
}

fn range(first: usize, end: usize) -> Vec<usize> {
    (first..end).collect()
}

// ---------------------------------------------------------------------
// c1: the resolved start, beside the written form, and in the render
// ---------------------------------------------------------------------

#[test]
fn s2_t011_c1_check_prints_the_start_with_a_plus_and_with_a_minus_offset_beside_the_written_form_and_info_and_timeline_agree() {
    let dir = fixture("s2_t011_c1_check", "relative-offset");
    let checked = check(&dir);
    assert_eq!(checked.code, 0, "{}", checked.all());
    // #intro ends at 3 s: +0.5 is 3.5 s, -0.5 is 2.5 s, and "after: intro + 1" is 4 s.
    assert_eq!(
        checked.stdout,
        "check passed\n\
         \x20 composition  main\n\
         \x20 size         320x180\n\
         \x20 rate         30 fps\n\
         \x20 length       5 s\n\
         \x20 frames       150\n\
         \x20 clips        4\n\
         \x20 tweens       0\n\
         \x20 clip #later  start 3.5 s (after:intro+0.5)\n\
         \x20 clip #sooner  start 2.5 s (after:intro-0.5)\n\
         \x20 clip #spaced  start 4 s (after: intro + 1)\n"
    );
    let info = framesmith(&["info", text(&dir)]);
    assert_eq!(info.code, 0, "{}", info.all());
    assert!(
        info.stdout.ends_with(
            "clip #later  start 3.5 s (after:intro+0.5)\nclip #sooner  start 2.5 s (after:intro-0.5)\nclip #spaced  start 4 s (after: intro + 1)\n"
        ),
        "{}",
        info.stdout
    );
    let timeline = framesmith(&["timeline", text(&dir)]);
    assert_eq!(timeline.code, 0, "{}", timeline.all());
    assert_eq!(
        timeline.stdout,
        "main  320x180  30 fps  5 s  150 frames\n\
         \x20 #intro   1 s   to 3 s    2 s\n\
         \x20 #later   3.5 s to 4.5 s  1 s\n\
         \x20 #sooner  2.5 s to 3.5 s  1 s\n\
         \x20 #spaced  4 s   to 4.5 s  0.5 s\n"
    );
    let json = framesmith(&["timeline", text(&dir), "--json"]);
    let clips = document(&json)["clips"].clone();
    assert_eq!(clips[1]["start"], 3.5);
    assert_eq!(clips[2]["start"], 2.5);
}

#[test]
fn s2_t011_c1_the_render_hides_each_follower_on_the_frame_before_its_start_and_shows_it_on_its_start_frame() {
    let dir = fixture("s2_t011_c1_render", "relative-offset");
    let (ran, frames) = render_frames(&dir, &[], &[]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert_eq!(frames.len(), 150);
    // 30 frames a second. #later: 3.5 s is frame 105. #sooner: 2.5 s is frame 75.
    assert_eq!(frames[104].count(GREEN, 0), 0, "the plus follower is shown a frame early");
    assert_eq!(
        frames[105].count(GREEN, 0),
        BOX,
        "the plus follower is not shown on its start frame"
    );
    assert_eq!(frames[74].count(BLUE, 0), 0, "the minus follower is shown a frame early");
    assert_eq!(frames[75].count(BLUE, 0), BOX, "the minus follower is not shown on its start frame");
    // Whole runs. #intro 1 s to 3 s (30..90); #later 3.5 s for 1 s (105..135); #sooner 2.5 s for 1 s
    // (75..105); #spaced "after: intro + 1" is 4 s for 0.5 s (120..135).
    assert_eq!(shown(&frames, RED), range(30, 90));
    assert_eq!(shown(&frames, GREEN), range(105, 135));
    assert_eq!(shown(&frames, BLUE), range(75, 105));
    assert_eq!(shown(&frames, YELLOW), range(120, 135));
    // The minus follower overlaps the named clip: both show on frames 75 to 89.
    for (number, frame) in frames.iter().enumerate().take(90).skip(75) {
        assert_eq!(frame.count(RED, 0), BOX, "#intro on frame {number}");
        assert_eq!(frame.count(BLUE, 0), BOX, "#sooner on frame {number}");
    }
    // The plus follower leaves a gap: nothing of the named clip or of #later on frames 90 to 104 (except #sooner).
    for (number, frame) in frames.iter().enumerate().take(105).skip(90) {
        assert_eq!(frame.count(RED, 0), 0, "#intro on frame {number}");
        assert_eq!(frame.count(GREEN, 0), 0, "#later on frame {number}");
    }
}

#[test]
fn s2_t011_c1_the_render_follows_the_offset_at_another_rate() {
    // 24 frames a second: #intro 1 s..3 s is frames 24..72; #later 3.5 s is frame 84; #sooner 2.5 s is
    // frame 60; #spaced 4 s is frame 96 and lasts 12 frames.
    let dir = fixture("s2_t011_c1_render_24", "relative-offset");
    let (ran, frames) = render_frames(&dir, &["--fps", "24"], &[]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert_eq!(frames.len(), 120);
    assert_eq!(shown(&frames, RED), range(24, 72));
    assert_eq!(shown(&frames, GREEN), range(84, 108));
    assert_eq!(shown(&frames, BLUE), range(60, 84));
    assert_eq!(shown(&frames, YELLOW), range(96, 108));
}

#[test]
fn s2_t011_c1_the_forms_of_the_spelling_all_give_the_same_start_and_are_printed_as_written() {
    // #a ends at 2 s. Spaces may stand around the sign; the written form is printed as written.
    let written = [
        "after:a+0.5",
        "after:a +0.5",
        "after:a+ 0.5",
        "after:a + 0.5",
        "after:  a  +  0.5  ",
        "after:a+.5",
    ];
    let mut body = clip("a", "1", Some("1"));
    for (i, form) in written.iter().enumerate() {
        body.push_str(&clip(&format!("f{i}"), form, Some("0.25")));
    }
    let dir = boxes("s2_t011_c1_spelling", 30, &body);
    let checked = check(&dir);
    assert_eq!(checked.code, 0, "{}", checked.all());
    for (i, form) in written.iter().enumerate() {
        assert!(
            checked.stdout.contains(&format!("  clip #f{i}  start 2.5 s ({})\n", form.trim())),
            "{form}: {}",
            checked.stdout
        );
    }
    // The minus forms give 1.5 s, and zero gives the end itself.
    let body = format!(
        "{}{}{}{}{}",
        clip("a", "1", Some("1")),
        clip("m", "after:a - 0.5", Some("0.25")),
        clip("n", "after:a-.5", Some("0.25")),
        clip("z", "after:a+0", Some("0.25")),
        clip("y", "after:a-0", Some("0.25"))
    );
    let dir = boxes("s2_t011_c1_spelling_minus", 30, &body);
    let checked = check(&dir);
    assert_eq!(checked.code, 0, "{}", checked.all());
    for line in [
        "  clip #m  start 1.5 s (after:a - 0.5)\n",
        "  clip #n  start 1.5 s (after:a-.5)\n",
        "  clip #z  start 2 s (after:a+0)\n",
        "  clip #y  start 2 s (after:a-0)\n",
    ] {
        assert!(checked.stdout.contains(line), "{line}: {}", checked.stdout);
    }
}

#[test]
fn s2_t011_c1_a_follower_with_an_offset_finds_a_named_clip_written_after_it_or_inside_another() {
    let body = "<div id=\"outer\" class=\"box\" data-fs-clip data-fs-start=\"0\" data-fs-duration=\"4\">\n\
                <div id=\"inner\" class=\"box\" data-fs-clip data-fs-start=\"1\" data-fs-duration=\"1.5\"></div>\n\
                </div>\n"
        .to_string()
        + &clip("b", "after:later-0.25", Some("1"))
        + &clip("c", "after:inner+1", Some("1"))
        + &clip("later", "2", Some("0.5"));
    let dir = boxes("s2_t011_c1_where", 30, &body);
    let checked = check(&dir);
    assert_eq!(checked.code, 0, "{}", checked.all());
    assert!(
        checked.stdout.contains("  clip #b  start 2.25 s (after:later-0.25)\n"),
        "{}",
        checked.stdout
    );
    assert!(
        checked.stdout.contains("  clip #c  start 3.5 s (after:inner+1)\n"),
        "{}",
        checked.stdout
    );
}

#[test]
fn s2_t011_c1_a_text_that_is_a_clip_id_names_that_clip_otherwise_it_is_read_as_an_offset_and_otherwise_it_names_nothing() {
    // `after:scene-2`: the clip `scene-2` (ends 1 s); else the clip `scene` (ends 5 s) less 2 s = 3 s; else nothing.
    let scene = clip("scene", "0", Some("5"));
    let scene2 = clip("scene-2", "0", Some("1"));
    let b = clip("b", "after:scene-2", Some("1"));

    let both = boxes("s2_t011_c1_ids_both", 30, &format!("{scene}{scene2}{b}"));
    let ran = check(&both);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert!(ran.stdout.contains("  clip #b  start 1 s (after:scene-2)\n"), "{}", ran.stdout);

    let only_scene = boxes("s2_t011_c1_ids_scene", 30, &format!("{scene}{b}"));
    let ran = check(&only_scene);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert!(ran.stdout.contains("  clip #b  start 3 s (after:scene-2)\n"), "{}", ran.stdout);

    let neither = boxes("s2_t011_c1_ids_neither", 30, &format!("{}{b}", clip("x", "0", Some("1"))));
    let ran = check(&neither);
    assert_eq!(ran.code, 1, "{}", ran.all());
    assert_eq!(codes(&ran.stdout), ["FS-E050"]);
    assert!(
        ran.stdout.contains(
            "The clip #b starts after \"scene-2\", but no clip with the id \"scene-2\" is in this composition; read as the clip \"scene\" and the offset \"-2\", no clip with the id \"scene\" is in it either; only an element marked data-fs-clip can be named."
        ),
        "{}",
        ran.stdout
    );
}

// ---------------------------------------------------------------------
// c2: the sum is exact, the frame is the frame grid rule's
// ---------------------------------------------------------------------

/// A written time with at most nine decimals, as a whole number of billionths of a second.
fn billionths(text: &str) -> i128 {
    let (negative, digits) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.strip_prefix('+').unwrap_or(text)),
    };
    let (whole, fraction) = digits.split_once('.').unwrap_or((digits, ""));
    assert!(fraction.len() <= 9, "{text}");
    let fraction = format!("{fraction:0<9}");
    let value = whole.parse::<i128>().unwrap() * 1_000_000_000 + fraction.parse::<i128>().unwrap();
    if negative {
        -value
    } else {
        value
    }
}

/// The frame grid rule on a time given in billionths of a second, with integers only: multiply by the
/// rate; when what is left over, as a part of a frame, is at most one thousandth, take the whole number
/// below, otherwise the next whole number.
fn grid_frame(time: i128, fps: i128) -> i128 {
    let scale: i128 = 1_000_000_000;
    let product = time * fps;
    let (whole, left) = (product.div_euclid(scale), product.rem_euclid(scale));
    if left * 1000 <= scale {
        whole
    } else {
        whole + 1
    }
}

/// `(end of the named clip, offset with its sign, rate, the frame worked out here by hand)`.
const EXACT: &[(&str, &str, u32, i128)] = &[
    // 0.1 + 0.2 is exactly 0.3 s, which is 3 frames at 10 a second.
    ("0.1", "+0.2", 10, 3),
    // 4.001 frames is exactly one thousandth above frame 4, inside the tolerance: frame 4.
    ("0.4", "+0.0001", 10, 4),
    // 3.999 frames: exactly one thousandth below frame 4, inside the tolerance: frame 4.
    ("0.4", "-0.0001", 10, 4),
    // About a third of a frame (0.011111111 s at 30 a second is 0.33333333 of a frame).
    ("1", "+0.011111111", 30, 31),
    ("1", "-0.011111111", 30, 30),
    // An end just below a frame time (0.999999999 s is 29.99999997 frames).
    ("0.999999999", "+0.011111111", 30, 31),
    ("0.999999999", "-0.011111111", 30, 30),
    // A third of a frame at 60 and at 24 a second.
    ("0.5", "+0.005555556", 60, 31),
    ("0.5", "-0.005555556", 60, 30),
    ("1", "+0.013888889", 24, 25),
    ("1", "-0.013888889", 24, 24),
];

#[test]
fn s2_t011_c2_the_expected_frames_worked_out_from_integers_are_the_ones_written_in_the_table() {
    for (end, offset, fps, frame) in EXACT {
        let sum = billionths(end) + billionths(offset);
        assert_eq!(grid_frame(sum, i128::from(*fps)), *frame, "{end} {offset} at {fps}");
    }
    // The first case, by hand: 0.1 + 0.2 is 300000000 billionths, times 10 is 3000000000, exactly 3 frames.
    assert_eq!(billionths("0.1") + billionths("0.2"), 300_000_000);
}

#[test]
fn s2_t011_c2_the_clip_appears_on_the_frame_the_frame_grid_rule_gives_for_the_exact_sum() {
    for (index, (end, offset, fps, frame)) in EXACT.iter().enumerate() {
        let written = format!("after:a{offset}");
        // #a starts at 0 and lasts `end`; #b follows. The composition lasts 2 s.
        let body = format!("{}{}", clip("a", "0", Some(end)), clip("b", &written, Some("0.5")));
        let dir = boxes_for(&format!("s2_t011_c2_{index}"), *fps, "2", &body);

        // `check` prints the exact sum.
        let sum = billionths(end) + billionths(offset);
        let seconds = {
            let digits = format!("{:09}", sum.abs() % 1_000_000_000);
            let digits = digits.trim_end_matches('0');
            let whole = sum.abs() / 1_000_000_000;
            if digits.is_empty() {
                format!("{whole}")
            } else {
                format!("{whole}.{digits}")
            }
        };
        let checked = check(&dir);
        assert_eq!(checked.code, 0, "{end} {offset}: {}", checked.all());
        assert!(
            checked.stdout.contains(&format!("  clip #b  start {seconds} s ({written})\n")),
            "{end} {offset}: {}",
            checked.stdout
        );

        // The render: hidden on the frame before, shown on the frame the rule gives.
        let (ran, frames) = render_frames(&dir, &[], &[]);
        assert_eq!(ran.code, 0, "{end} {offset}: {}", ran.all());
        let frame = *frame as usize;
        assert_eq!(frames[frame - 1].count(GREEN, 0), 0, "{end} {offset} at {fps}: shown a frame early");
        assert_eq!(
            frames[frame].count(GREEN, 0),
            BOX,
            "{end} {offset} at {fps}: not shown on frame {frame}"
        );
        let green = shown(&frames, GREEN);
        assert_eq!(green[0], frame, "{end} {offset} at {fps}");
        // The named clip ends on the frame the rule gives for its own end, not later.
        let a_end = grid_frame(billionths(end), i128::from(*fps)) as usize;
        assert_eq!(
            *shown(&frames, RED).last().unwrap() + 1,
            a_end,
            "{end} {offset} at {fps}: the named clip"
        );
    }
}

// ---------------------------------------------------------------------
// c3: an offset that is not a time
// ---------------------------------------------------------------------

#[test]
fn s2_t011_c3_the_fixture_lists_one_error_with_its_own_code_the_line_a_sentence_that_shows_what_was_written_and_a_remedy() {
    let dir = fixture("s2_t011_c3", "relative-offset-not-a-time");
    let ran = check(&dir);
    assert_eq!(ran.code, 1, "{}", ran.all());
    let line = lines_with("relative-offset-not-a-time", "after:intro+abc");
    assert_eq!(line.len(), 1);
    assert_eq!(
        ran.stdout,
        format!(
            "error FS-E051 index.html:{}\n  \
             The offset \"+abc\" in the start \"after:intro+abc\" of the clip #outro is not a time in seconds.\n  \
             fix: Write the offset as + or - and a number of seconds with at most nine decimals and no unit, for example after:intro+0.5, or write the start as a time in seconds.\n\
             \n\
             check failed: 1 error, 0 unsupported, 0 unknown, 0 warnings\n",
            line[0]
        )
    );
    let items = listed(&ran.stdout);
    assert_eq!(
        (items[0].kind.as_str(), items[0].file.as_str(), items[0].line),
        ("error", "index.html", line[0])
    );
    assert!(!ran.stdout.contains("check passed"));
}

#[test]
fn s2_t011_c3_each_text_that_is_not_a_time_shows_what_was_written_and_a_text_that_is_a_name_does_not() {
    // (the text after `after:`, the offset the message shows) for a project with a clip #intro that
    // starts at 0 and lasts 1 s.
    let not_times = [
        ("intro+abc", "+abc"),
        ("intro+", "+"),
        ("intro+1.2.3", "+1.2.3"),
        ("intro-1.2.3", "-1.2.3"),
        ("intro+0.5s", "+0.5s"),
        ("intro+1e3", "+1e3"),
        ("intro+0,5", "+0,5"),
        ("intro+1.", "+1."),
        ("intro+0.1234567891", "+0.1234567891"),
        ("intro+-0.5", "+-0.5"),
        ("intro--0.5", "--0.5"),
        ("intro++1", "++1"),
        ("intro-+1", "-+1"),
    ];
    for (index, (after, offset)) in not_times.iter().enumerate() {
        let body = format!(
            "{}{}",
            clip("intro", "0", Some("1")),
            clip("b", &format!("after:{after}"), Some("1"))
        );
        let dir = boxes(&format!("s2_t011_c3_text_{index}"), 30, &body);
        let ran = check(&dir);
        assert_eq!(ran.code, 1, "{after}: {}", ran.all());
        assert_eq!(codes(&ran.stdout), ["FS-E051"], "{after}: {}", ran.stdout);
        assert!(
            ran.stdout.contains(&format!(
                "The offset \"{offset}\" in the start \"after:{after}\" of the clip #b is not a time in seconds."
            )),
            "{after}: {}",
            ran.stdout
        );
    }
    // A hyphen followed by something that is not a number is part of a name: the id of no clip. The
    // message is about that id and never about an offset.
    let names = [
        "intro-abc",
        "intro-2b",
        "intro-0.5s",
        "intro-",
        "intro 0.5",
        "intro--abc",
        "+1",
        "-1",
    ];
    for (index, after) in names.iter().enumerate() {
        let body = format!(
            "{}{}",
            clip("intro", "0", Some("1")),
            clip("b", &format!("after:{after}"), Some("1"))
        );
        let dir = boxes(&format!("s2_t011_c3_name_{index}"), 30, &body);
        let ran = check(&dir);
        assert_eq!(ran.code, 1, "{after}: {}", ran.all());
        assert_eq!(codes(&ran.stdout), ["FS-E050"], "{after}: {}", ran.stdout);
        assert!(
            ran.stdout
                .contains(&format!("no clip with the id \"{after}\" is in this composition")),
            "{after}: {}",
            ran.stdout
        );
        assert!(!ran.stdout.contains("offset"), "{after}: {}", ran.stdout);
    }
}

#[test]
fn s2_t011_c3_an_offset_that_is_not_a_time_after_an_id_no_clip_has_lists_both_faults() {
    let dir = boxes("s2_t011_c3_both", 30, &clip("b", "after:nothing+abc", Some("1")));
    let ran = check(&dir);
    assert_eq!(ran.code, 1, "{}", ran.all());
    assert_eq!(codes(&ran.stdout), ["FS-E051", "FS-E050"], "{}", ran.stdout);
    assert!(
        ran.stdout.contains("check failed: 2 errors, 0 unsupported, 0 unknown, 0 warnings"),
        "{}",
        ran.stdout
    );
}

// ---------------------------------------------------------------------
// c4: a start before zero
// ---------------------------------------------------------------------

#[test]
fn s2_t011_c4_the_fixture_lists_one_error_with_its_own_code_that_gives_the_resolved_time() {
    let dir = fixture("s2_t011_c4", "relative-offset-before-zero");
    let ran = check(&dir);
    assert_eq!(ran.code, 1, "{}", ran.all());
    let line = lines_with("relative-offset-before-zero", "after:intro-2.5");
    assert_eq!(line.len(), 1);
    // #intro starts at 1 s and lasts 1 s, so it ends at 2 s; 2 - 2.5 = -0.5.
    assert_eq!(
        ran.stdout,
        format!(
            "error FS-E052 index.html:{}\n  \
             The clip #outro starts at -0.5 s (\"after:intro-2.5\", and #intro ends at 2 s), before the start of the composition.\n  \
             fix: Use an offset of -2 s or later, or write the start as a time of 0 or more in seconds. The clip is not moved to 0.\n\
             \n\
             check failed: 1 error, 0 unsupported, 0 unknown, 0 warnings\n",
            line[0]
        )
    );
    // The clip is not moved to zero: no resolved start is printed for it, and `info` and `timeline`
    // report the error instead of a table.
    assert!(
        !ran.stdout.contains("start 0 s") && !ran.stdout.contains("check passed"),
        "{}",
        ran.stdout
    );
    for command in ["info", "timeline"] {
        let other = framesmith(&[command, text(&dir)]);
        assert_eq!(other.code, 1, "{command}: {}", other.all());
        assert!(other.all().contains("FS-E052"), "{command}: {}", other.all());
    }
}

#[test]
fn s2_t011_c4_a_start_of_exactly_zero_is_accepted_and_the_clip_shows_from_the_first_frame() {
    // #a ends at 1 s; "after:a-1" is 0 s.
    let body = format!("{}{}", clip("a", "0", Some("1")), clip("b", "after:a-1", Some("1")));
    let dir = boxes("s2_t011_c4_zero", 30, &body);
    let ran = check(&dir);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert!(ran.stdout.contains("  clip #b  start 0 s (after:a-1)\n"), "{}", ran.stdout);
    let (rendered, frames) = render_frames(&dir, &[], &[]);
    assert_eq!(rendered.code, 0, "{}", rendered.all());
    assert_eq!(shown(&frames, GREEN), range(0, 30));
}

#[test]
fn s2_t011_c4_a_start_just_below_zero_is_refused_with_the_time_it_resolved_to() {
    let body = format!("{}{}", clip("a", "0", Some("1")), clip("b", "after:a-1.000000001", Some("1")));
    let ran = check(&boxes("s2_t011_c4_below", 30, &body));
    assert_eq!(ran.code, 1, "{}", ran.all());
    assert_eq!(codes(&ran.stdout), ["FS-E052"]);
    assert!(ran.stdout.contains("The clip #b starts at -0.000000001 s"), "{}", ran.stdout);
}

#[test]
fn s2_t011_c4_the_error_for_a_written_negative_start_keeps_its_code_and_meaning() {
    let body = clip("a", "-0.5", Some("1"));
    let ran = check(&boxes("s2_t011_c4_written", 30, &body));
    assert_eq!(ran.code, 1, "{}", ran.all());
    assert_eq!(codes(&ran.stdout), ["FS-E022"]);
}

#[test]
fn s2_t011_c3_c4_the_two_codes_are_new_differ_from_each_other_and_from_every_other_code_and_are_listed_once() {
    let matrix = spec("support-matrix.md");
    let rows_of = |code: &str| matrix.lines().filter(|line| line.starts_with(&format!("| {code} |"))).count();
    assert_eq!((rows_of("FS-E051"), rows_of("FS-E052")), (1, 1));
    let others: BTreeSet<String> = matrix
        .lines()
        .filter_map(|line| line.strip_prefix("| FS-"))
        .map(|rest| format!("FS-{}", rest.split(' ').next().unwrap()))
        .filter(|code| code != "FS-E051" && code != "FS-E052" && code != "FS-E053")
        .collect();
    assert!(others.contains("FS-E050") && !others.contains("FS-E051") && !others.contains("FS-E052"));
    let errors: Vec<&String> = others.iter().filter(|code| code.starts_with("FS-E")).collect();
    assert_eq!(
        errors.last().map(|code| code.as_str()),
        Some("FS-E050"),
        "the next free numbers were taken"
    );
    // The product raises each code in one place (the resolver), and each code is given to nothing else.
    let not_time = codes(&check(&fixture("s2_t011_codes_a", "relative-offset-not-a-time")).stdout);
    let before_zero = codes(&check(&fixture("s2_t011_codes_b", "relative-offset-before-zero")).stdout);
    assert_eq!(
        (not_time.as_slice(), before_zero.as_slice()),
        (&["FS-E051".to_string()][..], &["FS-E052".to_string()][..])
    );
    // The meanings in the table.
    let row = |code: &str| {
        matrix
            .lines()
            .find(|line| line.starts_with(&format!("| {code} |")))
            .unwrap()
            .to_string()
    };
    assert!(row("FS-E051").contains("is not a time"), "{}", row("FS-E051"));
    assert!(
        row("FS-E052").contains("before zero") && row("FS-E052").contains("not moved to zero"),
        "{}",
        row("FS-E052")
    );
}

// ---------------------------------------------------------------------
// c5: render refuses each fault and starts nothing
// ---------------------------------------------------------------------

#[test]
fn s2_t011_c5_render_prints_the_list_check_printed_exits_1_and_starts_no_browser_and_no_ffmpeg() {
    for (key, folder, code) in [
        ("not_a_time", "relative-offset-not-a-time", "FS-E051"),
        ("before_zero", "relative-offset-before-zero", "FS-E052"),
    ] {
        let dir = fixture(&format!("s2_t011_c5_{key}"), folder);
        let checked = check(&dir);
        assert_eq!(checked.code, 1, "{key}: {}", checked.all());
        let spies = Spies::new(&format!("s2_t011_c5_{key}"));
        let before = tree(&dir);
        let cwd = dir.parent().unwrap();
        let render = spies.run(cwd, &["render", text(&dir)]);
        let chosen = cwd.join("chosen").join("video.mp4");
        let render_to = spies.run(cwd, &["render", text(&dir), "--output", text(&chosen)]);
        let json = spies.run(cwd, &["render", text(&dir), "--json"]);
        assert_eq!(render.code, 1, "{key}: {}", render.all());
        assert_eq!(render_to.code, 1, "{key}: {}", render_to.all());
        assert_eq!(json.code, 1, "{key}: {}", json.all());
        assert_eq!(
            render.stderr, checked.stdout,
            "{key}: render must print the list that check printed"
        );
        assert_eq!(render_to.stderr, checked.stdout, "{key}");
        let doc = document(&json);
        assert_eq!(doc["outcome"], "failed", "{key}: {doc}");
        let from_json: Vec<(String, u64)> = doc["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| (item["code"].as_str().unwrap().to_string(), item["line"].as_u64().unwrap()))
            .collect();
        let from_text: Vec<(String, u64)> = listed(&checked.stdout)
            .iter()
            .map(|item| (item.code.clone(), u64::from(item.line)))
            .collect();
        assert_eq!(from_json, from_text, "{key}");
        assert_eq!(from_json[0].0, code, "{key}");
        assert_eq!(spies.started(), (false, false), "{key}: a browser or ffmpeg was started");
        assert!(!dir.join("out").exists(), "{key}: the default output folder was created");
        assert!(
            !chosen.exists() && !chosen.parent().unwrap().exists(),
            "{key}: something appeared at the chosen path"
        );
        assert_eq!(before, tree(&dir), "{key}: the project changed");
    }
}

#[test]
fn s2_t011_c5_the_stand_ins_do_see_a_browser_and_ffmpeg_when_the_fixture_with_offsets_renders() {
    let spies = Spies::new("s2_t011_c5_control");
    let dir = fixture("s2_t011_c5_control_project", "relative-offset");
    let ran = spies.run(dir.parent().unwrap(), &["render", text(&dir)]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert_eq!(spies.started(), (true, true));
}

// ---------------------------------------------------------------------
// FS-U003 after this ticket
// ---------------------------------------------------------------------

#[test]
fn s2_t011_fs_u003_is_no_longer_raised_for_an_offset_and_the_forms_that_wait_are_still_refused_with_one() {
    // An offset on a named clip with an absolute start and a written duration: supported, nothing refused.
    for written in ["after:a+0.5", "after:a-0.5", "after:a + 1", "after:a+0"] {
        let body = format!("{}{}", clip("a", "1", Some("2")), clip("b", written, Some("1")));
        let ran = check(&boxes("s2_t011_u003_supported", 30, &body));
        assert_eq!(ran.code, 0, "{written}: {}", ran.all());
        assert!(!ran.all().contains("FS-U003"), "{written}");
    }
    // The forms that wait stay refused (exit 2) with an offset as without one. (A chain no longer waits:
    // ticket T012. A follower of a link of a chain that has no written duration does.)
    let named = clip("a", "1", Some("2"));
    let unwritten = "<div id=\"a\" class=\"box\" data-fs-clip data-fs-start=\"1\"></div>\n".to_string();
    let unwritten_link = "<div id=\"b\" class=\"box\" data-fs-clip data-fs-start=\"after:a\"></div>\n".to_string();
    let cases: Vec<(&str, String, &str)> = vec![
        (
            "unwritten_plus",
            format!("{unwritten}{}", clip("b", "after:a+0.5", Some("1"))),
            "which has no written duration",
        ),
        (
            "unwritten_minus",
            format!("{unwritten}{}", clip("b", "after:a-0.5", Some("1"))),
            "which has no written duration",
        ),
        (
            "unwritten_link_plus",
            format!("{named}{unwritten_link}{}", clip("c", "after:b+0.5", Some("1"))),
            "which has no written duration",
        ),
        (
            "unwritten_link_minus",
            format!("{named}{unwritten_link}{}", clip("c", "after:b-0.5", Some("1"))),
            "which has no written duration",
        ),
    ];
    for (key, body, says) in cases {
        let dir = boxes(&format!("s2_t011_u003_{key}"), 30, &body);
        let ran = check(&dir);
        assert_eq!(ran.code, 2, "{key}: {}", ran.all());
        assert!(codes(&ran.stdout).iter().all(|code| code == "FS-U003"), "{key}: {}", ran.stdout);
        assert!(ran.stdout.contains(says), "{key}: {}", ran.stdout);
        assert!(!ran.stdout.contains("offsets are not supported"), "{key}: {}", ran.stdout);
        let spies = Spies::new(&format!("s2_t011_u003_{key}"));
        let render = spies.run(dir.parent().unwrap(), &["render", text(&dir)]);
        assert_eq!(render.code, 2, "{key}: {}", render.all());
        assert_eq!(spies.started(), (false, false), "{key}");
    }
    // The fixture of the row is a follower of a clip with no written duration (it was a chain until T012).
    let ran = check(&fixture("s2_t011_u003_fixture", "unsupported/fs-u003-relative-start"));
    assert_eq!(ran.code, 2, "{}", ran.all());
    assert!(ran.stdout.contains("which has no written duration"), "{}", ran.stdout);
}

#[test]
fn s2_t011_the_support_matrix_row_is_narrowed_and_the_message_for_an_offset_is_gone_from_the_product() {
    let matrix = spec("support-matrix.md");
    let row = matrix.lines().find(|line| line.starts_with("| FS-U003 |")).unwrap();
    let cells: Vec<&str> = row.trim().trim_matches('|').split('|').map(str::trim).collect();
    assert!(
        cells[1].contains("the form with an offset") && cells[1].contains("supported"),
        "{}",
        cells[1]
    );
    assert!(
        !cells[1].contains("with an offset (`after:<clip id>` followed by `+` or `-` and a time), naming"),
        "{}",
        cells[1]
    );
    assert!(
        !cells[3].contains("T011") && !cells[3].contains("T012") && cells[3].contains("T013") && cells[3].contains("T038"),
        "{}",
        cells[3]
    );
    for source in std::fs::read_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("..")).unwrap() {
        let src = source.unwrap().path().join("src").join("resolve.rs");
        if src.is_file() {
            assert!(!std::fs::read_to_string(&src).unwrap().contains("offsets are not supported"));
        }
    }
}

// ---------------------------------------------------------------------
// The page resolves the same start
// ---------------------------------------------------------------------

/// The failure description of a render of a project whose page script changes the page.
fn page_disagrees(test: &str, clips: &str, script: &str) -> String {
    let dir = boxes(test, 30, &format!("{clips}<script>\n{script}\n</script>"));
    assert_eq!(check(&dir).code, 0, "the files alone are fine");
    let ran = run_in(dir.parent().unwrap(), &["render", text(&dir), "--json"], &[]);
    assert_eq!(ran.code, 1, "{}", ran.all());
    let document = document(&ran);
    assert_eq!(document["error"]["code"], "FS-R010", "{document}");
    assert!(!dir.join("out").exists());
    document["error"]["description"].as_str().unwrap().to_string()
}

#[test]
fn s2_t011_page_the_page_resolves_each_follower_with_an_offset_to_the_same_start_as_the_files() {
    // The render compares the page's manifest with the one read from the files before any frame is
    // taken and stops when they differ (page-protocol.md). It passes, and the job it ran holds each
    // follower's resolved start and the form it was written in, offset included.
    let dir = fixture("s2_t011_page_agree", "relative-offset");
    let job_file = dir.parent().unwrap().join("job.json");
    let ran = run_in(
        dir.parent().unwrap(),
        &["render", text(&dir)],
        &[("FRAMESMITH_DEBUG_JOB", job_file.as_os_str())],
    );
    assert_eq!(ran.code, 0, "{}", ran.all());
    let job: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&job_file).unwrap()).unwrap();
    let clips = &job["manifest"]["clips"];
    assert_eq!(clips[0]["relative_start"], serde_json::Value::Null);
    assert_eq!(
        (clips[1]["start"].clone(), clips[1]["relative_start"].clone()),
        ("7/2".into(), "after:intro+0.5".into())
    );
    assert_eq!(
        (clips[2]["start"].clone(), clips[2]["relative_start"].clone()),
        ("5/2".into(), "after:intro-0.5".into())
    );
    assert_eq!(
        (clips[3]["start"].clone(), clips[3]["relative_start"].clone()),
        ("4/1".into(), "after: intro + 1".into())
    );
    assert!(
        ran.stdout.contains("probe     done      320x180, 30 fps, 5 s, 150 frames\n"),
        "{}",
        ran.stdout
    );
}

#[test]
fn s2_t011_page_the_page_works_the_sum_out_itself_and_a_difference_is_named_with_both_values() {
    // The follower is written first. A script moves the clip it names by one second before the page
    // reports ready: the page's own resolution puts the follower at 3.5 s where the files say 2.5 s.
    let clips = format!("{}{}", clip("b", "after:a-0.5", Some("1")), clip("a", "1", Some("2")));
    let said = page_disagrees(
        "s2_t011_page_moved",
        &clips,
        "document.getElementById(\"a\").setAttribute(\"data-fs-start\", \"2\");",
    );
    assert_eq!(
        said,
        "The page disagrees with the files: the start of clip 1 (#b) is 2.5 s in the files and 3.5 s in the page."
    );
    // A script that changes the sign gives another start: the first difference is the start.
    let said = page_disagrees(
        "s2_t011_page_sign",
        &clips,
        "document.getElementById(\"b\").setAttribute(\"data-fs-start\", \"after:a+0.5\");",
    );
    assert_eq!(
        said,
        "The page disagrees with the files: the start of clip 1 (#b) is 2.5 s in the files and 3.5 s in the page."
    );
    // A script that writes the same start another way (a zero offset with the other sign): the start is
    // the same, so the way it is written is what differs.
    let clips = format!("{}{}", clip("b", "after:a+0", Some("1")), clip("a", "1", Some("2")));
    let said = page_disagrees(
        "s2_t011_page_written",
        &clips,
        "document.getElementById(\"b\").setAttribute(\"data-fs-start\", \"after:a-0\");",
    );
    assert_eq!(
        said,
        "The page disagrees with the files: the way the start of clip 1 (#b) is written is \"after:a+0\" in the files and \"after:a-0\" in the page."
    );
    // A script that makes the page's start go before zero: the page cannot resolve its composition, and
    // the run stops with a disagreement rather than painting a wrong video.
    let clips = format!("{}{}", clip("b", "after:a-1", Some("1")), clip("a", "1", Some("2")));
    let dir = boxes(
        "s2_t011_page_before_zero",
        30,
        &format!("{clips}<script>\ndocument.getElementById(\"b\").setAttribute(\"data-fs-start\", \"after:a-9\");\n</script>"),
    );
    assert_eq!(check(&dir).code, 0);
    let ran = run_in(dir.parent().unwrap(), &["render", text(&dir), "--json"], &[]);
    assert_eq!(ran.code, 1, "{}", ran.all());
    let document = document(&ran);
    assert_eq!(document["error"]["code"], "FS-R010", "{document}");
    assert_eq!(
        document["error"]["description"],
        "The page disagrees with the files: the page could not resolve its composition: The clip #b starts at -6 s (\"after:a-9\", and #a ends at 3 s), before the start of the composition. (FS-E052)."
    );
    assert!(!dir.join("out").exists());
}
