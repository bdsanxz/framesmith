//! Sprint 2, ticket T008: `framesmith timeline <project>` prints a header and one line for each clip with
//! the start, end and duration the project resolved, marks what was filled in, agrees with a render,
//! gives `check`'s diagnostics for a project that does not pass, only prints, needs no browser and no
//! `ffmpeg`, and prints plain text.
//!
//! Notes for maintainers
//! ---------------------
//! * The times of `timeline` come from the manifest the check builds (the one reading every command
//!   shares). These tests read the text of the built binary only, never the manifest: the printed times
//!   are parsed out of the text.
//! * The test projects are small and written for this file. Their expected text is written out in the
//!   `Known` table (and in the tests that need another project), so it does not depend on the code that
//!   prints it.
//! * Only c3's render test and the control render of c6 need a browser and `ffmpeg`; all the others read
//!   files only.
//! * Scratch folders: every test names its projects with a scratch key of its own (the tests of a file
//!   run in parallel, and a key names a folder that is emptied when it is made).
//! * The printing of warnings before the timeline is exercised by `s2_t008_c1_warnings_...`; the same
//!   warning before the facts of `info` is exercised in `s2_t007_info_json.rs`.

mod common;

use common::*;
use measure::{Frame, Ran, Rgb};
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// A project of this file: how it is written and the text `timeline` is to print for it.
struct Known {
    name: &'static str,
    attrs: &'static str,
    /// The clips as (start, duration written), in seconds.
    clips: &'static [(&'static str, Option<&'static str>)],
    tweens: &'static str,
    /// What `timeline` is to print.
    timeline: &'static str,
}

const TWEEN: &str = "{ \"target\": \"#c0\", \"to\": { \"x\": 10 }, \"at\": 0, \"duration\": 0.5 }";

/// Everything written: nothing is marked.
const WRITTEN: Known = Known {
    name: "written",
    attrs: "data-fs-size=\"64x36\" data-fs-fps=\"24\" data-fs-duration=\"1\"",
    clips: &[("0", Some("0.5")), ("0.5", Some("0.5"))],
    tweens: TWEEN,
    timeline: "main  64x36  24 fps  1 s  24 frames\n\
               lane 0\n\
               \x20 #c0  0 s   to 0.5 s  0.5 s (lane filled in)\n\
               \x20 #c1  0.5 s to 1 s    0.5 s (lane filled in)\n",
};

/// The rate is left out: 30 is filled in.
const RATE_FILLED: Known = Known {
    name: "rate_filled",
    attrs: "data-fs-size=\"64x36\" data-fs-duration=\"1\"",
    clips: &[("0", Some("0.5")), ("0.5", Some("0.5"))],
    tweens: TWEEN,
    timeline: "main  64x36  30 fps (filled in)  1 s  30 frames\n\
               lane 0\n\
               \x20 #c0  0 s   to 0.5 s  0.5 s (lane filled in)\n\
               \x20 #c1  0.5 s to 1 s    0.5 s (lane filled in)\n",
};

/// The length is left out: it is worked out from the motion (2 s), and a clip with no duration runs to the
/// end of it.
const LENGTH_WORKED_OUT: Known = Known {
    name: "length_worked_out",
    attrs: "data-fs-size=\"64x36\" data-fs-fps=\"25\"",
    clips: &[("0", Some("0.5")), ("0.5", None)],
    tweens: "{ \"target\": \"#c0\", \"to\": { \"x\": 10 }, \"at\": 1, \"duration\": 1 }",
    timeline: "main  64x36  25 fps  2 s (filled in)  50 frames\n\
               lane 0\n\
               \x20 #c0  0 s   to 0.5 s  0.5 s (lane filled in)\n\
               \x20 #c1  0.5 s to 2 s    1.5 s (filled in) (lane filled in)\n",
};

/// The length is written (3 s) and two clips have no duration: only those clips are marked.
const DURATIONS_WORKED_OUT: Known = Known {
    name: "durations_worked_out",
    attrs: "data-fs-size=\"64x36\" data-fs-fps=\"10\" data-fs-duration=\"3\"",
    clips: &[("0", Some("1")), ("1", None), ("2.5", None)],
    tweens: TWEEN,
    timeline: "main  64x36  10 fps  3 s  30 frames\n\
               lane 0\n\
               \x20 #c0  0 s   to 1 s  1 s (lane filled in)\n\
               \x20 #c1  1 s   to 3 s  2 s (filled in) (lane filled in)\n\
               \x20 #c2  2.5 s to 3 s  0.5 s (filled in) (lane filled in)\n",
};

const ALL: [&Known; 4] = [&WRITTEN, &RATE_FILLED, &LENGTH_WORKED_OUT, &DURATIONS_WORKED_OUT];

impl Known {
    fn make(&self, test: &str) -> PathBuf {
        let body: String = self
            .clips
            .iter()
            .enumerate()
            .map(|(number, (start, duration))| {
                let duration = duration.map_or(String::new(), |d| format!(" data-fs-duration=\"{d}\""));
                format!("<div id=\"c{number}\" data-fs-clip data-fs-start=\"{start}\"{duration}></div>\n")
            })
            .collect();
        project(test, "known", &page(self.attrs, "", &body, &motion(self.tweens)))
    }
}

fn timeline(dir: &Path) -> Ran {
    framesmith(&["timeline", text(dir)])
}

fn info(dir: &Path) -> Ran {
    framesmith(&["info", text(dir)])
}

fn check(dir: &Path) -> Ran {
    framesmith(&["check", text(dir)])
}

/// Every entry under a folder, folders and files, as paths relative to it.
fn entries(dir: &Path) -> Vec<PathBuf> {
    fn walk(root: &Path, dir: &Path, found: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            found.push(path.strip_prefix(root).unwrap().to_path_buf());
            if path.is_dir() {
                walk(root, &path, found);
            }
        }
    }
    let mut found = Vec::new();
    walk(dir, dir, &mut found);
    found.sort();
    found
}

