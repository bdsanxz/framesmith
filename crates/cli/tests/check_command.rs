//! T007, T008, T009, T010, T012, T014, T020, T021, T022: what `check`
//! prints and returns, and `render` beginning with the same check.

mod common;

use common::*;
use std::ffi::OsStr;
use std::path::Path;

fn check(dir: &Path) -> measure::Ran {
    framesmith(&["check", text(dir)])
}

/// A project whose composition is written with these attributes, each on
/// its own line starting at line 3.
fn declared(test: &str, attrs: &[&str], body: &str, rest: &str) -> std::path::PathBuf {
    let index = format!(
        "<!doctype html>\n<html><head><meta charset=\"utf-8\"><title>t</title></head><body>\n<div {}>\n{body}\n</div>\n{rest}\n</body></html>\n",
        attrs.join("\n     ")
    );
    project(test, "project", &index)
}

const SMALL: &str = "data-fs-size=\"320x180\"";

#[test]
fn t007_c1_c3_c5_check_prints_the_facts_of_a_valid_project_and_changes_nothing() {
    let dir = declared(
        "t007_c1",
        &[
            "data-fs-composition=\"launch\"",
            "data-fs-size=\"1920x1080\"",
            "data-fs-fps=\"30\"",
            "data-fs-duration=\"6\"",
        ],
        "<p>Hello</p>",
        "",
    );
    let before = tree(&dir);
    let ran = check(&dir);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert_eq!(
        ran.stdout,
        "check passed\n  composition  launch\n  size         1920x1080\n  rate         30 fps\n  length       6 s\n  frames       180\n  clips        0\n  tweens       0\n"
    );
    // c5: the result is in words and nothing relies on colour.
    assert!(!ran.all().contains('\u{1b}'));
    // c3: nothing changed.
    assert_eq!(before, tree(&dir));
}

#[test]
fn t007_c2_t041_c3_check_needs_no_browser_and_no_ffmpeg() {
    let dir = declared(
        "t007_c2",
        &["data-fs-composition=\"main\"", SMALL, "data-fs-duration=\"1\""],
        "",
        "",
    );
    let usual = check(&dir);
    // An empty search path, an empty home folder and no named browser: nothing can be found.
    let empty = scratch("t007_c2_home");
    let bare = run_in(
        &dir,
        &["check", "."],
        &[
            ("PATH", OsStr::new("")),
            ("HOME", empty.as_os_str()),
            ("FRAMESMITH_BROWSER", OsStr::new("")),
        ],
    );
    assert_eq!((bare.code, bare.stdout.as_str()), (0, usual.stdout.as_str()), "{}", bare.all());
    assert!(bare.stdout.starts_with("check passed\n"));
    // The same machine cannot render: so the check really ran without them.
    let render = run_in(
        &dir,
        &["render", "."],
        &[
            ("PATH", OsStr::new("")),
            ("HOME", empty.as_os_str()),
            ("FRAMESMITH_BROWSER", OsStr::new("")),
        ],
    );
    assert_eq!(render.code, 1, "{}", render.all());
}

#[test]
fn t007_c4_a_missing_path_or_a_folder_with_no_page_is_named() {
    let root = scratch("t007_c4");
    let gone = root.join("nowhere");
    let ran = check(&gone);
    assert_eq!(ran.code, 1);
    assert!(ran.stdout.contains(&format!("error FS-E001 {}", gone.display())), "{}", ran.stdout);
    assert!(ran.stdout.contains("does not exist"));
    assert!(ran.stdout.contains("check failed: 1 error"));

    let bare = root.join("bare");
    std::fs::create_dir_all(&bare).unwrap();
    write(&bare, "notes.txt", "no page here");
    let ran = check(&bare);
    assert_eq!(ran.code, 1);
    assert!(ran.stdout.contains(&format!("error FS-E002 {}", bare.display())), "{}", ran.stdout);
    assert!(ran.stdout.contains("has no index.html"));
}

