//! Sprint 2, ticket T013: a clip that follows a clip with no written duration. After a still image that
//! has none, the follower starts at the image's start plus 3 seconds and `check` marks the image's
//! duration as filled in; after a clip that lasts to the end of the composition, it starts at the
//! composition's end, `check` gives the warning for a clip that can never show and `render` proceeds;
//! after a clip whose end cannot be worked out, `check` lists an error with its own code (FS-E054) that
//! names both clips and says what to write, and exits 1. The spec section for these rules names the
//! observation of the sampling as its source.
//!
//! Notes for maintainers
//! ---------------------
//! * The times are read from the text of the built binary and from the pictures of real renders; the
//!   expected values are written out here from the way each project is written, not taken from the code.
//! * The fixtures are `fixtures/follow-still-image`, `fixtures/follow-to-the-end` and
//!   `fixtures/follow-unknown-end`, one for each case. The tests that expected a start that names a clip
//!   with no written duration to be refused (FS-U003) were changed by this ticket, in `s2_t010_relative_start.rs`,
//!   `s2_t011_relative_offset.rs`, `s2_t012_relative_chain.rs`, `refuse.rs` and
//!   `s2_t005_refusals_that_stay.rs`: FS-U003 now waits only for a nested composition (ticket T038).
//! * Only the render tests and the page test need a browser and `ffmpeg`; the others read files only. The
//!   refusing tests use stand-ins that note being started.
//! * Scratch folders: every test names its projects with a scratch key of its own.

mod common;

use common::*;
use measure::{Frame, Ran, Rgb};
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

