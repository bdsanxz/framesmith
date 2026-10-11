//! T024: the render stops when the page's view of the composition differs
//! from the one read from the files.

mod common;

use common::*;
use std::ffi::OsStr;
use std::path::Path;

const HEAD: &str = "data-fs-size=\"64x36\" data-fs-duration=\"1\"";
const CLIP: &str = "<p id=\"a\" data-fs-clip data-fs-start=\"0\" data-fs-duration=\"1\">Hello</p>";

/// Renders a project whose script changes the composition while loading.
/// Returns the description of the failure after checking what must hold
/// whenever the render stops for this reason.
fn disagreement(test: &str, script: &str) -> String {
    let dir = project(test, "card", &page(HEAD, "", CLIP, &format!("<script>\n{script}\n</script>")));
    // The files alone are fine: only the page's own script makes the difference.
    assert_eq!(framesmith(&["check", text(&dir)]).code, 0);
    let frames = dir.parent().unwrap().join("frames");
    let ran = run_in(
        dir.parent().unwrap(),
        &["render", text(&dir), "--json"],
        &[("FRAMESMITH_DEBUG_FRAMES", frames.as_os_str())],
    );
    assert_eq!(ran.code, 1, "{}", ran.all());
    let document = document(&ran);
    assert_eq!(
        (document["outcome"].as_str(), document["error"]["code"].as_str()),
        (Some("failed"), Some("FS-R010"))
    );
    assert_eq!(document["error"]["stage"], "probe");
    // No output, c5: no frame was captured, and no browser is left.
    assert!(!dir.join("out").exists());
    assert!(!ran.stderr.contains("capture"), "{}", ran.stderr);
    assert_eq!(std::fs::read_dir(&frames).map(Iterator::count).unwrap_or(0), 0);
    assert!(!browser_left(document["run_id"].as_str().unwrap()));
    document["error"]["description"].as_str().unwrap().to_string()
}

fn fact<'a>(printed: &'a str, name: &str) -> &'a str {
    let line = printed
        .lines()
        .find(|line| line.trim_start().starts_with(name))
        .unwrap_or_else(|| panic!("no {name} in {printed}"));
    line.trim_start()[name.len()..].trim()
}

#[test]
fn t024_c1_with_no_script_the_render_uses_the_facts_check_prints() {
    let body = format!("{CLIP}\n<p id=\"b\" data-fs-clip data-fs-start=\"0.25\">World</p>");
    let tweens = "{ \"target\": \"#a\", \"to\": { \"x\": 10 }, \"at\": 0, \"duration\": 0.5 },\n\
                  { \"target\": \"#b\", \"from\": { \"opacity\": 0 }, \"at\": 0.25, \"duration\": 0.75 }";
    let dir = project("t024_c1", "card", &page("data-fs-size=\"64x36\"", "", &body, &motion(tweens)));
    let checked = framesmith(&["check", text(&dir)]);
    assert_eq!(checked.code, 0, "{}", checked.all());
    let job_file = dir.parent().unwrap().join("job.json");
    let ran = run_in(
        Path::new("/"),
        &["render", text(&dir)],
        &[("FRAMESMITH_DEBUG_JOB", job_file.as_os_str())],
    );
    assert_eq!(ran.code, 0, "{}", ran.all());

    // What check printed.
    let printed = &checked.stdout;
    assert_eq!(fact(printed, "size"), "64x36");
    assert_eq!(fact(printed, "rate"), "30 fps (filled in)");
    assert_eq!(fact(printed, "length"), "1 s (filled in)");
    assert_eq!(
        (fact(printed, "frames"), fact(printed, "clips"), fact(printed, "tweens")),
        ("30", "2", "2")
    );
    assert_eq!(fact(printed, "clip #b"), "duration 0.75 s (filled in)");

    // What the render used: its job, and the facts it printed after the page agreed.
    let job: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&job_file).unwrap()).unwrap();
    let manifest = &job["manifest"];
    assert_eq!((manifest["width"].as_u64(), manifest["height"].as_u64()), (Some(64), Some(36)));
    assert_eq!((job["fps"].as_u64(), manifest["fps_written"].as_bool()), (Some(30), Some(false)));
    assert_eq!(
        (manifest["length"].as_str(), manifest["length_written"].as_bool()),
        (Some("1/1"), Some(false))
    );
    assert_eq!(job["frames"].as_u64(), Some(30));
    assert_eq!(
        (
            manifest["clips"].as_array().unwrap().len(),
            manifest["tweens"].as_array().unwrap().len()
        ),
        (2, 2)
    );
    assert_eq!(manifest["clips"][1]["duration"].as_str(), Some("3/4"));
    assert!(
        ran.stdout.contains("probe     done      64x36, 30 fps, 1 s, 30 frames\n"),
        "{}",
        ran.stdout
    );
    assert_eq!(facts(&dir.join("out/card.mp4")).frames, 30);
}

