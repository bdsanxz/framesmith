//! Sprint 2, ticket T006: `framesmith info <project>` prints the facts of a project, marks what was
//! filled in, agrees with `check`, gives `check`'s diagnostics for a project that does not pass, needs
//! no browser and no `ffmpeg`, and prints plain text.
//!
//! Notes for maintainers
//! ---------------------
//! * The facts of `info` and of `check` come from one function in `src/text.rs` (`fact_rows`). The
//!   comparison test (c3) still parses both outputs, so a change to either printer is caught here.
//! * The test projects are small and written for this file. Their known values are stated in the
//!   `Known` table, and every test reads its expectations from there.
//! * Only c1's render test needs a browser and `ffmpeg`; all the others read files only.
//! * No project of these tests has a warning, so the printing of warnings before the facts is not
//!   exercised here; it is exercised in `s2_t007_info_json.rs`
//!   (`s2_t007_c1_a_warning_on_a_passing_project_is_carried_and_printed_before_the_facts`).

mod common;

use common::*;
use measure::Ran;
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// A project of this file: how it is written and what `info` is to say about it.
struct Known {
    name: &'static str,
    /// The attributes of the composition.
    attrs: &'static str,
    /// The clips as (start, duration written), in seconds.
    clips: &'static [(&'static str, Option<&'static str>)],
    /// The tweens, as JSON objects.
    tweens: &'static str,
    /// What `info` is to print, label by label (the `project` and `output` rows are worked out).
    rows: &'static [(&'static str, &'static str)],
    /// The `clip` lines it is to print.
    clip_lines: &'static [&'static str],
}

/// Everything written: nothing is filled in.
const WRITTEN: Known = Known {
    name: "written",
    attrs: "data-fs-size=\"64x36\" data-fs-fps=\"24\" data-fs-duration=\"1\"",
    clips: &[("0", Some("0.5")), ("0.5", Some("0.5"))],
    tweens: "{ \"target\": \"#c0\", \"to\": { \"x\": 10 }, \"at\": 0, \"duration\": 0.5 }",
    rows: &[
        ("composition", "main"),
        ("size", "64x36"),
        ("rate", "24 fps"),
        ("length", "1 s"),
        ("frames", "24"),
        ("compositions", "1"),
        ("clips", "2"),
        ("tweens", "1"),
    ],
    clip_lines: &[],
};

/// The rate is left out: 30 is filled in.
const RATE_FILLED: Known = Known {
    name: "rate_filled",
    attrs: "data-fs-size=\"64x36\" data-fs-duration=\"1\"",
    clips: &[("0", Some("0.5")), ("0.5", Some("0.5"))],
    tweens: "{ \"target\": \"#c0\", \"to\": { \"x\": 10 }, \"at\": 0, \"duration\": 0.5 }",
    rows: &[
        ("composition", "main"),
        ("size", "64x36"),
        ("rate", "30 fps (filled in)"),
        ("length", "1 s"),
        ("frames", "30"),
        ("compositions", "1"),
        ("clips", "2"),
        ("tweens", "1"),
    ],
    clip_lines: &[],
};

/// The length is left out: it is worked out from the clips and the tweens, and a clip with no duration
/// runs to the end.
const LENGTH_WORKED_OUT: Known = Known {
    name: "length_worked_out",
    attrs: "data-fs-size=\"64x36\" data-fs-fps=\"25\"",
    clips: &[("0", Some("0.5")), ("0.5", None)],
    tweens: "{ \"target\": \"#c0\", \"to\": { \"x\": 10 }, \"at\": 1, \"duration\": 1 }",
    rows: &[
        ("composition", "main"),
        ("size", "64x36"),
        ("rate", "25 fps"),
        ("length", "2 s (filled in)"),
        ("frames", "50"),
        ("compositions", "1"),
        ("clips", "2"),
        ("tweens", "1"),
    ],
    clip_lines: &["clip #c1  duration 1.5 s (filled in)"],
};

const ALL: [&Known; 3] = [&WRITTEN, &RATE_FILLED, &LENGTH_WORKED_OUT];

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

    /// The text `info` prints for the project in `dir`, given as `shown` (how the folder is written
    /// on the command line, made absolute) and the output path.
    fn expected(&self, shown: &Path, output: &Path) -> String {
        let mut text = format!("{:<13}{}\n", "project", shown.display());
        for (label, value) in self.rows {
            text.push_str(&format!("{label:<13}{value}\n"));
        }
        text.push_str(&format!("{:<13}{}\n", "output", output.display()));
        for line in self.clip_lines {
            text.push_str(line);
            text.push('\n');
        }
        text
    }
}

