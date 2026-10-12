//! Sprint 2, ticket T014: the author writes a lane number on a clip (`data-fs-track`, the name Sprint 1
//! gave to this number) and `timeline` prints the clips grouped by lane, in number order, with each
//! lane's clips beneath it; `timeline --json` carries each clip's lane. A clip with no lane is shown in
//! lane 0 and marked as filled in. A lane never changes a rendered frame. Two clips may share a lane and
//! overlap in time without a word from Framesmith. A lane that is not a whole number (FS-E028, from
//! Sprint 1) or is negative (FS-E055, new; Sprint 1 accepted a negative number) is an error, `check`
//! exits 1 and `render` refuses the project.
//!
//! Notes for maintainers
//! ---------------------
//! * The fixtures are `fixtures/lanes` (three lanes, one clip with none written, two clips that share a
//!   lane and overlap), `fixtures/lanes-swapped` (the same pictures, other lane numbers),
//!   `fixtures/lane-shared`, `fixtures/lane-negative` and `fixtures/lane-not-whole`. The expected text of
//!   each is written out here from the way the fixture is written, not taken from the code.
//! * The projects of the other tests are written inline. A value that the form of a lane accepts or
//!   refuses is tried in a table of its own (`LANE_VALUES`).
//! * Only the tests of c3 and the control of c5 need a browser and `ffmpeg`; the others read files only.
//!   The refusing tests use stand-ins that note being started.
//! * The tests that expected the output of `timeline` before this ticket (no lane lines, no mark of a lane
//!   filled in) were changed by this ticket, in `s2_t008_timeline.rs`, `s2_t009_timeline_json.rs`,
//!   `s2_t010_relative_start.rs`, `s2_t011_relative_offset.rs`, `s2_t012_relative_chain.rs` and
//!   `s2_t013_follow_clip_end.rs`. The same files, with `s2_t005_refusals_that_stay.rs` untouched, hold
//!   the tests that FS-E054 was the last error code; they now allow the code that follows it.
//! * Scratch folders: every test names its projects with a scratch key of its own.

mod common;

use common::*;
use measure::{Frame, Ran, Rgb};
use serde_json::Value;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

const RED: Rgb = [255, 0, 0];
const BLUE: Rgb = [0, 0, 255];
const GREEN: Rgb = [0, 255, 0];
const YELLOW: Rgb = [255, 255, 0];

// ---------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------

fn check(dir: &Path) -> Ran {
    framesmith(&["check", text(dir)])
}

fn timeline(dir: &Path) -> Ran {
    framesmith(&["timeline", text(dir)])
}

fn timeline_json(dir: &Path) -> Value {
    let ran = framesmith(&["timeline", text(dir), "--json"]);
    assert!(ran.stderr.is_empty(), "{}", ran.stderr);
    document(&ran)
}

/// One printed diagnostic header: kind word, code, file and line.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Listed {
    kind: String,
    code: String,
    file: String,
    line: u32,
}

/// The headers (`kind CODE file:line`) in what a command printed.
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

/// The line numbers (from 1) of the lines of a fixture's page that hold `needle`.
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

/// A 320 by 180 project at 10 frames a second, 2 s long, with the given clips in the body.
fn boxes(test: &str, clips: &str) -> PathBuf {
    project(
        test,
        "boxes",
        &page(
            "data-fs-size=\"320x180\" data-fs-fps=\"10\" data-fs-duration=\"2\"",
            BOX_STYLE,
            clips,
            "",
        ),
    )
}

/// A clip that is a box, with the lane written as `lane` (an attribute value) or not written.
fn clip(id: &str, start: &str, duration: &str, lane: Option<&str>) -> String {
    let lane = lane.map_or(String::new(), |lane| format!(" data-fs-track=\"{lane}\""));
    format!("<div id=\"{id}\" class=\"box\" data-fs-clip data-fs-start=\"{start}\" data-fs-duration=\"{duration}\"{lane}></div>\n")
}

/// What `timeline` printed as groups: for each lane line, the number and the clips under it as (name,
/// whether the line ends in the mark of a lane filled in). It is read from the text, not from the code.
fn groups(printed: &str) -> Vec<(String, Vec<(String, bool)>)> {
    let mut found: Vec<(String, Vec<(String, bool)>)> = Vec::new();
    for line in printed.lines().skip(1) {
        if let Some(number) = line.strip_prefix("lane ") {
            found.push((number.to_string(), Vec::new()));
        } else {
            let body = line
                .strip_prefix("  ")
                .unwrap_or_else(|| panic!("a clip line is indented by two spaces: {line:?}"));
            let name = body.split("  ").next().unwrap().trim_end().to_string();
            let filled = line.ends_with(" (lane filled in)");
            found
                .last_mut()
                .unwrap_or_else(|| panic!("a clip line comes before any lane line: {line:?}"))
                .1
                .push((name, filled));
        }
    }
    found
}

fn names(clips: &[(String, bool)]) -> Vec<&str> {
    clips.iter().map(|(name, _)| name.as_str()).collect()
}

// ---------------------------------------------------------------------
// c1: the lanes in number order with each lane's clips beneath it; the JSON carries each clip's lane
// ---------------------------------------------------------------------