/// Every file of a folder with the time it was last modified.
fn modified(dir: &Path) -> BTreeMap<PathBuf, std::time::SystemTime> {
    entries(dir)
        .into_iter()
        .map(|entry| {
            let time = std::fs::metadata(dir.join(&entry)).unwrap().modified().unwrap();
            (entry, time)
        })
        .collect()
}

/// The header of a timeline as (name, size, rate, length, frames), and its clip lines.
#[derive(Debug, PartialEq)]
struct Row {
    name: String,
    start: String,
    end: String,
    duration: String,
    filled_in: bool,
    /// The number of the lane line the clip stands under.
    lane: String,
    /// Whether the line ends in the mark of a lane that was filled in.
    lane_filled_in: bool,
}

/// Reads the printed timeline: the header's five cells (separated by two spaces), the lane lines and the
/// clip lines under them. A line that does not have the shape of the layout makes the test fail, so a change
/// of layout is seen. Since ticket T014 the clips stand under lane lines (`lane 0`), and a clip whose lane
/// was not written ends in `(lane filled in)`.
fn read_timeline(printed: &str) -> ([String; 5], Vec<Row>) {
    let mut lines = printed.lines();
    let header = lines.next().expect("a header line");
    let cells: Vec<String> = header.split("  ").map(String::from).collect();
    let header: [String; 5] = cells.try_into().unwrap_or_else(|c| panic!("the header is not five cells: {c:?}"));
    let mut lane = None;
    let mut rows = Vec::new();
    for line in lines {
        if let Some(number) = line.strip_prefix("lane ") {
            lane = Some(number.to_string());
            continue;
        }
        let lane = lane
            .clone()
            .unwrap_or_else(|| panic!("a clip line comes before any lane line: {line:?}"));
        let body = line
            .strip_prefix("  ")
            .unwrap_or_else(|| panic!("a clip line is indented by two spaces: {line:?}"));
        let (body, lane_filled_in) = match body.strip_suffix(" (lane filled in)") {
            Some(body) => (body, true),
            None => (body, false),
        };
        let (name, rest) = body
            .split_once("  ")
            .unwrap_or_else(|| panic!("no two spaces after the name: {line:?}"));
        let words: Vec<&str> = rest.split_whitespace().collect();
        let (times, filled_in) = match words[..] {
            [a, "s", "to", b, "s", c, "s"] => ([a, b, c], false),
            [a, "s", "to", b, "s", c, "s", "(filled", "in)"] => ([a, b, c], true),
            _ => panic!("a clip line does not have the shape of the layout: {line:?}"),
        };
        rows.push(Row {
            name: name.trim_end().to_string(),
            start: times[0].to_string(),
            end: times[1].to_string(),
            duration: times[2].to_string(),
            filled_in,
            lane,
            lane_filled_in,
        });
    }
    (header, rows)
}

// ---------------------------------------------------------------------
// c1
// ---------------------------------------------------------------------

#[test]
fn s2_t008_c1_timeline_prints_the_header_and_one_line_for_each_clip_and_exits_0() {
    for known in ALL {
        let dir = known.make(&format!("s2_t008_c1_{}", known.name));
        let ran = timeline(&dir);
        assert_eq!(ran.code, 0, "{}", ran.all());
        assert!(ran.stderr.is_empty(), "{}", ran.stderr);
        assert_eq!(ran.stdout, known.timeline, "{}", known.name);
        // One header line, one lane line (no lane is written, so every clip is in lane 0), then exactly one
        // line for each clip.
        assert_eq!(ran.stdout.lines().count(), 2 + known.clips.len(), "{}", known.name);
    }
}

#[test]
fn s2_t008_c1_the_header_holds_the_facts_info_and_check_print() {
    for known in ALL {
        let dir = known.make(&format!("s2_t008_c1_facts_{}", known.name));
        let (header, rows) = read_timeline(&timeline(&dir).stdout);
        let facts = |printout: &str| -> BTreeMap<String, String> {
            printout
                .lines()
                .filter(|line| line.len() > 13 && line.is_char_boundary(13) && line[..13].ends_with(' '))
                .map(|line| (line[..13].trim().to_string(), line[13..].to_string()))
                .collect()
        };
        let told = facts(&info(&dir).stdout);
        let [name, size, rate, length, frames] = &header;
        assert_eq!(name, &told["composition"], "{}", known.name);
        assert_eq!(size, &told["size"], "{}", known.name);
        assert_eq!(rate, &told["rate"], "{}", known.name);
        assert_eq!(length, &told["length"], "{}", known.name);
        assert_eq!(frames, &format!("{} frames", told["frames"]), "{}", known.name);
        assert_eq!(rows.len().to_string(), told["clips"], "{}", known.name);
        // And `check` prints the same facts (indented by two spaces).
        let checked = check(&dir).stdout;
        let checked = facts(&checked.replace("\n  ", "\n"));
        assert_eq!(size, &checked["size"]);
        assert_eq!(rate, &checked["rate"]);
        assert_eq!(length, &checked["length"]);
    }
}

