//! T043 and T044: `render --json` answers with exactly one JSON document on
//! standard output for every outcome, and the exit code matches the outcome.

mod common;

use common::*;
use measure::Ran;
use serde_json::Value;
use std::ffi::OsStr;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const KILL: &str = "/bin/kill";
const PGREP: &str = "/usr/bin/pgrep";

fn composition(test: &str, attrs: &str) -> PathBuf {
    project(test, "demo", &page(&format!("data-fs-size=\"64x36\" {attrs}"), "", "<p>x</p>", ""))
}

/// Standard output holds one line that is one JSON document and nothing else.
fn only_document(ran: &Ran) -> Value {
    assert!(ran.stdout.ends_with('\n'), "{:?}", ran.stdout);
    assert_eq!(ran.stdout.lines().count(), 1, "{:?}", ran.stdout);
    let mut stream = serde_json::Deserializer::from_str(&ran.stdout).into_iter::<Value>();
    let value = stream
        .next()
        .expect("a document")
        .unwrap_or_else(|e| panic!("{e}: {:?}", ran.stdout));
    assert_eq!(
        stream.byte_offset(),
        ran.stdout.trim_end().len(),
        "text after the document: {:?}",
        ran.stdout
    );
    assert!(stream.next().is_none(), "more than one document: {:?}", ran.stdout);
    assert!(value.is_object());
    value
}

/// A document that is not a success: it has a run id and no output path.
fn assert_not_success(doc: &Value) {
    let id = doc["run_id"].as_str().unwrap_or_else(|| panic!("no run id in {doc}"));
    assert!(id.len() == 12 && id.bytes().all(|b| b.is_ascii_hexdigit()), "{id}");
    assert!(doc.get("output").is_none(), "an output path in {doc}");
    assert_ne!(doc["outcome"], "completed", "{doc}");
}

