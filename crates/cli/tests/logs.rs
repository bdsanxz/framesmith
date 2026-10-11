//! T042: a run is read back from the log, in order, under one run id, with
//! no file contents in it, and the commands still work when it cannot be
//! written.

mod common;

use common::*;
use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

const SMALL: &str = "data-fs-size=\"64x36\" data-fs-duration=\"0.2\"";
const MARKER: &str = "ZQ7-MARKER-4f2a9c";

/// One line of the log taken apart.
struct Line {
    time: String,
    level: String,
    component: String,
    run: String,
    message: String,
}

impl Line {
    fn parse(line: &str) -> Result<Line, String> {
        let mut rest = line;
        let mut take = |what: &str| -> Result<String, String> {
            let (word, tail) = rest.split_once(' ').ok_or(format!("no {what} in {line:?}"))?;
            rest = tail.trim_start_matches(' ');
            Ok(word.to_string())
        };
        let time = take("timestamp")?;
        let level = take("level")?;
        let component = take("component")?;
        let run = take("run id")?;
        let run = run.strip_prefix("run=").ok_or(format!("no run id in {line:?}"))?.to_string();
        Ok(Line {
            time,
            level,
            component,
            run,
            message: rest.to_string(),
        })
    }

    /// The words of the message before its first `key=value` field.
    fn event(&self) -> String {
        let words: Vec<&str> = self.message.split(' ').take_while(|word| !word.contains('=')).collect();
        words.join(" ")
    }

    /// The value of a `key=value` field, without quotes.
    fn field(&self, key: &str) -> Option<String> {
        let start = if self.message.starts_with(&format!("{key}=")) {
            0
        } else {
            self.message.find(&format!(" {key}="))? + 1
        };
        let value = &self.message[start + key.len() + 1..];
        let text = match value.strip_prefix('"') {
            Some(quoted) => quoted.split('"').next().unwrap_or(""),
            None => value.split(' ').next().unwrap_or(""),
        };
        Some(text.to_string())
    }

    /// A short name for the event: component, words, and the stage when it has one.
    fn label(&self) -> String {
        match self.field("stage") {
            Some(stage) if self.event() == "stage started" => format!("{} {} {stage}", self.component, self.event()),
            _ => format!("{} {}", self.component, self.event()),
        }
    }
}

fn log_text(dir: &Path) -> String {
    let mut all = String::new();
    for entry in std::fs::read_dir(dir).into_iter().flatten() {
        let path = entry.unwrap().path();
        all.push_str(&std::fs::read_to_string(path).unwrap());
    }
    all
}

fn lines(dir: &Path) -> Vec<Line> {
    let text = std::fs::read_to_string(dir.join("framesmith.log")).unwrap_or_default();
    text.lines()
        .map(|line| Line::parse(line).unwrap_or_else(|e| panic!("{e}")))
        .collect()
}

fn runs(lines: &[Line]) -> BTreeSet<&str> {
    lines.iter().map(|line| line.run.as_str()).collect()
}