#[test]
fn s2_t008_c1_the_clips_are_listed_in_document_order_with_their_names_and_the_columns_line_up() {
    // Not in time order; one clip has no id (named by its number and tag); names of different widths.
    let body = "<div id=\"intro\" data-fs-clip data-fs-start=\"2\" data-fs-duration=\"1\"></div>\n\
                <div data-fs-clip data-fs-start=\"0\" data-fs-duration=\"0.25\"></div>\n\
                <p id=\"x\" data-fs-clip data-fs-start=\"1.5\" data-fs-duration=\"2\"></p>";
    let dir = project(
        "s2_t008_c1_names",
        "known",
        &page("data-fs-size=\"64x36\" data-fs-fps=\"10\" data-fs-duration=\"4\"", "", body, ""),
    );
    let ran = timeline(&dir);
    assert_eq!(ran.code, 0, "{}", ran.all());
    let (header, rows) = read_timeline(&ran.stdout);
    assert_eq!(header, ["main", "64x36", "10 fps", "4 s", "40 frames"]);
    let cells: Vec<[&str; 4]> = rows
        .iter()
        .map(|r| [r.name.as_str(), r.start.as_str(), r.end.as_str(), r.duration.as_str()])
        .collect();
    assert_eq!(
        cells,
        [
            ["#intro", "2", "3", "1"],
            ["number 2 <div>", "0", "0.25", "0.25"],
            ["#x", "1.5", "3.5", "2"],
        ]
    );
    assert!(rows.iter().all(|r| !r.filled_in));
    // The columns line up: the name begins at the same place on every line, so does the start, the
    // word `to`, the end and the duration; and no line ends in a space.
    let clip_lines: Vec<&str> = ran.stdout.lines().skip(2).collect();
    let at = |line: &str, text: &str| line.find(text).unwrap_or_else(|| panic!("{text:?} not in {line:?}"));
    let name_width = "number 2 <div>".len();
    for line in &clip_lines {
        assert!(!line.ends_with(' '), "{line:?}");
        assert!(line.starts_with("  ") && !line.starts_with("   "), "{line:?}");
        // The start begins two spaces after the widest name.
        assert_eq!(&line[2 + name_width..2 + name_width + 2], "  ", "{line:?}");
    }
    let start_col = 2 + name_width + 2;
    let to_cols: Vec<usize> = clip_lines.iter().map(|l| at(l, " to ")).collect();
    assert!(to_cols.windows(2).all(|w| w[0] == w[1]), "{to_cols:?}");
    // The widest start is "1.5 s" (5 columns), so ` to ` stands at start_col + 5.
    assert_eq!(to_cols[0], start_col + "1.5 s".len(), "{clip_lines:?}");
    let duration_cols: Vec<usize> = clip_lines
        .iter()
        .zip(&rows)
        .map(|(l, r)| l.rfind(&format!("  {} s", r.duration)).unwrap())
        .collect();
    assert!(duration_cols.windows(2).all(|w| w[0] == w[1]), "{duration_cols:?} {clip_lines:?}");
}

#[test]
fn s2_t008_c1_a_composition_with_no_clips_prints_the_header_only() {
    let dir = project(
        "s2_t008_c1_no_clips",
        "known",
        &page("data-fs-size=\"64x36\" data-fs-fps=\"24\" data-fs-duration=\"1\"", "", "", ""),
    );
    let ran = timeline(&dir);
    assert_eq!(ran.code, 0, "{}", ran.all());
    // No clip, so no lane line either.
    assert_eq!(ran.stdout, "main  64x36  24 fps  1 s  24 frames\n");
}

#[test]
fn s2_t008_c1_one_frame_is_one_frame() {
    let dir = project(
        "s2_t008_c1_one_frame",
        "known",
        &page("data-fs-size=\"64x36\" data-fs-fps=\"1\" data-fs-duration=\"1\"", "", "", ""),
    );
    let ran = timeline(&dir);
    assert_eq!(ran.stdout, "main  64x36  1 fps  1 s  1 frame\n", "{}", ran.all());
}

