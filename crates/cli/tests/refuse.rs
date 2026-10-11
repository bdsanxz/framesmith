//! T028 to T032: a feature that is recognised but not built is listed by
//! `check` as unsupported, a name that is not recognised is listed as
//! unknown, and `render` refuses both with exit code 2 before it starts a
//! browser or `ffmpeg`.

mod common;

use common::*;
use measure::Ran;
use std::collections::BTreeSet;
use std::ffi::{OsStr, OsString};
use std::net::TcpListener;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------

/// Every code the support matrix lists as unsupported or unknown.
fn all_codes() -> BTreeSet<String> {
    let mut codes: BTreeSet<String> = (1..=21).map(|n| format!("FS-U{n:03}")).collect();
    codes.extend((1..=5).map(|n| format!("FS-X{n:03}")));
    codes
}

fn unsupported_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/unsupported")
}

/// The code a fixture folder stands for: `fs-u001-video-element` is `FS-U001`.
fn code_of(folder: &str) -> String {
    let mut parts = folder.splitn(3, '-');
    format!("{}-{}", parts.next().unwrap(), parts.next().unwrap()).to_ascii_uppercase()
}

fn fixture_folders() -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(unsupported_root())
        .unwrap()
        .map(|entry| entry.unwrap())
        .filter(|entry| entry.path().is_dir())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// The first line of every diagnostic in what a command printed: lines that
/// start at the left edge, are not blank and are not the result line.
fn headers(output: &str) -> Vec<&str> {
    output
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with(' ') && !line.starts_with("check "))
        .collect()
}

/// The `file:line` places of one code in a printed list, for the given kind word.
fn places<'a>(output: &'a str, kind: &str, code: &str) -> Vec<&'a str> {
    let start = format!("{kind} {code} ");
    headers(output)
        .into_iter()
        .filter_map(|line| line.strip_prefix(start.as_str()))
        .collect()
}

/// A place reads as `file:line` with a line number.
fn is_place(place: &str) -> bool {
    match place.rsplit_once(':') {
        Some((file, line)) => !file.is_empty() && !line.is_empty() && line.bytes().all(|b| b.is_ascii_digit()),
        None => false,
    }
}

fn kind_word(code: &str) -> &'static str {
    if code.starts_with("FS-X") {
        "unknown"
    } else {
        "unsupported"
    }
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

    fn browser_started(&self) -> bool {
        self.browser_marker.exists()
    }

    fn ffmpeg_started(&self) -> bool {
        self.ffmpeg_marker.exists()
    }
}

/// What `check` and `render` did with one project.
struct Both {
    check: Ran,
    render: Ran,
}

/// Runs `check`, then `render` twice (default output, and `--output` to a
/// place of our choosing), with the spies in place, and asserts everything
/// that holds for every refused project: exit 2 from both, the same list
/// printed, no browser, no `ffmpeg`, nothing written anywhere.
fn refused_project(key: &str, dir: PathBuf) -> Both {
    let spies = Spies::new(key);
    let before = tree(&dir);
    let check = run_in(dir.parent().unwrap(), &["check", text(&dir)], &[]);
    let render = spies.run(dir.parent().unwrap(), &["render", text(&dir)]);
    let chosen = dir.parent().unwrap().join("chosen").join("video.mp4");
    let render_to = spies.run(dir.parent().unwrap(), &["render", text(&dir), "--output", text(&chosen)]);

    assert_eq!(check.code, 2, "check: {}", check.all());
    assert_eq!(render.code, 2, "render: {}", render.all());
    assert_eq!(render_to.code, 2, "render --output: {}", render_to.all());
    // The same list: render prints on standard error what check prints on standard output.
    assert_eq!(render.stderr, check.stdout, "render must print the list that check printed");
    assert_eq!(render_to.stderr, check.stdout);
    assert!(!spies.browser_started(), "a browser was started");
    assert!(!spies.ffmpeg_started(), "ffmpeg was started");
    assert!(!dir.join("out").exists(), "the default output folder was created");
    assert!(
        !chosen.exists() && !chosen.parent().unwrap().exists(),
        "a file or folder appeared at the chosen output path"
    );
    assert_eq!(before, tree(&dir), "the project changed");
    Both { check, render }
}

fn refused_fixture(test: &str, folder: &str) -> Both {
    let key = format!("{test}_{folder}");
    refused_project(&key, fixture(&key, &format!("unsupported/{folder}")))
}