fn info(dir: &Path) -> Ran {
    framesmith(&["info", text(dir)])
}

fn check(dir: &Path) -> Ran {
    framesmith(&["check", text(dir)])
}

/// The rows of a printout as label -> value, and its `clip` lines. Leading spaces are ignored, so
/// `check`'s indented facts and `info`'s plain ones read the same way. `check`'s first line (the
/// result) and anything else that is not a row is skipped.
fn read_facts(printout: &str) -> (BTreeMap<String, String>, Vec<String>) {
    let (mut rows, mut clips) = (BTreeMap::new(), Vec::new());
    for line in printout.lines() {
        let line = line.trim_start();
        if line.starts_with("clip ") {
            clips.push(line.to_string());
        } else if line.len() > 13 && line.is_char_boundary(13) && line[..13].ends_with(' ') {
            rows.insert(line[..13].trim_end().to_string(), line[13..].to_string());
        }
    }
    (rows, clips)
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

// ---------------------------------------------------------------------
// c1
// ---------------------------------------------------------------------

#[test]
fn s2_t006_c1_info_prints_every_fact_of_a_valid_project_and_exits_0() {
    for known in ALL {
        let dir = known.make(&format!("s2_t006_c1_{}", known.name));
        let ran = info(&dir);
        assert_eq!(ran.code, 0, "{}", ran.all());
        assert!(ran.stderr.is_empty(), "{}", ran.stderr);
        let output = dir.join("out/known.mp4");
        assert_eq!(ran.stdout, known.expected(&dir, &output), "{}", known.name);
    }
}

#[test]
fn s2_t006_c1_the_folder_is_printed_in_full_also_when_it_is_written_relative() {
    let dir = WRITTEN.make("s2_t006_c1_relative");
    let parent = std::fs::canonicalize(dir.parent().unwrap()).unwrap();
    let ran = run_in(&parent, &["info", "known"], &[]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    let full = parent.join("known");
    assert_eq!(ran.stdout, WRITTEN.expected(&full, &full.join("out/known.mp4")));
    // The same run from inside the project gives the same project and output.
    let inside = run_in(&full, &["info", "."], &[]);
    assert_eq!(inside.code, 0, "{}", inside.all());
    let (rows, _) = read_facts(&inside.stdout);
    assert_eq!(rows["project"], full.display().to_string());
    assert_eq!(rows["output"], full.join("out/known.mp4").display().to_string());
}

#[test]
fn s2_t006_c1_the_printed_output_path_is_where_render_then_writes_the_video() {
    let dir = WRITTEN.make("s2_t006_c1_render");
    let parent = std::fs::canonicalize(dir.parent().unwrap()).unwrap();
    let before = entries(&dir);
    assert!(!dir.join("out").exists());

    let told = run_in(&parent, &["info", "known"], &[]);
    assert_eq!(told.code, 0, "{}", told.all());
    let (rows, _) = read_facts(&told.stdout);
    let printed = PathBuf::from(&rows["output"]);
    assert!(printed.is_absolute(), "{printed:?}");
    assert!(!printed.exists(), "info must not make the video");
    assert_eq!(entries(&dir), before);

    // `render` with no --output, written the same way: it reports the same path and writes there.
    let rendered = run_in(&parent, &["render", "known", "--json"], &[]);
    assert_eq!(rendered.code, 0, "{}", rendered.all());
    let reported = document(&rendered)["output"].as_str().unwrap().to_string();
    assert_eq!(reported, printed.display().to_string());
    let video = facts(&printed);
    assert_eq!((video.frames, video.width, video.height), (24, 64, 36), "{video:?}");

    // Nothing but the video and its folder was added by the render.
    let added: Vec<PathBuf> = entries(&dir).into_iter().filter(|p| !before.contains(p)).collect();
    assert_eq!(added, [PathBuf::from("out"), PathBuf::from("out/known.mp4")]);

    // Run again, with the video there: `info` still prints the same, and the video is not touched.
    let size = std::fs::metadata(&printed).unwrap().len();
    let again = run_in(&parent, &["info", "known"], &[]);
    assert_eq!(again.stdout, told.stdout);
    assert_eq!(std::fs::metadata(&printed).unwrap().len(), size);
}

#[test]
fn s2_t006_c1_an_option_or_a_missing_or_extra_word_is_refused_as_check_refuses_it() {
    let dir = WRITTEN.make("s2_t006_c1_usage");
    // `--json` itself is an option of `info` since T007 (tests in s2_t007_info_json.rs); `--json=1` is not.
    for extra in [&["--json=1"][..], &["--output", "x.mp4"], &["--fps=24"]] {
        let mut args = vec!["info", text(&dir)];
        args.extend_from_slice(extra);
        let ran = framesmith(&args);
        assert_eq!(ran.code, 1, "{extra:?}: {}", ran.all());
        assert!(ran.stderr.starts_with("error FS-R004\n"), "{extra:?}: {}", ran.stderr);
        assert!(ran.stderr.contains("too many"), "{extra:?}: {}", ran.stderr);
    }
    // The option alone, where a path belongs.
    let ran = framesmith(&["info", "--json=1"]);
    assert_eq!(ran.code, 1, "{}", ran.all());
    assert!(ran.stderr.contains("`framesmith info` has no option --json=1."), "{}", ran.stderr);
    let same = framesmith(&["check", "--json=1"]);
    assert_eq!(ran.stderr.replace("`framesmith info`", "`framesmith check`"), same.stderr);
    let ran = framesmith(&["info"]);
    assert_eq!(ran.code, 1);
    assert!(
        ran.stderr.contains("`framesmith info` needs the path of a project folder."),
        "{}",
        ran.stderr
    );
    // None of it made a file in the project.
    assert!(!dir.join("out").exists());
}

#[test]
fn s2_t006_c1_the_command_is_logged_with_its_input_and_its_result_under_one_run_id() {
    let dir = RATE_FILLED.make("s2_t006_c1_log");
    let logs = scratch("s2_t006_c1_log_folder");
    let ran = run_in(
        dir.parent().unwrap(),
        &["info", text(&dir)],
        &[("FRAMESMITH_LOG_DIR", logs.as_os_str()), ("FRAMESMITH_LOG", OsStr::new("info"))],
    );
    assert_eq!(ran.code, 0, "{}", ran.all());
    let log = std::fs::read_to_string(logs.join("framesmith.log")).unwrap();
    let lines: Vec<&str> = log.lines().collect();
    assert_eq!(lines.len(), 2, "{log}");
    assert!(
        lines[0].contains("command started") && lines[0].contains("command=\"info\""),
        "{log}"
    );
    assert!(lines[0].contains(&format!("project={}", dir.display())), "{log}");
    assert!(lines[1].contains("command finished") && lines[1].contains("exit=0"), "{log}");
    let run = |line: &str| line.split_whitespace().find(|word| word.starts_with("run=")).map(String::from);
    assert!(run(lines[0]).is_some() && run(lines[0]) == run(lines[1]), "{log}");
    // No file contents in it.
    assert!(!log.contains("data-fs"), "{log}");
}

// ---------------------------------------------------------------------
// c2
// ---------------------------------------------------------------------

#[test]
fn s2_t006_c2_a_default_rate_and_a_worked_out_length_are_marked_and_written_values_are_not() {
    let marks = |known: &Known| -> Vec<String> {
        let dir = known.make(&format!("s2_t006_c2_{}", known.name));
        let ran = info(&dir);
        assert_eq!(ran.code, 0, "{}", ran.all());
        ran.stdout
            .lines()
            .filter(|line| line.contains("filled in"))
            .map(String::from)
            .collect()
    };
    // Everything written: no mark anywhere.
    assert_eq!(marks(&WRITTEN), Vec::<String>::new());
    // The rate left out: 30 fps, marked, and only that.
    assert_eq!(marks(&RATE_FILLED), ["rate         30 fps (filled in)"]);
    // The length left out: marked, with the clip duration that was worked out; the written rate is not.
    assert_eq!(
        marks(&LENGTH_WORKED_OUT),
        ["length       2 s (filled in)", "clip #c1  duration 1.5 s (filled in)"]
    );
}

#[test]
fn s2_t006_c2_a_rate_that_is_written_as_30_is_not_marked() {
    let dir = project(
        "s2_t006_c2_thirty",
        "known",
        &page("data-fs-size=\"64x36\" data-fs-fps=\"30\" data-fs-duration=\"1\"", "", "", ""),
    );
    let ran = info(&dir);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert!(ran.stdout.contains("\nrate         30 fps\n"), "{}", ran.stdout);
    assert!(!ran.stdout.contains("filled in"), "{}", ran.stdout);
}

// ---------------------------------------------------------------------
// c3
// ---------------------------------------------------------------------

/// The labels `info` has and `check` does not.
const ONLY_INFO: [&str; 3] = ["compositions", "output", "project"];

fn assert_same_facts(dir: &Path, what: &str) {
    let told = info(dir);
    let checked = check(dir);
    assert_eq!((told.code, checked.code), (0, 0), "{what}: {}{}", told.all(), checked.all());
    assert!(checked.stdout.starts_with("check passed\n"), "{what}");
    let (told_rows, told_clips) = read_facts(&told.stdout);
    let (checked_rows, checked_clips) = read_facts(&checked.stdout);
    assert!(checked_rows.len() >= 7, "{what}: {checked_rows:?}");
    for (label, value) in &checked_rows {
        assert_eq!(told_rows.get(label), Some(value), "{what}: {label}");
    }
    assert_eq!(told_clips, checked_clips, "{what}: the worked-out clip durations");
    let extra: Vec<&str> = told_rows
        .keys()
        .map(String::as_str)
        .filter(|l| !checked_rows.contains_key(*l))
        .collect();
    assert_eq!(extra, ONLY_INFO, "{what}");
}

#[test]
fn s2_t006_c3_the_facts_info_prints_equal_the_facts_check_prints() {
    for known in ALL {
        assert_same_facts(&known.make(&format!("s2_t006_c3_{}", known.name)), known.name);
    }
    // Fixtures of the repository: one with everything written, one with a rate and a length left out.
    for name in ["title-card", "title-card-unwritten"] {
        assert_same_facts(&fixture(&format!("s2_t006_c3_{name}"), name), name);
    }
}

#[test]
fn s2_t006_c3_the_comparison_sees_a_fact_that_differs() {
    // The parser is not blind: facts of two different projects do not compare equal.
    let a = info(&WRITTEN.make("s2_t006_c3_differs_a"));
    let b = info(&RATE_FILLED.make("s2_t006_c3_differs_b"));
    assert_ne!(read_facts(&a.stdout).0, read_facts(&b.stdout).0);
}

// ---------------------------------------------------------------------
// c4
// ---------------------------------------------------------------------

fn assert_same_diagnostics(dir: &Path, code: i32, what: &str) {
    let told = info(dir);
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
    // No facts of the project are printed beside the diagnostics.
    assert!(!told.stdout.contains("project      "), "{what}: {}", told.stdout);
    assert!(!told.stdout.contains("output       "), "{what}: {}", told.stdout);
}

#[test]
fn s2_t006_c4_a_project_with_an_error_gets_the_diagnostics_of_check_and_exit_1() {
    // Three errors at once, so that the list and its order are compared too.
    let dir = project(
        "s2_t006_c4_errors",
        "faulty",
        &page("data-fs-size=\"65x36\" data-fs-fps=\"23.976\" data-fs-duration=\"-1\"", "", "", ""),
    );
    assert_same_diagnostics(&dir, 1, "three errors");
    assert!(info(&dir)
        .stdout
        .contains("check failed: 3 errors, 0 unsupported, 0 unknown, 0 warnings\n"));

    // A folder that does not exist, and a folder with no page.
    let root = scratch("s2_t006_c4_places");
    assert_same_diagnostics(&root.join("nowhere"), 1, "a missing folder");
    assert!(info(&root.join("nowhere")).stdout.contains("FS-E001"));
    std::fs::create_dir_all(root.join("bare")).unwrap();
    write(&root, "bare/notes.txt", "no page here");
    assert_same_diagnostics(&root.join("bare"), 1, "a folder with no page");
}

#[test]
fn s2_t006_c4_a_project_with_an_unsupported_feature_gets_the_diagnostics_of_check_and_exit_2() {
    for (name, code) in [
        ("unsupported/fs-u001-video-element", "FS-U001"),
        ("unsupported/fs-u014-css-animation-in-stylesheet", "FS-U014"),
        ("unsupported/fs-u009-network-image", "FS-U009"),
        ("unsupported/fs-x001-unknown-attribute", "FS-X001"),
    ] {
        let dir = fixture(&format!("s2_t006_c4_{code}"), name);
        assert_same_diagnostics(&dir, 2, name);
        assert!(info(&dir).stdout.contains(code), "{name}");
        assert!(info(&dir).stdout.contains("check refused: "), "{name}");
    }
}

// ---------------------------------------------------------------------
// c5
// ---------------------------------------------------------------------

#[test]
fn s2_t006_c5_info_needs_no_browser_and_no_ffmpeg_starts_nothing_and_changes_nothing() {
    let dir = LENGTH_WORKED_OUT.make("s2_t006_c5");
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
    let beside_before = entries(root);
    let usual = info(&dir);
    assert_eq!(usual.code, 0, "{}", usual.all());

    // Nothing to be found: an empty search path, no named browser, an empty home folder and an empty
    // folder of applications.
    let bare = run_in(
        root,
        &["info", text(&dir)],
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
    // The same machine cannot render: so `info` really ran without them.
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
        &["info", text(&dir)],
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
fn s2_t006_c5_a_project_that_does_not_pass_changes_nothing_either() {
    let dir = project(
        "s2_t006_c5_faulty",
        "faulty",
        &page("data-fs-size=\"65x36\" data-fs-duration=\"1\"", "", "", ""),
    );
    write(&dir, "out/known.mp4", "an earlier video");
    let (files, all) = (tree(&dir), entries(&dir));
    let ran = run_in(
        dir.parent().unwrap(),
        &["info", text(&dir)],
        &[("PATH", OsStr::new("")), ("FRAMESMITH_BROWSER", OsStr::new(""))],
    );
    assert_eq!(ran.code, 1, "{}", ran.all());
    assert_eq!((tree(&dir), entries(&dir)), (files, all));
}

// ---------------------------------------------------------------------
// c6
// ---------------------------------------------------------------------

/// Runs `info` with the colour switches the usual tools honour set, and `NO_COLOR` taken out.
fn info_forcing_colour(dir: &Path) -> Ran {
    let output = Command::new(BIN)
        .args(["info", text(dir)])
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
fn s2_t006_c6_the_output_has_no_escape_and_no_control_character_with_or_without_colour_switches() {
    let valid = LENGTH_WORKED_OUT.make("s2_t006_c6_valid");
    let faulty = project(
        "s2_t006_c6_faulty",
        "faulty",
        &page("data-fs-size=\"65x36\" data-fs-duration=\"1\"", "", "", ""),
    );
    let refused = fixture("s2_t006_c6_refused", "unsupported/fs-u001-video-element");
    for (dir, code) in [(&valid, 0), (&faulty, 1), (&refused, 2)] {
        for ran in [info(dir), info_forcing_colour(dir)] {
            assert_eq!(ran.code, code, "{}", ran.all());
            let all = format!("{}{}", ran.stdout, ran.stderr);
            assert!(!all.is_empty());
            assert_eq!(control_bytes(&all), Vec::<u8>::new(), "{all:?}");
            assert!(!all.contains('\u{1b}') && !all.contains('\r') && !all.contains('\t'), "{all:?}");
        }
    }
    // The colour switches change nothing at all.
    assert_eq!(info(&valid).stdout, info_forcing_colour(&valid).stdout);
}

#[test]
fn s2_t006_c6_every_line_of_a_valid_projects_output_is_a_label_and_a_value() {
    let known = &LENGTH_WORKED_OUT;
    let dir = known.make("s2_t006_c6_lines");
    let ran = info_forcing_colour(&dir);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert!(ran.stdout.ends_with('\n'));
    let labels = [
        "project",
        "composition",
        "size",
        "rate",
        "length",
        "frames",
        "compositions",
        "clips",
        "tweens",
        "output",
    ];
    let mut seen = Vec::new();
    for line in ran.stdout.lines() {
        assert_eq!(line, line.trim_end(), "trailing space: {line:?}");
        if let Some(rest) = line.strip_prefix("clip ") {
            // `clip <label>  duration <n> s (filled in)`
            assert!(rest.contains("  duration ") && rest.ends_with("(filled in)"), "{line:?}");
        } else {
            let (label, value) = (line[..13].trim_end(), &line[13..]);
            assert!(labels.contains(&label) && line[..13].ends_with(' '), "{line:?}");
            assert!(!value.is_empty() && !value.starts_with(' '), "{line:?}");
            seen.push(label);
        }
    }
    assert_eq!(seen, labels);
}