/// A 320 by 180 project at 30 frames a second with `attrs` on the composition and the given clips in the
/// body.
fn boxes(test: &str, attrs: &str, clips: &str, rest: &str) -> PathBuf {
    let attrs = format!("data-fs-size=\"320x180\" data-fs-fps=\"30\" {attrs}");
    project(test, "boxes", &page(&attrs, BOX_STYLE, clips, rest))
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
// c1: a still image with no duration
// ---------------------------------------------------------------------

#[test]
fn s2_t013_c1_check_marks_the_image_duration_as_filled_in_and_prints_the_follower_start_beside_the_written_form() {
    let dir = fixture("s2_t013_c1_check", "follow-still-image");
    let checked = check(&dir);
    assert_eq!(checked.code, 0, "{}", checked.all());
    // #photo starts at 1 s and lasts 3 s (filled in); #next starts at 4 s, #later at 4.5 s, #earlier at 3.5 s.
    assert_eq!(
        checked.stdout,
        "check passed\n\
         \x20 composition  main\n\
         \x20 size         320x180\n\
         \x20 rate         30 fps\n\
         \x20 length       5.5 s (filled in)\n\
         \x20 frames       165\n\
         \x20 clips        4\n\
         \x20 tweens       0\n\
         \x20 clip #photo  duration 3 s (filled in)\n\
         \x20 clip #next  start 4 s (after:photo)\n\
         \x20 clip #later  start 4.5 s (after:photo + 0.5)\n\
         \x20 clip #earlier  start 3.5 s (after:photo - 0.5)\n"
    );
    // `info` prints the same lines; `timeline` shows the start, the end and the filled-in mark.
    let info = framesmith(&["info", text(&dir)]);
    assert_eq!(info.code, 0, "{}", info.all());
    assert!(
        info.stdout
            .contains("clip #photo  duration 3 s (filled in)\nclip #next  start 4 s (after:photo)\n"),
        "{}",
        info.stdout
    );
    let timeline = framesmith(&["timeline", text(&dir)]);
    assert_eq!(timeline.code, 0, "{}", timeline.all());
    assert_eq!(
        timeline.stdout,
        "main  320x180  30 fps  5.5 s (filled in)  165 frames\n\
         \x20 #photo    1 s   to 4 s    3 s (filled in)\n\
         \x20 #next     4 s   to 5 s    1 s\n\
         \x20 #later    4.5 s to 5.5 s  1 s\n\
         \x20 #earlier  3.5 s to 4.5 s  1 s\n"
    );
    let json = document(&framesmith(&["timeline", text(&dir), "--json"]));
    assert_eq!(json["clips"][0]["duration_filled_in"], true);
    assert_eq!(json["clips"][1]["name"], "#next");
    assert_eq!(
        (json["clips"][1]["start"].clone(), json["clips"][1]["duration_filled_in"].clone()),
        (4.into(), false.into())
    );
}

#[test]
fn s2_t013_c1_writing_a_duration_on_the_image_moves_the_follower_and_drops_the_mark() {
    // The control: the same project with 2 s written on the image. Its end is 3 s, so #next starts there,
    // and the image's duration is no longer marked as filled in.
    let dir = fixture("s2_t013_c1_control", "follow-still-image");
    let page_file = std::fs::read_to_string(dir.join("index.html")).unwrap();
    let edited = page_file.replace("data-fs-start=\"1\">", "data-fs-start=\"1\" data-fs-duration=\"2\">");
    assert_ne!(page_file, edited);
    write(&dir, "index.html", edited);
    let checked = check(&dir);
    assert_eq!(checked.code, 0, "{}", checked.all());
    assert!(
        checked.stdout.contains("  clip #next  start 3 s (after:photo)\n"),
        "{}",
        checked.stdout
    );
    assert!(!checked.stdout.contains("clip #photo  duration"), "{}", checked.stdout);
}

#[test]
fn s2_t013_c1_the_render_shows_each_follower_from_the_frame_of_its_start() {
    let dir = fixture("s2_t013_c1_render", "follow-still-image");
    let (ran, frames) = render_frames(&dir, &[], &[]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert_eq!(frames.len(), 165);
    // 30 frames a second: the image shows from 1 s (frame 30) to 4 s (frame 120); #next from 4 s (frame
    // 120) for 1 s; #later from 4.5 s (frame 135) for 1 s; #earlier from 3.5 s (frame 105) for 1 s.
    assert_eq!(shown(&frames, RED), range(30, 120));
    assert_eq!(shown(&frames, GREEN), range(120, 150));
    assert_eq!(shown(&frames, BLUE), range(135, 165));
    assert_eq!(shown(&frames, YELLOW), range(105, 135));
    // On the frame before 4 s the follower is not shown, and on the frame of 4 s the image is gone.
    assert_eq!(frames[119].count(GREEN, 0), 0);
    assert_eq!(frames[120].count(GREEN, 0), BOX);
    assert_eq!(frames[119].count(RED, 0), BOX);
    assert_eq!(frames[120].count(RED, 0), 0);
}

#[test]
fn s2_t013_c1_the_page_resolves_the_start_after_an_image_the_same_way_as_the_files() {
    // The render compares the page's manifest with the one read from the files before any frame is taken,
    // and stops when they differ. It passes, and the job it ran holds each follower's resolved start.
    let dir = fixture("s2_t013_c1_page", "follow-still-image");
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
        (clips[1]["start"].clone(), clips[1]["relative_start"].clone()),
        ("4/1".into(), "after:photo".into())
    );
    assert_eq!(clips[0]["duration"], "3/1");
    assert_eq!(clips[0]["duration_written"], false);
}

// ---------------------------------------------------------------------
// c2: a clip that lasts to the end of the composition
// ---------------------------------------------------------------------

#[test]
fn s2_t013_c2_check_gives_the_warning_for_a_clip_that_never_shows_and_passes() {
    let dir = fixture("s2_t013_c2_check", "follow-to-the-end");
    let checked = check(&dir);
    assert_eq!(checked.code, 0, "{}", checked.all());
    // The follower #never is on line 16 and starts at 4 s, the composition's end.
    let never = lines_with("follow-to-the-end", "id=\"never\"");
    assert_eq!(never, [16]);
    let found = listed(&checked.stdout);
    assert_eq!(
        found,
        [Listed {
            kind: "warning".into(),
            code: "FS-W001".into(),
            file: "index.html".into(),
            line: 16
        }]
    );
    assert!(
        checked
            .stdout
            .contains("The clip #never starts at 4 s, at or after the end of the composition (4 s), so it never shows."),
        "{}",
        checked.stdout
    );
    // The base lasts from 1 s to the end (filled in); #shows starts one second before the end.
    assert!(checked.stdout.contains("  length       4 s\n"), "{}", checked.stdout);
    assert!(
        checked.stdout.contains("  clip #base  duration 3 s (filled in)\n"),
        "{}",
        checked.stdout
    );
    assert!(
        checked.stdout.contains("  clip #never  start 4 s (after:base)\n"),
        "{}",
        checked.stdout
    );
    assert!(
        checked.stdout.contains("  clip #shows  start 3 s (after:base - 1)\n"),
        "{}",
        checked.stdout
    );
}

#[test]
fn s2_t013_c2_render_proceeds_and_the_follower_never_shows_while_the_one_before_the_end_does() {
    let dir = fixture("s2_t013_c2_render", "follow-to-the-end");
    let (ran, frames) = render_frames(&dir, &[], &[]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert!(
        ran.all().contains("FS-W001"),
        "the warning is printed by render too:\n{}",
        ran.all()
    );
    assert!(dir.join("out").join("follow-to-the-end.mp4").is_file());
    assert_eq!(frames.len(), 120);
    // #base lasts from 1 s (frame 30) to the end; #shows (after:base - 1) from 3 s (frame 90) to the end;
    // #never starts at the end and shows on no frame.
    assert_eq!(shown(&frames, RED), range(30, 120));
    assert_eq!(shown(&frames, BLUE), range(90, 120));
    assert_eq!(shown(&frames, GREEN), Vec::<usize>::new());
}

#[test]
fn s2_t013_c2_the_end_is_the_worked_out_length_when_none_is_written() {
    // Worked out from the end of the motion (4 s): the follower starts there and never shows.
    let body = format!("{}{}", clip("a", "1", None), clip("b", "after:a", Some("1")));
    let moving = motion("{ \"target\": \"#c\", \"to\": { \"x\": 1 }, \"at\": 0, \"duration\": 4 }");
    let dir = boxes("s2_t013_c2_motion", "", &format!("{body}{}", clip("c", "0", Some("0.5"))), &moving);
    let checked = check(&dir);
    assert_eq!(checked.code, 0, "{}", checked.all());
    assert!(checked.stdout.contains("  length       4 s (filled in)\n"), "{}", checked.stdout);
    assert!(checked.stdout.contains("  clip #b  start 4 s (after:a)\n"), "{}", checked.stdout);
    assert_eq!(codes(&checked.stdout), ["FS-W001"]);
    // Worked out from the end of another clip (4 s): the same.
    let dir = boxes("s2_t013_c2_clip", "", &format!("{body}{}", clip("c", "0", Some("4"))), "");
    let checked = check(&dir);
    assert_eq!(checked.code, 0, "{}", checked.all());
    assert!(checked.stdout.contains("  length       4 s (filled in)\n"), "{}", checked.stdout);
    assert!(checked.stdout.contains("  clip #b  start 4 s (after:a)\n"), "{}", checked.stdout);
    // The follower does not lengthen the composition: 5 s would be the length if it counted.
    assert!(checked.stdout.contains("frames       120\n"), "{}", checked.stdout);
    // And it is rendered: the composition is 120 frames, and the follower is on none.
    let (ran, frames) = render_frames(&dir, &[], &[]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert_eq!(frames.len(), 120);
    assert_eq!(shown(&frames, GREEN), Vec::<usize>::new());
    assert_eq!(shown(&frames, RED), range(30, 120));
}

#[test]
fn s2_t013_c2_an_offset_after_the_end_is_after_the_end_and_a_negative_one_shows() {
    let cases = [
        ("after:a", "4", true),
        ("after:a + 0.5", "4.5", true),
        ("after:a - 0.5", "3.5", false),
    ];
    for (key, (written, start, warns)) in cases.iter().enumerate() {
        let body = format!("{}{}", clip("a", "1", None), clip("b", written, Some("1")));
        let dir = boxes(&format!("s2_t013_c2_offset_{key}"), "data-fs-duration=\"4\"", &body, "");
        let checked = check(&dir);
        assert_eq!(checked.code, 0, "{written}: {}", checked.all());
        assert!(
            checked.stdout.contains(&format!("  clip #b  start {start} s ({written})\n")),
            "{written}: {}",
            checked.stdout
        );
        assert_eq!(codes(&checked.stdout), if *warns { vec!["FS-W001"] } else { vec![] }, "{written}");
    }
}

#[test]
fn s2_t013_c2_the_page_resolves_the_start_at_the_end_itself_and_a_difference_is_named_with_both_values() {
    // The follower is written first. A script gives the base a duration of 1 s before the page reports
    // ready: the page then puts the follower at 2 s, and the files say 4 s, the end of the composition.
    let body = format!("{}{}", clip("b", "after:a", Some("1")), clip("a", "1", None));
    let script = "<script>\ndocument.getElementById(\"a\").setAttribute(\"data-fs-duration\", \"1\");\n</script>";
    let dir = boxes("s2_t013_c2_page", "data-fs-duration=\"4\"", &body, script);
    assert_eq!(check(&dir).code, 0, "the files alone are fine");
    let ran = run_in(dir.parent().unwrap(), &["render", text(&dir), "--json"], &[]);
    assert_eq!(ran.code, 1, "{}", ran.all());
    let document = document(&ran);
    assert_eq!(document["error"]["code"], "FS-R010", "{document}");
    assert_eq!(
        document["error"]["description"],
        "The page disagrees with the files: the start of clip 1 (#b) is 4 s in the files and 2 s in the page."
    );
    assert!(!dir.join("out").exists());
}

// ---------------------------------------------------------------------
// c3: an end that cannot be worked out
// ---------------------------------------------------------------------

#[test]
fn s2_t013_c3_check_lists_an_error_with_its_own_code_that_names_both_clips_and_says_what_to_write_and_exits_1() {
    let dir = fixture("s2_t013_c3_check", "follow-unknown-end");
    let checked = check(&dir);
    assert_eq!(checked.code, 1, "{}", checked.all());
    let base = lines_with("follow-unknown-end", "id=\"base\"");
    let follower = lines_with("follow-unknown-end", "id=\"follower\"");
    assert_eq!((base.clone(), follower.clone()), (vec![14], vec![15]));
    // The named clip has its error from Sprint 1 (no length for it to last until); the follower has its own.
    assert_eq!(
        listed(&checked.stdout),
        [
            Listed {
                kind: "error".into(),
                code: "FS-E025".into(),
                file: "index.html".into(),
                line: base[0]
            },
            Listed {
                kind: "error".into(),
                code: "FS-E054".into(),
                file: "index.html".into(),
                line: follower[0]
            },
        ]
    );
    let block = checked
        .stdout
        .split("\n\n")
        .find(|block| block.starts_with("error FS-E054"))
        .unwrap_or_else(|| panic!("{}", checked.stdout));
    assert_eq!(
        block,
        "error FS-E054 index.html:15\n  \
         The clip #follower starts after #base, which has no duration and lasts until the end of the composition, but the composition has no length for it to last until: none is written, and there is no motion and no clip with its own end to work one out from, so the start of #follower cannot be worked out.\n  \
         fix: Write a duration on #base, for example data-fs-duration=\"2\", or a length on the composition, for example data-fs-duration=\"6\", or write the start of #follower as a time in seconds."
    );
    assert!(
        checked
            .stdout
            .ends_with("check failed: 2 errors, 0 unsupported, 0 unknown, 0 warnings\n"),
        "{}",
        checked.stdout
    );
}

#[test]
fn s2_t013_c3_render_refuses_with_the_same_list_exits_1_and_starts_nothing() {
    let dir = fixture("s2_t013_c3_render", "follow-unknown-end");
    let checked = check(&dir);
    let spies = Spies::new("s2_t013_c3_render");
    let before = tree(&dir);
    let render = spies.run(dir.parent().unwrap(), &["render", text(&dir)]);
    assert_eq!(render.code, 1, "{}", render.all());
    assert_eq!(render.stderr, checked.stdout);
    assert_eq!(spies.started(), (false, false));
    assert!(!dir.join("out").exists());
    assert_eq!(before, tree(&dir), "the project changed");
    // `info` and `timeline` list the same errors and exit 1, as `check` does.
    for command in ["info", "timeline"] {
        let ran = framesmith(&[command, text(&dir)]);
        assert_eq!(ran.code, 1, "{command}: {}", ran.all());
        assert_eq!(codes(&ran.stdout), ["FS-E025", "FS-E054"], "{command}: {}", ran.stdout);
    }
    // The structured form lists the error with its kind, file, line, description and remedy.
    let json = document(&framesmith(&["info", text(&dir), "--json"]));
    assert_eq!(json["outcome"], "failed");
    let error = &json["diagnostics"][1];
    assert_eq!(
        (error["code"].clone(), error["kind"].clone(), error["line"].clone()),
        ("FS-E054".into(), "error".into(), 15.into())
    );
    assert!(error["description"]
        .as_str()
        .unwrap()
        .starts_with("The clip #follower starts after #base,"));
    assert!(error["remedy"].as_str().unwrap().starts_with("Write a duration on #base,"));
}

#[test]
fn s2_t013_c3_each_thing_the_fix_says_to_write_makes_the_project_pass() {
    let original =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/follow-unknown-end/index.html")).unwrap();
    let base = "data-fs-clip data-fs-start=\"1\">";
    let composition = "data-fs-fps=\"30\">";
    let follower = "data-fs-start=\"after:base\" data-fs-duration=\"1\">";
    assert!(original.contains(base) && original.contains(composition) && original.contains(follower));
    // (what is written, the page, a line `check` prints, the warnings it lists)
    let fixes = [
        (
            "a duration on the named clip",
            original.replace(base, "data-fs-clip data-fs-start=\"1\" data-fs-duration=\"2\">"),
            "  clip #follower  start 3 s (after:base)\n",
            vec![],
        ),
        (
            "a length on the composition",
            original.replace(composition, "data-fs-fps=\"30\" data-fs-duration=\"6\">"),
            "  clip #follower  start 6 s (after:base)\n",
            vec!["FS-W001".to_string()],
        ),
        (
            "the start of the follower as a time",
            original.replace("data-fs-start=\"after:base\"", "data-fs-start=\"2\""),
            "  length       3 s (filled in)\n",
            vec![],
        ),
    ];
    for (index, (what, text_of_page, line, warnings)) in fixes.into_iter().enumerate() {
        let dir = project(&format!("s2_t013_c3_fix_{index}"), "p", &text_of_page);
        let checked = check(&dir);
        assert_eq!(checked.code, 0, "{what}: {}", checked.all());
        assert_eq!(codes(&checked.stdout), warnings, "{what}");
        assert!(checked.stdout.contains(line), "{what}: {}", checked.stdout);
    }
}

#[test]
fn s2_t013_c3_the_error_belongs_to_the_clip_that_follows_and_a_follower_of_that_one_is_silent() {
    let body = format!(
        "{}{}{}{}",
        clip("a", "1", None),
        clip("b", "after:a", Some("1")),
        clip("c", "after:b", Some("1")),
        clip("d", "after:a + 1", Some("1"))
    );
    let dir = boxes("s2_t013_c3_chain", "", &body, "");
    let checked = check(&dir);
    assert_eq!(checked.code, 1, "{}", checked.all());
    // #a: FS-E025; #b and #d follow #a: FS-E054 each; #c follows #b and has none of its own.
    assert_eq!(codes(&checked.stdout), ["FS-E025", "FS-E054", "FS-E054"], "{}", checked.stdout);
    assert!(checked.stdout.contains("The clip #b starts after #a,"), "{}", checked.stdout);
    assert!(checked.stdout.contains("The clip #d starts after #a,"), "{}", checked.stdout);
    assert!(!checked.stdout.contains("The clip #c starts"), "{}", checked.stdout);
}

#[test]
fn s2_t013_c3_a_project_with_a_fault_in_the_length_gets_that_fault_and_not_this_error() {
    let body = format!("{}{}", clip("a", "1", None), clip("b", "after:a", Some("1")));
    let dir = boxes("s2_t013_c3_wrong_length", "data-fs-duration=\"soon\"", &body, "");
    let checked = check(&dir);
    assert_eq!(checked.code, 1, "{}", checked.all());
    assert_eq!(codes(&checked.stdout), ["FS-E009"], "{}", checked.stdout);
}

#[test]
fn s2_t013_c3_the_stand_ins_do_see_a_browser_and_ffmpeg_when_the_project_renders() {
    // The control for "starts no browser and no ffmpeg": the project with the fix written does start both.
    let spies = Spies::new("s2_t013_c3_control");
    let dir = fixture("s2_t013_c3_control_project", "follow-unknown-end");
    let page_file = std::fs::read_to_string(dir.join("index.html")).unwrap();
    write(
        &dir,
        "index.html",
        page_file.replace("data-fs-fps=\"30\">", "data-fs-fps=\"30\" data-fs-duration=\"3\">"),
    );
    let ran = spies.run(dir.parent().unwrap(), &["render", text(&dir)]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert_eq!(spies.started(), (true, true));
}

// ---------------------------------------------------------------------
// c4: the spec section names the observation of the sampling as its source
// ---------------------------------------------------------------------

/// The section of `composition.md` for rule 3 of the Sprint 2 rules for values that are not written.
fn rule_three() -> String {
    let composition = spec("composition.md");
    let start = composition
        .find("**3. The start of a clip that follows a clip with no written duration**")
        .expect("the section for rule 3 is in composition.md");
    let end = composition[start..].find("**4. ").expect("rule 4 follows rule 3") + start;
    composition[start..end].to_string()
}

#[test]
fn s2_t013_c4_the_spec_section_names_the_sampling_as_its_source_with_the_values_observed() {
    let section = rule_three();
    // The source: the sampling, its date and ticket, and where it is recorded.
    assert!(
        section.contains("- Source: sampling of 2026-10-11, ticket T002, recorded in `PROGRESS.md`"),
        "{section}"
    );
    assert!(
        section.contains("The start of a clip that follows a clip with no end of its own: sampled (ticket T002)"),
        "{section}"
    );
    // The observed values that agree with the rule, with the frames the reference showed them on.
    for observed in [
        "frames 90, 105 and 75",
        "frames 120, 135 and 105",
        "Followed still image from 1 s with no duration",
    ] {
        assert!(section.contains(observed), "{observed}: {section}");
    }
    // The values that differ are recorded as the gap the product owner approved, with the date.
    for observed in ["frames 30, 45 and 15", "approved gap", "2026-10-12"] {
        assert!(section.contains(observed), "{observed}: {section}");
    }
    // The old state is gone: the section no longer waits for a decision.
    assert!(!section.contains("Open: the product owner decides"), "{section}");
    // The status line of the spec names the ticket and the heading is no longer "not yet built".
    let composition = spec("composition.md");
    let status = composition.lines().nth(2).unwrap();
    assert!(status.contains("ticket T013 added"), "{status}");
    assert!(!composition.contains("Sprint 2 rules for values that are not written (not yet built)"));
}

#[test]
fn s2_t013_c4_the_spec_states_the_three_cases_and_the_new_error_with_its_message() {
    let section = rule_three();
    for wanted in [
        "lasts 3 seconds",
        "the follower starts there",
        "FS-W001",
        "`render` proceeds",
        "FS-E054",
        "names both clips",
        "`check` exits 1",
    ] {
        assert!(section.contains(wanted), "{wanted}: {section}");
    }
    let composition = spec("composition.md");
    assert!(composition.contains("| FS-E054 |"));
    assert!(composition.contains("The clip #b starts after #a, which has no duration and lasts until the end of the composition"));
    // A start that names a clip with no written duration is not a form that waits any more.
    let waits: Vec<&str> = composition
        .lines()
        .skip_while(|line| !line.starts_with("**Forms that wait.**"))
        .skip(1)
        .take_while(|line| line.starts_with("- "))
        .collect();
    assert_eq!(waits.len(), 1, "{waits:?}");
    assert!(waits[0].contains("nested composition") && waits[0].contains("T038"), "{waits:?}");
}

// ---------------------------------------------------------------------
// The new code and FS-U003 after this ticket
// ---------------------------------------------------------------------

#[test]
fn s2_t013_the_new_code_is_listed_once_follows_the_last_error_code_and_is_given_to_nothing_else() {
    let matrix = spec("support-matrix.md");
    let rows = matrix.lines().filter(|line| line.starts_with("| FS-E054 |")).count();
    assert_eq!(rows, 1);
    let errors: Vec<String> = matrix
        .lines()
        .filter_map(|line| line.strip_prefix("| FS-E"))
        .map(|rest| format!("FS-E{}", rest.split(' ').next().unwrap()))
        .collect();
    assert_eq!(errors.last().map(String::as_str), Some("FS-E054"));
    assert_eq!(errors[errors.len() - 2], "FS-E053");
    let row = matrix.lines().find(|line| line.starts_with("| FS-E054 |")).unwrap();
    assert!(
        row.contains("lasts to the end of the composition") && row.contains("names both clips"),
        "{row}"
    );
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
                } else if path.extension().is_some_and(|e| e == "rs") && std::fs::read_to_string(&path).unwrap().contains("\"FS-E054\"") {
                    files.push(path.strip_prefix(&sources).unwrap().to_path_buf());
                }
            }
        }
    }
    assert_eq!(files, [PathBuf::from("model/src/resolve.rs")]);
}