#[test]
fn t007_c6_t016_c6_t028_c7_the_specs_for_attributes_motion_and_support_exist() {
    for (name, title) in [
        ("composition.md", "# Composition attributes"),
        ("motion-block.md", "# Motion block format"),
        ("support-matrix.md", "# Support matrix"),
        ("commands.md", "# Commands"),
    ] {
        let spec = spec(name);
        assert!(spec.starts_with(title), "{name}");
        assert!(spec.contains("Status: v0 (Sprint 1)"), "{name}");
    }
}

#[test]
fn t008_c1_c2_a_rate_that_was_not_written_is_30_and_marked() {
    let unwritten = declared(
        "t008_c1",
        &["data-fs-composition=\"main\"", SMALL, "data-fs-duration=\"2\""],
        "",
        "",
    );
    let ran = check(&unwritten);
    assert_eq!(ran.code, 0);
    assert!(ran.stdout.contains("  rate         30 fps (filled in)\n"), "{}", ran.stdout);
    assert!(ran.stdout.contains("  frames       60\n"));

    let written = declared(
        "t008_c2",
        &[
            "data-fs-composition=\"main\"",
            SMALL,
            "data-fs-fps=\"24\"",
            "data-fs-duration=\"2\"",
        ],
        "",
        "",
    );
    let ran = check(&written);
    assert!(ran.stdout.contains("  rate         24 fps\n"), "{}", ran.stdout);
    assert!(!ran.stdout.contains("filled in"));
}

#[test]
fn t008_c4_a_rate_that_is_not_a_whole_number_from_1_to_240_is_an_error() {
    for (name, rate) in [("decimal", "29.97"), ("zero", "0"), ("text", "fast"), ("large", "241")] {
        let dir = declared(
            &format!("t008_c4_{name}"),
            &[
                "data-fs-composition=\"main\"",
                SMALL,
                &format!("data-fs-fps=\"{rate}\""),
                "data-fs-duration=\"2\"",
            ],
            "",
            "",
        );
        let ran = check(&dir);
        assert_eq!(ran.code, 1, "{rate}: {}", ran.stdout);
        assert!(ran.stdout.contains("error FS-E008 index.html:5\n"), "{}", ran.stdout);
        assert!(ran
            .stdout
            .contains(&format!("The frame rate \"{rate}\" is not a whole number from 1 to 240.")));
        assert!(ran
            .stdout
            .contains("fix: Write a whole number of frames per second, for example data-fs-fps=\"30\""));
    }
}

#[test]
fn t009_c1_a_page_with_no_composition_is_one_error() {
    let dir = project(
        "t009_c1",
        "project",
        "<!doctype html>\n<html><body><p>Nothing marked.</p></body></html>\n",
    );
    let ran = check(&dir);
    assert_eq!(ran.code, 1);
    assert!(
        ran.stdout
            .starts_with("error FS-E003 index.html\n  The page has no composition.\n  fix: Add data-fs-composition="),
        "{}",
        ran.stdout
    );
    assert!(ran
        .stdout
        .ends_with("check failed: 1 error, 0 unsupported, 0 unknown, 0 warnings\n"));
}

#[test]
fn t009_c2_c3_c4_size_and_length_errors_carry_file_line_and_the_expected_form() {
    let cases: [(&str, &[&str], &str, &str); 7] = [
        (
            "no_size",
            &["data-fs-composition=\"main\"", "data-fs-duration=\"2\""],
            "error FS-E005 index.html:3",
            "data-fs-size=\"1920x1080\"",
        ),
        (
            "bad_size",
            &["data-fs-composition=\"main\"", "data-fs-size=\"wide\"", "data-fs-duration=\"2\""],
            "error FS-E006 index.html:4",
            "WIDTHxHEIGHT",
        ),
        (
            "odd_width",
            &["data-fs-composition=\"main\"", "data-fs-size=\"321x180\"", "data-fs-duration=\"2\""],
            "error FS-E007 index.html:4",
            "the width and the height must both be even",
        ),
        (
            "odd_height",
            &["data-fs-composition=\"main\"", "data-fs-size=\"320x181\"", "data-fs-duration=\"2\""],
            "error FS-E007 index.html:4",
            "the width and the height must both be even",
        ),
        (
            "zero",
            &["data-fs-composition=\"main\"", SMALL, "data-fs-duration=\"0\""],
            "error FS-E010 index.html:5",
            "it must be greater than zero",
        ),
        (
            "negative",
            &["data-fs-composition=\"main\"", SMALL, "data-fs-duration=\"-3\""],
            "error FS-E010 index.html:5",
            "it must be greater than zero",
        ),
        (
            "words",
            &["data-fs-composition=\"main\"", SMALL, "data-fs-duration=\"long\""],
            "error FS-E009 index.html:5",
            "is not a number of seconds",
        ),
    ];
    for (name, attrs, place, words) in cases {
        let dir = declared(&format!("t009_{name}"), attrs, "", "");
        let ran = check(&dir);
        assert_eq!(ran.code, 1, "{name}: {}", ran.stdout);
        assert!(ran.stdout.contains(&format!("{place}\n")), "{name}: {}", ran.stdout);
        assert!(ran.stdout.contains(words), "{name}: {}", ran.stdout);
    }
}

