//! Sprint 2, ticket T010: a clip whose start is written `after:<clip id>` starts at the end of the named
//! clip. `check` prints the resolved start beside the written form, the render shows the clip from the
//! frame of that start, the named clip is found wherever it stands, the two faults of a single reference
//! (a missing clip, the clip itself) are errors with codes of their own, several faults are all listed
//! and block `render` before it starts anything, the refusal FS-U003 stays only for the forms that wait,
//! and a project of absolute starts is checked exactly as before.
//!
//! Notes for maintainers
//! ---------------------
//! * The times are read from the text of the built binary and from the pictures of real renders; the
//!   expected values are written out here from the way each project is written, not taken from the code.
//! * The fixtures are `fixtures/relative-start*`; the forms that wait are written out in this file as small
//!   projects, one per form (the form that waits has a fixture of its own, `unsupported/fs-u003-nested-composition-start`;
//!   the fixture was an offset until ticket T011 built the offset, a chain until ticket T012 built
//!   chains, and a clip with no written duration until ticket T013 built it; the tests that expected them to be refused were replaced then by `s2_t011_relative_offset.rs`
//!   and `s2_t012_relative_chain.rs`).
//! * Only the render tests (c1, and the page tests at the end) need a browser and `ffmpeg`; the others
//!   read files only. The refusing tests use stand-ins that note being started.
//! * Scratch folders: every test names its projects with a scratch key of its own (the tests of a file run
//!   in parallel, and a key names a folder that is emptied when it is made).
//! * c7 keeps the text `check` printed for the Sprint 1 fixtures before ticket T010 changed the model
//!   (taken from the binary built from the Sprint 1 and earlier Sprint 2 code); the frames of those
//!   fixtures are guarded by `s2_t003_sprint1_frames.rs`.

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

/// A 320 by 180 project at `fps` frames a second, 5 s long, with the given clips in the body.
fn boxes(test: &str, fps: u32, clips: &str) -> PathBuf {
    let attrs = format!("data-fs-size=\"320x180\" data-fs-fps=\"{fps}\" data-fs-duration=\"5\"");
    project(test, "boxes", &page(&attrs, BOX_STYLE, clips, ""))
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
fn s2_t010_c1_check_prints_three_seconds_beside_the_written_form_and_info_and_timeline_agree() {
    let dir = fixture("s2_t010_c1_check", "relative-start");
    let checked = check(&dir);
    assert_eq!(checked.code, 0, "{}", checked.all());
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
         \x20 clip #outro  start 3 s (after:intro)\n\
         \x20 clip #echo  start 1 s (after:last)\n"
    );
    // `info` prints the same lines (without the indent) from the same function.
    let info = framesmith(&["info", text(&dir)]);
    assert_eq!(info.code, 0, "{}", info.all());
    assert!(
        info.stdout
            .ends_with("clip #outro  start 3 s (after:intro)\nclip #echo  start 1 s (after:last)\n"),
        "{}",
        info.stdout
    );
    // `timeline` prints the resolved starts in its table.
    let timeline = framesmith(&["timeline", text(&dir)]);
    assert_eq!(timeline.code, 0, "{}", timeline.all());
    assert_eq!(
        timeline.stdout,
        "main  320x180  30 fps  5 s  150 frames\n\
        lane 0\n\
         \x20 #intro  1 s   to 3 s  2 s (lane filled in)\n\
         \x20 #outro  3 s   to 4 s  1 s (lane filled in)\n\
         \x20 #echo   1 s   to 2 s  1 s (lane filled in)\n\
         \x20 #last   0.5 s to 1 s  0.5 s (lane filled in)\n"
    );
    let json = framesmith(&["timeline", text(&dir), "--json"]);
    let clips = document(&json)["clips"].clone();
    assert_eq!(clips[1]["name"], "#outro");
    assert_eq!(clips[1]["start"], 3);
}