#[test]
fn s2_t008_c1_the_folder_may_be_written_relative_and_the_output_is_the_same() {
    let dir = WRITTEN.make("s2_t008_c1_relative");
    let parent = std::fs::canonicalize(dir.parent().unwrap()).unwrap();
    let ran = run_in(&parent, &["timeline", "known"], &[]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert_eq!(ran.stdout, WRITTEN.timeline);
    let inside = run_in(&parent.join("known"), &["timeline", "."], &[]);
    assert_eq!(inside.stdout, WRITTEN.timeline, "{}", inside.all());
}

#[test]
fn s2_t008_c1_warnings_of_a_passing_project_are_printed_before_the_timeline() {
    // A clip that starts at or after the end of the composition is the warning FS-W001; the project still
    // passes and the clip is listed.
    let body = "<div id=\"c0\" data-fs-clip data-fs-start=\"0\" data-fs-duration=\"1\"></div>\n\
                <div id=\"late\" data-fs-clip data-fs-start=\"5\" data-fs-duration=\"1\"></div>";
    let dir = project(
        "s2_t008_c1_warning",
        "known",
        &page(
            "data-fs-size=\"64x36\" data-fs-fps=\"24\" data-fs-duration=\"2\"",
            "",
            body,
            &motion(TWEEN),
        ),
    );
    let ran = timeline(&dir);
    assert_eq!(ran.code, 0, "{}", ran.all());
    let checked = check(&dir);
    assert_eq!(checked.code, 0, "{}", checked.all());
    // The warning block, as `check` prints it, then a blank line, then the header.
    let warning: String = checked.stdout.split("check passed\n").next().unwrap().to_string();
    assert!(warning.starts_with("warning FS-W001 index.html:"), "{warning}");
    assert!(warning.ends_with("\n\n"), "{warning:?}");
    assert_eq!(
        ran.stdout,
        format!("{warning}main  64x36  24 fps  2 s  48 frames\nlane 0\n  #c0    0 s to 1 s  1 s (lane filled in)\n  #late  5 s to 6 s  1 s (lane filled in)\n")
    );
}

#[test]
fn s2_t008_c1_the_help_lists_the_command() {
    let ran = framesmith(&["--help"]);
    assert_eq!(ran.code, 0);
    assert!(ran.stdout.contains("framesmith timeline <project>"), "{}", ran.stdout);
}

#[test]
fn s2_t008_c1_the_command_is_logged_with_its_input_and_its_result_under_one_run_id() {
    let dir = RATE_FILLED.make("s2_t008_c1_log");
    let logs = scratch("s2_t008_c1_log_folder");
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
    assert!(lines[0].contains(&format!("project={}", dir.display())), "{log}");
    assert!(lines[1].contains("command finished") && lines[1].contains("exit=0"), "{log}");
    assert!(lines[1].contains("clips=2"), "{log}");
    let run = |line: &str| line.split_whitespace().find(|word| word.starts_with("run=")).map(String::from);
    assert!(run(lines[0]).is_some() && run(lines[0]) == run(lines[1]), "{log}");
    // No file contents in it.
    assert!(!log.contains("data-fs"), "{log}");
}

// ---------------------------------------------------------------------
// c2
// ---------------------------------------------------------------------

/// The marks a timeline carries, as (where, cell) pairs: `header: rate`, `header: length` and
/// `clip #name`.
fn marks(printed: &str) -> Vec<String> {
    let (header, rows) = read_timeline(printed);
    let mut found = Vec::new();
    if header[2].contains("(filled in)") {
        found.push("header: rate".to_string());
    }
    if header[3].contains("(filled in)") {
        found.push("header: length".to_string());
    }
    for row in rows.iter().filter(|r| r.filled_in) {
        found.push(format!("clip {}", row.name));
    }
    // Nothing else of the text carries the words, but the mark of a lane that was filled in (ticket T014),
    // which is not one of these marks and is counted apart.
    let lane_marks = rows.iter().filter(|r| r.lane_filled_in).count();
    assert_eq!(printed.matches("filled in").count(), found.len() + lane_marks, "{printed}");
    found
}

#[test]
fn s2_t008_c2_a_worked_out_duration_and_a_worked_out_length_are_marked_and_written_values_are_not() {
    let marks_of = |known: &Known| {
        let dir = known.make(&format!("s2_t008_c2_{}", known.name));
        let ran = timeline(&dir);
        assert_eq!(ran.code, 0, "{}", ran.all());
        marks(&ran.stdout)
    };
    // Everything written: no mark anywhere.
    assert_eq!(marks_of(&WRITTEN), Vec::<String>::new());
    // The rate left out: only the rate is marked.
    assert_eq!(marks_of(&RATE_FILLED), ["header: rate"]);
    // The length left out (a written rate, a written duration are not marked): the length and the clip
    // whose duration was worked out.
    assert_eq!(marks_of(&LENGTH_WORKED_OUT), ["header: length", "clip #c1"]);
    // The length written: it is not marked although two clip durations were worked out.
    assert_eq!(marks_of(&DURATIONS_WORKED_OUT), ["clip #c1", "clip #c2"]);
}

#[test]
fn s2_t008_c2_the_mark_is_in_the_header_after_the_length_and_after_the_duration_of_the_clip() {
    let dir = LENGTH_WORKED_OUT.make("s2_t008_c2_place");
    let ran = timeline(&dir);
    let lines: Vec<&str> = ran.stdout.lines().collect();
    assert_eq!(lines[0], "main  64x36  25 fps  2 s (filled in)  50 frames");
    // (ticket T014: a lane line stands between the header and the clips, and a clip with no lane written
    // ends in the mark of a lane that was filled in, after the mark of the duration)
    assert_eq!(lines[1], "lane 0");
    assert!(
        lines[2].ends_with("  0.5 s (lane filled in)") && !lines[2].contains("(filled in)"),
        "{}",
        lines[2]
    );
    assert!(lines[3].ends_with("  1.5 s (filled in) (lane filled in)"), "{}", lines[3]);
}

#[test]
fn s2_t008_c2_a_rate_written_as_30_and_a_length_written_are_not_marked() {
    let dir = project(
        "s2_t008_c2_thirty",
        "known",
        &page(
            "data-fs-size=\"64x36\" data-fs-fps=\"30\" data-fs-duration=\"2\"",
            "",
            // (ticket T014: the lane is written too, 0, so that nothing at all is filled in)
            "<div id=\"c0\" data-fs-clip data-fs-start=\"0\" data-fs-duration=\"1\" data-fs-track=\"0\"></div>",
            "",
        ),
    );
    let ran = timeline(&dir);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert!(!ran.stdout.contains("filled in"), "{}", ran.stdout);
    assert!(ran.stdout.starts_with("main  64x36  30 fps  2 s  60 frames\n"), "{}", ran.stdout);
}

#[test]
fn s2_t008_c2_the_marks_are_those_of_info() {
    for known in ALL {
        let dir = known.make(&format!("s2_t008_c2_info_{}", known.name));
        let told = info(&dir).stdout;
        let printed = timeline(&dir).stdout;
        let from_info: Vec<String> = told
            .lines()
            .filter_map(|line| line.strip_prefix("clip "))
            .filter_map(|rest| rest.split_once("  duration ").map(|(name, _)| format!("clip {name}")))
            .collect();
        let from_timeline: Vec<String> = marks(&printed).into_iter().filter(|m| m.starts_with("clip ")).collect();
        assert_eq!(from_timeline, from_info, "{}", known.name);
        assert_eq!(
            told.contains("rate         30 fps (filled in)"),
            printed.contains("30 fps (filled in)"),
            "{}",
            known.name
        );
        assert_eq!(
            told.lines().any(|l| l.starts_with("length") && l.contains("(filled in)")),
            read_timeline(&printed).0[3].contains("(filled in)"),
            "{}",
            known.name
        );
    }
}

// ---------------------------------------------------------------------
// c3
// ---------------------------------------------------------------------

const RED: Rgb = [255, 0, 0];
const GREEN: Rgb = [0, 255, 0];
const BLUE: Rgb = [0, 0, 255];
const STYLE: &str = ".box { position: absolute; width: 20px; height: 20px; }\n\
#red { left: 10px; top: 10px; background: #ff0000; }\n\
#green { left: 40px; top: 10px; background: #00ff00; }\n\
#blue { left: 70px; top: 10px; background: #0000ff; }";

/// The project of c3, at 20 frames a second and with its length worked out (3 s, 60 frames from the
/// motion): a red clip with a written start and duration on the frame grid; a green clip whose start and
/// end fall between frame times; a blue clip with no duration, which lasts until the end.
fn render_project(test: &str) -> PathBuf {
    let body = "<div id=\"red\" class=\"box\" data-fs-clip data-fs-start=\"0.5\" data-fs-duration=\"1\"></div>\n\
                <div id=\"green\" class=\"box\" data-fs-clip data-fs-start=\"1.23\" data-fs-duration=\"0.8\"></div>\n\
                <div id=\"blue\" class=\"box\" data-fs-clip data-fs-start=\"2\"></div>\n\
                <div id=\"beat\"></div>";
    let index = page(
        "data-fs-size=\"100x40\" data-fs-fps=\"20\"",
        STYLE,
        body,
        &motion("{ \"target\": \"#beat\", \"to\": { \"opacity\": 1 }, \"at\": 0, \"duration\": 3 }"),
    );
    project(test, "clips", &index)
}

/// The frames on which a box of the colour is painted, whole.
fn shown(frames: &[Frame], colour: Rgb) -> Vec<usize> {
    let mut found = Vec::new();
    for (number, frame) in frames.iter().enumerate() {
        match frame.count(colour, 0) {
            0 => {}
            400 => found.push(number),
            part => panic!("frame {number} shows {part} pixels of the box, neither none nor all"),
        }
    }
    found
}

/// The frame a printed time falls on: the first frame whose time is at or after it, counted at `fps`
/// (a time within a thousandth of a frame of a frame time is that frame, as in the frame rule).
fn frame_of(seconds: &str, fps: u32) -> usize {
    let frames: f64 = seconds.parse::<f64>().unwrap() * f64::from(fps);
    (frames - 0.001).ceil().max(0.0) as usize
}

/// What the render shows for each clip of a timeline: (name, first frame it shows on, first frame on which
/// it is gone or the frame count when it shows to the end), and the frame rate and count of the header.
fn disagreements(printed: &str, frames: &[Frame]) -> Vec<String> {
    let (header, rows) = read_timeline(printed);
    let fps: u32 = header[2].split(' ').next().unwrap().parse().unwrap();
    let count: usize = header[4].split(' ').next().unwrap().parse().unwrap();
    let mut found = Vec::new();
    if count != frames.len() {
        found.push(format!("the header says {count} frames, the render has {}", frames.len()));
    }
    for row in rows {
        let colour = match row.name.as_str() {
            "#red" => RED,
            "#green" => GREEN,
            "#blue" => BLUE,
            other => panic!("unexpected clip {other}"),
        };
        let on = shown(frames, colour);
        let Some(&first) = on.first() else {
            found.push(format!("{} never shows", row.name));
            continue;
        };
        let gone = (first..frames.len()).find(|n| !on.contains(n)).unwrap_or(frames.len());
        // The clip shows on every frame between the two and on none other.
        if on != (first..gone).collect::<Vec<_>>() {
            found.push(format!("{} shows on frames {on:?}, which are not one run", row.name));
        }
        let (start, end) = (frame_of(&row.start, fps), frame_of(&row.end, fps).min(frames.len()));
        if first != start {
            found.push(format!(
                "{}: starts at {} s = frame {start}, but first shows on frame {first}",
                row.name, row.start
            ));
        }
        if gone != end {
            found.push(format!(
                "{}: ends at {} s = frame {end}, but is gone on frame {gone}",
                row.name, row.end
            ));
        }
    }
    found
}

#[test]
fn s2_t008_c3_the_printed_times_equal_the_frames_on_which_each_clip_appears_and_disappears() {
    let dir = render_project("s2_t008_c3_render");
    let printed = timeline(&dir);
    assert_eq!(printed.code, 0, "{}", printed.all());
    let (_, rows) = read_timeline(&printed.stdout);
    // The projects has written and worked-out durations: the red and green are written, the blue is not.
    assert_eq!(rows.iter().map(|r| r.filled_in).collect::<Vec<_>>(), [false, false, true]);
    // What is printed (known values, from the way the project is written).
    let cells: Vec<[&str; 4]> = rows
        .iter()
        .map(|r| [r.name.as_str(), r.start.as_str(), r.end.as_str(), r.duration.as_str()])
        .collect();
    assert_eq!(
        cells,
        [
            ["#red", "0.5", "1.5", "1"],
            ["#green", "1.23", "2.03", "0.8"],
            ["#blue", "2", "3", "1"],
        ]
    );

    let (ran, frames) = render_frames(&dir, &[], &[]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert_eq!(frames.len(), 60);
    // The frames of the render, measured: the red box is on frames 10 to 29, the green box on 25 to 40
    // (its start 1.23 s and end 2.03 s fall between frame times), the blue box on 40 to 59 (to the end).
    assert_eq!(shown(&frames, RED), (10..30).collect::<Vec<_>>());
    assert_eq!(shown(&frames, GREEN), (25..41).collect::<Vec<_>>());
    assert_eq!(shown(&frames, BLUE), (40..60).collect::<Vec<_>>());
    // The printed times, converted with the frame rate, are those frames.
    assert_eq!(disagreements(&printed.stdout, &frames), Vec::<String>::new());
    // The same run of the command on the same files gives the same text after the render.
    assert_eq!(timeline(&dir).stdout, printed.stdout);
}

#[test]
fn s2_t008_c3_the_comparison_sees_a_time_that_is_wrong() {
    // The control: a printed time that is a frame off, or a header that is off, is found.
    let dir = render_project("s2_t008_c3_control");
    let printed = timeline(&dir).stdout;
    let (_, frames) = render_frames(&dir, &[], &[]);
    assert_eq!(frames.len(), 60);
    assert_eq!(disagreements(&printed, &frames), Vec::<String>::new());

    let wrong_start = printed.replace("#red    0.5 s", "#red    0.55 s");
    assert_ne!(wrong_start, printed);
    let found = disagreements(&wrong_start, &frames);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("#red") && found[0].contains("starts at 0.55 s"), "{found:?}");

    let wrong_end = printed.replace("to 3 s", "to 2.95 s");
    assert_ne!(wrong_end, printed);
    let found = disagreements(&wrong_end, &frames);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("#blue") && found[0].contains("ends at 2.95 s"), "{found:?}");

    let wrong_count = printed.replace("60 frames", "59 frames");
    assert!(!disagreements(&wrong_count, &frames).is_empty());
}