#[test]
fn t009_c5_c6_c7_three_errors_are_listed_in_one_run_and_fixing_them_passes() {
    let dir = declared(
        "t009_c5",
        &[
            "data-fs-composition=\"main\"",
            "data-fs-size=\"321x180\"",
            "data-fs-fps=\"23.976\"",
            "data-fs-duration=\"-1\"",
        ],
        "",
        "",
    );
    let ran = check(&dir);
    assert_eq!(ran.code, 1);
    // c6: every diagnostic has the kind in words, a code, file and line, a description and a remedy.
    let blocks: Vec<&str> = ran.stdout.split("\n\n").collect();
    assert_eq!(blocks.len(), 4, "{}", ran.stdout);
    for (block, place) in blocks.iter().zip([
        "error FS-E007 index.html:4",
        "error FS-E008 index.html:5",
        "error FS-E010 index.html:6",
    ]) {
        let lines: Vec<&str> = block.lines().collect();
        assert_eq!(lines.len(), 3, "{block}");
        assert_eq!(lines[0], place);
        assert!(lines[1].starts_with("  ") && lines[1].ends_with('.'));
        assert!(lines[2].starts_with("  fix: "));
    }
    assert_eq!(blocks[3], "check failed: 3 errors, 0 unsupported, 0 unknown, 0 warnings\n");
    assert!(!ran.stdout.contains('\u{1b}'));

    // c7: the author fixes all three.
    let fixed = std::fs::read_to_string(dir.join("index.html"))
        .unwrap()
        .replace("321x180", "320x180")
        .replace("23.976", "24")
        .replace("\"-1\"", "\"1\"");
    write(&dir, "index.html", fixed);
    let ran = check(&dir);
    assert_eq!(ran.code, 0, "{}", ran.stdout);
    assert!(ran.stdout.starts_with("check passed\n"));
}

#[test]
fn t010_c1_c2_c3_render_stops_on_an_invalid_project_before_starting_anything() {
    let dir = declared(
        "t010",
        &["data-fs-composition=\"main\"", "data-fs-size=\"321x180\"", "data-fs-duration=\"1\""],
        "",
        "",
    );
    let tools = dir.parent().unwrap().join("tools");
    std::fs::create_dir_all(&tools).unwrap();
    let (spy_browser, browser_started) = spy(&tools, "browser-spy", &browser());
    let (_, ffmpeg_started) = spy(&tools, "ffmpeg", &ffmpeg());
    // A video from an earlier render is at the output path.
    write(&dir, "out/project.mp4", "an earlier video");
    let checked = check(&dir);
    let path = format!("{}:{}", tools.display(), std::env::var("PATH").unwrap());
    let ran = run_in(
        dir.parent().unwrap(),
        &["render", text(&dir)],
        &[("FRAMESMITH_BROWSER", spy_browser.as_os_str()), ("PATH", OsStr::new(&path))],
    );
    assert_eq!((checked.code, ran.code), (1, 1));
    // c1: the same diagnostics as check.
    assert_eq!(ran.stderr, checked.stdout);
    assert!(ran.stderr.contains("error FS-E007 index.html:4"));
    // c2: neither program was started.
    assert!(!browser_started.exists() && !ffmpeg_started.exists());
    // c3: the earlier video is untouched and nothing else appeared.
    assert_eq!(std::fs::read_to_string(dir.join("out/project.mp4")).unwrap(), "an earlier video");
    assert_eq!(std::fs::read_dir(dir.join("out")).unwrap().count(), 1);
}

