//! Sprint 2, ticket T012: a clip may follow a clip whose own start is relative, to any depth, in whatever
//! order the clips are written; changing the duration of the first clip moves every clip after it by the
//! same amount in `check`, `info`, `timeline` and in the render; starts that depend on each other in a
//! circle are one error (FS-E053) that names every clip of the circle, `check` exits 1 and `render`
//! refuses it and starts nothing; and the page works the chain out itself, to the same starts.
//!
//! Notes for maintainers
//! ---------------------
//! * The times are read from the text of the built binary and from the pictures of real renders; the
//!   expected values are written out here from the way each project is written, not taken from the code.
//! * The fixtures are `fixtures/relative-chain`, `relative-cycle-two` and `relative-cycle-three`; the
//!   other projects are written out in this file.
//! * Only the render tests (c1, c2 and the page tests at the end) need a browser and `ffmpeg`; the others
//!   read files only. The refusing tests use stand-ins that note being started.
//! * Scratch folders: every test names its projects with a scratch key of its own (the tests of a file run
//!   in parallel, and a key names a folder that is emptied when it is made).

mod common;

use common::*;
use measure::{Frame, Ran, Rgb};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

const RED: Rgb = [255, 0, 0];
const GREEN: Rgb = [0, 255, 0];
const BLUE: Rgb = [0, 0, 255];
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