// ---------------------------------------------------------------------
// c4
// ---------------------------------------------------------------------

fn assert_same_diagnostics(dir: &Path, code: i32, what: &str) {
    let told = timeline(dir);
    let checked = check(dir);
    assert_eq!(checked.code, code, "{what}: {}", checked.all());
    assert_eq!(told.code, checked.code, "{what}: {}", told.all());
    assert_eq!(told.stdout, checked.stdout, "{what}");
    assert_eq!(told.stderr, checked.stderr, "{what}");
    assert!(
        told.stdout.contains("check failed") || told.stdout.contains("check refused"),
        "{what}: {}",
        told.stdout
    );
    // No timeline is printed beside the diagnostics: no header line, no clip line.
    assert!(
        !told.stdout.lines().any(|l| l.contains(" fps") && l.ends_with(" frames")),
        "{what}: {}",
        told.stdout
    );
    assert!(
        !told.stdout.lines().any(|l| l.starts_with("  #") && l.contains(" s to ")),
        "{what}: {}",
        told.stdout
    );
}

#[test]
fn s2_t008_c4_a_project_with_an_error_gets_the_diagnostics_of_check_and_exit_1() {
    // Three errors at once, so that the list and its order are compared too.
    let dir = project(
        "s2_t008_c4_errors",
        "faulty",
        &page(
            "data-fs-size=\"65x36\" data-fs-fps=\"23.976\" data-fs-duration=\"-1\"",
            "",
            "<div id=\"c0\" data-fs-clip data-fs-start=\"0\" data-fs-duration=\"1\"></div>",
            "",
        ),
    );
    assert_same_diagnostics(&dir, 1, "three errors");
    assert!(timeline(&dir)
        .stdout
        .contains("check failed: 3 errors, 0 unsupported, 0 unknown, 0 warnings\n"));

    // A folder that does not exist, and a folder with no page.
    let root = scratch("s2_t008_c4_places");
    assert_same_diagnostics(&root.join("nowhere"), 1, "a missing folder");
    assert!(timeline(&root.join("nowhere")).stdout.contains("FS-E001"));
    std::fs::create_dir_all(root.join("bare")).unwrap();
    write(&root, "bare/notes.txt", "no page here");
    assert_same_diagnostics(&root.join("bare"), 1, "a folder with no page");
}