#[test]
fn t010_c4_a_project_that_passes_check_is_not_refused_by_render() {
    for name in ["still", "empty"] {
        let dir = fixture(&format!("t010_c4_{name}"), name);
        assert_eq!(check(&dir).code, 0);
        let ran = framesmith(&["render", text(&dir), "--fps", "2"]);
        assert_eq!(ran.code, 0, "{}", ran.all());
    }
}

const THREE_CLIPS: &str = "<div id=\"a\" data-fs-clip data-fs-start=\"0\" data-fs-duration=\"1\"></div>\n\
<div id=\"b\" data-fs-clip data-fs-start=\"0.5\" data-fs-duration=\"1\" data-fs-track=\"2\"></div>\n\
<div data-fs-clip data-fs-start=\"1\" data-fs-duration=\"1\"></div>";

#[test]
fn t012_c1_the_facts_include_the_clip_count() {
    let dir = declared(
        "t012_c1",
        &["data-fs-composition=\"main\"", SMALL, "data-fs-duration=\"2\""],
        THREE_CLIPS,
        "",
    );
    let ran = check(&dir);
    assert_eq!(ran.code, 0, "{}", ran.stdout);
    assert!(ran.stdout.contains("  clips        3\n"), "{}", ran.stdout);
}

#[test]
fn t012_c2_c3_c5_clip_errors_carry_file_and_line_and_are_listed_with_declaration_errors() {
    let body = "<div id=\"a\" data-fs-clip data-fs-duration=\"1\"></div>\n\
<div id=\"b\" data-fs-clip data-fs-start=\"-1\" data-fs-duration=\"1\"></div>\n\
<div data-fs-clip data-fs-start=\"0\" data-fs-duration=\"0\"></div>\n\
<div id=\"d\" data-fs-clip data-fs-start=\"0\" data-fs-duration=\"-2\"></div>";
    // The composition is on lines 3 to 5, so the clips are on lines 6 to 9.
    let dir = declared(
        "t012_c2",
        &["data-fs-composition=\"main\"", "data-fs-size=\"320x181\"", "data-fs-duration=\"2\""],
        body,
        "",
    );
    let ran = check(&dir);
    assert_eq!(ran.code, 1);
    for expected in [
        "error FS-E007 index.html:4\n",
        "error FS-E020 index.html:6\n  The clip #a has no start.\n",
        "error FS-E022 index.html:7\n  The clip #b starts at -1 seconds, before the start of the composition.\n",
        "error FS-E024 index.html:8\n  The clip number 3 <div> has a duration of 0 seconds; it must be greater than zero.\n",
        "error FS-E024 index.html:9\n  The clip #d has a duration of -2 seconds; it must be greater than zero.\n",
        "check failed: 5 errors, 0 unsupported, 0 unknown, 0 warnings\n",
    ] {
        assert!(ran.stdout.contains(expected), "missing {expected:?} in\n{}", ran.stdout);
    }
}

#[test]
fn t012_c4_t028_c5_a_clip_that_can_never_show_is_a_warning_and_the_check_passes() {
    let body = "<div id=\"late\" data-fs-clip data-fs-start=\"2\" data-fs-duration=\"1\"></div>\n\
<div id=\"later\" data-fs-clip data-fs-start=\"2.5\" data-fs-duration=\"1\"></div>";
    let dir = declared(
        "t012_c4",
        &["data-fs-composition=\"main\"", SMALL, "data-fs-duration=\"2\""],
        body,
        "",
    );
    let ran = check(&dir);
    assert_eq!(ran.code, 0, "{}", ran.stdout);
    assert!(
        ran.stdout.starts_with(
            "warning FS-W001 index.html:6\n  The clip #late starts at 2 s, at or after the end of the composition (2 s), so it never shows.\n"
        ),
        "{}",
        ran.stdout
    );
    assert!(ran
        .stdout
        .contains("warning FS-W001 index.html:7\n  The clip #later starts at 2.5 s"));
    assert!(ran.stdout.contains("\ncheck passed\n  composition  main\n"));
}