#[test]
fn s2_t010_c1_the_render_hides_the_follower_before_3_seconds_and_shows_it_on_the_frame_of_3_seconds() {
    let dir = fixture("s2_t010_c1_render", "relative-start");
    let (ran, frames) = render_frames(&dir, &[], &[]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert_eq!(frames.len(), 150);
    // 30 frames a second: 3 s is the frame time of frame 90.
    assert_eq!(frames[89].count(GREEN, 0), 0, "the follower is shown on the last frame before 3 s");
    assert_eq!(frames[90].count(GREEN, 0), BOX, "the follower is not shown on the frame of 3 s");
    // The named clip is gone on that frame: it ends where the follower starts.
    assert_eq!(frames[89].count(RED, 0), BOX);
    assert_eq!(frames[90].count(RED, 0), 0);
    // Whole runs: #intro 1 s to 3 s, #outro 3 s to 4 s, #last 0.5 s to 1 s, and #echo, written before
    // the clip it follows, from the end of #last (1 s) for 1 s.
    assert_eq!(shown(&frames, RED), range(30, 90));
    assert_eq!(shown(&frames, GREEN), range(90, 120));
    assert_eq!(shown(&frames, YELLOW), range(15, 30));
    assert_eq!(shown(&frames, BLUE), range(30, 60));
}

#[test]
fn s2_t010_c1_the_render_follows_the_end_of_the_named_clip_at_another_rate() {
    // 24 frames a second: 3 s is the frame time of frame 72, and 1 s of frame 24.
    let dir = fixture("s2_t010_c1_render_24", "relative-start");
    let (ran, frames) = render_frames(&dir, &["--fps", "24"], &[]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert_eq!(frames.len(), 120);
    assert_eq!(frames[71].count(GREEN, 0), 0);
    assert_eq!(frames[72].count(GREEN, 0), BOX);
    assert_eq!(shown(&frames, RED), range(24, 72));
    assert_eq!(shown(&frames, GREEN), range(72, 96));
    assert_eq!(shown(&frames, YELLOW), range(12, 24));
    assert_eq!(shown(&frames, BLUE), range(24, 48));
}

#[test]
fn s2_t010_c1_the_render_is_right_where_the_end_of_the_named_clip_falls_between_frame_times() {
    // #a starts at 1.03 s and lasts 2 s, so it ends at 3.03 s and #b starts there. At 30 frames a
    // second that is 90.9 frames: #a shows on frames 31 to 90 and #b first on frame 91. At 24 it is
    // 72.72 frames: #a on frames 25 to 72, #b first on frame 73.
    let clips = format!("{}{}", clip("a", "1.03", Some("2")), clip("b", "after:a", Some("1")));
    for (fps, a_first, b_first, b_end) in [(30, 31, 91, 121), (24, 25, 73, 97)] {
        let dir = boxes(&format!("s2_t010_c1_between_{fps}"), fps, &clips);
        let checked = check(&dir);
        assert!(checked.stdout.contains("  clip #b  start 3.03 s (after:a)\n"), "{}", checked.all());
        let (ran, frames) = render_frames(&dir, &[], &[]);
        assert_eq!(ran.code, 0, "{}", ran.all());
        assert_eq!(frames.len(), 5 * fps as usize);
        assert_eq!(frames[b_first - 1].count(GREEN, 0), 0, "{fps} fps: shown a frame early");
        assert_eq!(frames[b_first].count(GREEN, 0), BOX, "{fps} fps: not shown on its first frame");
        assert_eq!(shown(&frames, RED), range(a_first, b_first), "{fps} fps");
        assert_eq!(shown(&frames, GREEN), range(b_first, b_end), "{fps} fps");
    }
}

// ---------------------------------------------------------------------
// c2: the named clip is found among the clips of the composition
// ---------------------------------------------------------------------

/// What `timeline` printed for the clips of a project: the name and the start of each line.
fn starts(dir: &Path) -> Vec<(String, String)> {
    let ran = framesmith(&["timeline", text(dir)]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    // (ticket T014: a lane line stands over the clips of a lane)
    ran.stdout
        .lines()
        .skip(1)
        .filter(|line| !line.starts_with("lane "))
        .map(|line| {
            let cells: Vec<&str> = line.split_whitespace().collect();
            (cells[0].to_string(), cells[1].to_string())
        })
        .collect()
}

#[test]
fn s2_t010_c2_the_named_clip_is_found_whether_it_is_written_before_or_after() {
    let named = clip("a", "1", Some("2"));
    let follower = clip("b", "after:a", Some("1"));
    let after = boxes("s2_t010_c2_after", 30, &format!("{named}{follower}"));
    let before = boxes("s2_t010_c2_before", 30, &format!("{follower}{named}"));
    assert_eq!(
        starts(&after),
        [("#a".to_string(), "1".to_string()), ("#b".to_string(), "3".to_string())]
    );
    // The same project with the two clips written the other way round: the follower still starts at 3 s.
    assert_eq!(
        starts(&before),
        [("#b".to_string(), "3".to_string()), ("#a".to_string(), "1".to_string())]
    );
    for dir in [&after, &before] {
        let checked = check(dir);
        assert_eq!(checked.code, 0, "{}", checked.all());
        assert!(checked.stdout.contains("  clip #b  start 3 s (after:a)\n"), "{}", checked.stdout);
    }
}

#[test]
fn s2_t010_c2_the_named_clip_may_stand_inside_another_clip_and_the_id_is_read_as_written() {
    // #inner is a clip inside #outer; #b follows it by its id. The start is on the composition's time.
    let body = "<div id=\"outer\" class=\"box\" data-fs-clip data-fs-start=\"0\" data-fs-duration=\"4\">\n\
                <div id=\"inner\" class=\"box\" data-fs-clip data-fs-start=\"1\" data-fs-duration=\"1.5\"></div>\n\
                </div>\n\
                <div id=\"scene-2\" class=\"box\" data-fs-clip data-fs-start=\"2\" data-fs-duration=\"0.25\"></div>\n"
        .to_string();
    let follow_inner = clip("b", "after: inner ", Some("1"));
    let follow_hyphen = clip("c", "after:scene-2", Some("1"));
    let dir = boxes("s2_t010_c2_inner", 30, &format!("{body}{follow_inner}{follow_hyphen}"));
    let checked = check(&dir);
    assert_eq!(checked.code, 0, "{}", checked.all());
    // The written form is printed as written, with the spaces at its ends removed.
    assert!(
        checked.stdout.contains("  clip #b  start 2.5 s (after: inner)\n"),
        "{}",
        checked.stdout
    );
    assert!(
        checked.stdout.contains("  clip #c  start 2.25 s (after:scene-2)\n"),
        "{}",
        checked.stdout
    );
}

#[test]
fn s2_t010_c2_when_two_clips_have_the_same_id_the_first_in_the_document_is_named() {
    let body = format!(
        "{}{}{}",
        clip("a", "1", Some("1")),
        clip("a", "2", Some("2")),
        clip("b", "after:a", Some("1"))
    );
    let dir = boxes("s2_t010_c2_twice", 30, &body);
    let checked = check(&dir);
    assert_eq!(checked.code, 0, "{}", checked.all());
    assert!(checked.stdout.contains("  clip #b  start 2 s (after:a)\n"), "{}", checked.stdout);
}

#[test]
fn s2_t010_c2_an_id_that_belongs_to_no_clip_names_nothing() {
    // An element with the id that is not a clip, an id no element has, and the id in another letter case.
    let cases = [
        ("an_element_that_is_not_a_clip", "<p id=\"a\">text</p>\n", "after:a"),
        ("no_element_at_all", "", "after:a"),
        ("another_case", "", "after:A"),
    ];
    for (key, extra, written) in cases {
        let body = format!("{extra}{}{}", clip("x", "1", Some("1")), clip("b", written, Some("1")));
        let dir = boxes(&format!("s2_t010_c2_{key}"), 30, &body);
        let checked = check(&dir);
        assert_eq!(checked.code, 1, "{key}: {}", checked.all());
        assert_eq!(codes(&checked.stdout), ["FS-E050"], "{key}: {}", checked.all());
        assert!(!checked.stdout.contains("check passed"), "{key}");
    }
    // The clip with the id exists but is not marked as a clip: it is the same fault with the same words.
    let dir = boxes(
        "s2_t010_c2_marked_wrongly",
        30,
        &format!(
            "<div id=\"a\" class=\"box\" data-fs-start=\"1\"></div>\n{}",
            clip("b", "after:a", Some("1"))
        ),
    );
    let checked = check(&dir);
    assert!(codes(&checked.stdout).contains(&"FS-E050".to_string()), "{}", checked.all());
}

// ---------------------------------------------------------------------
// c3 and c4: the two faults
// ---------------------------------------------------------------------

#[test]
fn s2_t010_c3_a_missing_clip_is_an_error_with_its_own_code_the_file_the_line_a_sentence_and_a_remedy() {
    let dir = fixture("s2_t010_c3", "relative-start-missing-clip");
    let ran = check(&dir);
    assert_eq!(ran.code, 1, "{}", ran.all());
    let line = lines_with("relative-start-missing-clip", "after:opening");
    assert_eq!(line.len(), 1);
    assert_eq!(
        ran.stdout,
        format!(
            "error FS-E050 index.html:{}\n  \
             The clip #outro starts after \"opening\", but no clip with the id \"opening\" is in this composition; only an element marked data-fs-clip can be named.\n  \
             fix: Write the id of a clip of this composition, give the clip you mean data-fs-clip and that id, or write the start as a time in seconds.\n\
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
    // The remedy never speaks of a hyphen: a text that is no clip and ends in no sign and number is the id of no clip.
    let hyphen = boxes(
        "s2_t010_c3_hyphen",
        30,
        &format!("{}{}", clip("a", "1", Some("2")), clip("b", "after:lower-third", Some("1"))),
    );
    let ran = check(&hyphen);
    assert_eq!(ran.code, 1, "{}", ran.all());
    assert_eq!(codes(&ran.stdout), ["FS-E050"], "{}", ran.stdout);
    assert!(
        ran.stdout.contains("no clip with the id \"lower-third\" is in this composition"),
        "{}",
        ran.stdout
    );
    assert!(!ran.stdout.contains("joined"), "{}", ran.stdout);
}

/// The joined-minus rule: the text after `after:` that is exactly a clip id names that clip; otherwise a
/// sign and a number make it an offset, which ticket T011 built (read as the clip before the sign); when
/// no such clip exists either, it is an id no clip has (FS-E050), as T010 reported for every text without
/// a sign and a number. (Until T011 an offset was refused as FS-U003.)
#[test]
fn s2_t010_c3_a_text_with_a_hyphen_is_the_clip_when_the_clip_is_there_and_an_id_no_clip_has_when_it_is_not() {
    // (the text after `after:`, whether the absent message says that the offset reading was tried too)
    let cases = [("first-0.5", true), ("scene-2", true), ("lower-third", false)];
    for (id, tried_as_offset) in cases {
        let key = id.replace(['-', '.'], "_");
        let written = format!("after:{id}");

        // With a clip that has exactly this id: the plain form, resolved, nothing refused.
        let present = boxes(
            &format!("s2_t010_c3_joined_{key}_present"),
            30,
            &format!("{}{}", clip(id, "1", Some("2")), clip("b", &written, Some("1"))),
        );
        let ran = check(&present);
        assert_eq!(ran.code, 0, "{id} present: {}", ran.all());
        assert!(
            !ran.all().contains("FS-U003") && !ran.all().contains("FS-E050"),
            "{id}: {}",
            ran.all()
        );
        assert!(
            ran.stdout.contains(&format!("  clip #b  start 3 s ({written})\n")),
            "{id}: {}",
            ran.stdout
        );
        assert_eq!(
            starts(&present),
            [(format!("#{id}"), "1".to_string()), ("#b".to_string(), "3".to_string())]
        );

        // With no such clip: an id no clip has.
        let absent = boxes(
            &format!("s2_t010_c3_joined_{key}_absent"),
            30,
            &format!("{}{}", clip("x", "1", Some("2")), clip("b", &written, Some("1"))),
        );
        let ran = check(&absent);
        assert_eq!(ran.code, 1, "{id} absent: {}", ran.all());
        assert_eq!(codes(&ran.stdout), ["FS-E050"], "{id} absent: {}", ran.stdout);
        assert!(
            ran.stdout.contains(&format!("no clip with the id \"{id}\"")),
            "{id}: {}",
            ran.stdout
        );
        assert_eq!(ran.stdout.contains("read as the clip"), tried_as_offset, "{id}: {}", ran.stdout);
        // `render` refuses it the same way and starts nothing.
        let spies = Spies::new(&format!("s2_t010_c3_joined_{key}"));
        let render = spies.run(absent.parent().unwrap(), &["render", text(&absent)]);
        assert_eq!(render.code, 1, "{id}: {}", render.all());
        assert_eq!(spies.started(), (false, false), "{id}");
    }
}

#[test]
fn s2_t010_c4_a_clip_that_names_itself_is_an_error_with_a_code_different_from_the_missing_clip() {
    let dir = fixture("s2_t010_c4", "relative-start-names-itself");
    let ran = check(&dir);
    assert_eq!(ran.code, 1, "{}", ran.all());
    let line = lines_with("relative-start-names-itself", "after:outro");
    assert_eq!(line.len(), 1);
    assert_eq!(
        ran.stdout,
        format!(
            "error FS-E049 index.html:{}\n  \
             The clip #outro starts after itself (\"after:outro\"), so its start cannot be worked out.\n  \
             fix: Name the clip it should follow, for example data-fs-start=\"after:intro\", or write the start as a time in seconds.\n\
             \n\
             check failed: 1 error, 0 unsupported, 0 unknown, 0 warnings\n",
            line[0]
        )
    );
    let missing = check(&fixture("s2_t010_c4_missing", "relative-start-missing-clip"));
    let (itself, absent) = (codes(&ran.stdout), codes(&missing.stdout));
    assert_eq!(
        (itself.as_slice(), absent.as_slice()),
        (&["FS-E049".to_string()][..], &["FS-E050".to_string()][..])
    );
    // A clip with no id of its own cannot name itself; it is named by its position, as other messages do.
}

#[test]
fn s2_t010_c3_c4_the_two_codes_are_new_and_each_is_listed_once_in_the_support_matrix() {
    let matrix = spec("support-matrix.md");
    let rows_of = |code: &str| matrix.lines().filter(|line| line.starts_with(&format!("| {code} |"))).count();
    assert_eq!((rows_of("FS-E049"), rows_of("FS-E050")), (1, 1));
    // Every code listed in the matrix before: none is either of these.
    let before: BTreeSet<String> = matrix
        .lines()
        .filter_map(|line| line.strip_prefix("| FS-"))
        .map(|rest| format!("FS-{}", rest.split(' ').next().unwrap()))
        .filter(|code| !["FS-E049", "FS-E050", "FS-E051", "FS-E052", "FS-E053", "FS-E054", "FS-E055"].contains(&code.as_str()))
        .collect();
    assert!(before.contains("FS-E048") && before.contains("FS-U003"));
    assert!(!before.contains("FS-E049") && !before.contains("FS-E050"));
    // The highest code before them was FS-E048, in the same family.
    let errors: Vec<&String> = before.iter().filter(|code| code.starts_with("FS-E")).collect();
    assert_eq!(errors.last().map(|code| code.as_str()), Some("FS-E048"));
}

// ---------------------------------------------------------------------
// c5: several faults, listed together; render prints the same and starts nothing
// ---------------------------------------------------------------------

#[test]
fn s2_t010_c5_several_timing_faults_are_all_listed_and_render_prints_the_same_list_and_starts_nothing() {
    let dir = fixture("s2_t010_c5", "relative-start-several-faults");
    let checked = check(&dir);
    assert_eq!(checked.code, 1, "{}", checked.all());
    // A duration of zero (Sprint 1), a missing clip and a clip that names itself, in the order of the lines.
    let items = listed(&checked.stdout);
    let found: Vec<(&str, u32)> = items.iter().map(|item| (item.code.as_str(), item.line)).collect();
    let at = |needle: &str| lines_with("relative-start-several-faults", needle)[0];
    assert_eq!(
        found,
        [
            ("FS-E024", at("data-fs-duration=\"0\"")),
            ("FS-E050", at("after:opening")),
            ("FS-E049", at("after:third")),
        ]
    );
    assert!(
        checked
            .stdout
            .ends_with("check failed: 3 errors, 0 unsupported, 0 unknown, 0 warnings\n"),
        "{}",
        checked.stdout
    );
    // The follower of the clip with the zero duration is not reported again.
    assert!(!checked.stdout.contains("#fourth"), "{}", checked.stdout);

    let spies = Spies::new("s2_t010_c5");
    let before = tree(&dir);
    let cwd = dir.parent().unwrap();
    let render = spies.run(cwd, &["render", text(&dir)]);
    let json = spies.run(cwd, &["render", text(&dir), "--json"]);
    assert_eq!(render.code, 1, "{}", render.all());
    assert_eq!(json.code, 1, "{}", json.all());
    assert_eq!(render.stderr, checked.stdout, "render must print the list that check printed");
    let doc = document(&json);
    assert_eq!(doc["outcome"], "failed");
    let from_json: Vec<(String, u64)> = doc["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| (item["code"].as_str().unwrap().to_string(), item["line"].as_u64().unwrap()))
        .collect();
    let from_text: Vec<(String, u64)> = items.iter().map(|item| (item.code.clone(), u64::from(item.line))).collect();
    assert_eq!(from_json, from_text);
    assert_eq!(spies.started(), (false, false), "a browser or ffmpeg was started");
    assert!(!dir.join("out").exists());
    assert_eq!(before, tree(&dir));
}

#[test]
fn s2_t010_c5_the_stand_ins_do_see_a_browser_and_ffmpeg_when_the_fixture_renders() {
    // The control for "starts no browser": the passing fixture does start both.
    let spies = Spies::new("s2_t010_c5_control");
    let dir = fixture("s2_t010_c5_control_project", "relative-start");
    let ran = spies.run(dir.parent().unwrap(), &["render", text(&dir)]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert_eq!(spies.started(), (true, true));
}

// ---------------------------------------------------------------------
// c6: the refusal stays only for the forms that wait
// ---------------------------------------------------------------------

/// A small project with the clips and the text `check` must print for it.
struct Waits {
    key: &'static str,
    body: String,
    /// A piece of the message that says which form it is.
    says: &'static str,
    /// The codes expected, besides none other.
    codes: &'static [&'static str],
}

fn waits() -> Vec<Waits> {
    let nested = "<div id=\"inner\" data-fs-composition=\"inner\" data-fs-size=\"64x36\"></div>\n";
    vec![
        // Ticket T011 built the offset, ticket T012 built chains and ticket T013 built a named clip with no
        // written duration, so none of them is a form that waits; a nested composition as the named clip
        // still waits (ticket T038).
        Waits {
            key: "nested_composition",
            body: format!("{nested}{}", clip("b", "after:inner", Some("1"))),
            says: "starts after \"inner\", a nested composition; following a nested composition is not supported in this version.",
            codes: &["FS-U006", "FS-U003"],
        },
        Waits {
            key: "nested_composition_with_an_offset",
            body: format!("{nested}{}", clip("b", "after:inner + 0.5", Some("1"))),
            says: "starts after \"inner\", a nested composition;",
            codes: &["FS-U006", "FS-U003"],
        },
        Waits {
            key: "nested_composition_followed_by_a_chain",
            body: format!(
                "{nested}{}{}",
                clip("b", "after:inner", Some("1")),
                clip("c", "after:b -0.5", Some("1"))
            ),
            says: "starts after \"inner\", a nested composition;",
            codes: &["FS-U006", "FS-U003"],
        },
    ]
}

/// A stand-in picture: `check` reads files only and needs the file to exist.
fn small_png() -> &'static str {
    "a file the check only looks for"
}

#[test]
fn s2_t010_c6_the_plain_form_raises_no_fs_u003() {
    let dir = fixture("s2_t010_c6_plain", "relative-start");
    let ran = check(&dir);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert!(!ran.all().contains("FS-U003"), "{}", ran.all());
    // Written before the named clip, with spaces, and with an id that holds a hyphen: none is refused.
    for (key, written, id) in [
        ("spaces", "after:  a ", "a"),
        ("plain", "after:a", "a"),
        ("hyphen_id", "after:scene-2", "scene-2"),
    ] {
        let body = format!("{}{}", clip("b", written, Some("1")), clip(id, "1", Some("2")));
        let ran = check(&boxes(&format!("s2_t010_c6_{key}"), 30, &body));
        assert_eq!(ran.code, 0, "{key}: {}", ran.all());
        assert!(!ran.all().contains("FS-U003"), "{key}: {}", ran.all());
    }
}

#[test]
fn s2_t010_c6_the_forms_that_wait_are_still_refused_and_say_which_form_they_are() {
    for case in waits() {
        let test = format!("s2_t010_c6_waits_{}", case.key);
        let dir = boxes(&test, 30, &case.body);
        write(&dir, "a.png", small_png());
        let checked = check(&dir);
        assert_eq!(checked.code, 2, "{}: {}", case.key, checked.all());
        assert_eq!(codes(&checked.stdout), case.codes, "{}: {}", case.key, checked.stdout);
        assert!(checked.stdout.contains(case.says), "{}: {}", case.key, checked.stdout);
        assert!(
            checked.stdout.contains("unsupported FS-U003 index.html:"),
            "{}: {}",
            case.key,
            checked.stdout
        );

        // `render` refuses it: the same list, exit 2, nothing started.
        let spies = Spies::new(&test);
        let render = spies.run(dir.parent().unwrap(), &["render", text(&dir)]);
        assert_eq!(render.code, 2, "{}: {}", case.key, render.all());
        assert_eq!(render.stderr, checked.stdout, "{}", case.key);
        assert_eq!(spies.started(), (false, false), "{}", case.key);
        assert!(!dir.join("out").exists(), "{}", case.key);
    }
    // The fixture of the matrix is a follower of a nested composition (it was an offset until T011, a
    // chain until T012 and a clip with no written duration until T013).
    let fixture_dir = fixture("s2_t010_c6_offset_fixture", "unsupported/fs-u003-nested-composition-start");
    let ran = check(&fixture_dir);
    assert_eq!(ran.code, 2, "{}", ran.all());
    assert_eq!(codes(&ran.stdout), ["FS-U006", "FS-U003"]);
}

#[test]
fn s2_t010_c6_a_fault_and_a_form_that_waits_in_one_file_exit_1_and_list_both() {
    // A follower of a nested composition (a form that waits) and an id no clip has.
    let body = format!(
        "{}{}{}",
        "<div id=\"inner\" data-fs-composition=\"inner\" data-fs-size=\"64x36\"></div>\n",
        clip("b", "after:inner", Some("1")),
        clip("d", "after:nothing", Some("1"))
    );
    let ran = check(&boxes("s2_t010_c6_both", 30, &body));
    assert_eq!(ran.code, 1, "{}", ran.all());
    let mut found = codes(&ran.stdout);
    found.sort();
    assert_eq!(found, ["FS-E050", "FS-U003", "FS-U006"]);
}

#[test]
fn s2_t010_c6_the_code_is_given_to_nothing_else_and_the_support_matrix_lists_relative_starts_as_supported() {
    let matrix = spec("support-matrix.md");
    let rows = |code: &str| -> Vec<String> {
        matrix
            .lines()
            .filter(|line| line.starts_with(&format!("| {code} |")))
            .map(str::to_string)
            .collect()
    };
    let row = rows("FS-U003");
    assert_eq!(row.len(), 1, "{row:?}");
    let cells: Vec<&str> = row[0].trim().trim_matches('|').split('|').map(str::trim).collect();
    assert_eq!(cells.len(), 4, "{cells:?}");
    // The meaning names only relative starts; the plain form is said to be supported, not refused.
    assert!(
        cells[1].starts_with("A clip start written relative to another clip in a form that is not built yet"),
        "{}",
        cells[1]
    );
    assert!(
        cells[1].contains("The plain form `after:<clip id>`") && cells[1].contains("supported"),
        "{}",
        cells[1]
    );
    // It waits for the ticket that builds the other form, and no longer for T010, T011, T012 or T013.
    assert!(cells[3].contains("T038"), "{}", cells[3]);
    assert!(
        !cells[3].contains("T010") && !cells[3].contains("T011") && !cells[3].contains("T012") && !cells[3].contains("T013"),
        "{}",
        cells[3]
    );
    // The "Supported in this version" paragraph lists the relative start.
    let supported = matrix
        .lines()
        .skip_while(|line| *line != "## Supported in this version")
        .nth(2)
        .unwrap();
    assert!(supported.contains("`data-fs-start=\"after:<clip id>\"`"), "{supported}");
    // The two new codes are errors of the error table.
    let errors: Vec<&str> = matrix
        .lines()
        .skip_while(|line| *line != "## Errors")
        .take_while(|line| *line != "## Warnings")
        .collect();
    assert!(errors.iter().any(|line| line.starts_with("| FS-E049 |")));
    assert!(errors.iter().any(|line| line.starts_with("| FS-E050 |")));
    // (ticket T011 added FS-E051 and FS-E052 after them)

    // The number is written in one place of the product: the resolver of the model. It is given to no
    // other meaning there: every message that carries it speaks of a clip that starts relative to another.
    let sources = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut files_with_code = Vec::new();
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
                } else if path.extension().is_some_and(|e| e == "rs") && std::fs::read_to_string(&path).unwrap().contains("\"FS-U003\"") {
                    files_with_code.push(path.strip_prefix(&sources).unwrap().to_path_buf());
                }
            }
        }
    }
    assert_eq!(files_with_code, [PathBuf::from("model/src/resolve.rs")]);
    for case in waits() {
        let dir = boxes(&format!("s2_t010_c6_meaning_{}", case.key), 30, &case.body);
        write(&dir, "a.png", small_png());
        let ran = check(&dir);
        for block in ran.stdout.split("\n\n").filter(|block| block.contains("FS-U003")) {
            assert!(
                block.contains("starts after") || block.contains("starts relative"),
                "{}: {block}",
                case.key
            );
        }
    }
}

// ---------------------------------------------------------------------
// c7: absolute starts only: the same check output
// ---------------------------------------------------------------------

/// What `check` printed for these fixtures before ticket T010 changed the model: taken from the binary
/// built before the change. (title-card, title-card-unwritten, pair-move, repeat, easings, empty, and two
/// projects that are refused.)
const BEFORE: &[(&str, i32, &str)] = &[
    (
        "title-card",
        0,
        "check passed\n  composition  title-card\n  size         1280x720\n  rate         30 fps\n  length       4 s\n  frames       120\n  clips        3\n  tweens       7\n",
    ),
    (
        "title-card-unwritten",
        0,
        "check passed\n  composition  title-card\n  size         1280x720\n  rate         30 fps (filled in)\n  length       4 s (filled in)\n  frames       120\n  clips        3\n  tweens       7\n  clip #title  duration 3.2 s (filled in)\n",
    ),
    (
        "pair-move",
        0,
        "check passed\n  composition  pair-move\n  size         1280x720\n  rate         30 fps\n  length       3 s\n  frames       90\n  clips        1\n  tweens       1\n",
    ),
    (
        "repeat",
        0,
        "check passed\n  composition  repeat\n  size         320x180\n  rate         30 fps\n  length       2 s\n  frames       60\n  clips        2\n  tweens       4\n",
    ),
    (
        "easings",
        0,
        "check passed\n  composition  easings\n  size         1280x720\n  rate         30 fps\n  length       3 s\n  frames       90\n  clips        0\n  tweens       10\n",
    ),
    (
        "empty",
        0,
        "check passed\n  composition  empty\n  size         320x180\n  rate         30 fps\n  length       1 s\n  frames       30\n  clips        0\n  tweens       0\n",
    ),
    (
        "unsupported/fs-u007-declared-variable",
        2,
        "unsupported FS-U007 index.html:8\n  The attribute data-fs-var-title declares a variable; variables are not supported in this version.\n  fix: Remove the attribute and write the value directly in the page.\n\ncheck refused: 0 errors, 1 unsupported, 0 unknown, 0 warnings\n",
    ),
    (
        "unsupported/fs-x001-unknown-clip-end",
        2,
        "unknown FS-X001 index.html:12\n  The attribute data-fs-end is not one Framesmith knows.\n  fix: Check the spelling. The attributes are data-fs-composition, data-fs-size, data-fs-fps, data-fs-duration, data-fs-clip, data-fs-start and data-fs-track.\n\ncheck refused: 0 errors, 0 unsupported, 1 unknown, 0 warnings\n",
    ),
];

#[test]
fn s2_t010_c7_check_output_of_projects_with_absolute_starts_is_what_it_was() {
    for (index, (name, code, before)) in BEFORE.iter().enumerate() {
        let dir = fixture(&format!("s2_t010_c7_{index}"), name);
        let ran = check(&dir);
        assert_eq!(ran.code, *code, "{name}: {}", ran.all());
        assert_eq!(&ran.stdout, before, "{name}");
        assert_eq!(ran.stderr, "", "{name}");
    }
}

#[test]
fn s2_t010_c7_a_project_of_absolute_starts_prints_no_line_for_a_relative_start() {
    for name in ["title-card", "title-card-unwritten", "pair-move", "repeat"] {
        let dir = fixture(&format!("s2_t010_c7_lines_{name}"), name);
        for command in ["check", "info"] {
            let ran = framesmith(&[command, text(&dir)]);
            assert_eq!(ran.code, 0, "{name}: {}", ran.all());
            assert!(
                !ran.stdout.contains("(after:") && !ran.stdout.contains(" start "),
                "{name} {command}: {}",
                ran.stdout
            );
            // The only `clip` lines are the ones of a worked-out duration.
            assert!(ran
                .stdout
                .lines()
                .filter(|line| line.trim_start().starts_with("clip "))
                .all(|line| line.contains(" duration ")));
        }
    }
}

// ---------------------------------------------------------------------
// The page resolves the same start
// ---------------------------------------------------------------------

/// The failure description of a render of the fixture-like project whose page script changes the page.
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
fn s2_t010_page_the_page_resolves_the_follower_to_the_same_start_as_the_files() {
    // The render compares the manifest the page resolved with the one read from the files before any
    // frame is taken, and stops when they differ (page-protocol.md). It passes for the fixture, and the
    // job it ran holds the follower's resolved start and the form it was written in.
    let dir = fixture("s2_t010_page_agree", "relative-start");
    let job_file = dir.parent().unwrap().join("job.json");
    let ran = run_in(
        dir.parent().unwrap(),
        &["render", text(&dir)],
        &[("FRAMESMITH_DEBUG_JOB", job_file.as_os_str())],
    );
    assert_eq!(ran.code, 0, "{}", ran.all());
    let job: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&job_file).unwrap()).unwrap();
    let clips = &job["manifest"]["clips"];
    assert_eq!(clips[1]["start"], "3/1");
    assert_eq!(clips[1]["relative_start"], "after:intro");
    assert_eq!(clips[0]["relative_start"], serde_json::Value::Null);
    assert!(
        ran.stdout.contains("probe     done      320x180, 30 fps, 5 s, 150 frames\n"),
        "{}",
        ran.stdout
    );
}

#[test]
fn s2_t010_page_the_page_works_the_start_out_itself_from_the_document() {
    // The follower is written first. A script moves the clip it names by one second before the page
    // reports ready: the page's own resolution puts the follower at 4 s where the files say 3 s, and the
    // first difference is the follower's start, with both values.
    let clips = format!("{}{}", clip("b", "after:a", Some("1")), clip("a", "1", Some("2")));
    let said = page_disagrees(
        "s2_t010_page_moved",
        &clips,
        "document.getElementById(\"a\").setAttribute(\"data-fs-start\", \"2\");",
    );
    assert_eq!(
        said,
        "The page disagrees with the files: the start of clip 1 (#b) is 3 s in the files and 4 s in the page."
    );
    // A script that writes the follower's start as a time gives the same start, written another way:
    // the page's manifest names the difference in the written form.
    let said = page_disagrees(
        "s2_t010_page_written",
        &clips,
        "document.getElementById(\"b\").setAttribute(\"data-fs-start\", \"3\");",
    );
    assert_eq!(
        said,
        "The page disagrees with the files: the way the start of clip 1 (#b) is written is \"after:a\" in the files and a time in seconds in the page."
    );
}