/// `refused_fixture`, and the places the code is listed at, found by `check`.
fn listed(test: &str, folder: &str) -> (Both, Vec<String>) {
    let both = refused_fixture(test, folder);
    let code = code_of(folder);
    let found: Vec<String> = places(&both.check.stdout, kind_word(&code), &code)
        .iter()
        .map(|p| p.to_string())
        .collect();
    assert!(!found.is_empty(), "{code} is not listed for {folder}:\n{}", both.check.stdout);
    assert!(found.iter().all(|place| is_place(place)), "{found:?}");
    (both, found)
}

fn read(dir: &Path, name: &str) -> String {
    std::fs::read_to_string(dir.join(name)).unwrap()
}

/// A project of one composition for the cases that need only a little.
fn small(test: &str, body: &str, rest: &str) -> PathBuf {
    project(
        test,
        "small",
        &page("data-fs-size=\"64x36\" data-fs-duration=\"0.2\"", "", body, rest),
    )
}

// ---------------------------------------------------------------------
// The fixtures as a whole
// ---------------------------------------------------------------------

#[test]
fn t028_to_t032_every_fixture_is_listed_by_check_and_refused_by_render() {
    let folders = fixture_folders();
    let codes: BTreeSet<String> = folders.iter().map(|folder| code_of(folder)).collect();
    assert_eq!(
        codes,
        all_codes(),
        "the fixtures must cover FS-U001 to FS-U021 and FS-X001 to FS-X005 exactly"
    );
    for folder in &folders {
        let code = code_of(folder);
        let kind = kind_word(&code);
        let both = refused_fixture("table", folder);
        for output in [&both.check.stdout, &both.render.stderr] {
            let found = places(output, kind, &code);
            assert!(!found.is_empty(), "{folder}: no `{kind} {code} <file>:<line>` line in\n{output}");
            assert!(found.iter().all(|place| is_place(place)), "{folder}: {found:?}");
        }
        // The fixture holds exactly that one use and nothing else.
        for line in headers(&both.check.stdout) {
            assert!(line.starts_with(&format!("{kind} {code} ")), "{folder} also lists: {line}");
        }
    }
}