#[test]
fn t014_c1_c2_c4_missing_and_outside_files_are_errors_at_the_line_of_the_reference() {
    let body = "<img src=\"pictures/missing.png\" alt=\"\">\n<img src=\"../outside.png\" alt=\"\">";
    let index = format!(
        "<!doctype html>\n<html><head>\n<link rel=\"stylesheet\" href=\"gone.css\">\n</head><body>\n<div data-fs-composition=\"main\" {SMALL} data-fs-duration=\"1\">\n{body}\n</div>\n</body></html>\n"
    );
    let dir = project("t014_c1", "project", &index);
    write(dir.parent().unwrap(), "outside.png", "exists, but not in the project");
    let ran = check(&dir);
    assert_eq!(ran.code, 1);
    for expected in [
        "error FS-E030 index.html:3\n  The style sheet \"gone.css\" does not exist",
        "error FS-E030 index.html:6\n  The image \"pictures/missing.png\" does not exist",
        "error FS-E031 index.html:7\n  The image \"../outside.png\" points outside the project folder; assets must be inside the project.\n",
    ] {
        assert!(ran.stdout.contains(expected), "missing {expected:?} in\n{}", ran.stdout);
    }
}

#[test]
fn t014_c3_no_error_when_every_referenced_file_exists() {
    let index = format!(
        "<!doctype html>\n<html><head>\n<link rel=\"stylesheet\" href=\"look.css\">\n</head><body>\n<div data-fs-composition=\"main\" {SMALL} data-fs-duration=\"1\">\n<img src=\"pictures/dot.svg\" alt=\"\">\n</div>\n</body></html>\n"
    );
    let dir = project("t014_c3", "project", &index);
    write(&dir, "look.css", "p { color: red; }\n");
    write(
        &dir,
        "pictures/dot.svg",
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"4\" height=\"4\"><rect width=\"4\" height=\"4\"/></svg>\n",
    );
    let ran = check(&dir);
    assert_eq!(ran.code, 0, "{}", ran.stdout);
    assert!(!ran.stdout.contains("FS-E030") && !ran.stdout.contains("FS-E031"));
}

fn with_motion(test: &str, tweens: &str) -> std::path::PathBuf {
    // The block starts on line 8; its tweens are written one per line from line 9.
    declared(
        test,
        &["data-fs-composition=\"main\"", SMALL, "data-fs-duration=\"2\""],
        "<div id=\"a\"></div>",
        &format!("<script type=\"application/framesmith-motion+json\">\n[ {tweens} ]\n</script>"),
    )
}

#[test]
fn t020_c1_the_facts_include_the_tween_count() {
    let tween = "{ \"target\": \"#a\", \"to\": { \"x\": 10 }, \"at\": 0, \"duration\": 1 }";
    let dir = with_motion("t020_c1", &[tween; 4].join(",\n  "));
    let ran = check(&dir);
    assert_eq!(ran.code, 0, "{}", ran.stdout);
    assert!(ran.stdout.contains("  tweens       4\n"), "{}", ran.stdout);
}

#[test]
fn t020_c2_a_block_that_is_not_json_is_an_error_at_the_line_of_the_first_fault() {
    let dir = with_motion(
        "t020_c2",
        "{ \"target\": \"#a\", \"to\": { \"x\": 10 }, \"at\": 0, \"duration\": 1 },\n  { \"target\": \"#a\" \"to\": 3 },\n  { oops }",
    );
    let ran = check(&dir);
    assert_eq!(ran.code, 1);
    assert!(
        ran.stdout
            .starts_with("error FS-E040 index.html:10\n  The motion block is not valid JSON: "),
        "{}",
        ran.stdout
    );
    assert!(ran.stdout.contains("check failed: 1 error"));
}