#[test]
fn s2_t013_fs_u003_is_no_longer_raised_for_a_clip_with_no_written_duration_and_waits_only_for_a_nested_composition() {
    let matrix = spec("support-matrix.md");
    let row = matrix.lines().find(|line| line.starts_with("| FS-U003 |")).unwrap();
    let cells: Vec<&str> = row.trim().trim_matches('|').split('|').map(str::trim).collect();
    assert!(cells[1].contains("naming a nested composition"), "{}", cells[1]);
    assert!(
        cells[1].contains("a start that names a clip with no written duration") && cells[1].contains("supported"),
        "{}",
        cells[1]
    );
    assert!(!cells[1].contains("naming a clip with no written duration"), "{}", cells[1]);
    assert!(cells[3].contains("T038") && !cells[3].contains("T013"), "{}", cells[3]);
    // The fixture of the row is a clip that follows a nested composition: the nested composition is
    // listed beside it (FS-U006), and no start written today raises FS-U003 alone.
    let ran = check(&fixture("s2_t013_u003_fixture", "unsupported/fs-u003-nested-composition-start"));
    assert_eq!(ran.code, 2, "{}", ran.all());
    assert_eq!(codes(&ran.stdout), ["FS-U006", "FS-U003"]);
    // The three fixtures of this ticket do not raise it.
    for folder in ["follow-still-image", "follow-to-the-end", "follow-unknown-end"] {
        let ran = check(&fixture(&format!("s2_t013_u003_{folder}"), folder));
        assert!(!ran.all().contains("FS-U003"), "{folder}: {}", ran.all());
    }
}

#[test]
fn s2_t013_a_project_of_absolute_starts_and_written_durations_is_checked_as_before() {
    // No relative start, no clip without a duration: the output is the one of Sprint 1.
    let body = format!("{}{}", clip("a", "0", Some("1")), clip("b", "1", Some("1")));
    let dir = boxes("s2_t013_before", "data-fs-duration=\"3\"", &body, "");
    let checked = check(&dir);
    assert_eq!(
        checked.stdout,
        "check passed\n  composition  main\n  size         320x180\n  rate         30 fps\n  length       3 s\n  frames       90\n  clips        2\n  tweens       0\n"
    );
}