fn logged(dir: &Path, level: Option<&str>) -> Vec<(&'static str, std::ffi::OsString)> {
    let mut env: Vec<(&'static str, std::ffi::OsString)> = vec![("FRAMESMITH_LOG_DIR", dir.as_os_str().to_owned())];
    if let Some(level) = level {
        env.push(("FRAMESMITH_LOG", level.into()));
    }
    env
}

fn run_logged(project: &Path, args: &[&str], logs: &Path, level: Option<&str>) -> measure::Ran {
    let env = logged(logs, level);
    let env: Vec<(&str, &OsStr)> = env.iter().map(|(name, value)| (*name, value.as_os_str())).collect();
    run_in(project.parent().unwrap(), args, &env)
}

fn tiny(test: &str) -> (PathBuf, PathBuf) {
    let dir = project(test, "tiny", &page(SMALL, "", "<p>x</p>", ""));
    let logs = dir.parent().unwrap().join("logs");
    (dir, logs)
}

#[test]
fn t042_c1_a_completed_render_is_told_in_order_under_one_run_id() {
    let (dir, logs) = tiny("t042_c1");
    let ran = run_logged(&dir, &["render", text(&dir)], &logs, None);
    assert_eq!(ran.code, 0, "{}", ran.all());
    let lines = lines(&logs);
    assert_eq!(runs(&lines).len(), 1, "one run id for the whole run");
    let labels: Vec<String> = lines.iter().map(Line::label).collect();
    let expected = [
        "cli command started",
        "render stage started compile",
        "render render job created",
        "render stage started probe",
        "browser child process started",
        "render stage started capture",
        "media child process started",
        "render stage started encode",
        "media child process exited",
        "browser child process exited",
        "render stage started commit",
        "cli command finished",
    ];
    assert_eq!(
        labels.iter().filter(|label| label.contains("stage started")).count(),
        5,
        "five stage changes: {labels:?}"
    );
    let mut at = 0;
    for want in expected {
        let found = labels[at..].iter().position(|label| label == want);
        assert!(
            found.is_some(),
            "{want:?} is missing or out of order after position {at}: {labels:?}"
        );
        at += found.unwrap() + 1;
    }
    assert_eq!(labels.first().unwrap(), "cli command started");
    assert_eq!(labels.last().unwrap(), "cli command finished");

    // The command with its inputs, and the result.
    let started = &lines[0];
    assert_eq!(started.field("command").as_deref(), Some("render"));
    assert_eq!(started.field("project").as_deref(), Some(text(&dir)));
    assert_eq!(started.field("output").unwrap(), format!("{}/out/tiny.mp4", text(&dir)));
    assert_eq!(started.field("fps").as_deref(), Some("as written"));
    let finished = lines.last().unwrap();
    assert_eq!(finished.field("exit").as_deref(), Some("0"));
    assert_eq!(finished.field("outcome").as_deref(), Some("completed"));
    assert!(finished.field("output").unwrap().ends_with("/tiny/out/tiny.mp4"));

    // The job, and each child's start and exit tell which process they are about.
    let job = lines.iter().find(|line| line.event() == "render job created").unwrap();
    let job_facts = ["width", "height", "fps", "frames"].map(|key| job.field(key).unwrap());
    assert_eq!(job_facts, ["64", "36", "30", "6"]);
    for component in ["browser", "media"] {
        let of = |event: &str| lines.iter().find(|l| l.component == component && l.event() == event).unwrap();
        let (start, exit) = (of("child process started"), of("child process exited"));
        assert_eq!(start.field("pid"), exit.field("pid"));
        assert!(start.field("program").is_some_and(|program| program.starts_with('/')));
        assert_eq!(exit.field("forced").as_deref(), Some("false"));
    }
}

#[test]
fn t042_c2_check_is_told_with_its_inputs_and_result_under_one_run_id() {
    let (dir, logs) = tiny("t042_c2");
    let ran = run_logged(&dir, &["check", text(&dir)], &logs, None);
    assert_eq!(ran.code, 0, "{}", ran.all());
    let lines = lines(&logs);
    assert_eq!(runs(&lines).len(), 1);
    let labels: Vec<String> = lines.iter().map(Line::label).collect();
    assert_eq!(labels, ["cli command started", "cli command finished"]);
    assert_eq!(lines[0].field("command").as_deref(), Some("check"));
    assert_eq!(lines[0].field("project").as_deref(), Some(text(&dir)));
    let result = ["exit", "errors", "unsupported", "unknown", "warnings"].map(|key| lines[1].field(key).unwrap());
    assert_eq!(result, ["0", "0", "0", "0", "0"]);

    // A check that fails says so in its result.
    let broken = project("t042_c2_broken", "broken", "<html><body>no composition</body></html>");
    let logs = broken.parent().unwrap().join("logs");
    let ran = run_logged(&broken, &["check", text(&broken)], &logs, None);
    assert_eq!(ran.code, 1, "{}", ran.all());
    let lines = self::lines(&logs);
    let result = lines.last().unwrap();
    assert_eq!(result.field("exit").as_deref(), Some("1"));
    assert_ne!(result.field("errors").as_deref(), Some("0"));
}

#[test]
fn t042_c3_two_commands_one_after_the_other_have_two_run_ids() {
    let (dir, logs) = tiny("t042_c3");
    for args in [vec!["check", text(&dir)], vec!["render", text(&dir)], vec!["check", text(&dir)]] {
        let ran = run_logged(&dir, &args, &logs, None);
        assert_eq!(ran.code, 0, "{}", ran.all());
    }
    let lines = lines(&logs);
    let ids = runs(&lines);
    assert_eq!(ids.len(), 3, "three commands, three run ids");
    // Each run's lines are together, and each starts with its command.
    let mut order: Vec<&str> = Vec::new();
    for line in &lines {
        if order.last() != Some(&line.run.as_str()) {
            order.push(&line.run);
        }
    }
    assert_eq!(order.len(), 3, "the lines of one run are not mixed with another's");
    for id in ids {
        let first = lines.iter().find(|line| line.run == id).unwrap();
        assert_eq!(first.label(), "cli command started");
        assert_eq!(id.len(), 12);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()), "{id}");
    }
}