#[test]
fn s2_t014_c1_timeline_prints_the_lanes_in_number_order_with_each_lanes_clips_beneath_it() {
    let dir = fixture("s2_t014_c1_text", "lanes");
    let ran = timeline(&dir);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert!(ran.stderr.is_empty(), "{}", ran.stderr);
    // Document order is #sky (lane 2), #cloud (lane 1), #sun (none), #bird (lane 2).
    assert_eq!(
        ran.stdout,
        "main  320x180  10 fps  2 s  20 frames\n\
         lane 0\n\
         \x20 #sun    0.5 s to 1.5 s  1 s (lane filled in)\n\
         lane 1\n\
         \x20 #cloud  0 s   to 1.5 s  1.5 s\n\
         lane 2\n\
         \x20 #sky    0 s   to 2 s    2 s\n\
         \x20 #bird   1 s   to 2 s    1 s\n"
    );
}

#[test]
fn s2_t014_c1_lanes_are_in_numeric_order_and_a_lane_with_no_clip_has_no_line() {
    // 10 and 2 would swap if the numbers were sorted as text; lanes 3 to 9 hold nothing.
    let body = format!(
        "{}{}{}{}",
        clip("a", "0", "1", Some("10")),
        clip("b", "0", "1", Some("2")),
        clip("c", "0", "1", Some("100")),
        clip("d", "0", "1", Some("2")),
    );
    let dir = boxes("s2_t014_c1_numeric", &body);
    let ran = timeline(&dir);
    assert_eq!(ran.code, 0, "{}", ran.all());
    let grouped = groups(&ran.stdout);
    let lanes: Vec<&str> = grouped.iter().map(|(lane, _)| lane.as_str()).collect();
    assert_eq!(lanes, ["2", "10", "100"], "{}", ran.stdout);
    assert_eq!(names(&grouped[0].1), ["#b", "#d"], "document order within a lane");
    assert_eq!(names(&grouped[1].1), ["#a"]);
    assert_eq!(names(&grouped[2].1), ["#c"]);
    // Every lane line is the word, a space and the number, and nothing else.
    for line in ran.stdout.lines().filter(|line| line.starts_with("lane")) {
        assert!(line.strip_prefix("lane ").is_some_and(|n| n.parse::<u64>().is_ok()), "{line:?}");
    }
}

#[test]
fn s2_t014_c1_the_columns_line_up_from_one_lane_to_the_next_and_no_line_ends_in_a_space() {
    let body = format!(
        "{}{}{}",
        clip("short", "0", "1", Some("1")),
        clip("a-much-longer-name", "1.25", "0.5", Some("2")),
        clip("mid", "0.5", "1", Some("1")),
    );
    let dir = boxes("s2_t014_c1_columns", &body);
    let ran = timeline(&dir);
    assert_eq!(ran.code, 0, "{}", ran.all());
    let clip_lines: Vec<&str> = ran.stdout.lines().filter(|line| line.starts_with("  ")).collect();
    assert_eq!(clip_lines.len(), 3, "{}", ran.stdout);
    let to_columns: Vec<usize> = clip_lines.iter().map(|line| line.find(" to ").unwrap()).collect();
    assert!(
        to_columns.windows(2).all(|pair| pair[0] == pair[1]),
        "{to_columns:?}\n{}",
        ran.stdout
    );
    // The name column is as wide as the widest name over all the lanes, not over the lane alone.
    let widest = "#a-much-longer-name".len();
    for line in &clip_lines {
        assert_eq!(&line[2 + widest..2 + widest + 2], "  ", "{line:?}");
    }
    for line in ran.stdout.lines() {
        assert_eq!(line, line.trim_end(), "trailing space: {line:?}");
    }
    assert!(
        !ran.stdout.lines().any(|line| line.starts_with("lane") && line.contains("  ")),
        "{}",
        ran.stdout
    );
}

#[test]
fn s2_t014_c1_timeline_json_carries_each_clips_lane_in_document_order() {
    let dir = fixture("s2_t014_c1_json", "lanes");
    let doc = timeline_json(&dir);
    assert_eq!(doc["outcome"], "passed");
    let clips = doc["clips"].as_array().unwrap();
    // The list is not grouped: it keeps the order of the document, and each entry has its lane.
    let seen: Vec<(&str, &Value, &Value)> = clips
        .iter()
        .map(|clip| (clip["name"].as_str().unwrap(), &clip["lane"], &clip["lane_filled_in"]))
        .collect();
    assert_eq!(
        seen,
        [
            ("#sky", &Value::from(2), &Value::from(false)),
            ("#cloud", &Value::from(1), &Value::from(false)),
            ("#sun", &Value::from(0), &Value::from(true)),
            ("#bird", &Value::from(2), &Value::from(false)),
        ]
    );
    // The lane is a JSON number, never a string, and nothing else of the entry changed.
    for clip in clips {
        assert!(clip["lane"].is_u64(), "{clip}");
        assert!(clip["lane_filled_in"].is_boolean(), "{clip}");
        let mut keys: Vec<&str> = clip.as_object().unwrap().keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            ["duration", "duration_filled_in", "end", "lane", "lane_filled_in", "name", "start"]
        );
    }
    assert_eq!(clips[0]["start"], 0);
    assert_eq!(clips[3]["start"], 1);
    assert_eq!(doc["diagnostics"], serde_json::json!([]));
}