#[test]
fn s2_t008_c4_a_project_with_an_unsupported_feature_gets_the_diagnostics_of_check_and_exit_2() {
    for (name, code) in [
        ("unsupported/fs-u001-video-element", "FS-U001"),
        ("unsupported/fs-u014-css-animation-in-stylesheet", "FS-U014"),
        ("unsupported/fs-u009-network-image", "FS-U009"),
        ("unsupported/fs-x001-unknown-attribute", "FS-X001"),
    ] {
        let dir = fixture(&format!("s2_t008_c4_{code}"), name);
        assert_same_diagnostics(&dir, 2, name);
        assert!(timeline(&dir).stdout.contains(code), "{name}");
        assert!(timeline(&dir).stdout.contains("check refused: "), "{name}");
    }
}

// ---------------------------------------------------------------------
// c5
// ---------------------------------------------------------------------

#[test]
fn s2_t008_c5_an_edit_word_after_the_project_is_refused_changes_nothing_and_says_timeline_only_prints() {
    let dir = LENGTH_WORKED_OUT.make("s2_t008_c5_words");
    let root = dir.parent().unwrap();
    let (files, all, times, beside) = (tree(&dir), entries(&dir), modified(&dir), entries(root));
    for words in [
        &["move"][..],
        &["move", "#c0", "--to", "2"],
        &["trim"],
        &["delete", "#c1"],
        &["set", "#c0", "start", "1"],
    ] {
        let mut args = vec!["timeline", text(&dir)];
        args.extend_from_slice(words);
        let ran = framesmith(&args);
        assert_eq!(ran.code, 1, "{words:?}: {}", ran.all());
        assert_eq!(ran.stdout, "", "{words:?}");
        assert!(ran.stderr.starts_with("error FS-R004\n"), "{words:?}: {}", ran.stderr);
        // The message names the word and says the command only prints and has no edit commands.
        assert!(
            ran.stderr.contains(&format!("{} is not one of its words", words[0])),
            "{words:?}: {}",
            ran.stderr
        );
        assert!(ran.stderr.contains("only prints the timeline"), "{words:?}: {}", ran.stderr);
        assert!(ran.stderr.contains("has no edit commands"), "{words:?}: {}", ran.stderr);
        // Nothing of the project, or beside it, changed.
        assert_eq!(tree(&dir), files, "{words:?}");
        assert_eq!(entries(&dir), all, "{words:?}");
        assert_eq!(modified(&dir), times, "{words:?}");
        assert_eq!(entries(root), beside, "{words:?}");
        assert!(!dir.join("out").exists());
    }
    // The exact message, once.
    let ran = framesmith(&["timeline", text(&dir), "move"]);
    assert_eq!(
        ran.stderr,
        "error FS-R004\n  `framesmith timeline` only prints the timeline and has no edit commands; move is not one of its words.\n  \
         fix: Run `framesmith --help` to see the commands and their options.\n"
    );
    // The same project without the word still prints, so the refusal is about the word.
    let plain = timeline(&dir);
    assert_eq!(plain.code, 0, "{}", plain.all());
    assert_eq!(plain.stdout, LENGTH_WORKED_OUT.timeline);
}