#[test]
fn t042_c4_every_line_has_a_timestamp_a_level_a_component_and_the_run_id() {
    let (dir, logs) = tiny("t042_c4");
    let failing = project(
        "t042_c4_failing",
        "failing",
        &page(SMALL, "", "<p>x</p>", "<script>throw new Error('boom');</script>"),
    );
    let starter = dir.parent().unwrap().join("starter-made");
    let broken = project("t042_c4_broken", "broken", "<html></html>");
    for args in [
        vec!["check", text(&dir)],
        vec!["render", text(&dir)],
        vec!["render", text(&failing)],
        vec!["check", text(&broken)],
        vec!["init", text(&starter)],
        vec!["init", text(&starter)],
    ] {
        run_logged(&dir, &args, &logs, Some("trace"));
    }
    let text = std::fs::read_to_string(logs.join("framesmith.log")).unwrap();
    let mut seen_components = BTreeSet::new();
    let mut count = 0;
    for raw in text.lines() {
        count += 1;
        let line = Line::parse(raw).unwrap_or_else(|e| panic!("{e}"));
        // 2026-10-11T08:15:30.123Z
        let t = line.time.as_bytes();
        let shape_ok = t.len() == 24
            && t.iter().enumerate().all(|(i, &b)| match i {
                4 | 7 => b == b'-',
                10 => b == b'T',
                13 | 16 => b == b':',
                19 => b == b'.',
                23 => b == b'Z',
                _ => b.is_ascii_digit(),
            });
        assert!(shape_ok, "bad timestamp in {raw:?}");
        assert!(
            ["ERROR", "WARN", "INFO", "DEBUG", "TRACE"].contains(&line.level.as_str()),
            "bad level in {raw:?}"
        );
        assert!(
            ["cli", "render", "browser", "media"].contains(&line.component.as_str()),
            "bad component in {raw:?}"
        );
        assert!(
            line.run.len() == 12 && line.run.chars().all(|c| c.is_ascii_hexdigit()),
            "bad run id in {raw:?}"
        );
        assert!(!line.message.is_empty(), "no message in {raw:?}");
        seen_components.insert(line.component);
    }
    assert!(count > 20, "only {count} lines");
    assert_eq!(seen_components.len(), 4, "{seen_components:?}");
    // Lines are one per event: no message breaks a line in two.
    assert_eq!(text.lines().count(), text.matches('\n').count());
}

#[test]
fn t042_c5_with_warnings_only_a_successful_render_adds_no_informational_lines() {
    let (dir, logs) = tiny("t042_c5");
    for level in ["warn", "error"] {
        let ran = run_logged(&dir, &["render", text(&dir)], &logs, Some(level));
        assert_eq!(ran.code, 0, "{level}: {}", ran.all());
        let ran = run_logged(&dir, &["check", text(&dir)], &logs, Some(level));
        assert_eq!(ran.code, 0, "{level}: {}", ran.all());
    }
    let quiet = lines(&logs);
    assert!(
        quiet.iter().all(|line| line.level == "ERROR" || line.level == "WARN"),
        "an informational line was written"
    );
    assert!(
        quiet.is_empty(),
        "nothing at all was expected for good runs, got {} lines",
        quiet.len()
    );

    // The same render at the default level does write informational lines: the quiet result means something.
    let ran = run_logged(&dir, &["render", text(&dir)], &logs, None);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert!(lines(&logs).iter().filter(|line| line.level == "INFO").count() >= 10);

    // Errors still get through at the warning level.
    let failing = project(
        "t042_c5_failing",
        "failing",
        &page(SMALL, "", "<p>x</p>", "<script>throw new Error('boom');</script>"),
    );
    let logs = failing.parent().unwrap().join("logs");
    let ran = run_logged(&failing, &["render", text(&failing)], &logs, Some("warn"));
    assert_eq!(ran.code, 1, "{}", ran.all());
    let lines = lines(&logs);
    assert!(!lines.is_empty() && lines.iter().all(|line| line.level == "ERROR" || line.level == "WARN"));
    assert!(lines.iter().any(|line| line.level == "ERROR"));
}