#[test]
fn t024_c2_c5_a_clip_added_by_a_script_is_named_as_the_first_difference() {
    let said = disagreement(
        "t024_c2",
        "var added = document.createElement(\"p\");\nadded.id = \"extra\";\nadded.setAttribute(\"data-fs-clip\", \"\");\n\
         added.setAttribute(\"data-fs-start\", \"0.5\");\nadded.setAttribute(\"data-fs-duration\", \"0.25\");\n\
         document.querySelector(\"[data-fs-composition]\").appendChild(added);",
    );
    assert_eq!(
        said,
        "The page disagrees with the files: the clip #extra is missing in the files and present in the page."
    );
}

#[test]
fn t024_c3_c5_a_changed_clip_start_shows_both_values() {
    let said = disagreement(
        "t024_c3",
        "document.getElementById(\"a\").setAttribute(\"data-fs-start\", \"0.5\");",
    );
    assert_eq!(
        said,
        "The page disagrees with the files: the start of clip 1 (#a) is 0 s in the files and 0.5 s in the page."
    );
}

#[test]
fn t024_c4_c5_a_changed_size_or_length_is_named() {
    let composition = "document.querySelector(\"[data-fs-composition]\")";
    let said = disagreement(
        "t024_c4_size",
        &format!("{composition}.setAttribute(\"data-fs-size\", \"128x72\");"),
    );
    assert_eq!(
        said,
        "The page disagrees with the files: the composition size is 64x36 in the files and 128x72 in the page."
    );
    let said = disagreement(
        "t024_c4_length",
        &format!("{composition}.setAttribute(\"data-fs-duration\", \"2\");"),
    );
    assert_eq!(
        said,
        "The page disagrees with the files: the composition length is 1 s in the files and 2 s in the page."
    );
    let said = disagreement("t024_c4_rate", &format!("{composition}.setAttribute(\"data-fs-fps\", \"24\");"));
    assert_eq!(
        said,
        "The page disagrees with the files: the frame rate is 30 in the files and 24 in the page."
    );
}

#[test]
fn t024_a_tween_changed_by_a_script_is_a_difference_too() {
    let dir_script = "document.querySelector(\"script[type='application/framesmith-motion+json']\").textContent = \
                      '[{ \"target\": \"#a\", \"to\": { \"x\": 99 }, \"at\": 0, \"duration\": 1 }]';";
    let page = page(
        HEAD,
        "",
        CLIP,
        &format!(
            "{}\n<script>\n{dir_script}\n</script>",
            motion("{ \"target\": \"#a\", \"to\": { \"x\": 10 }, \"at\": 0, \"duration\": 1 }")
        ),
    );
    let dir = project("t024_tween", "card", &page);
    let ran = run_in(Path::new("/"), &["render", text(&dir)], &[("UNUSED", OsStr::new(""))]);
    assert_eq!(ran.code, 1, "{}", ran.all());
    assert!(
        ran.stderr
            .contains("error FS-R010\n  The page disagrees with the files: tween 1 (target \"#a\")"),
        "{}",
        ran.stderr
    );
    assert!(!dir.join("out").exists());
}