#[test]
fn t028_c3_the_spies_do_see_a_browser_and_ffmpeg_when_a_project_renders() {
    // The control for "starts no browser and no ffmpeg": a valid project does start both.
    let spies = Spies::new("t028_control");
    let dir = small("t028_control_project", "<p>x</p>", "");
    let ran = spies.run(dir.parent().unwrap(), &["render", text(&dir)]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert!(spies.browser_started() && spies.ffmpeg_started());
}

// ---------------------------------------------------------------------
// T028: the mechanism, with video and audio
// ---------------------------------------------------------------------

#[test]
fn t028_c1_a_video_element_is_listed_as_unsupported() {
    let (both, found) = listed("t028_c1", "fs-u001-video-element");
    assert_eq!(found, ["index.html:9"]);
    let out = &both.check.stdout;
    assert!(out.contains("video is not supported in this version"), "{out}");
    assert!(out.contains("fix: "), "{out}");
    assert!(
        out.contains("check refused: 0 errors, 1 unsupported, 0 unknown, 0 warnings"),
        "{out}"
    );
}

#[test]
fn t028_c2_an_audio_element_is_listed_as_unsupported() {
    let (both, found) = listed("t028_c2", "fs-u002-audio-element");
    assert_eq!(found, ["index.html:9"]);
    assert!(
        both.check.stdout.contains("audio is not supported in this version"),
        "{}",
        both.check.stdout
    );
}

#[test]
fn t028_c3_render_prints_the_same_list_exits_2_and_starts_nothing() {
    for folder in ["fs-u001-video-element", "fs-u002-audio-element"] {
        let both = refused_fixture("t028_c3", folder);
        let code = code_of(folder);
        assert_eq!(places(&both.render.stderr, "unsupported", &code), ["index.html:9"], "{folder}");
        assert!(both.render.stderr.contains("not supported in this version"));
        // Nothing but the refusal was printed on standard output.
        assert!(both.render.stdout.trim().is_empty(), "{}", both.render.stdout);
    }
}

#[test]
fn t028_c3_render_has_no_option_to_render_anyway() {
    let dir = fixture("t028_c3_anyway", "unsupported/fs-u001-video-element");
    for flag in ["--force", "--ignore-unsupported", "--anyway"] {
        let ran = framesmith(&["render", text(&dir), flag]);
        assert_eq!(ran.code, 1, "{flag}: {}", ran.all());
        assert!(!dir.join("out").exists());
    }
}

#[test]
fn t028_c4_an_error_and_an_unsupported_use_are_both_listed_and_the_exit_is_1() {
    // A video element, and a frame rate that is not a number.
    let dir = project(
        "t028_c4",
        "both",
        &page(
            "data-fs-size=\"64x36\" data-fs-fps=\"fast\" data-fs-duration=\"1\"",
            "",
            "<video></video>",
            "",
        ),
    );
    let spies = Spies::new("t028_c4_spies");
    let check = framesmith(&["check", text(&dir)]);
    let render = spies.run(dir.parent().unwrap(), &["render", text(&dir)]);
    assert_eq!(check.code, 1, "{}", check.all());
    assert_eq!(render.code, 1, "{}", render.all());
    assert_eq!(places(&check.stdout, "error", "FS-E008").len(), 1, "{}", check.stdout);
    assert_eq!(places(&check.stdout, "unsupported", "FS-U001").len(), 1, "{}", check.stdout);
    assert!(
        check.stdout.contains("check failed: 1 error, 1 unsupported, 0 unknown, 0 warnings"),
        "{}",
        check.stdout
    );
    assert_eq!(render.stderr, check.stdout);
    assert!(!spies.browser_started() && !spies.ffmpeg_started());
    assert!(!dir.join("out").exists());
}

#[test]
fn t028_c5_only_warnings_and_no_unsupported_use_gives_exit_0() {
    // A clip that starts after the composition ends can never show: FS-W001.
    let dir = project(
        "t028_c5",
        "warned",
        &page(
            "data-fs-size=\"64x36\" data-fs-duration=\"1\"",
            "",
            "<p data-fs-clip data-fs-start=\"5\" data-fs-duration=\"1\">late</p>",
            "",
        ),
    );
    let check = framesmith(&["check", text(&dir)]);
    assert_eq!(check.code, 0, "{}", check.all());
    assert_eq!(places(&check.stdout, "warning", "FS-W001").len(), 1, "{}", check.stdout);
    assert!(check.stdout.contains("check passed"), "{}", check.stdout);
    assert!(!check.stdout.contains("unsupported FS-") && !check.stdout.contains("unknown FS-"));
    // The warning does not stop a render.
    let render = framesmith(&["render", text(&dir)]);
    assert_eq!(render.code, 0, "{}", render.all());
    assert!(dir.join("out/warned.mp4").is_file());
}

#[test]
fn t028_c6_removing_the_unsupported_element_lets_check_pass_and_render_proceed() {
    for (folder, element) in [
        ("fs-u001-video-element", "<video width=\"320\" height=\"180\"></video>"),
        ("fs-u002-audio-element", "<audio></audio>"),
    ] {
        let test = format!("t028_c6_{folder}");
        let dir = fixture(&test, &format!("unsupported/{folder}"));
        let code = code_of(folder);
        let before = framesmith(&["check", text(&dir)]);
        assert_eq!(before.code, 2, "{}", before.all());
        assert!(!places(&before.stdout, "unsupported", &code).is_empty());

        let page = read(&dir, "index.html");
        assert!(page.contains(element), "{folder}");
        write(&dir, "index.html", page.replace(element, ""));

        let after = framesmith(&["check", text(&dir)]);
        assert_eq!(after.code, 0, "{}", after.all());
        assert!(after.stdout.contains("check passed"), "{}", after.stdout);
        let render = framesmith(&["render", text(&dir)]);
        assert_eq!(render.code, 0, "{}", render.all());
        assert!(dir.join("out").join(format!("{folder}.mp4")).is_file(), "{folder}");
    }
}

#[test]
fn t028_c7_the_support_matrix_spec_exists_and_lists_every_code() {
    let matrix = spec("support-matrix.md");
    for code in all_codes() {
        assert!(matrix.contains(&format!("| {code} |")), "{code} is missing from the support matrix");
    }
    // Its kinds table gives each kind's effect on the exit code.
    assert!(matrix.contains("| unsupported |") && matrix.contains("| unknown |"));
    assert!(matrix.contains("Every entry has a fixture in `fixtures/unsupported/"));
}

// ---------------------------------------------------------------------
// T029: deferred timing and structure
// ---------------------------------------------------------------------

#[test]
fn t029_c1_a_clip_start_that_names_another_clip_is_unsupported() {
    let (both, found) = listed("t029_c1", "fs-u003-relative-start");
    assert_eq!(found, ["index.html:13"]);
    assert!(
        both.check.stdout.contains("relative starts are not supported in this version"),
        "{}",
        both.check.stdout
    );
}

#[test]
fn t029_c2_a_frame_rate_written_as_a_ratio_is_unsupported() {
    let (both, found) = listed("t029_c2", "fs-u004-ratio-rate");
    assert_eq!(found, ["index.html:11"]);
    assert!(
        both.check.stdout.contains("ratio frame rates are not supported in this version"),
        "{}",
        both.check.stdout
    );
}

#[test]
fn t029_c3_fps_given_a_ratio_exits_2_before_a_browser_and_says_so() {
    let dir = small("t029_c3", "<p>x</p>", "");
    let spies = Spies::new("t029_c3_spies");
    let ran = spies.run(dir.parent().unwrap(), &["render", text(&dir), "--fps", "30000/1001"]);
    assert_eq!(ran.code, 2, "{}", ran.all());
    assert!(ran.stderr.contains("unsupported FS-U004"), "{}", ran.stderr);
    assert!(
        ran.stderr.contains("ratio frame rates are not supported in this version"),
        "{}",
        ran.stderr
    );
    assert!(!spies.browser_started() && !spies.ffmpeg_started());
    assert!(!dir.join("out").exists());
    // A whole number is still accepted in the same place (the refusal is about the ratio only).
    let fine = spies.run(dir.parent().unwrap(), &["render", text(&dir), "--fps", "24"]);
    assert_eq!(fine.code, 0, "{}", fine.all());
}

#[test]
fn t029_c4_a_second_composition_is_listed_as_unsupported() {
    let (both, found) = listed("t029_c4_same_page", "fs-u005-second-composition");
    assert_eq!(found, ["index.html:11"]);
    assert!(
        both.check.stdout.contains("\"other\" is a second composition"),
        "{}",
        both.check.stdout
    );
    let (both, found) = listed("t029_c4_other_page", "fs-u005-other-page");
    assert_eq!(found, ["other.html:8"]);
    assert!(
        both.check.stdout.contains("other.html holds a composition"),
        "{}",
        both.check.stdout
    );
}

#[test]
fn t029_c5_a_nested_composition_is_listed_as_unsupported() {
    let (both, found) = listed("t029_c5", "fs-u006-nested-composition");
    assert_eq!(found, ["index.html:9"]);
    assert!(
        both.check.stdout.contains("nested inside another composition"),
        "{}",
        both.check.stdout
    );
}

#[test]
fn t029_c6_a_declared_variable_is_listed_as_unsupported() {
    let (both, found) = listed("t029_c6", "fs-u007-declared-variable");
    assert_eq!(found, ["index.html:8"]);
    assert!(
        both.check.stdout.contains("data-fs-var-title declares a variable"),
        "{}",
        both.check.stdout
    );
}

// ---------------------------------------------------------------------
// T030: animated images and network assets
// ---------------------------------------------------------------------

#[test]
fn t030_c1_an_image_that_points_at_an_animated_file_is_unsupported() {
    let (both, found) = listed("t030_c1", "fs-u008-animated-gif");
    assert_eq!(found, ["index.html:9"]);
    assert!(
        both.check.stdout.contains("animated images are not supported in this version"),
        "{}",
        both.check.stdout
    );
    // A one-frame GIF made the same way is a still image and is accepted.
    let dir = fixture("t030_c1_still", "unsupported/fs-u008-animated-gif");
    let ffmpeg = common::ffmpeg();
    let made = measure::run(
        &ffmpeg,
        &[
            "-y",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=c=red:s=16x16:d=1",
            "-frames:v",
            "1",
            text(&dir.join("blink.gif")),
        ],
        &dir,
        &[],
    );
    assert_eq!(made.code, 0, "{}", made.all());
    let check = framesmith(&["check", text(&dir)]);
    assert_eq!(check.code, 0, "{}", check.all());
}

#[test]
fn t030_c2_a_network_image_stylesheet_font_or_script_is_unsupported() {
    // One fixture each for an image, a stylesheet and a script.
    for (folder, place) in [
        ("fs-u009-network-image", "index.html:9"),
        ("fs-u009-network-stylesheet", "index.html:6"),
        ("fs-u009-network-script", "index.html:6"),
    ] {
        let (both, found) = listed("t030_c2", folder);
        assert_eq!(found, [place], "{folder}");
        assert!(
            both.check.stdout.contains("network assets are not supported in this version"),
            "{folder}"
        );
        assert!(both.check.stdout.contains("example.invalid"), "{folder}");
    }
    // All four kinds in one project are each listed, in the order of the page.
    let all = "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<title>All four</title>\n\
<link rel=\"stylesheet\" href=\"https://example.invalid/a.css\">\n\
<link rel=\"preload\" as=\"font\" href=\"https://example.invalid/b.woff2\">\n\
<script src=\"//example.invalid/c.js\"></script>\n</head>\n<body>\n\
<div data-fs-composition=\"main\" data-fs-size=\"64x36\" data-fs-duration=\"1\">\n\
<img src=\"http://example.invalid/d.png\" alt=\"\">\n</div>\n</body>\n</html>\n";
    let dir = project("t030_c2_all", "all", all);
    let both = refused_project("t030_c2_all_run", dir);
    let found = places(&both.check.stdout, "unsupported", "FS-U009");
    assert_eq!(found.len(), 4, "{}", both.check.stdout);
    for what in ["style sheet", "file \"https://example.invalid/b.woff2\"", "script", "image"] {
        assert!(both.check.stdout.contains(what), "no entry for {what}:\n{}", both.check.stdout);
    }
}

#[test]
fn t030_c3_a_network_font_or_image_in_a_project_stylesheet_names_the_stylesheet_and_line() {
    let (both, found) = listed("t030_c3_font", "fs-u009-network-font-in-stylesheet");
    assert_eq!(found, ["style.css:3"]);
    assert!(both.check.stdout.contains("remote.woff2"), "{}", both.check.stdout);
    let (both, found) = listed("t030_c3_image", "fs-u009-network-image-in-stylesheet");
    assert_eq!(found, ["style.css:4"]);
    assert!(both.check.stdout.contains("//example.invalid/picture.png"), "{}", both.check.stdout);
}

/// What a command printed, with the exit code, as one value to compare.
fn outcome(ran: &Ran) -> (i32, String, String) {
    (ran.code, ran.stdout.clone(), ran.stderr.clone())
}

/// Runs the binary under an inline macOS sandbox profile that denies every
/// network operation. (The named profile `no-network` is killed on start on
/// this machine, so the profile is written out.)
fn offline(cwd: &Path, args: &[&str], env: &[(&str, &OsStr)]) -> Ran {
    let mut all = vec!["-p", "(version 1)(allow default)(deny network*)", common::BIN];
    all.extend_from_slice(args);
    measure::run(Path::new("/usr/bin/sandbox-exec"), &all, cwd, env)
}

#[test]
fn t030_c4_the_results_are_the_same_with_the_network_denied() {
    // First prove the sandbox does deny the network: a local connection works
    // outside it and fails inside it.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let connect = format!("exec 3<>/dev/tcp/127.0.0.1/{port}");
    let open = measure::run(Path::new("/bin/bash"), &["-c", connect.as_str()], Path::new("/"), &[]);
    assert_eq!(open.code, 0, "the control connection must work outside the sandbox: {}", open.all());
    let denied = measure::run(
        Path::new("/usr/bin/sandbox-exec"),
        &[
            "-p",
            "(version 1)(allow default)(deny network*)",
            "/bin/bash",
            "-c",
            connect.as_str(),
        ],
        Path::new("/"),
        &[],
    );
    assert_ne!(denied.code, 0, "the sandbox did not deny the network");
    assert!(denied.stderr.contains("Operation not permitted"), "{}", denied.stderr);

    // Then the same commands, with and without it, on every project that names a network address.
    for folder in [
        "fs-u009-network-image",
        "fs-u009-network-stylesheet",
        "fs-u009-network-script",
        "fs-u009-network-font-in-stylesheet",
        "fs-u009-network-image-in-stylesheet",
    ] {
        let key = format!("t030_c4_{folder}");
        let dir = fixture(&key, &format!("unsupported/{folder}"));
        let cwd = dir.parent().unwrap();
        let normal_check = run_in(cwd, &["check", text(&dir)], &[]);
        let offline_check = offline(cwd, &["check", text(&dir)], &[]);
        assert_eq!(normal_check.code, 2, "{}", normal_check.all());
        assert_eq!(outcome(&normal_check), outcome(&offline_check), "{folder}: check differs offline");

        let normal_render = run_in(cwd, &["render", text(&dir)], &[]);
        let offline_render = offline(cwd, &["render", text(&dir)], &[]);
        assert_eq!(normal_render.code, 2, "{}", normal_render.all());
        assert_eq!(
            outcome(&normal_render),
            outcome(&offline_render),
            "{folder}: render differs offline"
        );
        assert_eq!(places(&offline_check.stdout, "unsupported", "FS-U009").len(), 1);
        assert!(!dir.join("out").exists(), "{folder}");
    }
    // The animated image is refused the same way offline.
    let dir = fixture("t030_c4_gif", "unsupported/fs-u008-animated-gif");
    let cwd = dir.parent().unwrap();
    assert_eq!(
        outcome(&run_in(cwd, &["check", text(&dir)], &[])),
        outcome(&offline(cwd, &["check", text(&dir)], &[]))
    );
}

// ---------------------------------------------------------------------
// T031: animation outside the Sprint 1 set
// ---------------------------------------------------------------------

#[test]
fn t031_c1_an_easing_outside_the_ten_is_unsupported() {
    let (both, found) = listed("t031_c1", "fs-u011-bounce-easing");
    assert_eq!(found, ["index.html:18"]);
    assert!(
        both.check.stdout.contains("\"out-bounce\", which is not supported in this version"),
        "{}",
        both.check.stdout
    );
    // Other recognised names outside the ten are refused the same way.
    for ease in [
        "in-out-sine",
        "in-quint",
        "out-expo",
        "in-circ",
        "out-back",
        "in-elastic",
        "steps(4)",
        "spring",
        "cubic-bezier(0.1,0.2,0.3,0.4)",
    ] {
        let key = format!("t031_c1_{}", ease.replace(['(', ')', ',', '.'], "_"));
        let dir = fixture(&key, "unsupported/fs-u011-bounce-easing");
        let page = read(&dir, "index.html").replace("out-bounce", ease);
        write(&dir, "index.html", page);
        let check = framesmith(&["check", text(&dir)]);
        assert_eq!(check.code, 2, "{ease}: {}", check.all());
        assert_eq!(places(&check.stdout, "unsupported", "FS-U011").len(), 1, "{ease}: {}", check.stdout);
    }
}

#[test]
fn t031_c2_a_property_outside_the_five_is_unsupported() {
    let (both, found) = listed("t031_c2", "fs-u012-colour-property");
    assert_eq!(found, ["index.html:18"]);
    assert!(
        both.check
            .stdout
            .contains("\"background-color\", which is not supported in this version"),
        "{}",
        both.check.stdout
    );
    for property in [
        "scale-x",
        "skew-y",
        "rotation-x",
        "z",
        "width",
        "height",
        "color",
        "border-radius",
        "blur",
        "x-percent",
    ] {
        let key = format!("t031_c2_{property}");
        let dir = fixture(&key, "unsupported/fs-u012-colour-property");
        let page = read(&dir, "index.html").replace("\"background-color\": \"#ff0000\"", &format!("\"{property}\": 1"));
        write(&dir, "index.html", page);
        let check = framesmith(&["check", text(&dir)]);
        assert_eq!(check.code, 2, "{property}: {}", check.all());
        assert_eq!(
            places(&check.stdout, "unsupported", "FS-U012").len(),
            1,
            "{property}: {}",
            check.stdout
        );
    }
}

#[test]
fn t031_c3_a_tween_option_outside_the_set_is_unsupported() {
    for (folder, option) in [("fs-u013-repeat-option", "repeat"), ("fs-u013-stagger-option", "stagger")] {
        let (both, found) = listed("t031_c3", folder);
        assert_eq!(found, ["index.html:18"], "{folder}");
        assert!(
            both.check.stdout.contains(&format!("the option \"{option}\"")),
            "{}",
            both.check.stdout
        );
    }
    let (both, found) = listed("t031_c3_block", "fs-u013-block-defaults");
    assert_eq!(found, ["index.html:17"]);
    assert!(both.check.stdout.contains("field \"defaults\""), "{}", both.check.stdout);
    // The other tween kinds, the block field `labels`, and the other options.
    let (both, found) = listed("t031_c3_kind", "fs-u010-keyframes-tween");
    assert_eq!(found, ["index.html:18"]);
    assert!(both.check.stdout.contains("\"keyframes\" tween"), "{}", both.check.stdout);
    let (both, _) = listed("t031_c3_path", "fs-u010-path-tween");
    assert!(both.check.stdout.contains("\"path\" tween"), "{}", both.check.stdout);
    for option in ["repeat-delay", "alternate", "delay", "label"] {
        let key = format!("t031_c3_{option}");
        let dir = fixture(&key, "unsupported/fs-u013-repeat-option");
        write(
            &dir,
            "index.html",
            read(&dir, "index.html").replace("\"repeat\": 2", &format!("\"{option}\": 1")),
        );
        let check = framesmith(&["check", text(&dir)]);
        assert_eq!(check.code, 2, "{option}: {}", check.all());
        assert_eq!(
            places(&check.stdout, "unsupported", "FS-U013").len(),
            1,
            "{option}: {}",
            check.stdout
        );
    }
}

#[test]
fn t031_c4_a_css_animation_or_transition_is_unsupported() {
    let (both, found) = listed("t031_c4_sheet_animation", "fs-u014-css-animation-in-stylesheet");
    assert_eq!(found, ["style.css:1", "style.css:2"]);
    assert!(both.check.stdout.contains("@keyframes slide"), "{}", both.check.stdout);
    let (_, found) = listed("t031_c4_attribute_animation", "fs-u014-css-animation-in-style-attribute");
    assert_eq!(found, ["index.html:12"]);
    let (_, found) = listed("t031_c4_sheet_transition", "fs-u015-css-transition-in-stylesheet");
    assert_eq!(found, ["style.css:1"]);
    let (both, found) = listed("t031_c4_attribute_transition", "fs-u015-css-transition-in-style-attribute");
    assert_eq!(found, ["index.html:12"]);
    assert!(
        both.check.stdout.contains("CSS transitions are not supported in this version"),
        "{}",
        both.check.stdout
    );
    // A rule in a style element in the page is found too, with its own line.
    let dir = project(
        "t031_c4_style_element",
        "styled",
        &page(
            "data-fs-size=\"64x36\" data-fs-duration=\"1\"",
            ".a {\n  transition: opacity 1s;\n}",
            "<p class=\"a\">x</p>",
            "",
        ),
    );
    let check = framesmith(&["check", text(&dir)]);
    assert_eq!(check.code, 2, "{}", check.all());
    assert_eq!(
        places(&check.stdout, "unsupported", "FS-U015"),
        ["index.html:8"],
        "{}",
        check.stdout
    );
    // `none` and a zero duration declare nothing and are accepted.
    let quiet = project(
        "t031_c4_none",
        "quiet",
        &page(
            "data-fs-size=\"64x36\" data-fs-duration=\"1\"",
            ".a { animation: none; transition: none; transition-duration: 0s; }",
            "<p class=\"a\">x</p>",
            "",
        ),
    );
    let check = framesmith(&["check", text(&quiet)]);
    assert_eq!(check.code, 0, "{}", check.all());
}

#[test]
fn t031_c5_a_script_that_calls_the_browsers_animation_interface_is_unsupported() {
    let (both, found) = listed("t031_c5_file", "fs-u016-browser-animation-interface");
    assert_eq!(found, ["script.js:2"]);
    assert!(both.check.stdout.contains(".animate("), "{}", both.check.stdout);
    // The same call in a script written in the page, and the other forms.
    for (name, code) in [
        ("inline", "document.body.animate([], 1);"),
        ("constructor", "const a = new Animation();"),
        ("effect", "const e = new KeyframeEffect(null, [], 1);"),
        ("getanimations", "const all = document.getAnimations();"),
        ("timeline", "const t = document.timeline;"),
    ] {
        let dir = small(&format!("t031_c5_{name}"), "<p>x</p>", &format!("<script>\n{code}\n</script>"));
        let check = framesmith(&["check", text(&dir)]);
        assert_eq!(check.code, 2, "{name}: {}", check.all());
        assert_eq!(places(&check.stdout, "unsupported", "FS-U016").len(), 1, "{name}: {}", check.stdout);
        let render = framesmith(&["render", text(&dir)]);
        assert_eq!(render.code, 2, "{name}: {}", render.all());
        assert!(!dir.join("out").exists());
    }
}

#[test]
fn t031_c5_the_script_interface_global_is_unsupported() {
    let (both, found) = listed("t031_c5_global", "fs-u017-script-interface");
    assert_eq!(found, ["script.js:1"]);
    assert!(both.check.stdout.contains("framesmith script interface"), "{}", both.check.stdout);
}

#[test]
fn t031_c6_removing_the_listed_uses_lets_check_pass() {
    // (fixture, file, text to take out, what to put there)
    let cases: [(&str, &str, &str, &str); 9] = [
        ("fs-u010-keyframes-tween", "index.html", ", \"keyframes\": [0, 50, 100]", ""),
        ("fs-u011-bounce-easing", "index.html", "out-bounce", "out-cubic"),
        (
            "fs-u012-colour-property",
            "index.html",
            "\"background-color\": \"#ff0000\"",
            "\"opacity\": 0.5",
        ),
        ("fs-u013-repeat-option", "index.html", ", \"repeat\": 2", ""),
        (
            "fs-u014-css-animation-in-style-attribute",
            "index.html",
            " style=\"animation-name: slide\"",
            "",
        ),
        (
            "fs-u015-css-transition-in-style-attribute",
            "index.html",
            " style=\"transition-duration: 0.5s\"",
            "",
        ),
        ("fs-u015-css-transition-in-stylesheet", "style.css", " transition: left 1s;", ""),
        (
            "fs-u016-browser-animation-interface",
            "script.js",
            "box.animate([{ opacity: 0 }, { opacity: 1 }], { duration: 1000 });",
            "box.title = \"box\";",
        ),
        (
            "fs-u014-css-animation-in-stylesheet",
            "style.css",
            "animation: slide 1s linear;",
            "",
        ),
    ];
    for (folder, file, from, to) in cases {
        let key = format!("t031_c6_{folder}");
        let dir = fixture(&key, &format!("unsupported/{folder}"));
        let before = framesmith(&["check", text(&dir)]);
        assert_eq!(before.code, 2, "{folder}: {}", before.all());
        let mut source = read(&dir, file);
        assert!(source.contains(from), "{folder}: {from} not found");
        source = source.replace(from, to);
        if folder == "fs-u014-css-animation-in-stylesheet" {
            // The rule that defines the animation goes too.
            source = source.split("@keyframes").next().unwrap().to_string();
        }
        write(&dir, file, source);
        let after = framesmith(&["check", text(&dir)]);
        assert_eq!(after.code, 0, "{folder}: {}", after.all());
        assert!(after.stdout.contains("check passed"), "{folder}: {}", after.stdout);
    }
}

// ---------------------------------------------------------------------
// T032: nothing unrecognised is ignored
// ---------------------------------------------------------------------

#[test]
fn t032_c1_an_unrecognised_data_fs_attribute_is_unknown() {
    let (both, found) = listed("t032_c1", "fs-x001-unknown-attribute");
    assert_eq!(found, ["index.html:12"]);
    assert!(
        both.check.stdout.contains("data-fs-strat is not one Framesmith knows"),
        "{}",
        both.check.stdout
    );
    assert!(
        both.check
            .stdout
            .contains("check refused: 0 errors, 0 unsupported, 1 unknown, 0 warnings"),
        "{}",
        both.check.stdout
    );
}

#[test]
fn t032_c2_an_unrecognised_tween_field_or_block_field_is_unknown() {
    let (both, found) = listed("t032_c2", "fs-x002-unknown-tween-field");
    assert_eq!(found, ["index.html:18"]);
    assert!(both.check.stdout.contains("unknown field \"eas\""), "{}", both.check.stdout);
    // A field of the block itself.
    let motion_block = motion("{ \"target\": \"p\", \"to\": { \"x\": 1 }, \"at\": 0, \"duration\": 1 }")
        .replace("[{", "{ \"twens\": 1, \"tweens\": [{")
        .replace("}]", "}] }");
    let dir = small("t032_c2_block", "<p>x</p>", &motion_block);
    let check = framesmith(&["check", text(&dir)]);
    assert_eq!(check.code, 2, "{}", check.all());
    assert_eq!(places(&check.stdout, "unknown", "FS-X002").len(), 1, "{}", check.stdout);
    assert!(check.stdout.contains("unknown field \"twens\""), "{}", check.stdout);
    let render = framesmith(&["render", text(&dir)]);
    assert_eq!(render.code, 2, "{}", render.all());
    assert!(!dir.join("out").exists());
}

#[test]
fn t032_c3_an_unrecognised_tween_kind_property_or_easing_is_unknown() {
    let (both, found) = listed("t032_c3_kind", "fs-x003-unknown-kind");
    assert_eq!(found, ["index.html:18"]);
    assert!(both.check.stdout.contains("unknown kind \"tween\""), "{}", both.check.stdout);
    let (both, found) = listed("t032_c3_property", "fs-x004-unknown-property");
    assert_eq!(found, ["index.html:18"]);
    assert!(both.check.stdout.contains("unknown property \"opacty\""), "{}", both.check.stdout);
    let (both, found) = listed("t032_c3_easing", "fs-x005-unknown-easing");
    assert_eq!(found, ["index.html:18"]);
    assert!(both.check.stdout.contains("unknown easing \"out-quadd\""), "{}", both.check.stdout);
}

#[test]
fn t032_c4_ordinary_and_foreign_data_attributes_are_left_alone() {
    let body = "<p id=\"a\" class=\"b\" title=\"t\" lang=\"en\" role=\"note\" aria-label=\"l\" tabindex=\"0\" hidden \
data-id=\"7\" data-framesmith=\"x\" data-fsx=\"y\" data-f-s=\"z\" data-fs=\"w\" data-custom-thing=\"1\">x</p>";
    let dir = small("t032_c4", body, "");
    let check = framesmith(&["check", text(&dir)]);
    assert_eq!(check.code, 0, "{}", check.all());
    assert!(check.stdout.contains("check passed"), "{}", check.stdout);
    assert!(headers(&check.stdout).is_empty(), "{}", check.stdout);
    for word in [
        "data-id",
        "data-fsx",
        "data-custom-thing",
        "data-framesmith",
        "unknown",
        "unsupported",
    ] {
        assert!(!check.stdout.contains(word), "{word} is mentioned:\n{}", check.stdout);
    }
    let render = framesmith(&["render", text(&dir)]);
    assert_eq!(render.code, 0, "{}", render.all());
}