#[test]
fn t042_c6_a_failed_render_has_an_error_line_with_the_stage_and_what_is_needed_to_reproduce_it() {
    let failing = project(
        "t042_c6",
        "failing",
        &page(SMALL, "", "<p>x</p>", "<script>throw new Error('boom');</script>"),
    );
    let logs = failing.parent().unwrap().join("logs");
    let ran = run_logged(&failing, &["render", text(&failing), "--fps", "24"], &logs, None);
    assert_eq!(ran.code, 1, "{}", ran.all());
    let lines = lines(&logs);
    let errors: Vec<&Line> = lines.iter().filter(|line| line.level == "ERROR").collect();
    assert!(!errors.is_empty());
    let stopped = errors
        .iter()
        .find(|line| line.event().starts_with("render stopped"))
        .expect("an error line from the render");
    assert_eq!(stopped.field("code").as_deref(), Some("FS-R007"));
    assert_eq!(stopped.field("stage").as_deref(), Some("probe"));
    // What is needed to run it again: the project, the frame rate of the run, and the browser.
    let canonical = std::fs::canonicalize(&failing).unwrap();
    assert_eq!(stopped.field("project").as_deref(), Some(text(&canonical)));
    assert_eq!(stopped.field("fps").as_deref(), Some("24"));
    assert_eq!(stopped.field("browser").as_deref(), Some(text(&browser())));
    let finished = lines.last().unwrap();
    assert_eq!(finished.label(), "cli command finished");
    assert_eq!(finished.level, "ERROR");
    assert_eq!(finished.field("exit").as_deref(), Some("1"));
    assert_eq!(finished.field("outcome").as_deref(), Some("failed"));
    assert_eq!(finished.field("codes").as_deref(), Some("FS-R007"));
    assert_eq!(finished.field("stage").as_deref(), Some("probe"));
    assert_eq!(runs(&lines).len(), 1);

    // A run stopped before any stage says the same: code, and "none" for the stage.
    let (dir, logs) = tiny("t042_c6_early");
    let nothing = dir.parent().unwrap().join("nothing");
    std::fs::create_dir_all(&nothing).unwrap();
    let path = nothing.as_os_str().to_owned();
    let env = [
        ("FRAMESMITH_LOG_DIR", logs.as_os_str()),
        ("PATH", path.as_os_str()),
        ("FRAMESMITH_BROWSER", nothing.as_os_str()),
    ];
    let ran = run_in(dir.parent().unwrap(), &["render", text(&dir)], &env);
    assert_eq!(ran.code, 1, "{}", ran.all());
    let lines = self::lines(&logs);
    let finished = lines.last().unwrap();
    assert_eq!((finished.level.as_str(), finished.field("exit").as_deref()), ("ERROR", Some("1")));
    // No ffmpeg is on this search path either, so both prerequisites are named.
    assert_eq!(finished.field("codes").as_deref(), Some("FS-R002,FS-R003"));
    assert_eq!(finished.field("stage").as_deref(), Some("none"));
    assert!(lines.iter().all(|line| line.event() != "stage started"));
}

/// A project whose text, style comments and script messages all hold the marker.
fn marked(root: &Path, name: &str, script: &str) -> PathBuf {
    let dir = root.join(name);
    write(
        &dir,
        "index.html",
        format!(
            "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<title>{MARKER} title</title>\n\
<link rel=\"stylesheet\" href=\"style.css\">\n<style>/* {MARKER} inline comment */ p {{ color: #fff; }}</style>\n</head>\n<body>\n\
<!-- {MARKER} html comment -->\n<div data-fs-composition=\"main\" {SMALL} data-note=\"{MARKER} attribute\">\n<p>{MARKER} visible text</p>\n</div>\n\
{script}\n</body>\n</html>\n"
        ),
    );
    write(
        &dir,
        "style.css",
        format!("/* {MARKER} sheet comment */\np::after {{ content: \"\"; }}\n"),
    );
    dir
}