#[test]
fn s2_t014_c1_the_text_form_and_the_document_give_each_clip_the_same_lane() {
    for fixture_name in ["lanes", "lanes-swapped", "lane-shared"] {
        let dir = fixture(&format!("s2_t014_c1_agree_{fixture_name}"), fixture_name);
        let printed = timeline(&dir).stdout;
        let doc = timeline_json(&dir);
        // The lane each clip stands under in the text...
        let mut from_text: Vec<(String, String, bool)> = groups(&printed)
            .into_iter()
            .flat_map(|(lane, clips)| clips.into_iter().map(move |(name, filled)| (name, lane.clone(), filled)))
            .collect();
        from_text.sort();
        // ...is the lane the document carries for it.
        let mut from_json: Vec<(String, String, bool)> = doc["clips"]
            .as_array()
            .unwrap()
            .iter()
            .map(|clip| {
                (
                    clip["name"].as_str().unwrap().to_string(),
                    clip["lane"].to_string(),
                    clip["lane_filled_in"].as_bool().unwrap(),
                )
            })
            .collect();
        from_json.sort();
        assert_eq!(from_text, from_json, "{fixture_name}");
        assert!(!from_text.is_empty());
    }
}

#[test]
fn s2_t014_c1_a_lane_does_not_move_a_clip_in_time() {
    // The same four clips with other lane numbers, or none: every start, end and duration is the same.
    let times = |fixture_name: &str| -> Vec<(String, Value, Value, Value)> {
        let dir = fixture(&format!("s2_t014_c1_times_{fixture_name}"), fixture_name);
        let mut found: Vec<_> = timeline_json(&dir)["clips"]
            .as_array()
            .unwrap()
            .iter()
            .map(|clip| {
                (
                    clip["name"].as_str().unwrap().to_string(),
                    clip["start"].clone(),
                    clip["end"].clone(),
                    clip["duration"].clone(),
                )
            })
            .collect();
        found.sort_by(|a, b| a.0.cmp(&b.0));
        found
    };
    assert_eq!(times("lanes"), times("lanes-swapped"));
}

#[test]
fn s2_t014_c1_a_clip_is_in_the_lane_of_its_own_number_whatever_the_lane_of_the_clip_around_it() {
    let body = "<div id=\"outer\" class=\"box\" data-fs-clip data-fs-start=\"0\" data-fs-duration=\"2\" data-fs-track=\"4\">\n\
                <div id=\"inner\" class=\"box\" data-fs-clip data-fs-start=\"0.5\" data-fs-duration=\"1\" data-fs-track=\"1\"></div>\n\
                <div id=\"plain\" class=\"box\" data-fs-clip data-fs-start=\"1\" data-fs-duration=\"0.5\"></div>\n\
                </div>";
    let dir = boxes("s2_t014_c1_nested", body);
    let ran = timeline(&dir);
    assert_eq!(ran.code, 0, "{}", ran.all());
    let grouped = groups(&ran.stdout);
    let seen: Vec<(&str, Vec<&str>)> = grouped.iter().map(|(lane, clips)| (lane.as_str(), names(clips))).collect();
    assert_eq!(seen, [("0", vec!["#plain"]), ("1", vec!["#inner"]), ("4", vec!["#outer"])]);
}

#[test]
fn s2_t014_c1_the_command_is_logged_with_its_input_and_its_result_and_the_number_of_clips() {
    let dir = fixture("s2_t014_c1_log", "lanes");
    let logs = scratch("s2_t014_c1_log_folder");
    let ran = run_in(
        dir.parent().unwrap(),
        &["timeline", text(&dir)],
        &[("FRAMESMITH_LOG_DIR", logs.as_os_str()), ("FRAMESMITH_LOG", OsStr::new("info"))],
    );
    assert_eq!(ran.code, 0, "{}", ran.all());
    let log = std::fs::read_to_string(logs.join("framesmith.log")).unwrap();
    let lines: Vec<&str> = log.lines().collect();
    assert_eq!(lines.len(), 2, "{log}");
    assert!(
        lines[0].contains("command started") && lines[0].contains("command=\"timeline\""),
        "{log}"
    );
    assert!(
        lines[1].contains("command finished") && lines[1].contains("exit=0") && lines[1].contains("clips=4"),
        "{log}"
    );
    // No file contents in it (the project's folder is named `lanes`, so the word alone proves nothing).
    assert!(!log.contains("data-fs") && !log.contains("lane="), "{log}");
}

// ---------------------------------------------------------------------
// c2: a clip with no lane number is shown in lane 0
// ---------------------------------------------------------------------

#[test]
fn s2_t014_c2_a_clip_with_no_lane_number_is_shown_in_lane_0_and_marked_as_filled_in() {
    let dir = fixture("s2_t014_c2_fixture", "lanes");
    let ran = timeline(&dir);
    let grouped = groups(&ran.stdout);
    let lane_zero = grouped
        .iter()
        .find(|(lane, _)| lane == "0")
        .unwrap_or_else(|| panic!("{}", ran.stdout));
    assert_eq!(lane_zero.1, [("#sun".to_string(), true)]);
    // The lanes that were written carry no mark.
    assert!(grouped
        .iter()
        .filter(|(lane, _)| lane != "0")
        .all(|(_, clips)| clips.iter().all(|(_, filled)| !filled)));
    let doc = timeline_json(&dir);
    let sun = doc["clips"].as_array().unwrap().iter().find(|clip| clip["name"] == "#sun").unwrap();
    assert_eq!((sun["lane"].as_u64(), sun["lane_filled_in"].as_bool()), (Some(0), Some(true)));
}