#[test]
fn s2_t008_c5_the_edit_word_is_refused_also_when_the_project_does_not_pass_and_is_not_read() {
    // The refusal comes from reading the command line, before the project is read: a faulty project
    // and a folder that does not exist give the same answer.
    let faulty = project(
        "s2_t008_c5_faulty",
        "faulty",
        &page("data-fs-size=\"65x36\" data-fs-duration=\"1\"", "", "", ""),
    );
    let root = scratch("s2_t008_c5_places");
    for dir in [faulty, root.join("nowhere")] {
        let ran = framesmith(&["timeline", text(&dir), "move"]);
        assert_eq!(ran.code, 1, "{}", ran.all());
        assert_eq!(ran.stdout, "");
        assert!(ran.stderr.contains("only prints the timeline"), "{}", ran.stderr);
        assert!(!ran.stderr.contains("FS-E"), "{}", ran.stderr);
    }
    assert!(!root.join("nowhere").exists());
}

#[test]
fn s2_t008_c5_an_option_is_refused_as_info_refuses_one_it_does_not_have() {
    let dir = WRITTEN.make("s2_t008_c5_options");
    let (files, all) = (tree(&dir), entries(&dir));
    // `--json` is an option of `timeline` since T009 (its refusal is no longer asserted here; see
    // `s2_t009_timeline_json.rs`). `--composition` belongs to a later ticket, and the others are not
    // options `timeline` has: for now they are all refused.
    for extra in [&["--composition", "main"][..], &["--bogus"], &["--output", "x.mp4"], &["--fps=24"]] {
        let mut args = vec!["timeline", text(&dir)];
        args.extend_from_slice(extra);
        let ran = framesmith(&args);
        assert_eq!(ran.code, 1, "{extra:?}: {}", ran.all());
        assert_eq!(ran.stdout, "", "{extra:?}");
        assert!(ran.stderr.starts_with("error FS-R004\n"), "{extra:?}: {}", ran.stderr);
        assert!(ran.stderr.contains("too many"), "{extra:?}: {}", ran.stderr);
        // Worded as `info` words it for the same option.
        let mut same = vec!["info", text(&dir)];
        same.extend_from_slice(extra);
        let told = framesmith(&same);
        assert_eq!(
            ran.stderr.replace("`framesmith timeline`", "`framesmith info`"),
            told.stderr,
            "{extra:?}"
        );
    }
    // The option alone, where a path belongs; and no word at all.
    let ran = framesmith(&["timeline", "--bogus"]);
    assert_eq!(ran.code, 1, "{}", ran.all());
    assert!(
        ran.stderr.contains("`framesmith timeline` has no option --bogus."),
        "{}",
        ran.stderr
    );
    let same = framesmith(&["check", "--bogus"]);
    assert_eq!(ran.stderr.replace("`framesmith timeline`", "`framesmith check`"), same.stderr);
    let ran = framesmith(&["timeline"]);
    assert_eq!(ran.code, 1);
    assert!(
        ran.stderr.contains("`framesmith timeline` needs the path of a project folder."),
        "{}",
        ran.stderr
    );
    assert_eq!((tree(&dir), entries(&dir)), (files, all));
}

// ---------------------------------------------------------------------
// c6
// ---------------------------------------------------------------------

#[test]
fn s2_t008_c6_timeline_needs_no_browser_and_no_ffmpeg_starts_nothing_and_changes_nothing() {
    let dir = LENGTH_WORKED_OUT.make("s2_t008_c6");
    let root = dir.parent().unwrap();
    let [empty_home, empty_applications, empty_tmp, logs, tools] = ["home", "applications", "tmp", "logs", "tools"].map(|name| {
        let folder = root.join(name);
        std::fs::create_dir_all(&folder).unwrap();
        folder
    });
    let (spy_browser, browser_started) = spy(&tools, "browser-spy", &browser());
    let (_, ffmpeg_started) = spy(&tools, "ffmpeg", &ffmpeg());

    let project_before = tree(&dir);
    let entries_before = entries(&dir);
    let times_before = modified(&dir);
    let beside_before = entries(root);
    let usual = timeline(&dir);
    assert_eq!(usual.code, 0, "{}", usual.all());
    assert_eq!(usual.stdout, LENGTH_WORKED_OUT.timeline);

    // Nothing to be found: an empty search path, no named browser, an empty home folder and an empty
    // folder of applications.
    let bare = run_in(
        root,
        &["timeline", text(&dir)],
        &[
            ("PATH", OsStr::new("")),
            ("HOME", empty_home.as_os_str()),
            ("FRAMESMITH_BROWSER", OsStr::new("")),
            ("FRAMESMITH_DEBUG_APPLICATIONS", empty_applications.as_os_str()),
            ("TMPDIR", empty_tmp.as_os_str()),
            ("FRAMESMITH_LOG_DIR", logs.as_os_str()),
        ],
    );
    assert_eq!(bare.code, 0, "{}", bare.all());
    assert_eq!(bare.stdout, usual.stdout);
    assert!(bare.stderr.is_empty(), "{}", bare.stderr);
    // The same machine cannot render: so `timeline` really ran without them.
    let render = run_in(
        root,
        &["render", text(&dir)],
        &[
            ("PATH", OsStr::new("")),
            ("HOME", empty_home.as_os_str()),
            ("FRAMESMITH_BROWSER", OsStr::new("")),
            ("FRAMESMITH_DEBUG_APPLICATIONS", empty_applications.as_os_str()),
            ("TMPDIR", empty_tmp.as_os_str()),
            ("FRAMESMITH_LOG_DIR", logs.as_os_str()),
        ],
    );
    assert_eq!(render.code, 1, "{}", render.all());
    assert!(
        render.stderr.contains("FS-R001") && render.stderr.contains("FS-R003"),
        "{}",
        render.stderr
    );

    // With the stand-ins where a browser and `ffmpeg` would be found, neither is started.
    let path = format!("{}:{}", tools.display(), std::env::var("PATH").unwrap());
    let with_spies = run_in(
        root,
        &["timeline", text(&dir)],
        &[
            ("FRAMESMITH_BROWSER", spy_browser.as_os_str()),
            ("PATH", OsStr::new(&path)),
            ("TMPDIR", empty_tmp.as_os_str()),
            ("FRAMESMITH_LOG_DIR", logs.as_os_str()),
        ],
    );
    assert_eq!(with_spies.code, 0, "{}", with_spies.all());
    assert_eq!(with_spies.stdout, usual.stdout);
    assert!(!browser_started.exists() && !ffmpeg_started.exists(), "a program was started");
    // The control: the stand-ins do work, so a render with them starts both.
    let rendered = run_in(
        root,
        &["render", text(&dir), "--output", text(&root.join("control.mp4"))],
        &[("FRAMESMITH_BROWSER", spy_browser.as_os_str()), ("PATH", OsStr::new(&path))],
    );
    assert_eq!(rendered.code, 0, "{}", rendered.all());
    assert!(
        browser_started.exists() && ffmpeg_started.exists(),
        "the stand-ins were not used by render"
    );

    // Every file of the project is unchanged, and no file or folder (the output folder included) was added.
    assert_eq!(tree(&dir), project_before);
    assert_eq!(entries(&dir), entries_before);
    assert_eq!(modified(&dir), times_before);
    assert!(!dir.join("out").exists());
    assert_eq!(entries(&empty_tmp), Vec::<PathBuf>::new());
    // Beside the project only what this test put there (and the control video).
    let added: Vec<PathBuf> = entries(root).into_iter().filter(|p| !beside_before.contains(p)).collect();
    let allowed = |p: &PathBuf| {
        p.starts_with("home")
            || p.starts_with("logs")
            || p.starts_with("tools")
            || p == Path::new("control.mp4")
            || p == Path::new("browser-spy.started")
            || p == Path::new("tools/browser-spy.started")
            || p == Path::new("tools/ffmpeg.started")
    };
    assert!(added.iter().all(allowed), "{added:?}");
}