/// The text after the first `error FS-E053` header line up to the next blank line, as printed.
fn circle_message(output: &str) -> String {
    let at = output.find("error FS-E053").expect("an FS-E053 error");
    let block = output[at..].split("\n\n").next().unwrap();
    block.lines().nth(1).unwrap().trim().to_string()
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
#c { left: 180px; background: #0000ff; }";

/// A 320 by 180 project at `fps` frames a second, `length` seconds long, with the given clips in the body.
fn boxes_for(test: &str, fps: u32, length: &str, clips: &str) -> PathBuf {
    let attrs = format!("data-fs-size=\"320x180\" data-fs-fps=\"{fps}\" data-fs-duration=\"{length}\"");
    project(test, "boxes", &page(&attrs, BOX_STYLE, clips, ""))
}

fn boxes(test: &str, fps: u32, clips: &str) -> PathBuf {
    boxes_for(test, fps, "8", clips)
}

/// A clip element that is a box of its own colour.
fn clip(id: &str, start: &str, duration: Option<&str>) -> String {
    let duration = duration.map_or(String::new(), |d| format!(" data-fs-duration=\"{d}\""));
    format!("<div id=\"{id}\" class=\"box\" data-fs-clip data-fs-start=\"{start}\"{duration}></div>\n")
}

/// The three clips of the chain of `relative-chain`: #a from 1 s for `a_duration`, #b after #a for 1.5 s,
/// #c after #b plus 0.5 s for 1 s.
fn chain_parts(a_duration: &str) -> Vec<String> {
    vec![
        clip("a", "1", Some(a_duration)),
        clip("b", "after:a", Some("1.5")),
        clip("c", "after:b+0.5", Some("1")),
    ]
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

/// The `clip #id  start N s (written)` lines of `check`'s output, by id.
fn start_lines(output: &str) -> BTreeMap<String, String> {
    output
        .lines()
        .filter_map(|line| line.trim_start().strip_prefix("clip #"))
        .map(|rest| {
            let (id, tail) = rest.split_once("  start ").unwrap();
            (id.to_string(), tail.to_string())
        })
        .collect()
}

fn permutations(items: &[usize]) -> Vec<Vec<usize>> {
    if items.len() <= 1 {
        return vec![items.to_vec()];
    }
    let mut all = Vec::new();
    for at in 0..items.len() {
        let mut rest = items.to_vec();
        let head = rest.remove(at);
        for mut tail in permutations(&rest) {
            tail.insert(0, head);
            all.push(tail);
        }
    }
    all
}

// ---------------------------------------------------------------------
// c1: the starts of a chain in check, info and timeline; one second longer moves both by one second
// ---------------------------------------------------------------------

#[test]
fn s2_t012_c1_check_prints_the_starts_of_the_clips_of_a_chain_and_info_and_timeline_agree() {
    let dir = fixture("s2_t012_c1_check", "relative-chain");
    let checked = check(&dir);
    assert_eq!(checked.code, 0, "{}", checked.all());
    // #a ends at 3 s, so #b starts at 3 s and ends at 4.5 s, so #c (+0.5) starts at 5 s.
    assert_eq!(
        checked.stdout,
        "check passed\n\
         \x20 composition  main\n\
         \x20 size         320x180\n\
         \x20 rate         30 fps\n\
         \x20 length       8 s\n\
         \x20 frames       240\n\
         \x20 clips        3\n\
         \x20 tweens       0\n\
         \x20 clip #b  start 3 s (after:a)\n\
         \x20 clip #c  start 5 s (after:b+0.5)\n"
    );
    let info = framesmith(&["info", text(&dir)]);
    assert_eq!(info.code, 0, "{}", info.all());
    assert!(
        info.stdout
            .ends_with("clip #b  start 3 s (after:a)\nclip #c  start 5 s (after:b+0.5)\n"),
        "{}",
        info.stdout
    );
    let timeline = framesmith(&["timeline", text(&dir)]);
    assert_eq!(timeline.code, 0, "{}", timeline.all());
    assert_eq!(
        timeline.stdout,
        "main  320x180  30 fps  8 s  240 frames\n\
         \x20 #a  1 s to 3 s    2 s\n\
         \x20 #b  3 s to 4.5 s  1.5 s\n\
         \x20 #c  5 s to 6 s    1 s\n"
    );
    let json = framesmith(&["timeline", text(&dir), "--json"]);
    let clips = document(&json)["clips"].clone();
    assert_eq!(
        (clips[0]["start"].clone(), clips[1]["start"].clone(), clips[2]["start"].clone()),
        (1.into(), 3.into(), 5.into())
    );
}

#[test]
fn s2_t012_c1_when_the_first_clip_lasts_one_second_longer_check_info_and_timeline_move_the_next_two_by_one_second() {
    let before = boxes("s2_t012_c1_longer_before", 30, &chain_parts("2").concat());
    let after = boxes("s2_t012_c1_longer_after", 30, &chain_parts("3").concat());
    let (checked_before, checked_after) = (check(&before), check(&after));
    assert_eq!(
        (checked_before.code, checked_after.code),
        (0, 0),
        "{}{}",
        checked_before.all(),
        checked_after.all()
    );
    let (was, is) = (start_lines(&checked_before.stdout), start_lines(&checked_after.stdout));
    assert_eq!(was["b"], "3 s (after:a)");
    assert_eq!(was["c"], "5 s (after:b+0.5)");
    assert_eq!(is["b"], "4 s (after:a)");
    assert_eq!(is["c"], "6 s (after:b+0.5)");
    // info prints the same lines.
    let info = framesmith(&["info", text(&after)]);
    assert!(
        info.stdout
            .ends_with("clip #b  start 4 s (after:a)\nclip #c  start 6 s (after:b+0.5)\n"),
        "{}",
        info.stdout
    );
    // timeline prints the start of every clip.
    let timeline = framesmith(&["timeline", text(&after)]);
    assert_eq!(timeline.code, 0, "{}", timeline.all());
    assert_eq!(
        timeline.stdout,
        "main  320x180  30 fps  8 s  240 frames\n\
         \x20 #a  1 s to 4 s    3 s\n\
         \x20 #b  4 s to 5.5 s  1.5 s\n\
         \x20 #c  6 s to 7 s    1 s\n"
    );
    let json_before = document(&framesmith(&["timeline", text(&before), "--json"]));
    let json_after = document(&framesmith(&["timeline", text(&after), "--json"]));
    for (index, name) in [(1usize, "#b"), (2, "#c")] {
        let (was, is) = (
            json_before["clips"][index]["start"].as_f64().unwrap(),
            json_after["clips"][index]["start"].as_f64().unwrap(),
        );
        assert_eq!(json_after["clips"][index]["name"], name);
        assert_eq!(is - was, 1.0, "{name}");
    }
}

#[test]
fn s2_t012_c1_the_render_shows_the_next_two_clips_on_their_new_frames_and_not_on_the_frame_before() {
    // First run: #a 1 s to 3 s (frames 30..90); #b from 3 s (frame 90) for 1.5 s; #c from 5 s (frame 150) for 1 s.
    let dir = boxes("s2_t012_c1_render_before", 30, &chain_parts("2").concat());
    let (ran, frames) = render_frames(&dir, &[], &[]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert_eq!(frames.len(), 240);
    assert_eq!(frames[89].count(GREEN, 0), 0, "#b is shown a frame early");
    assert_eq!(frames[90].count(GREEN, 0), BOX, "#b is not shown on its start frame");
    assert_eq!(frames[149].count(BLUE, 0), 0, "#c is shown a frame early");
    assert_eq!(frames[150].count(BLUE, 0), BOX, "#c is not shown on its start frame");
    assert_eq!(shown(&frames, RED), range(30, 90));
    assert_eq!(shown(&frames, GREEN), range(90, 135));
    assert_eq!(shown(&frames, BLUE), range(150, 180));

    // Second run: #a lasts one second longer, so #b and #c each start 30 frames later.
    let dir = boxes("s2_t012_c1_render_after", 30, &chain_parts("3").concat());
    let (ran, frames) = render_frames(&dir, &[], &[]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert_eq!(frames.len(), 240);
    assert_eq!(frames[119].count(GREEN, 0), 0, "#b is shown a frame early");
    assert_eq!(frames[120].count(GREEN, 0), BOX, "#b is not shown on its new start frame");
    assert_eq!(frames[179].count(BLUE, 0), 0, "#c is shown a frame early");
    assert_eq!(frames[180].count(BLUE, 0), BOX, "#c is not shown on its new start frame");
    assert_eq!(shown(&frames, RED), range(30, 120));
    assert_eq!(shown(&frames, GREEN), range(120, 165));
    assert_eq!(shown(&frames, BLUE), range(180, 210));
}

#[test]
fn s2_t012_c1_the_fixture_renders_at_another_rate_with_the_same_starts_in_seconds() {
    // 24 frames a second: #a 1 s..3 s is frames 24..72; #b from 3 s (frame 72) for 1.5 s (36 frames);
    // #c from 5 s (frame 120) for 1 s (24 frames).
    let dir = fixture("s2_t012_c1_render_24", "relative-chain");
    let (ran, frames) = render_frames(&dir, &["--fps", "24"], &[]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert_eq!(frames.len(), 192);
    assert_eq!(shown(&frames, RED), range(24, 72));
    assert_eq!(shown(&frames, GREEN), range(72, 108));
    assert_eq!(shown(&frames, BLUE), range(120, 144));
}

// ---------------------------------------------------------------------
// c2: the same chain whatever order it is written in
// ---------------------------------------------------------------------

#[test]
fn s2_t012_c2_all_six_file_orders_of_the_chain_give_the_same_check_output() {
    let mut printed: BTreeSet<String> = BTreeSet::new();
    for order in permutations(&[0, 1, 2]) {
        let parts = chain_parts("2");
        let body: String = order.iter().map(|&which| parts[which].as_str()).collect();
        let key: String = order.iter().map(|n| n.to_string()).collect();
        let dir = boxes(&format!("s2_t012_c2_order_{key}"), 30, &body);
        let ran = check(&dir);
        assert_eq!(ran.code, 0, "{order:?}: {}", ran.all());
        // The lines for #b and #c are the same whatever the order; `check` prints them in file order, so
        // they are compared by clip.
        let lines = start_lines(&ran.stdout);
        assert_eq!(lines["b"], "3 s (after:a)", "{order:?}");
        assert_eq!(lines["c"], "5 s (after:b+0.5)", "{order:?}");
        assert_eq!(lines.len(), 2, "{order:?}");
        let header: String = ran.stdout.lines().take(8).collect::<Vec<_>>().join("\n");
        printed.insert(header);
        let timeline = framesmith(&["timeline", text(&dir), "--json"]);
        let json = document(&timeline);
        let by_name: BTreeMap<String, f64> = json["clips"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| (item["name"].as_str().unwrap().to_string(), item["start"].as_f64().unwrap()))
            .collect();
        assert_eq!(
            by_name,
            BTreeMap::from([("#a".into(), 1.0), ("#b".into(), 3.0), ("#c".into(), 5.0)]),
            "{order:?}"
        );
    }
    assert_eq!(printed.len(), 1, "the facts differ between orders: {printed:?}");
}

#[test]
fn s2_t012_c2_the_render_shows_the_same_frames_when_the_clips_are_written_in_reverse_or_with_the_middle_clip_first() {
    for (key, order) in [("reverse", [2usize, 1, 0]), ("middle_first", [1, 0, 2])] {
        let parts = chain_parts("2");
        let body: String = order.iter().map(|&which| parts[which].as_str()).collect();
        let dir = boxes(&format!("s2_t012_c2_render_{key}"), 30, &body);
        let (ran, frames) = render_frames(&dir, &[], &[]);
        assert_eq!(ran.code, 0, "{key}: {}", ran.all());
        assert_eq!(frames.len(), 240, "{key}");
        assert_eq!(shown(&frames, RED), range(30, 90), "{key}");
        assert_eq!(shown(&frames, GREEN), range(90, 135), "{key}");
        assert_eq!(shown(&frames, BLUE), range(150, 180), "{key}");
    }
}

#[test]
fn s2_t012_c2_a_chain_of_two_hundred_clips_resolves() {
    // #c0 starts at 0 and lasts 1 s; each next clip follows the one before; written in reverse order.
    let count = 200usize;
    let mut body = String::new();
    for n in (0..count).rev() {
        let start = if n == 0 { "0".to_string() } else { format!("after:c{}", n - 1) };
        body.push_str(&clip(&format!("c{n}"), &start, Some("1")));
    }
    let dir = boxes_for("s2_t012_c2_long", 30, "201", &body);
    let ran = check(&dir);
    assert_eq!(ran.code, 0, "{}", ran.all());
    let lines = start_lines(&ran.stdout);
    assert_eq!(lines.len(), count - 1);
    assert_eq!(lines["c1"], "1 s (after:c0)");
    assert_eq!(lines["c100"], "100 s (after:c99)");
    assert_eq!(lines["c199"], "199 s (after:c198)");
    assert!(ran.stdout.contains("  clips        200\n"), "{}", ran.stdout);
}

// ---------------------------------------------------------------------
// c3: a cycle of two
// ---------------------------------------------------------------------

#[test]
fn s2_t012_c3_two_clips_that_follow_each_other_are_one_error_with_its_own_code_naming_both_and_check_exits_1() {
    let dir = fixture("s2_t012_c3", "relative-cycle-two");
    let ran = check(&dir);
    assert_eq!(ran.code, 1, "{}", ran.all());
    let line = lines_with("relative-cycle-two", "id=\"first\"");
    assert_eq!(line.len(), 1);
    assert_eq!(
        ran.stdout,
        format!(
            "error FS-E053 index.html:{}\n  \
             The starts of #first and #second depend on each other in a circle: #first starts after #second and #second starts after #first, so none of them can be worked out.\n  \
             fix: Break the circle: write the start of one of the clips of the circle as a time in seconds, or make it start after a clip that is not in the circle.\n\
             \n\
             check failed: 1 error, 0 unsupported, 0 unknown, 0 warnings\n",
            line[0]
        )
    );
    assert_eq!(listed(&ran.stdout).len(), 1);
    assert_eq!(listed(&ran.stdout)[0].kind, "error");
    assert!(!ran.stdout.contains("check passed"));
    // info and timeline refuse the project with the same list.
    for command in ["info", "timeline"] {
        let other = framesmith(&[command, text(&dir)]);
        assert_eq!(other.code, 1, "{command}: {}", other.all());
        assert_eq!(other.stdout, ran.stdout, "{command}");
    }
}

#[test]
fn s2_t012_c3_the_circle_of_two_is_one_error_whatever_the_offsets_and_the_order() {
    for (key, body) in [
        (
            "plain",
            format!("{}{}", clip("a", "after:b", Some("1")), clip("b", "after:a", Some("1"))),
        ),
        (
            "reverse",
            format!("{}{}", clip("b", "after:a", Some("1")), clip("a", "after:b", Some("1"))),
        ),
        (
            "offsets",
            format!("{}{}", clip("a", "after:b+1", Some("1")), clip("b", "after:a - 2", Some("1"))),
        ),
    ] {
        let ran = check(&boxes(&format!("s2_t012_c3_{key}"), 30, &body));
        assert_eq!(ran.code, 1, "{key}: {}", ran.all());
        assert_eq!(codes(&ran.stdout), ["FS-E053"], "{key}");
        let said = circle_message(&ran.stdout);
        assert!(said.contains("#a") && said.contains("#b"), "{key}: {said}");
    }
}

// ---------------------------------------------------------------------
// c4: a cycle of three
// ---------------------------------------------------------------------

#[test]
fn s2_t012_c4_three_clips_in_a_circle_are_one_error_with_the_same_code_naming_all_three() {
    let dir = fixture("s2_t012_c4", "relative-cycle-three");
    let ran = check(&dir);
    assert_eq!(ran.code, 1, "{}", ran.all());
    let line = lines_with("relative-cycle-three", "id=\"one\"");
    assert_eq!(line.len(), 1);
    assert_eq!(
        ran.stdout,
        format!(
            "error FS-E053 index.html:{}\n  \
             The starts of #one, #three and #two depend on each other in a circle: #one starts after #three, #three starts after #two and #two starts after #one, so none of them can be worked out.\n  \
             fix: Break the circle: write the start of one of the clips of the circle as a time in seconds, or make it start after a clip that is not in the circle.\n\
             \n\
             check failed: 1 error, 0 unsupported, 0 unknown, 0 warnings\n",
            line[0]
        )
    );
    assert_eq!(codes(&ran.stdout), ["FS-E053"]);
}

#[test]
fn s2_t012_c4_the_three_clip_circle_in_each_of_the_six_file_orders_is_one_error_naming_all_three() {
    let written = [("one", "after:three"), ("two", "after:one"), ("three", "after:two")];
    let mut messages: BTreeSet<String> = BTreeSet::new();
    for order in permutations(&[0, 1, 2]) {
        let body: String = order
            .iter()
            .map(|&which| clip(written[which].0, written[which].1, Some("1")))
            .collect();
        let key: String = order.iter().map(|n| n.to_string()).collect();
        let ran = check(&boxes(&format!("s2_t012_c4_order_{key}"), 30, &body));
        assert_eq!(ran.code, 1, "{order:?}: {}", ran.all());
        assert_eq!(codes(&ran.stdout), ["FS-E053"], "{order:?}");
        let said = circle_message(&ran.stdout);
        for (id, _) in written {
            assert!(said.contains(&format!("#{id}")), "{order:?}: {said}");
        }
        // The message begins with the clip written first, and the error is on that clip's line (found
        // in the file the test wrote).
        assert!(
            said.starts_with(&format!("The starts of #{}", written[order[0]].0)),
            "{order:?}: {said}"
        );
        let header = &listed(&ran.stdout)[0];
        let first_clip_line = std::fs::read_to_string(
            Path::new(env!("CARGO_TARGET_TMPDIR"))
                .join("cli")
                .join(format!("s2_t012_c4_order_{key}"))
                .join("boxes")
                .join("index.html"),
        )
        .unwrap()
        .lines()
        .position(|line| line.contains(&format!("id=\"{}\"", written[order[0]].0)))
        .unwrap() as u32
            + 1;
        assert_eq!(header.line, first_clip_line, "{order:?}");
        messages.insert(said);
    }
    // Three different clips can be written first, and each gives its own text, once.
    assert_eq!(messages.len(), 3);
}

// ---------------------------------------------------------------------
// The decisions
// ---------------------------------------------------------------------

#[test]
fn s2_t012_two_circles_in_one_composition_are_two_errors_a_follower_of_a_circle_has_none_and_exit_is_1() {
    let body = format!(
        "{}{}{}{}{}{}",
        clip("a", "after:b", Some("1")),
        clip("b", "after:a", Some("1")),
        clip("tail", "after:a+1", Some("1")),
        clip("c", "after:b", Some("1")), // follows the first circle but is not in it
        clip("d", "after:e", Some("1")),
        clip("e", "after:d", Some("1")),
    );
    let ran = check(&boxes("s2_t012_decisions_two", 30, &body));
    assert_eq!(ran.code, 1, "{}", ran.all());
    assert_eq!(codes(&ran.stdout), ["FS-E053", "FS-E053"], "{}", ran.stdout);
    assert!(
        ran.stdout.contains("The starts of #a and #b depend on each other"),
        "{}",
        ran.stdout
    );
    assert!(
        ran.stdout.contains("The starts of #d and #e depend on each other"),
        "{}",
        ran.stdout
    );
    assert!(!ran.stdout.contains("#tail"), "{}", ran.stdout);
    assert!(
        ran.stdout.contains("check failed: 2 errors, 0 unsupported, 0 unknown, 0 warnings"),
        "{}",
        ran.stdout
    );
}

#[test]
fn s2_t012_a_member_whose_offset_is_not_a_time_lists_both_faults_and_a_member_with_no_written_duration_only_the_circle() {
    let body = format!("{}{}", clip("a", "after:b+abc", Some("1")), clip("b", "after:a", Some("1")));
    let ran = check(&boxes("s2_t012_decisions_offset", 30, &body));
    assert_eq!(ran.code, 1, "{}", ran.all());
    assert_eq!(codes(&ran.stdout), ["FS-E051", "FS-E053"], "{}", ran.stdout);
    let unwritten = "<div id=\"a\" class=\"box\" data-fs-clip data-fs-start=\"after:b\"></div>\n".to_string();
    let body = format!("{unwritten}{}", clip("b", "after:a", Some("1")));
    let ran = check(&boxes("s2_t012_decisions_unwritten", 30, &body));
    assert_eq!(ran.code, 1, "{}", ran.all());
    assert_eq!(codes(&ran.stdout), ["FS-E053"], "{}", ran.stdout);
}

#[test]
fn s2_t012_a_clip_that_names_itself_is_still_fs_e049_and_a_follower_of_a_clip_with_another_fault_is_not_listed() {
    let ran = check(&boxes("s2_t012_decisions_self", 30, &clip("a", "after:a", Some("1"))));
    assert_eq!((ran.code, codes(&ran.stdout)), (1, vec!["FS-E049".to_string()]));
    for (key, written, code) in [
        ("missing", "after:nothing", "FS-E050"),
        ("offset", "after:z+abc", "FS-E051"),
        ("before_zero", "after:z-9", "FS-E052"),
    ] {
        let body = format!(
            "{}{}{}",
            clip("z", "1", Some("1")),
            clip("b", written, Some("1")),
            clip("c", "after:b", Some("1"))
        );
        let ran = check(&boxes(&format!("s2_t012_decisions_{key}"), 30, &body));
        assert_eq!(ran.code, 1, "{key}: {}", ran.all());
        assert_eq!(codes(&ran.stdout), [code], "{key}: {}", ran.stdout);
    }
}

// ---------------------------------------------------------------------
// c5: render refuses a cycle and starts nothing
// ---------------------------------------------------------------------

#[test]
fn s2_t012_c5_render_prints_the_list_check_printed_exits_1_and_starts_no_browser_and_no_ffmpeg() {
    for (key, folder) in [("two", "relative-cycle-two"), ("three", "relative-cycle-three")] {
        let dir = fixture(&format!("s2_t012_c5_{key}"), folder);
        let checked = check(&dir);
        assert_eq!(checked.code, 1, "{key}: {}", checked.all());
        let spies = Spies::new(&format!("s2_t012_c5_{key}"));
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
        assert_eq!(from_json.len(), 1, "{key}");
        assert_eq!(from_json[0].0, "FS-E053", "{key}");
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
fn s2_t012_c5_the_stand_ins_do_see_a_browser_and_ffmpeg_when_the_chain_renders() {
    let spies = Spies::new("s2_t012_c5_control");
    let dir = fixture("s2_t012_c5_control_project", "relative-chain");
    let ran = spies.run(dir.parent().unwrap(), &["render", text(&dir)]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert_eq!(spies.started(), (true, true));
}

// ---------------------------------------------------------------------
// The new code, and FS-U003 after this ticket
// ---------------------------------------------------------------------

#[test]
fn s2_t012_the_new_code_is_listed_once_follows_the_last_error_code_and_is_given_to_nothing_else() {
    let matrix = spec("support-matrix.md");
    let rows_of = |code: &str| matrix.lines().filter(|line| line.starts_with(&format!("| {code} |"))).count();
    assert_eq!(rows_of("FS-E053"), 1);
    let errors: Vec<String> = matrix
        .lines()
        .filter_map(|line| line.strip_prefix("| FS-E"))
        .map(|rest| format!("FS-E{}", rest.split(' ').next().unwrap()))
        .collect();
    // (ticket T013 added FS-E054 after it)
    assert_eq!(errors.get(errors.len().saturating_sub(2)).map(String::as_str), Some("FS-E053"));
    let row = matrix.lines().find(|line| line.starts_with("| FS-E053 |")).unwrap();
    assert!(row.contains("circle") && row.contains("every clip"), "{row}");
    // The number is written in one place of the product, the resolver of the model, and the spec of the
    // composition gives its message.
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
                } else if path.extension().is_some_and(|e| e == "rs") && std::fs::read_to_string(&path).unwrap().contains("\"FS-E053\"") {
                    files.push(path.strip_prefix(&sources).unwrap().to_path_buf());
                }
            }
        }
    }
    assert_eq!(files, [PathBuf::from("model/src/resolve.rs")]);
    assert!(spec("composition.md").contains("| FS-E053 |"));
}

#[test]
fn s2_t012_fs_u003_is_no_longer_raised_for_a_chain_and_the_form_that_waits_is_still_refused_with_it() {
    // A chain of any of the shapes: supported, nothing refused.
    for (key, body) in [
        (
            "plain",
            format!(
                "{}{}{}",
                clip("a", "1", Some("2")),
                clip("b", "after:a", Some("1")),
                clip("c", "after:b", Some("1"))
            ),
        ),
        (
            "offsets",
            format!(
                "{}{}{}",
                clip("a", "1", Some("2")),
                clip("b", "after:a+1", Some("1")),
                clip("c", "after:b-0.5", Some("1"))
            ),
        ),
        (
            "reverse",
            format!(
                "{}{}{}",
                clip("c", "after:b", Some("1")),
                clip("b", "after:a", Some("1")),
                clip("a", "1", Some("2"))
            ),
        ),
    ] {
        let ran = check(&boxes(&format!("s2_t012_u003_{key}"), 30, &body));
        assert_eq!(ran.code, 0, "{key}: {}", ran.all());
        assert!(!ran.all().contains("FS-U003"), "{key}");
    }
    // A named clip with no written duration, also when it is a link of a chain, is refused no more
    // (ticket T013); a nested composition as the named clip still is (exit 2).
    let nested = "<div id=\"inner\" data-fs-composition=\"inner\" data-fs-size=\"64x36\"></div>\n";
    let cases: Vec<(&str, String)> = vec![
        ("nested", format!("{nested}{}", clip("b", "after:inner", Some("1")))),
        (
            "nested_link",
            format!("{nested}{}{}", clip("b", "after:inner", Some("1")), clip("c", "after:b", Some("1"))),
        ),
    ];
    for (key, body) in cases {
        let dir = boxes(&format!("s2_t012_u003_waits_{key}"), 30, &body);
        let ran = check(&dir);
        assert_eq!(ran.code, 2, "{key}: {}", ran.all());
        assert_eq!(codes(&ran.stdout), ["FS-U006", "FS-U003"], "{key}");
        assert!(ran.stdout.contains("a nested composition"), "{key}: {}", ran.stdout);
        assert!(!ran.stdout.contains("chains and cycles"), "{key}: {}", ran.stdout);
        let spies = Spies::new(&format!("s2_t012_u003_waits_{key}"));
        let render = spies.run(dir.parent().unwrap(), &["render", text(&dir)]);
        assert_eq!(render.code, 2, "{key}: {}", render.all());
        assert_eq!(spies.started(), (false, false), "{key}");
    }
    // The fixture of the row is a follower of a nested composition.
    let ran = check(&fixture("s2_t012_u003_fixture", "unsupported/fs-u003-nested-composition-start"));
    assert_eq!(ran.code, 2, "{}", ran.all());
    assert_eq!(codes(&ran.stdout), ["FS-U006", "FS-U003"]);
    assert!(ran.stdout.contains("a nested composition"), "{}", ran.stdout);
    assert!(!ran.stdout.contains("whose own start is also relative"), "{}", ran.stdout);
}

#[test]
fn s2_t012_the_message_for_a_chain_is_gone_from_the_product_and_the_matrix_row_waits_only_for_the_ticket_left() {
    let matrix = spec("support-matrix.md");
    let row = matrix.lines().find(|line| line.starts_with("| FS-U003 |")).unwrap();
    let cells: Vec<&str> = row.trim().trim_matches('|').split('|').map(str::trim).collect();
    assert!(
        cells[1].contains("chains of them") && cells[1].contains("supported"),
        "{}",
        cells[1]
    );
    assert!(
        !cells[1].contains("naming a clip whose own start is relative"),
        "the row still refuses a chain: {}",
        cells[1]
    );
    assert!(
        !cells[3].contains("T013") && cells[3].contains("T038") && !cells[3].contains("T012"),
        "{}",
        cells[3]
    );
    for source in std::fs::read_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("..")).unwrap() {
        let src = source.unwrap().path().join("src").join("resolve.rs");
        if src.is_file() {
            assert!(!std::fs::read_to_string(&src)
                .unwrap()
                .contains("chains and cycles of relative starts"));
        }
    }
    // The composition spec lists one form that waits, no more (ticket T013 built the other).
    let composition = spec("composition.md");
    let waits: Vec<&str> = composition
        .lines()
        .skip_while(|line| !line.starts_with("**Forms that wait.**"))
        .skip(1)
        .take_while(|line| line.starts_with("- "))
        .collect();
    assert_eq!(waits.len(), 1, "{waits:?}");
    assert!(waits[0].contains("T038") && !waits[0].contains("T013"), "{waits:?}");
}

// ---------------------------------------------------------------------
// The page resolves the same chain
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
fn s2_t012_page_the_page_resolves_each_link_of_the_chain_to_the_same_start_as_the_files() {
    // The render compares the page's manifest with the one read from the files before any frame is
    // taken and stops when they differ (page-protocol.md). It passes, and the job it ran holds each
    // link's resolved start and the form it was written in. The clips are written in reverse order, so
    // the page works the chain out from the last clip to the first.
    let parts = chain_parts("2");
    let body = format!("{}{}{}", parts[2], parts[1], parts[0]);
    let dir = boxes("s2_t012_page_agree", 30, &body);
    let job_file = dir.parent().unwrap().join("job.json");
    let ran = run_in(
        dir.parent().unwrap(),
        &["render", text(&dir)],
        &[("FRAMESMITH_DEBUG_JOB", job_file.as_os_str())],
    );
    assert_eq!(ran.code, 0, "{}", ran.all());
    let job: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&job_file).unwrap()).unwrap();
    let clips = &job["manifest"]["clips"];
    assert_eq!(
        (clips[0]["start"].clone(), clips[0]["relative_start"].clone()),
        ("5/1".into(), "after:b+0.5".into())
    );
    assert_eq!(
        (clips[1]["start"].clone(), clips[1]["relative_start"].clone()),
        ("3/1".into(), "after:a".into())
    );
    assert_eq!(
        (clips[2]["start"].clone(), clips[2]["relative_start"].clone()),
        ("1/1".into(), serde_json::Value::Null)
    );
    assert!(
        ran.stdout.contains("probe     done      320x180, 30 fps, 8 s, 240 frames\n"),
        "{}",
        ran.stdout
    );
}

#[test]
fn s2_t012_page_the_page_works_the_chain_out_itself_and_a_difference_further_down_the_chain_is_named_with_both_values() {
    // #c is written first. A script lengthens #a by one second before the page reports ready: the page's
    // own resolution puts #b one second and #c one second later than the files do, and the first
    // difference in file order is the start of #c.
    let parts = chain_parts("2");
    let clips = format!("{}{}{}", parts[2], parts[1], parts[0]);
    let said = page_disagrees(
        "s2_t012_page_longer",
        &clips,
        "document.getElementById(\"a\").setAttribute(\"data-fs-duration\", \"3\");",
    );
    assert_eq!(
        said,
        "The page disagrees with the files: the start of clip 1 (#c) is 5 s in the files and 6 s in the page."
    );
    // A script that makes the chain a circle: the page cannot resolve its composition and the run stops
    // with a disagreement rather than painting a wrong video.
    let said = page_disagrees(
        "s2_t012_page_circle",
        &clips,
        "document.getElementById(\"a\").setAttribute(\"data-fs-start\", \"after:c\");",
    );
    assert_eq!(
        said,
        "The page disagrees with the files: the page could not resolve its composition: The starts of #c, #b and #a depend on each other in a circle: #c starts after #b, #b starts after #a and #a starts after #c, so none of them can be worked out. (FS-E053)."
    );
}