#[test]
fn t020_c3_c4_c5_c6_faulty_tweens_are_each_named() {
    let dir = with_motion(
        "t020_c3",
        "{ \"to\": { \"x\": 1 }, \"at\": 0, \"duration\": 1 },\n  \
         { \"target\": \"#a\", \"at\": 0, \"duration\": 1 },\n  \
         { \"target\": \"#a\", \"to\": { \"x\": 1 }, \"duration\": 1 },\n  \
         { \"target\": \"#a\", \"to\": { \"x\": 1 }, \"at\": -1, \"duration\": 1 },\n  \
         { \"target\": \"#a\", \"to\": { \"x\": 1 }, \"at\": 0, \"duration\": -2 },\n  \
         { \"target\": \"#nobody\", \"to\": { \"x\": 1 }, \"at\": 0, \"duration\": 1 }",
    );
    let ran = check(&dir);
    assert_eq!(ran.code, 1);
    for expected in [
        "error FS-E042 index.html:9\n  Tween 1 has no \"target\".\n",
        "error FS-E042 index.html:10\n  Tween 2 (target \"#a\") has no values: none of \"to\", \"from\" or \"set\" is written.\n",
        "error FS-E042 index.html:11\n  Tween 3 (target \"#a\") has no start time \"at\".\n",
        "error FS-E043 index.html:12\n  Tween 4 (target \"#a\") has a negative start time \"at\" (-1).\n",
        "error FS-E043 index.html:13\n  Tween 5 (target \"#a\") has a negative \"duration\" (-2).\n",
        "error FS-E044 index.html:14\n  The tween target \"#nobody\" matches no element in the composition.\n",
        "check failed: 6 errors, 0 unsupported, 0 unknown, 0 warnings\n",
    ] {
        assert!(ran.stdout.contains(expected), "missing {expected:?} in\n{}", ran.stdout);
    }
}

const DOT: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"4\" height=\"4\"><rect width=\"4\" height=\"4\"/></svg>\n";

#[test]
fn t021_c3_c4_worked_out_clip_durations_are_printed_and_marked() {
    let body = "<img id=\"photo\" data-fs-clip data-fs-start=\"1\" src=\"dot.svg\" alt=\"\">\n\
<p id=\"words\" data-fs-clip data-fs-start=\"2\">Hello</p>\n\
<p id=\"kept\" data-fs-clip data-fs-start=\"0\" data-fs-duration=\"2\">Kept</p>";
    let dir = declared(
        "t021_c3",
        &["data-fs-composition=\"main\"", SMALL, "data-fs-duration=\"6\""],
        body,
        "<script type=\"application/framesmith-motion+json\">\n[ { \"target\": \"#words\", \"to\": { \"x\": 10 }, \"at\": 4, \"duration\": 1 } ]\n</script>",
    );
    write(&dir, "dot.svg", DOT);
    let ran = check(&dir);
    assert_eq!(ran.code, 0, "{}", ran.stdout);
    assert!(ran.stdout.contains("  clip #photo  duration 3 s (filled in)\n"), "{}", ran.stdout);
    // A clip that is not an image lasts from its start (2 s) to the end of the composition (6 s).
    assert!(ran.stdout.contains("  clip #words  duration 4 s (filled in)\n"), "{}", ran.stdout);
    // c4: the written duration is used and is not marked.
    assert!(!ran.stdout.contains("#kept"));
    assert!(ran.stdout.contains("  length       6 s\n"));
}