/// Runs the command with a cleared environment: only what is listed is set.
fn run_clear(dir: &Path, args: &[&str], env: &[(&str, &OsStr)]) -> Ran {
    use std::os::unix::process::ExitStatusExt;
    let output = Command::new(BIN)
        .args(args)
        .current_dir(dir)
        .env_clear()
        .envs(env.iter().copied())
        .stdin(Stdio::null())
        .output()
        .unwrap();
    Ran {
        code: output
            .status
            .code()
            .or_else(|| output.status.signal().map(|s| 128 + s))
            .unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

/// The `file:line` places a list of diagnostics names.
fn diagnostics(doc: &Value) -> &Vec<Value> {
    doc["diagnostics"].as_array().unwrap_or_else(|| panic!("no diagnostics in {doc}"))
}

fn assert_diagnostic_fields(item: &Value) {
    for key in ["code", "kind", "file", "description", "remedy"] {
        assert!(item[key].as_str().is_some_and(|s| !s.is_empty()), "{key} in {item}");
    }
    assert!(item["line"].as_u64().is_some_and(|n| n > 0), "line in {item}");
}

// ---------------------------------------------------------------------
// T043
// ---------------------------------------------------------------------

#[test]
fn t043_c1_standard_output_is_exactly_one_json_document() {
    let dir = composition("t043_c1", "data-fs-fps=\"10\" data-fs-duration=\"1\"");
    let ran = run_in(dir.parent().unwrap(), &["render", "demo", "--json"], &[]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert!(ran.stdout.starts_with('{') && ran.stdout.ends_with("}\n"), "{:?}", ran.stdout);
    let doc = only_document(&ran);
    assert_eq!(doc["outcome"], "completed");
}

#[test]
fn t043_c2_the_document_gives_the_outcome_and_the_facts_of_the_file() {
    // 1.5 s at 10 fps: a length that is not a whole number of seconds.
    let dir = composition("t043_c2", "data-fs-fps=\"10\" data-fs-duration=\"1.5\"");
    let ran = run_in(dir.parent().unwrap(), &["render", "demo", "--json"], &[]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    let doc = only_document(&ran);
    assert_eq!(doc["outcome"], "completed");
    let mut keys: Vec<&str> = doc.as_object().unwrap().keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        ["duration", "fps", "frames", "height", "outcome", "output", "run_id", "width"]
    );

    let output = Path::new(doc["output"].as_str().expect("an output path"));
    assert!(output.is_absolute());
    assert_eq!(
        std::fs::canonicalize(output).unwrap(),
        std::fs::canonicalize(dir.join("out/demo.mp4")).unwrap()
    );
    let video = facts(output);
    assert_eq!(doc["width"].as_u64(), Some(u64::from(video.width)));
    assert_eq!(doc["height"].as_u64(), Some(u64::from(video.height)));
    assert_eq!(doc["frames"].as_u64(), Some(video.frames));
    assert_eq!(video.frame_rate, format!("{}/1", doc["fps"]));
    assert!(
        (doc["duration"].as_f64().unwrap() - video.duration).abs() < 0.001,
        "{doc} against {}",
        video.duration
    );
    assert_eq!((video.width, video.height, video.frames), (64, 36, 15));
}

#[test]
fn t043_c3_the_run_id_is_the_one_on_every_line_the_run_wrote_to_the_log() {
    let dir = composition("t043_c3", "data-fs-fps=\"10\" data-fs-duration=\"1\"");
    let logs = scratch("t043_c3_logs");
    let ran = run_in(
        dir.parent().unwrap(),
        &["render", "demo", "--json"],
        &[("FRAMESMITH_LOG_DIR", logs.as_os_str())],
    );
    assert_eq!(ran.code, 0, "{}", ran.all());
    let id = only_document(&ran)["run_id"].as_str().unwrap().to_string();
    let log = std::fs::read_to_string(logs.join("framesmith.log")).unwrap();
    let lines: Vec<&str> = log.lines().collect();
    assert!(lines.len() >= 5, "the run wrote {} lines", lines.len());
    let mark = format!("run={id}");
    let own = lines.iter().filter(|line| line.split_whitespace().any(|word| word == mark)).count();
    assert_eq!(own, lines.len(), "every line of the run carries run={id}");
    assert!(lines.iter().any(|line| line.contains("command finished")));
}

#[test]
fn t043_c4_stage_and_frame_progress_stays_off_standard_output() {
    let dir = composition("t043_c4", "data-fs-fps=\"10\" data-fs-duration=\"1\"");
    let ran = run_in(dir.parent().unwrap(), &["render", "demo", "--json"], &[]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    only_document(&ran);
    for word in ["compile", "probe", "capture", "encode", "commit", "Rendered", "/10"] {
        assert!(!ran.stdout.contains(word), "{word} on standard output: {:?}", ran.stdout);
    }
    // Progress went to standard error instead.
    for line in [
        "compile   done",
        "probe     done      64x36, 10 fps, 1 s, 10 frames",
        "capture   10/10",
        "encode    done",
    ] {
        assert!(
            ran.stderr.lines().any(|l| l == line),
            "{line} not on standard error: {:?}",
            ran.stderr
        );
    }
    assert!(ran.stderr.contains("Rendered 10 frames"));
}

#[test]
fn t043_c5_with_fps_24_the_document_gives_24_as_the_frame_rate() {
    let dir = composition("t043_c5", "data-fs-fps=\"10\" data-fs-duration=\"1\"");
    let ran = run_in(dir.parent().unwrap(), &["render", "demo", "--json", "--fps", "24"], &[]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    let doc = only_document(&ran);
    assert_eq!(
        (doc["outcome"].as_str(), doc["fps"].as_u64(), doc["frames"].as_u64()),
        (Some("completed"), Some(24), Some(24))
    );
    let video = facts(&dir.join("out/demo.mp4"));
    assert_eq!((video.frame_rate.as_str(), video.frames), ("24/1", 24));
}

// ---------------------------------------------------------------------
// T044
// ---------------------------------------------------------------------

#[test]
fn t044_c1_a_project_with_an_error_gives_failed_with_the_diagnostics_and_exit_1() {
    // Odd width.
    let dir = project(
        "t044_c1",
        "demo",
        &page(
            "data-fs-size=\"65x36\" data-fs-fps=\"10\" data-fs-duration=\"1\"",
            "",
            "<p>x</p>",
            "",
        ),
    );
    let ran = run_in(dir.parent().unwrap(), &["render", "demo", "--json"], &[]);
    assert_eq!(ran.code, 1, "{}", ran.all());
    let doc = only_document(&ran);
    assert_eq!(doc["outcome"], "failed");
    assert_not_success(&doc);
    let list = diagnostics(&doc);
    assert!(!list.is_empty());
    list.iter().for_each(assert_diagnostic_fields);
    assert!(list.iter().all(|item| item["kind"] == "error"), "{doc}");
    assert!(
        list.iter().any(|item| item["code"] == "FS-E007" && item["file"] == "index.html"),
        "{doc}"
    );
    assert!(!dir.join("out").exists());
}

#[test]
fn t044_c2_an_unsupported_feature_gives_refused_with_each_use_and_exit_2() {
    // Two uses in one project, a video element and an audio element: each is listed.
    let dir = project(
        "t044_c2_two",
        "demo",
        &page(
            "data-fs-size=\"64x36\" data-fs-fps=\"10\" data-fs-duration=\"1\"",
            "",
            "<video src=\"a.mp4\"></video>\n<audio src=\"b.mp3\"></audio>",
            "",
        ),
    );
    let ran = run_in(dir.parent().unwrap(), &["render", "demo", "--json"], &[]);
    assert_eq!(ran.code, 2, "{}", ran.all());
    let doc = only_document(&ran);
    assert_eq!(doc["outcome"], "refused");
    assert_not_success(&doc);
    let list = diagnostics(&doc);
    list.iter().for_each(assert_diagnostic_fields);
    let places: Vec<(&str, u64)> = list
        .iter()
        .map(|item| (item["code"].as_str().unwrap(), item["line"].as_u64().unwrap()))
        .collect();
    assert!(
        places.iter().any(|p| p.0 == "FS-U001") && places.iter().any(|p| p.0 == "FS-U002"),
        "{places:?}"
    );
    assert!(
        list.iter()
            .all(|item| item["kind"] == "unsupported" && item["file"] == "index.html"),
        "{doc}"
    );
    assert!(!dir.join("out").exists());
}

#[test]
fn t044_c2_every_unsupported_and_unknown_fixture_is_refused_with_its_code_and_place() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/unsupported");
    let mut folders: Vec<String> = std::fs::read_dir(&root)
        .unwrap()
        .map(|entry| entry.unwrap())
        .filter(|entry| entry.path().is_dir())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    folders.sort();
    let mut covered = std::collections::BTreeSet::new();
    for folder in folders {
        let code = {
            let mut parts = folder.splitn(3, '-');
            format!("{}-{}", parts.next().unwrap(), parts.next().unwrap()).to_ascii_uppercase()
        };
        let dir = fixture("t044_c2_fixtures", &format!("unsupported/{folder}"));
        let ran = run_in(dir.parent().unwrap(), &["render", text(&dir), "--json"], &[]);
        assert_eq!(ran.code, 2, "{folder}: {}", ran.all());
        let doc = only_document(&ran);
        assert_eq!(doc["outcome"], "refused", "{folder}");
        assert_not_success(&doc);
        let list = diagnostics(&doc);
        list.iter().for_each(assert_diagnostic_fields);
        let kind = if code.starts_with("FS-X") { "unknown" } else { "unsupported" };
        assert!(
            list.iter().any(|item| item["code"] == code.as_str() && item["kind"] == kind),
            "{folder}: {doc}"
        );
        assert!(!dir.join("out").exists(), "{folder} wrote an output");
        covered.insert(code);
    }
    // Every code the support matrix lists is among them.
    let listed: std::collections::BTreeSet<String> = (1..=21)
        .map(|n| format!("FS-U{n:03}"))
        .chain((1..=5).map(|n| format!("FS-X{n:03}")))
        .collect();
    assert_eq!(covered, listed);
}

/// A folder that holds only a link to ffmpeg, to be the whole search path.
fn only_ffmpeg(test: &str) -> PathBuf {
    let bin = scratch(test).join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::os::unix::fs::symlink(ffmpeg(), bin.join("ffmpeg")).unwrap();
    bin
}

fn browser_in_applications() -> bool {
    let places = browser::Places {
        applications: vec![PathBuf::from("/Applications")],
        ..browser::Places::default()
    };
    browser::find(&places).is_ok()
}

fn assert_missing(doc: &Value, program: &str, code: &str) {
    assert_eq!(doc["outcome"], "failed", "{doc}");
    assert_not_success(doc);
    let error = &doc["error"];
    assert_eq!(
        (error["program"].as_str(), error["code"].as_str()),
        (Some(program), Some(code)),
        "{doc}"
    );
    assert!(error["description"]
        .as_str()
        .is_some_and(|s| s.to_lowercase().contains(if program == "browser" { "browser" } else { "ffmpeg" })));
    assert!(error["remedy"].as_str().is_some_and(|s| s.starts_with("Install ")), "{doc}");
    assert!(doc.get("diagnostics").is_none());
}

#[test]
fn t044_c3_a_missing_browser_or_ffmpeg_gives_failed_with_the_program_and_the_fix() {
    let dir = composition("t044_c3", "data-fs-fps=\"10\" data-fs-duration=\"1\"");
    let cwd = dir.parent().unwrap();
    let home = scratch("t044_c3_home");
    let empty = scratch("t044_c3_empty");

    // No ffmpeg: the browser is named, the search path holds nothing.
    let browser = browser();
    let ran = run_clear(
        cwd,
        &["render", "demo", "--json"],
        &[
            ("HOME", home.as_os_str()),
            ("PATH", empty.as_os_str()),
            ("FRAMESMITH_BROWSER", browser.as_os_str()),
        ],
    );
    assert_eq!(ran.code, 1, "{}", ran.all());
    let doc = only_document(&ran);
    assert_missing(&doc, "ffmpeg", "FS-R003");
    assert!(doc.get("errors").is_none());
    assert!(!dir.join("out").exists());

    // No browser: an empty home, no named browser, a search path with only ffmpeg.
    if browser_in_applications() {
        eprintln!("a browser is installed in /Applications: the no-browser case cannot be simulated here");
        return;
    }
    let bin = only_ffmpeg("t044_c3_bin");
    let ran = run_clear(
        cwd,
        &["render", "demo", "--json"],
        &[("HOME", home.as_os_str()), ("PATH", bin.as_os_str())],
    );
    assert_eq!(ran.code, 1, "{}", ran.all());
    let doc = only_document(&ran);
    assert_missing(&doc, "browser", "FS-R001");
    assert!(doc.get("errors").is_none());

    // Neither: the error is the browser, and both are listed.
    let ran = run_clear(
        cwd,
        &["render", "demo", "--json"],
        &[("HOME", home.as_os_str()), ("PATH", empty.as_os_str())],
    );
    assert_eq!(ran.code, 1, "{}", ran.all());
    let doc = only_document(&ran);
    assert_missing(&doc, "browser", "FS-R001");
    let both: Vec<&str> = doc["errors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["program"].as_str().unwrap())
        .collect();
    assert_eq!(both, ["browser", "ffmpeg"], "{doc}");
    assert!(!dir.join("out").exists());
}

/// Starts a render of a project that takes several seconds to capture.
fn start_long(test: &str, extra_env: &[(&str, &OsStr)]) -> (std::process::Child, PathBuf) {
    // 300 frames of a small picture: about ten seconds.
    let dir = composition(test, "data-fs-fps=\"30\" data-fs-duration=\"10\"");
    let child = Command::new(BIN)
        .args(["render", "demo", "--json"])
        .current_dir(dir.parent().unwrap())
        .envs(extra_env.iter().copied())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    (child, dir)
}

fn pids_of_children(pid: u32) -> Vec<String> {
    let found = measure::run(Path::new(PGREP), &["-P", &pid.to_string()], Path::new("/"), &[]);
    found.stdout.split_whitespace().map(str::to_string).collect()
}

#[test]
fn t044_c4_c6_when_the_browser_is_stopped_during_the_run_the_document_names_the_program_stage_and_frame() {
    let frames = scratch("t044_c4_frames");
    let (mut child, dir) = start_long("t044_c4", &[("FRAMESMITH_DEBUG_FRAMES", frames.as_os_str())]);
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let read_out = std::thread::spawn(move || {
        let mut text = String::new();
        BufReader::new(stdout).read_to_string(&mut text).unwrap();
        text
    });
    let read_err = std::thread::spawn(move || {
        let mut text = String::new();
        BufReader::new(stderr).read_to_string(&mut text).unwrap();
        text
    });
    let began = Instant::now();
    let captured = || std::fs::read_dir(&frames).map(Iterator::count).unwrap_or(0);
    while captured() < 5 {
        assert!(began.elapsed() < Duration::from_secs(60), "no frame was captured");
        std::thread::sleep(Duration::from_millis(5));
    }
    // The browser is the child with the pipe flag on its command line.
    let own = child.id().to_string();
    let found = measure::run(Path::new(PGREP), &["-P", &own, "-f", "remote-debugging-pipe"], Path::new("/"), &[]);
    let browser_pid = found
        .stdout
        .split_whitespace()
        .next()
        .expect("the browser is a child of the command")
        .to_string();
    let at_kill = captured();
    measure::run(Path::new(KILL), &["-KILL", &browser_pid], Path::new("/"), &[]);
    let status = child.wait().unwrap();
    let ran = Ran {
        code: status.code().unwrap_or(-1),
        stdout: read_out.join().unwrap(),
        stderr: read_err.join().unwrap(),
    };
    assert_eq!(ran.code, 1, "{}", ran.all());
    let doc = only_document(&ran);
    assert_eq!(doc["outcome"], "failed", "{doc}");
    assert_not_success(&doc);
    let error = &doc["error"];
    assert_eq!(error["program"], "browser", "{doc}");
    assert_eq!(error["stage"], "capture", "{doc}");
    let reached = error["frame"].as_u64().unwrap_or_else(|| panic!("no frame in {doc}"));
    assert!((5..=300).contains(&reached), "frame {reached}");
    assert!(
        reached.abs_diff(at_kill as u64) <= 10,
        "frame {reached}, {at_kill} files when the browser was stopped"
    );
    assert_eq!(error["code"], "FS-R011");
    assert!(error["description"].as_str().is_some_and(|s| !s.is_empty()));
    assert!(error["remedy"].as_str().is_some_and(|s| !s.is_empty()));
    assert!(!dir.join("out").exists());
    // Nothing of the run is left running.
    assert!(pids_of_children(child.id()).is_empty());
    assert!(!browser_left(doc["run_id"].as_str().unwrap()));
}

#[test]
fn t044_c5_c6_an_interrupted_render_gives_cancelled_with_the_stage_and_exit_130() {
    let (mut child, dir) = start_long("t044_c5", &[("FRAMESMITH_DEBUG_HOLD", OsStr::new("capture"))]);
    let mut stdout = child.stdout.take().unwrap();
    let mut stderr = BufReader::new(child.stderr.take().unwrap());
    let read_out = std::thread::spawn(move || {
        let mut text = String::new();
        stdout.read_to_string(&mut text).unwrap();
        text
    });
    // Wait until the run says it is holding at the start of the capture.
    let mut seen = String::new();
    loop {
        let mut line = String::new();
        let n = stderr.read_line(&mut line).unwrap();
        assert!(n > 0, "the run ended before it held in capture: {seen}");
        seen.push_str(&line);
        if line.contains("holding in capture") {
            break;
        }
    }
    measure::run(Path::new(KILL), &["-INT", &child.id().to_string()], Path::new("/"), &[]);
    let mut rest = String::new();
    stderr.read_to_string(&mut rest).unwrap();
    let status = child.wait().unwrap();
    let ran = Ran {
        code: status.code().unwrap_or(-1),
        stdout: read_out.join().unwrap(),
        stderr: format!("{seen}{rest}"),
    };
    assert_eq!(ran.code, 130, "{}", ran.all());
    let doc = only_document(&ran);
    assert_eq!(doc["outcome"], "cancelled", "{doc}");
    assert_eq!(doc["stage"], "capture", "{doc}");
    assert_not_success(&doc);
    assert!(!dir.join("out").exists());
    assert!(!browser_left(doc["run_id"].as_str().unwrap()));
}

#[test]
fn t044_c6_every_other_document_has_a_run_id_and_no_output_and_nothing_else_on_standard_output() {
    let cwd_of = |dir: &Path| dir.parent().unwrap().to_path_buf();
    let mut documents = Vec::new();

    let invalid = project(
        "t044_c6_invalid",
        "demo",
        &page("data-fs-size=\"65x36\" data-fs-duration=\"1\"", "", "<p>x</p>", ""),
    );
    documents.push((1, run_in(&cwd_of(&invalid), &["render", "demo", "--json"], &[])));

    let refused = fixture("t044_c6_refused", "unsupported/fs-u001-video-element");
    documents.push((2, run_in(&cwd_of(&refused), &["render", text(&refused), "--json"], &[])));

    let ok = composition("t044_c6_prereq", "data-fs-fps=\"10\" data-fs-duration=\"1\"");
    let empty = scratch("t044_c6_empty");
    let browser = browser();
    documents.push((
        1,
        run_clear(
            &cwd_of(&ok),
            &["render", "demo", "--json"],
            &[
                ("HOME", empty.as_os_str()),
                ("PATH", empty.as_os_str()),
                ("FRAMESMITH_BROWSER", browser.as_os_str()),
            ],
        ),
    ));
    documents.push((
        1,
        run_clear(
            &cwd_of(&ok),
            &["render", "demo", "--json"],
            &[("HOME", empty.as_os_str()), ("PATH", empty.as_os_str())],
        ),
    ));

    // A command line that cannot be read, with --json.
    documents.push((1, run_in(&cwd_of(&ok), &["render", "demo", "--json", "--fps", "0"], &[])));

    // The run is stopped before the capture begins by an interrupt held in the probe.
    let (mut child, dir) = start_long("t044_c6_cancel", &[("FRAMESMITH_DEBUG_HOLD", OsStr::new("probe"))]);
    let mut stdout = child.stdout.take().unwrap();
    let mut stderr = BufReader::new(child.stderr.take().unwrap());
    let read_out = std::thread::spawn(move || {
        let mut text = String::new();
        stdout.read_to_string(&mut text).unwrap();
        text
    });
    let mut line = String::new();
    while !line.contains("holding in probe") {
        line.clear();
        assert!(stderr.read_line(&mut line).unwrap() > 0, "the run ended before it held in probe");
    }
    measure::run(Path::new(KILL), &["-INT", &child.id().to_string()], Path::new("/"), &[]);
    let mut rest = String::new();
    stderr.read_to_string(&mut rest).unwrap();
    let code = child.wait().unwrap().code().unwrap_or(-1);
    documents.push((
        130,
        Ran {
            code,
            stdout: read_out.join().unwrap(),
            stderr: rest,
        },
    ));
    assert!(!dir.join("out").exists());

    for (code, ran) in documents {
        assert_eq!(ran.code, code, "{}", ran.all());
        let doc = only_document(&ran);
        assert_not_success(&doc);
        let expected = match code {
            2 => "refused",
            130 => "cancelled",
            _ => "failed",
        };
        assert_eq!(doc["outcome"], expected, "{doc}");
    }
}