#[test]
fn t042_c7_text_comments_and_script_messages_of_the_project_never_reach_the_log() {
    let root = scratch("t042_c7");
    let good = marked(
        &root,
        "good",
        &format!("<script>console.log('{MARKER} console'); console.warn('{MARKER} warning');</script>"),
    );
    let bad = marked(
        &root,
        "bad",
        &format!("<script>throw new Error('{MARKER} script message');</script>"),
    );
    for level in [None, Some("trace")] {
        let logs = root.join(format!("logs-{}", level.unwrap_or("default")));
        let check = run_logged(&good, &["check", text(&good)], &logs, level);
        assert_eq!(check.code, 0, "{}", check.all());
        let check_bad = run_logged(&bad, &["check", text(&bad)], &logs, level);
        assert_eq!(check_bad.code, 0, "{}", check_bad.all());
        let ok = run_logged(&good, &["render", text(&good)], &logs, level);
        assert_eq!(ok.code, 0, "{}", ok.all());
        assert!(good.join("out/good.mp4").is_file());
        let fail = run_logged(&bad, &["render", text(&bad), "--json"], &logs, level);
        assert_eq!(fail.code, 1, "{}", fail.all());
        // The script's own words really did reach the user: the marker was in play.
        assert!(
            fail.stderr.contains(MARKER),
            "the failure message does not show the script's text: {}",
            fail.stderr
        );
        let log = log_text(&logs);
        assert_eq!(log.matches("command started").count(), 4, "all four commands are in the log");
        assert!(!log.contains(MARKER), "the marker is in the log (level {level:?})");
        assert!(!log.to_lowercase().contains(&MARKER.to_lowercase()));
        for word in [
            "visible text",
            "inline comment",
            "sheet comment",
            "html comment",
            "script message",
            "console",
            "attribute",
        ] {
            assert!(!log.contains(word), "{word:?} is in the log");
        }
    }
}

fn only_unavailable(ran: &measure::Ran) {
    assert_eq!(ran.stdout.matches("logging is unavailable").count(), 0);
    assert_eq!(
        ran.stderr.matches("logging is unavailable: ").count(),
        1,
        "said once: {:?}",
        ran.stderr
    );
    assert!(
        ran.stderr.starts_with("logging is unavailable: ") && ran.stderr.lines().count() == 1,
        "{:?}",
        ran.stderr
    );
}

#[test]
fn t042_c8_a_log_that_cannot_be_written_still_gives_the_normal_result_and_is_said_once() {
    let (dir, good_logs) = tiny("t042_c8");
    let root = dir.parent().unwrap().to_path_buf();
    let normal_check = run_logged(&dir, &["check", text(&dir)], &good_logs, None);
    assert_eq!(normal_check.code, 0, "{}", normal_check.all());
    assert_eq!(normal_check.stderr, "");

    // A regular file where the folder should be, and a folder nobody can write to.
    let a_file = root.join("log-is-a-file");
    write(&root, "log-is-a-file", "not a folder");
    let read_only = root.join("read-only");
    std::fs::create_dir_all(&read_only).unwrap();
    std::fs::set_permissions(&read_only, std::fs::Permissions::from_mode(0o555)).unwrap();
    // A folder that cannot be made, below something that is not a folder.
    let below_a_file = a_file.join("deeper");

    for logs in [&a_file, &read_only, &below_a_file] {
        // check: the same words as with a log, exit 0.
        let ran = run_logged(&dir, &["check", text(&dir)], logs, None);
        assert_eq!((ran.code, &ran.stdout), (0, &normal_check.stdout), "{}", ran.all());
        only_unavailable(&ran);

        // check of a broken project: the same failing result, and one notice.
        let missing = root.join("nowhere");
        let ran = run_logged(&dir, &["check", text(&missing)], logs, None);
        assert_eq!(ran.code, 1, "{}", ran.all());
        assert!(ran.stdout.contains("error FS-E001"), "{}", ran.stdout);
        assert_eq!(ran.stderr.matches("logging is unavailable: ").count(), 1, "{}", ran.stderr);

        // render: the video is made, the picture is as it should be, and one notice.
        let video = root.join("video.mp4");
        let _ = std::fs::remove_file(&video);
        let ran = run_logged(&dir, &["render", text(&dir), "--output", text(&video)], logs, None);
        assert_eq!(ran.code, 0, "{}", ran.all());
        only_unavailable(&ran);
        assert!(ran.stdout.contains("Rendered 6 frames, 64x36, 30 fps"), "{}", ran.stdout);
        let facts = facts(&video);
        assert_eq!((facts.width, facts.height, facts.frames), (64, 36, 6));

        // init: the starter is written, and one notice.
        let made = root.join("made");
        let _ = std::fs::remove_dir_all(&made);
        let ran = run_logged(&dir, &["init", text(&made)], logs, None);
        assert_eq!(ran.code, 0, "{}", ran.all());
        only_unavailable(&ran);
        assert!(made.join("index.html").is_file() && made.join("style.css").is_file());
    }
    // Nothing was written into the unwritable places, and the folder is made writable again.
    assert_eq!(std::fs::read_dir(&read_only).unwrap().count(), 0);
    assert_eq!(std::fs::read_to_string(&a_file).unwrap(), "not a folder");
    std::fs::set_permissions(&read_only, std::fs::Permissions::from_mode(0o755)).unwrap();
}