#[test]
fn t021_c5_a_clip_with_no_duration_and_no_length_to_last_until_is_an_error() {
    // No written length, no motion, and no clip with an end of its own.
    let dir = declared(
        "t021_c5",
        &["data-fs-composition=\"main\"", SMALL],
        "<p id=\"words\" data-fs-clip data-fs-start=\"0\">Hello</p>",
        "",
    );
    let ran = check(&dir);
    assert_eq!(ran.code, 1);
    assert!(
        ran.stdout.contains(
            "error FS-E025 index.html:5\n  The clip #words has no duration, and the composition has no length for it to last until: none is written, and there is no motion and no clip with its own end to work one out from.\n  fix: Write a duration on the clip, for example data-fs-duration=\"2\", or a length on the composition.\n"
        ),
        "{}",
        ran.stdout
    );
    assert!(ran.stdout.contains("check failed: 1 error,"), "{}", ran.stdout);
    let rendered = framesmith(&["render", text(&dir)]);
    assert_eq!(rendered.code, 1, "{}", rendered.all());
    assert!(!dir.join("out").exists());

    // With a written length the same clip is valid: it lasts until the end.
    let dir = declared(
        "t021_c5_written_length",
        &["data-fs-composition=\"main\"", SMALL, "data-fs-duration=\"6\""],
        "<p id=\"words\" data-fs-clip data-fs-start=\"0\">Hello</p>",
        "",
    );
    let ran = check(&dir);
    assert_eq!(ran.code, 0, "{}", ran.stdout);
    assert!(ran.stdout.contains("  clip #words  duration 6 s (filled in)\n"), "{}", ran.stdout);
}

fn length_case(test: &str, length: Option<&str>, clip_end: &str, tween_end: &str) -> measure::Ran {
    let mut attrs = vec!["data-fs-composition=\"main\"", SMALL, "data-fs-fps=\"30\""];
    let written = length.map(|l| format!("data-fs-duration=\"{l}\""));
    attrs.extend(written.as_deref());
    let dir = declared(
        test,
        &attrs,
        &format!("<div id=\"a\" data-fs-clip data-fs-start=\"0\" data-fs-duration=\"{clip_end}\"></div>"),
        &format!("<script type=\"application/framesmith-motion+json\">\n[ {{ \"target\": \"#a\", \"to\": {{ \"x\": 10 }}, \"at\": 0, \"duration\": {tween_end} }} ]\n</script>"),
    );
    check(&dir)
}

#[test]
fn t022_c1_c2_c3_the_length_is_the_end_of_the_motion_unless_one_is_written() {
    let ran = length_case("t022_c1", None, "4", "5");
    assert_eq!(ran.code, 0, "{}", ran.stdout);
    assert!(
        ran.stdout.contains("  length       5 s (filled in)\n  frames       150\n"),
        "{}",
        ran.stdout
    );
    // c2: the clip ends later than the motion; the end of the motion is still the length.
    let ran = length_case("t022_c2", None, "6.5", "5");
    assert_eq!(ran.code, 0, "{}", ran.stdout);
    assert!(
        ran.stdout.contains("  length       5 s (filled in)\n  frames       150\n"),
        "{}",
        ran.stdout
    );
    // c3: a written length wins and is not marked.
    let ran = length_case("t022_c3", Some("2"), "4", "5");
    assert!(ran.stdout.contains("  length       2 s\n  frames       60\n"), "{}", ran.stdout);
}

#[test]
fn t022_c4_nothing_to_work_the_length_out_from_is_an_error() {
    let dir = declared("t022_c4", &["data-fs-composition=\"main\"", SMALL], "<p>Hello</p>", "");
    let ran = check(&dir);
    assert_eq!(ran.code, 1);
    assert!(
        ran.stdout.contains(
            "error FS-E011 index.html:3\n  The composition has no length, and no motion and no clip with its own end to work one out from.\n  fix: Write the length in seconds on the composition"
        ),
        "{}",
        ran.stdout
    );
}

#[test]
fn t022_c5_with_no_motion_the_last_clip_with_its_own_end_gives_the_length() {
    // No motion. One clip is written to end at 5 s; the image clip starts at 4 s and lasts 3 s by rule.
    let body = "<img id=\"photo\" data-fs-clip data-fs-start=\"4\" src=\"dot.svg\" alt=\"\">\n<div id=\"a\" data-fs-clip data-fs-start=\"0\" data-fs-duration=\"5\"></div>";
    let dir = declared("t022_c5", &["data-fs-composition=\"main\"", SMALL], body, "");
    write(&dir, "dot.svg", DOT);
    let ran = check(&dir);
    assert_eq!(ran.code, 0, "{}", ran.stdout);
    assert!(
        ran.stdout.contains("  length       7 s (filled in)\n  frames       210\n"),
        "{}",
        ran.stdout
    );
    assert!(ran.stdout.contains("  clip #photo  duration 3 s (filled in)\n"));
}