#[test]
fn s2_t014_c2_a_project_that_writes_no_lane_prints_one_lane_0_with_every_clip_in_it() {
    let body = format!(
        "{}{}{}",
        clip("a", "0", "1", None),
        clip("b", "0.5", "1", None),
        clip("c", "1", "0.5", None)
    );
    let dir = boxes("s2_t014_c2_none_written", &body);
    let ran = timeline(&dir);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert_eq!(
        ran.stdout,
        "main  320x180  10 fps  2 s  20 frames\n\
         lane 0\n\
         \x20 #a  0 s   to 1 s    1 s (lane filled in)\n\
         \x20 #b  0.5 s to 1.5 s  1 s (lane filled in)\n\
         \x20 #c  1 s   to 1.5 s  0.5 s (lane filled in)\n"
    );
    let doc = timeline_json(&dir);
    for entry in doc["clips"].as_array().unwrap() {
        assert_eq!(
            (entry["lane"].as_u64(), entry["lane_filled_in"].as_bool()),
            (Some(0), Some(true)),
            "{entry}"
        );
    }
}

#[test]
fn s2_t014_c2_a_written_zero_is_lane_0_without_the_mark_and_shares_the_group_with_a_clip_that_has_none() {
    let body = format!("{}{}", clip("written", "0", "1", Some("0")), clip("unwritten", "0", "1", None));
    let dir = boxes("s2_t014_c2_zero_and_none", &body);
    let ran = timeline(&dir);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert_eq!(
        groups(&ran.stdout),
        [(
            "0".to_string(),
            vec![("#written".to_string(), false), ("#unwritten".to_string(), true)]
        )],
        "{}",
        ran.stdout
    );
    let doc = timeline_json(&dir);
    let flags: Vec<(u64, bool)> = doc["clips"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| (entry["lane"].as_u64().unwrap(), entry["lane_filled_in"].as_bool().unwrap()))
        .collect();
    assert_eq!(flags, [(0, false), (0, true)]);
}

#[test]
fn s2_t014_c2_a_composition_with_no_clips_prints_the_header_and_no_lane() {
    let dir = project(
        "s2_t014_c2_no_clips",
        "known",
        &page("data-fs-size=\"64x36\" data-fs-fps=\"24\" data-fs-duration=\"1\"", "", "", ""),
    );
    let ran = timeline(&dir);
    assert_eq!(ran.stdout, "main  64x36  24 fps  1 s  24 frames\n");
    assert_eq!(timeline_json(&dir)["clips"], serde_json::json!([]));
}

// ---------------------------------------------------------------------
// c3: renders that differ only in lane numbers give identical frame hashes
// ---------------------------------------------------------------------

fn hashes(frames: &[Frame]) -> Vec<u64> {
    frames.iter().map(Frame::hash).collect()
}