#[test]
fn s2_t008_c6_a_project_that_does_not_pass_changes_nothing_either() {
    let dir = project(
        "s2_t008_c6_faulty",
        "faulty",
        &page("data-fs-size=\"65x36\" data-fs-duration=\"1\"", "", "", ""),
    );
    write(&dir, "out/known.mp4", "an earlier video");
    let (files, all, times) = (tree(&dir), entries(&dir), modified(&dir));
    let ran = run_in(
        dir.parent().unwrap(),
        &["timeline", text(&dir)],
        &[("PATH", OsStr::new("")), ("FRAMESMITH_BROWSER", OsStr::new(""))],
    );
    assert_eq!(ran.code, 1, "{}", ran.all());
    assert_eq!((tree(&dir), entries(&dir), modified(&dir)), (files, all, times));
}

// ---------------------------------------------------------------------
// c7
// ---------------------------------------------------------------------

/// Runs `timeline` with the colour switches the usual tools honour set, and `NO_COLOR` taken out.
fn timeline_forcing_colour(dir: &Path) -> Ran {
    let output = Command::new(BIN)
        .args(["timeline", text(dir)])
        .current_dir(dir.parent().unwrap())
        .env("CLICOLOR_FORCE", "1")
        .env("FORCE_COLOR", "1")
        .env("TERM", "xterm-256color")
        .env_remove("NO_COLOR")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    Ran {
        code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

/// Bytes below 0x20 other than the line feed, and the delete character.
fn control_bytes(text: &str) -> Vec<u8> {
    text.bytes().filter(|&b| (b < 0x20 && b != b'\n') || b == 0x7f).collect()
}

#[test]
fn s2_t008_c7_the_output_has_no_escape_and_no_control_character_with_or_without_colour_switches() {
    let valid = LENGTH_WORKED_OUT.make("s2_t008_c7_valid");
    let faulty = project(
        "s2_t008_c7_faulty",
        "faulty",
        &page("data-fs-size=\"65x36\" data-fs-duration=\"1\"", "", "", ""),
    );
    let refused = fixture("s2_t008_c7_refused", "unsupported/fs-u001-video-element");
    for (dir, code) in [(&valid, 0), (&faulty, 1), (&refused, 2)] {
        for ran in [timeline(dir), timeline_forcing_colour(dir)] {
            assert_eq!(ran.code, code, "{}", ran.all());
            let all = format!("{}{}", ran.stdout, ran.stderr);
            assert!(!all.is_empty());
            assert_eq!(control_bytes(&all), Vec::<u8>::new(), "{all:?}");
            assert!(!all.contains('\u{1b}') && !all.contains('\r') && !all.contains('\t'), "{all:?}");
            assert!(all.is_ascii(), "{all:?}");
        }
    }
    // The colour switches change nothing at all.
    assert_eq!(timeline(&valid).stdout, timeline_forcing_colour(&valid).stdout);
    // The refusal of a word is plain too.
    let ran = framesmith(&["timeline", text(&valid), "move"]);
    assert_eq!(control_bytes(&ran.stderr), Vec::<u8>::new(), "{:?}", ran.stderr);
    assert!(ran.stderr.is_ascii());
}

#[test]
fn s2_t008_c7_every_line_is_a_header_or_a_clip_line_with_no_trailing_space() {
    let dir = DURATIONS_WORKED_OUT.make("s2_t008_c7_lines");
    let ran = timeline_forcing_colour(&dir);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert!(ran.stdout.ends_with('\n'));
    for line in ran.stdout.lines() {
        assert_eq!(line, line.trim_end(), "trailing space: {line:?}");
    }
    // It reads with the shape of the layout (the parser fails on any other shape).
    let (header, rows) = read_timeline(&ran.stdout);
    assert_eq!(header[0], "main");
    assert_eq!(rows.len(), 3);
}
