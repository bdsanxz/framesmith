//! T033: an unsupported use that `check` cannot see in the files is found
//! while the page runs, and `render` stops with exit code 2.

mod common;

use common::*;
use std::path::PathBuf;

/// A page whose script reaches the script interface without the word being readable in the files.
const SCRIPT_INTERFACE: &str = "window[[\"frame\", \"smith\"].join(\"\")].timeline();";

/// A page whose script calls the browser's animation interface through a built-up name.
const BROWSER_ANIMATION: &str =
    "var method = \"ani\" + \"mate\";\ndocument.getElementById(\"x\")[method]([{ opacity: 0 }, { opacity: 1 }], 1000);";

/// The same call, but only after a timer fires inside the composition's time.
const LATER_ANIMATION: &str = "setTimeout(function () {\n  var method = \"ani\" + \"mate\";\n  \
document.getElementById(\"x\")[method]([{ opacity: 0 }, { opacity: 1 }], 1000);\n}, 500);";

fn with_script(test: &str, script: &str, duration: &str) -> PathBuf {
    project(
        test,
        "project",
        &page(
            &format!("data-fs-size=\"64x36\" data-fs-duration=\"{duration}\""),
            "",
            "<p id=\"x\">x</p>",
            &format!("<script>\n{script}\n</script>"),
        ),
    )
}

#[test]
fn t033_c1_a_script_that_calls_the_script_interface_stops_the_render_with_exit_2_and_no_output() {
    let dir = with_script("t033_c1", SCRIPT_INTERFACE, "0.2");
    // The files show nothing, so check passes.
    let checked = framesmith(&["check", text(&dir)]);
    assert_eq!(checked.code, 0, "{}", checked.all());
    let ran = framesmith(&["render", text(&dir)]);
    assert_eq!(ran.code, 2, "{}", ran.all());
    assert!(ran.stdout.contains("render stopped in probe:"), "{}", ran.all());
    let message = "A page script uses the framesmith script interface for building motion; it is not supported in this version.";
    assert!(ran.stderr.contains("unsupported FS-U017"), "{}", ran.all());
    assert!(ran.stderr.contains(message), "{}", ran.all());
    // Nothing was written: no video, no output folder, no staging file.
    assert!(!dir.join("out").exists(), "{:?}", tree(&dir));
    assert_eq!(
        tree(&dir).keys().map(|p| p.to_string_lossy().into_owned()).collect::<Vec<_>>(),
        ["index.html"]
    );
}

#[test]
fn t033_c1_with_an_output_path_the_script_interface_leaves_no_file_there() {
    let dir = with_script("t033_c1_output", SCRIPT_INTERFACE, "0.2");
    let output = dir.parent().unwrap().join("elsewhere/video.mp4");
    let ran = framesmith(&["render", text(&dir), "--output", text(&output)]);
    assert_eq!(ran.code, 2, "{}", ran.all());
    assert!(ran.stderr.contains("FS-U017"), "{}", ran.all());
    assert!(!output.exists() && !output.parent().unwrap().exists());
}

#[test]
fn t033_c2_a_browser_animation_that_check_cannot_see_stops_the_render_with_exit_2() {
    let dir = with_script("t033_c2", BROWSER_ANIMATION, "0.2");
    let checked = framesmith(&["check", text(&dir)]);
    assert_eq!(checked.code, 0, "{}", checked.all());
    let ran = framesmith(&["render", text(&dir)]);
    assert_eq!(ran.code, 2, "{}", ran.all());
    assert!(ran.stderr.contains("unsupported FS-U016"), "{}", ran.all());
    assert!(
        ran.stderr.contains(
            "The page animates with the browser's own animation features (Element.animate); they are not supported in this version."
        ),
        "{}",
        ran.all()
    );
    assert!(!dir.join("out").exists());
}

#[test]
fn t033_c1_c2_a_call_made_later_by_a_timer_is_found_during_capture() {
    for (name, script, code) in [
        ("animate", LATER_ANIMATION, "FS-U016"),
        ("interface", &later_interface(), "FS-U017"),
    ] {
        let dir = with_script(&format!("t033_later_{name}"), script, "1");
        let checked = framesmith(&["check", text(&dir)]);
        assert_eq!(checked.code, 0, "{name}: {}", checked.all());
        let ran = framesmith(&["render", text(&dir)]);
        assert_eq!(ran.code, 2, "{name}: {}", ran.all());
        assert!(ran.stderr.contains(&format!("unsupported {code}")), "{name}: {}", ran.all());
        assert!(ran.stdout.contains("render stopped in capture:"), "{name}: {}", ran.all());
        assert!(!dir.join("out").exists(), "{name}: {:?}", tree(&dir));
    }
}

fn later_interface() -> String {
    format!("setTimeout(function () {{\n  {SCRIPT_INTERFACE}\n}}, 500);")
}

#[test]
fn t033_c3_the_browser_is_stopped_and_an_earlier_video_is_unchanged() {
    for (name, script, duration) in [
        ("interface", SCRIPT_INTERFACE.to_string(), "0.2"),
        ("animate", BROWSER_ANIMATION.to_string(), "0.2"),
        ("later", LATER_ANIMATION.to_string(), "1"),
    ] {
        let dir = with_script(&format!("t033_c3_{name}"), &script, duration);
        let earlier = dir.join("out/project.mp4");
        write(&dir, "out/project.mp4", [0u8, 1, 2, 3, 250, 251, 252, 253]);
        let before = std::fs::read(&earlier).unwrap();
        let ran = framesmith(&["render", text(&dir), "--json"]);
        assert_eq!(ran.code, 2, "{name}: {}", ran.all());
        let run_id = document(&ran)["run_id"].as_str().unwrap().to_string();
        // The earlier video is the same bytes, and it is the only file in the folder.
        assert_eq!(std::fs::read(&earlier).unwrap(), before, "{name}");
        assert_eq!(std::fs::read_dir(dir.join("out")).unwrap().count(), 1, "{name}");
        // No browser process and no profile folder of this run is left.
        assert!(!browser_left(&run_id), "{name}: the run's browser is still there");
    }
}

#[test]
fn t033_c4_the_same_project_with_the_call_removed_renders() {
    for (name, script, duration) in [
        ("interface", SCRIPT_INTERFACE.to_string(), "0.2"),
        ("animate", BROWSER_ANIMATION.to_string(), "0.2"),
        ("later", LATER_ANIMATION.to_string(), "1"),
    ] {
        let dir = with_script(&format!("t033_c4_{name}"), &script, duration);
        assert_eq!(framesmith(&["render", text(&dir)]).code, 2, "{name}");
        // The offending call is taken out; the rest of the page stays.
        write(
            &dir,
            "index.html",
            page(
                &format!("data-fs-size=\"64x36\" data-fs-duration=\"{duration}\""),
                "",
                "<p id=\"x\">x</p>",
                "<script>\nvar harmless = 1;\n</script>",
            ),
        );
        let ran = framesmith(&["render", text(&dir)]);
        assert_eq!(ran.code, 0, "{name}: {}", ran.all());
        let frames = if duration == "1" { 30 } else { 6 };
        assert_eq!(facts(&dir.join("out/project.mp4")).frames, frames, "{name}");
    }
}