/// Renders a project and returns its frames, after checking that the render completed.
fn frames_of(dir: &Path) -> Vec<Frame> {
    let (ran, frames) = render_frames(dir, &[], &[]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert_eq!(frames.len(), 20, "{}", ran.all());
    frames
}

/// The fixture `lanes` with every `data-fs-track` attribute taken out of the page.
fn without_lanes(page_text: &str) -> String {
    let mut out = String::new();
    for line in page_text.lines() {
        let mut line = line.to_string();
        while let Some(at) = line.find(" data-fs-track=\"") {
            let end = at + " data-fs-track=\"".len() + line[at + " data-fs-track=\"".len()..].find('"').unwrap() + 1;
            line.replace_range(at..end, "");
        }
        out.push_str(&line);
        out.push('\n');
    }
    out
}

#[test]
fn s2_t014_c3_two_renders_of_projects_that_differ_only_in_lane_numbers_give_identical_frame_hashes() {
    let lanes = fixture("s2_t014_c3_lanes", "lanes");
    let swapped = fixture("s2_t014_c3_swapped", "lanes-swapped");
    // A third project: the first with the lane numbers taken out, which is also a difference in lanes only.
    let bare = fixture("s2_t014_c3_bare", "lanes");
    let page_text = std::fs::read_to_string(bare.join("index.html")).unwrap();
    assert!(page_text.contains("data-fs-track"));
    write(&bare, "index.html", without_lanes(&page_text));
    assert!(!std::fs::read_to_string(bare.join("index.html")).unwrap().contains("data-fs-track"));
    // The three projects really differ in lanes (the timeline says so) and in nothing else the text shows.
    assert_ne!(timeline(&lanes).stdout, timeline(&swapped).stdout);
    assert_ne!(timeline(&lanes).stdout, timeline(&bare).stdout);

    let (a, b, c) = (frames_of(&lanes), frames_of(&swapped), frames_of(&bare));
    assert_eq!(measure::first_difference(&hashes(&a), &hashes(&b)), None);
    assert_eq!(measure::first_difference(&hashes(&a), &hashes(&c)), None);
    // The pictures are not trivial: on frame 12 all four boxes are painted, over one another.
    for colour in [RED, BLUE, GREEN, YELLOW] {
        assert!(a[12].count(colour, 0) > 0, "colour {colour:?} missing on frame 12");
    }
    // And the hashes are not all one value: the picture changes as clips come and go.
    assert!(hashes(&a).iter().collect::<std::collections::BTreeSet<_>>().len() > 3);
}

#[test]
fn s2_t014_c3_the_comparison_sees_a_change_of_paint_order_which_css_alone_decides() {
    // The control: the same project with the stacking order of two boxes changed in the CSS gives
    // another picture, so the hashes above are able to tell. The lanes are the same as in `lanes`.
    let original = fixture("s2_t014_c3_control_original", "lanes");
    let changed = fixture("s2_t014_c3_control_changed", "lanes");
    let page_text = std::fs::read_to_string(changed.join("index.html")).unwrap();
    assert!(page_text.contains("#bird  { left: 140px; top: 40px; background: #ffff00; z-index: 4; }"));
    write(
        &changed,
        "index.html",
        page_text.replace(
            "#bird  { left: 140px; top: 40px; background: #ffff00; z-index: 4; }",
            "#bird  { left: 140px; top: 40px; background: #ffff00; z-index: 0; }",
        ),
    );
    assert_eq!(timeline(&original).stdout, timeline(&changed).stdout);
    let (a, b) = (frames_of(&original), frames_of(&changed));
    let first = measure::first_difference(&hashes(&a), &hashes(&b));
    // #bird shows from 1 s (frame 10) and overlaps #sun and #cloud: the first frame to differ is frame 10.
    assert_eq!(first, Some(10), "{first:?}");
}

#[test]
fn s2_t014_c3_the_picture_of_a_lane_project_is_decided_by_css_and_not_by_the_lane_order() {
    // On frame 12 all four boxes are painted. Their z-index values in the CSS are #sky 1, #cloud 2, #sun 3 and
    // #bird 4, while their lanes are 2, 1, 0 and 2: the box in front is the one with the higher z-index
    // wherever the lanes stand. The boxes are 100 by 100 at (20, 20) #sky, (60, 40) #cloud, (100, 20) #sun and
    // (140, 40) #bird.
    let dir = fixture("s2_t014_c3_front", "lanes");
    let frames = frames_of(&dir);
    let frame = &frames[12];
    // (110, 70) is in #sky, #cloud and #sun: #sun (z-index 3, lane 0) is in front.
    assert_eq!(frame.pixel(110, 70), GREEN);
    // (150, 70) is in #cloud, #sun and #bird: #bird (z-index 4) is in front.
    assert_eq!(frame.pixel(150, 70), YELLOW);
    // (80, 70) is in #sky and #cloud only: #cloud (z-index 2, lane 1) is in front of #sky (lane 2).
    assert_eq!(frame.pixel(80, 70), BLUE);
    // (30, 30) is in #sky alone.
    assert_eq!(frame.pixel(30, 30), RED);
}

// ---------------------------------------------------------------------
// c4: two clips that share a lane and overlap are both listed and nothing is reported about them
// ---------------------------------------------------------------------

#[test]
fn s2_t014_c4_two_clips_that_share_a_lane_and_overlap_are_both_listed_and_nothing_is_reported() {
    let dir = fixture("s2_t014_c4_shared", "lane-shared");
    // #sky shows from 0 s to 1.5 s and #cloud from 0.5 s to 2 s: they overlap from 0.5 s to 1.5 s, in lane 3.
    let checked = check(&dir);
    assert_eq!(checked.code, 0, "{}", checked.all());
    assert!(checked.stderr.is_empty(), "{}", checked.stderr);
    assert!(listed(&checked.stdout).is_empty(), "{}", checked.stdout);
    assert!(checked.stdout.starts_with("check passed\n"), "{}", checked.stdout);
    assert!(
        !checked.stdout.contains("warning") && !checked.stdout.contains("overlap"),
        "{}",
        checked.stdout
    );
    let ran = timeline(&dir);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert!(ran.stderr.is_empty(), "{}", ran.stderr);
    assert_eq!(
        ran.stdout,
        "main  320x180  10 fps  2 s  20 frames\n\
         lane 3\n\
         \x20 #sky    0 s   to 1.5 s  1.5 s\n\
         \x20 #cloud  0.5 s to 2 s    1.5 s\n"
    );
    let doc = timeline_json(&dir);
    assert_eq!(doc["diagnostics"], serde_json::json!([]));
    let lanes: Vec<(String, u64)> = doc["clips"]
        .as_array()
        .unwrap()
        .iter()
        .map(|clip| (clip["name"].as_str().unwrap().to_string(), clip["lane"].as_u64().unwrap()))
        .collect();
    assert_eq!(lanes, [("#sky".to_string(), 3), ("#cloud".to_string(), 3)]);
    // `info` says nothing either.
    let told = framesmith(&["info", text(&dir)]);
    assert_eq!(told.code, 0, "{}", told.all());
    assert!(
        !told.stdout.contains("warning") && !told.stdout.contains("overlap"),
        "{}",
        told.stdout
    );
}

#[test]
fn s2_t014_c4_the_overlap_changes_nothing_in_what_check_prints_but_the_lane_does_not_matter() {
    // The control for "nothing is reported": the same two clips in two lanes print the same check text, and
    // a real warning (a clip that can never show) is printed beside the shared lane when there is one, so
    // the output is able to carry a report.
    let shared = fixture("s2_t014_c4_same", "lane-shared");
    let apart = fixture("s2_t014_c4_apart", "lane-shared");
    let page_text = std::fs::read_to_string(apart.join("index.html")).unwrap();
    write(
        &apart,
        "index.html",
        page_text.replacen("data-fs-track=\"3\"", "data-fs-track=\"4\"", 1),
    );
    assert_eq!(check(&shared).stdout, check(&apart).stdout);
    assert_eq!(check(&shared).code, 0);
    let late = fixture("s2_t014_c4_warning", "lane-shared");
    let page_text = std::fs::read_to_string(late.join("index.html")).unwrap();
    write(
        &late,
        "index.html",
        page_text.replace(
            "  <div id=\"cloud\"",
            "  <div id=\"late\" class=\"box\" data-fs-clip data-fs-start=\"5\" data-fs-duration=\"1\" data-fs-track=\"3\"></div>\n  <div id=\"cloud\"",
        ),
    );
    let ran = check(&late);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert_eq!(codes(&ran.stdout), ["FS-W001"], "{}", ran.stdout);
    // The warning is about the clip that never shows and not about the clips that share a lane.
    assert!(ran.stdout.contains("#late") && !ran.stdout.contains("#sky"), "{}", ran.stdout);
}

#[test]
fn s2_t014_c4_render_proceeds_for_clips_that_share_a_lane_and_overlap() {
    let dir = fixture("s2_t014_c4_render", "lane-shared");
    let frames = frames_of(&dir);
    // Both boxes are painted together on frame 8, which is inside both windows.
    assert!(frames[8].count(RED, 0) > 0 && frames[8].count(BLUE, 0) > 0);
}

// ---------------------------------------------------------------------
// c5: a lane that is not a whole number, or is negative, is an error with its own code; check exits 1
// ---------------------------------------------------------------------

#[test]
fn s2_t014_c5_a_negative_lane_is_listed_as_an_error_with_its_own_code_and_check_exits_1() {
    let dir = fixture("s2_t014_c5_negative", "lane-negative");
    let checked = check(&dir);
    assert_eq!(checked.code, 1, "{}", checked.all());
    let line = lines_with("lane-negative", "data-fs-track=\"-1\"");
    assert_eq!(line, vec![17]);
    assert_eq!(
        listed(&checked.stdout),
        [Listed {
            kind: "error".into(),
            code: "FS-E055".into(),
            file: "index.html".into(),
            line: line[0]
        }]
    );
    assert_eq!(
        checked.stdout,
        "error FS-E055 index.html:17\n  \
         The lane (data-fs-track) of the clip #cloud, \"-1\", is negative; a lane is 0 or more.\n  \
         fix: Write the lane as a whole number of 0 or more, for example data-fs-track=\"1\", or remove it.\n\
         \n\
         check failed: 1 error, 0 unsupported, 0 unknown, 0 warnings\n"
    );
}

#[test]
fn s2_t014_c5_a_lane_that_is_not_a_whole_number_is_listed_as_an_error_and_check_exits_1() {
    let dir = fixture("s2_t014_c5_not_whole", "lane-not-whole");
    let checked = check(&dir);
    assert_eq!(checked.code, 1, "{}", checked.all());
    // Both faults of the file are listed in one run, each on the line of its attribute.
    let sky = lines_with("lane-not-whole", "data-fs-track=\"1.5\"");
    let bird = lines_with("lane-not-whole", "data-fs-track=\"x\"");
    assert_eq!((sky.clone(), bird.clone()), (vec![16], vec![19]));
    assert_eq!(
        listed(&checked.stdout),
        [
            Listed {
                kind: "error".into(),
                code: "FS-E028".into(),
                file: "index.html".into(),
                line: sky[0]
            },
            Listed {
                kind: "error".into(),
                code: "FS-E028".into(),
                file: "index.html".into(),
                line: bird[0]
            },
        ]
    );
    assert!(
        checked.stdout.contains(
            "error FS-E028 index.html:16\n  The lane (data-fs-track) of the clip #sky, \"1.5\", is not a whole number.\n  fix: Write the lane as a whole number of 0 or more, for example data-fs-track=\"1\", or remove it.\n"
        ),
        "{}",
        checked.stdout
    );
    assert!(
        checked
            .stdout
            .ends_with("check failed: 2 errors, 0 unsupported, 0 unknown, 0 warnings\n"),
        "{}",
        checked.stdout
    );
}

/// A value of `data-fs-track` and what the reader makes of it: `Ok` with the lane it gives, or the code of the
/// error it raises.
const LANE_VALUES: &[(&str, Result<u64, &str>)] = &[
    // Whole numbers of 0 or more, written as digits.
    ("0", Ok(0)),
    ("1", Ok(1)),
    ("12", Ok(12)),
    (" 7 ", Ok(7)),
    ("007", Ok(7)),
    ("999999999", Ok(999_999_999)),
    // A minus sign before zero is zero.
    ("-0", Ok(0)),
    // Negative: its own code.
    ("-1", Err("FS-E055")),
    ("-7", Err("FS-E055")),
    (" -3 ", Err("FS-E055")),
    ("-007", Err("FS-E055")),
    ("-999999999", Err("FS-E055")),
    // Not a whole number written as digits: the code of Sprint 1.
    ("1.5", Err("FS-E028")),
    ("1.0", Err("FS-E028")),
    ("-1.5", Err("FS-E028")),
    ("+3", Err("FS-E028")),
    ("1e2", Err("FS-E028")),
    ("3x", Err("FS-E028")),
    ("x", Err("FS-E028")),
    ("", Err("FS-E028")),
    ("-", Err("FS-E028")),
    ("--1", Err("FS-E028")),
    ("- 1", Err("FS-E028")),
    ("1 2", Err("FS-E028")),
    ("1,5", Err("FS-E028")),
    ("1234567890", Err("FS-E028")),
    ("-1234567890", Err("FS-E028")),
];

#[test]
fn s2_t014_c5_every_form_of_a_lane_is_accepted_or_raises_the_code_the_spec_gives_it() {
    for (index, (value, expected)) in LANE_VALUES.iter().enumerate() {
        let body = clip("only", "0", "1", Some(value));
        let dir = boxes(&format!("s2_t014_c5_value_{index}"), &body);
        let checked = check(&dir);
        match expected {
            Ok(lane) => {
                assert_eq!(checked.code, 0, "{value:?}: {}", checked.all());
                assert!(listed(&checked.stdout).is_empty(), "{value:?}: {}", checked.stdout);
                let doc = timeline_json(&dir);
                assert_eq!(doc["clips"][0]["lane"].as_u64(), Some(*lane), "{value:?}");
                assert_eq!(doc["clips"][0]["lane_filled_in"], false, "{value:?}");
            }
            Err(code) => {
                assert_eq!(checked.code, 1, "{value:?}: {}", checked.all());
                assert_eq!(codes(&checked.stdout), [*code], "{value:?}: {}", checked.stdout);
                assert!(checked.stdout.contains("#only"), "{value:?}: {}", checked.stdout);
            }
        }
    }
}

#[test]
fn s2_t014_c5_a_fault_in_a_lane_is_listed_with_the_other_faults_of_the_run_and_a_clip_inside_a_clip_has_it_too() {
    let body = "<div id=\"outer\" class=\"box\" data-fs-clip data-fs-start=\"0\" data-fs-duration=\"2\" data-fs-track=\"-2\">\n\
                <div id=\"inner\" class=\"box\" data-fs-clip data-fs-start=\"-1\" data-fs-duration=\"1\" data-fs-track=\"2.5\"></div>\n\
                </div>\n\
                <div id=\"third\" class=\"box\" data-fs-clip data-fs-start=\"1\" data-fs-duration=\"1\" data-fs-track=\"-9\"></div>";
    let dir = boxes("s2_t014_c5_several", body);
    let checked = check(&dir);
    assert_eq!(checked.code, 1, "{}", checked.all());
    // In the order of the clips: #outer's lane, #inner's start and lane, #third's lane.
    assert_eq!(
        codes(&checked.stdout),
        ["FS-E055", "FS-E022", "FS-E028", "FS-E055"],
        "{}",
        checked.stdout
    );
    assert!(
        checked
            .stdout
            .ends_with("check failed: 4 errors, 0 unsupported, 0 unknown, 0 warnings\n"),
        "{}",
        checked.stdout
    );
}

#[test]
fn s2_t014_c5_info_and_timeline_print_what_check_prints_and_exit_1_for_a_negative_lane() {
    let dir = fixture("s2_t014_c5_same_list", "lane-negative");
    let checked = check(&dir);
    for command in ["info", "timeline"] {
        let ran = framesmith(&[command, text(&dir)]);
        assert_eq!(ran.code, 1, "{command}: {}", ran.all());
        assert_eq!(ran.stdout, checked.stdout, "{command}");
    }
    // In JSON the document is the failure document, with the same diagnostic and no clips.
    let ran = framesmith(&["timeline", text(&dir), "--json"]);
    assert_eq!(ran.code, 1, "{}", ran.all());
    let doc = document(&ran);
    assert_eq!(doc["outcome"], "failed");
    assert!(doc.get("clips").is_none());
    let diagnostics = doc["diagnostics"].as_array().unwrap();
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0]["code"], "FS-E055");
    assert_eq!(diagnostics[0]["kind"], "error");
    assert_eq!(diagnostics[0]["file"], "index.html");
    assert_eq!(diagnostics[0]["line"], 17);
    assert!(diagnostics[0]["description"].as_str().unwrap().contains("negative"));
    assert!(diagnostics[0]["remedy"].as_str().unwrap().contains("0 or more"));
}

#[test]
fn s2_t014_c5_render_refuses_a_negative_lane_with_the_same_list_exit_1_and_starts_nothing() {
    for folder in ["lane-negative", "lane-not-whole"] {
        let dir = fixture(&format!("s2_t014_c5_render_{folder}"), folder);
        let checked = check(&dir);
        let spies = Spies::new(&format!("s2_t014_c5_render_{folder}"));
        let before = tree(&dir);
        let render = spies.run(dir.parent().unwrap(), &["render", text(&dir)]);
        assert_eq!(render.code, 1, "{folder}: {}", render.all());
        assert!(
            render.stdout.is_empty() || !render.stdout.contains("frame"),
            "{folder}: {}",
            render.stdout
        );
        assert!(
            render.stderr.contains(checked.stdout.trim_end().lines().next().unwrap()),
            "{folder}: {}",
            render.stderr
        );
        assert_eq!(codes(&render.stderr), codes(&checked.stdout), "{folder}");
        assert_eq!(spies.started(), (false, false), "{folder}");
        assert_eq!(
            tree(&dir),
            before,
            "{folder}: the project is unchanged and no output folder was made"
        );
    }
}

#[test]
fn s2_t014_c5_the_stand_ins_do_see_a_browser_and_ffmpeg_when_the_project_renders() {
    // The control for "starts nothing": the project with the lane mended does start both.
    let spies = Spies::new("s2_t014_c5_control");
    let dir = fixture("s2_t014_c5_control_project", "lane-negative");
    let page_text = std::fs::read_to_string(dir.join("index.html")).unwrap();
    write(&dir, "index.html", page_text.replace("data-fs-track=\"-1\"", "data-fs-track=\"1\""));
    let ran = spies.run(dir.parent().unwrap(), &["render", text(&dir)]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert_eq!(spies.started(), (true, true));
}

#[test]
fn s2_t014_c5_removing_the_lane_or_writing_it_as_0_or_more_mends_the_project() {
    let original = fixture("s2_t014_c5_mend", "lane-negative");
    let page_text = std::fs::read_to_string(original.join("index.html")).unwrap();
    for (what, mended) in [
        ("removed", page_text.replace(" data-fs-track=\"-1\"", "")),
        ("zero", page_text.replace("data-fs-track=\"-1\"", "data-fs-track=\"0\"")),
        ("positive", page_text.replace("data-fs-track=\"-1\"", "data-fs-track=\"1\"")),
    ] {
        let dir = project(&format!("s2_t014_c5_mend_{what}"), "p", &mended);
        let checked = check(&dir);
        assert_eq!(checked.code, 0, "{what}: {}", checked.all());
        assert!(listed(&checked.stdout).is_empty(), "{what}: {}", checked.stdout);
    }
}

#[test]
fn s2_t014_c5_the_new_code_is_listed_once_follows_the_last_error_code_and_is_given_to_nothing_else() {
    let matrix = spec("support-matrix.md");
    let rows_of = |code: &str| matrix.lines().filter(|line| line.starts_with(&format!("| {code} |"))).count();
    assert_eq!(rows_of("FS-E055"), 1);
    assert_eq!(rows_of("FS-E028"), 1);
    let errors: Vec<String> = matrix
        .lines()
        .filter_map(|line| line.strip_prefix("| FS-E"))
        .map(|rest| format!("FS-E{}", rest.split(' ').next().unwrap()))
        .collect();
    assert_eq!(errors.last().map(String::as_str), Some("FS-E055"));
    assert_eq!(errors[errors.len() - 2], "FS-E054");
    // The old code keeps its number and its meaning.
    assert!(matrix
        .lines()
        .any(|line| line == "| FS-E028 | A track that is not a whole number. |"));
    let row = matrix.lines().find(|line| line.starts_with("| FS-E055 |")).unwrap();
    assert!(row.contains("negative") && row.contains("Sprint 1 accepted it"), "{row}");
    // The number is written in one place of the product, the resolver of the model.
    let sources = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut files = Vec::new();
    for entry in std::fs::read_dir(&sources).unwrap() {
        let src = entry.unwrap().path().join("src");
        if !src.is_dir() {
            continue;
        }
        let mut stack = vec![src];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|e| e == "rs") && std::fs::read_to_string(&path).unwrap().contains("\"FS-E055\"") {
                    files.push(path.strip_prefix(&sources).unwrap().to_path_buf());
                }
            }
        }
    }
    assert_eq!(files, [PathBuf::from("model/src/resolve.rs")]);
}

#[test]
fn s2_t014_the_spec_names_the_lane_rules_their_sources_and_the_deliberate_change() {
    let composition = spec("composition.md");
    let status = composition.lines().nth(2).unwrap();
    assert!(
        status.contains("ticket T014 added the lane") && status.contains("FS-E055"),
        "{status}"
    );
    let start = composition.find("### Lanes").expect("a section on lanes");
    let end = composition[start..].find("### How a clip is hidden").unwrap() + start;
    let section = &composition[start..end];
    for needed in [
        "A clip with no lane is in lane 0",
        "Two clips may share a lane",
        "A lane never changes a rendered frame",
        "A deliberate change",
        "FS-E028",
        "FS-E055",
        "number order (2 before 10)",
    ] {
        assert!(section.contains(needed), "{needed}: {section}");
    }
    let commands = spec("commands.md");
    assert!(commands.lines().nth(2).unwrap().contains("ticket T014 added the lanes"));
    assert!(commands.contains("`lane` (the clip's lane") && commands.contains("`lane_filled_in`"));
    let matrix = spec("support-matrix.md");
    assert!(matrix.lines().nth(2).unwrap().contains("Ticket T014 added the error FS-E055"));
}
