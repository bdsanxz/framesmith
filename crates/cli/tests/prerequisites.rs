//! T040 and T041: `render` says when no browser or no `ffmpeg` can be found,
//! names a bad `FRAMESMITH_BROWSER`, and starts nothing in those cases.

mod common;

use common::*;
use measure::Ran;
use std::ffi::OsString;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

const NO_BROWSER: &str = "error FS-R001\n  No Chromium-based browser was found.\n  fix: Install a Chromium-based browser, \
for example Google Chrome or Chromium, or set FRAMESMITH_BROWSER to the path of one.\n";
const NO_FFMPEG: &str = "error FS-R003\n  ffmpeg was not found on the search path.\n  fix: Install ffmpeg and make sure the folder \
that holds it is on PATH.\n";
const FIX_NAMED: &str = "  fix: Set FRAMESMITH_BROWSER to the path of a Chromium-based browser program, or unset it so Framesmith \
looks for one.\n";

/// One test's machine: an empty home folder, an empty temporary folder, its
/// own log folder, an `ffmpeg` that leaves a marker when it is started, and
/// a tiny project.
struct Kit {
    root: PathBuf,
    home: PathBuf,
    tmp: PathBuf,
    logs: PathBuf,
    project: PathBuf,
    /// A folder on the search path that holds only the `ffmpeg` stand-in.
    tools: PathBuf,
    ffmpeg_started: PathBuf,
    /// A folder that holds nothing, to be used as the whole search path.
    empty: PathBuf,
}

fn kit(test: &str) -> Kit {
    let project = project(
        test,
        "tiny",
        &page("data-fs-size=\"64x36\" data-fs-duration=\"0.2\"", "", "<p>x</p>", ""),
    );
    let root = project.parent().unwrap().to_path_buf();
    let [home, tmp, logs, tools, empty] = ["home", "tmp", "logs", "tools", "empty"].map(|name| {
        let dir = root.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    });
    let (_, ffmpeg_started) = spy(&tools, "ffmpeg", &ffmpeg());
    Kit {
        root,
        home,
        tmp,
        logs,
        project,
        tools,
        ffmpeg_started,
        empty,
    }
}

impl Kit {
    /// Runs `render` on the project with this machine's folders. `path` is the
    /// whole search path; `named` is the value of `FRAMESMITH_BROWSER` (empty
    /// means it is not set); `home` is the home folder.
    fn render<P: AsRef<Path>>(&self, path: &[P], named: &Path, home: &Path, extra: &[&str]) -> Ran {
        let path = std::env::join_paths(path.iter().map(AsRef::as_ref)).unwrap();
        let mut args = vec!["render", text(&self.project)];
        args.extend_from_slice(extra);
        run_in(
            &self.root,
            &args,
            &[
                ("PATH", &path),
                ("HOME", home.as_os_str()),
                ("FRAMESMITH_BROWSER", named.as_os_str()),
                ("TMPDIR", self.tmp.as_os_str()),
                ("FRAMESMITH_LOG_DIR", self.logs.as_os_str()),
            ],
        )
    }

    /// A machine where nothing is found: the empty home folder, and only the
    /// given folders on the search path.
    fn bare<P: AsRef<Path>>(&self, path: &[P], named: &Path, extra: &[&str]) -> Ran {
        self.render(path, named, &self.home, extra)
    }

    fn entries(&self, dir: &Path) -> Vec<OsString> {
        let mut found: Vec<OsString> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name()).collect();
        found.sort();
        found
    }

    /// What a run that stopped early may leave: nothing started, nothing
    /// written, nothing downloaded.
    fn assert_untouched(&self, project_before: &std::collections::BTreeMap<PathBuf, (u64, u64)>, root_before: &[OsString]) {
        assert_eq!(&tree(&self.project), project_before, "the project changed");
        assert!(!self.project.join("out").exists(), "an output folder was made");
        assert!(self.entries(&self.home).is_empty(), "the home folder was written to");
        assert!(self.entries(&self.tmp).is_empty(), "the temporary folder was written to");
        assert_eq!(self.entries(&self.root), root_before, "something new appeared beside the project");
        assert!(!self.ffmpeg_started.exists(), "ffmpeg was started");
    }
}

/// The log of the run, read for its stage lines only.
fn stages_in_log(kit: &Kit) -> usize {
    std::fs::read_to_string(kit.logs.join("framesmith.log"))
        .unwrap_or_default()
        .lines()
        .filter(|line| line.contains("stage started"))
        .count()
}

/// The tests that need "no browser" cannot be run on a machine where a
/// browser sits in an application folder: the empty home folder and search
/// path hide everything else.
fn assert_no_browser_in_applications() {
    let places = browser::Places {
        applications: vec![PathBuf::from("/Applications")],
        ..Default::default()
    };
    if let Ok(found) = browser::find(&places) {
        panic!(
            "this test cannot simulate a machine with no browser: {} is installed in /Applications, which no environment \
change can hide",
            found.display()
        );
    }
}

fn assert_rendered(ran: &Ran, video: &Path) {
    assert_eq!(ran.code, 0, "{}", ran.all());
    let facts = facts(video);
    assert_eq!((facts.width, facts.height, facts.frames), (64, 36, 6));
}

/// A browser stand-in on the search path under the name `chromium`.
fn browser_on_path(kit: &Kit) -> (PathBuf, PathBuf) {
    let dir = kit.root.join("on-path");
    std::fs::create_dir_all(&dir).unwrap();
    let (_, marker) = spy(&dir, "chromium", &browser());
    (dir, marker)
}

#[test]
fn t040_c1_c4_no_browser_stops_before_compiling_and_nothing_starts_or_is_written() {
    assert_no_browser_in_applications();
    let kit = kit("t040_c1");
    let project_before = tree(&kit.project);
    let root_before = kit.entries(&kit.root);
    let nothing = Path::new("");
    let ran = kit.bare(&[&kit.tools], nothing, &[]);
    assert_eq!(ran.code, 1, "{}", ran.all());
    // c1: the message says what is missing and the two ways to fix it; no stage line, no compile.
    assert_eq!(ran.stderr, NO_BROWSER);
    assert_eq!(ran.stdout, "", "no progress line is printed");
    assert_eq!(stages_in_log(&kit), 0, "no stage began");
    // c4: no program was started, nothing was written, so nothing was downloaded.
    kit.assert_untouched(&project_before, &root_before);
    let again = kit.bare(&[&kit.tools], nothing, &["--output", text(&kit.root.join("elsewhere/video.mp4"))]);
    assert_eq!((again.code, again.stderr.as_str()), (1, NO_BROWSER));
    assert!(!kit.root.join("elsewhere").exists(), "the output folder was not made");
}

#[test]
fn t040_c2_a_named_browser_is_used_even_when_another_is_discoverable() {
    let kit = kit("t040_c2");
    let (on_path, found_marker) = browser_on_path(&kit);
    let named_dir = kit.root.join("named");
    std::fs::create_dir_all(&named_dir).unwrap();
    let (named, named_marker) = spy(&named_dir, "browser-spy", &browser());
    let path = [on_path.as_path(), kit.tools.as_path()];

    // Without the setting the browser on the search path is the one used: so it is discoverable.
    let ran = kit.bare(&path, Path::new(""), &["--output", text(&kit.root.join("found.mp4"))]);
    assert_rendered(&ran, &kit.root.join("found.mp4"));
    assert!(found_marker.exists() && !named_marker.exists());
    std::fs::remove_file(&found_marker).unwrap();

    // With the setting, the named one is used and the other is never started.
    let ran = kit.bare(&path, &named, &["--output", text(&kit.root.join("named.mp4"))]);
    assert_rendered(&ran, &kit.root.join("named.mp4"));
    assert!(named_marker.exists(), "the named browser was started");
    assert!(!found_marker.exists(), "the discoverable browser was started");
}

#[test]
fn t040_c3_a_named_browser_that_is_missing_or_cannot_run_is_named_and_nothing_replaces_it() {
    let kit = kit("t040_c3");
    let (on_path, found_marker) = browser_on_path(&kit);
    let path = [on_path.as_path(), kit.tools.as_path()];
    let missing = kit.root.join("no-such-browser");
    let not_runnable = kit.root.join("not-runnable");
    write(&kit.root, "not-runnable", "#!/bin/sh\n");
    std::fs::set_permissions(&not_runnable, std::fs::Permissions::from_mode(0o644)).unwrap();
    let folder = kit.root.join("a-folder");
    std::fs::create_dir_all(&folder).unwrap();
    let project_before = tree(&kit.project);
    let root_before = kit.entries(&kit.root);

    for (named, reason) in [
        (&missing, "it does not exist"),
        (&not_runnable, "it is not marked as a program that can be run"),
        (&folder, "it is not a file"),
    ] {
        // The real home folder is used too, so that its browser caches could have been used as a fallback.
        for home in [kit.home.clone(), PathBuf::from(std::env::var_os("HOME").unwrap())] {
            let ran = kit.render(&path, named, &home, &[]);
            assert_eq!(ran.code, 1, "{}", ran.all());
            let expected = format!(
                "error FS-R002\n  FRAMESMITH_BROWSER names {}, but {reason}.\n{FIX_NAMED}",
                named.display()
            );
            assert_eq!(ran.stderr, expected);
            assert_eq!(ran.stdout, "");
            assert!(!found_marker.exists(), "the browser on the search path was started instead");
            assert_eq!(stages_in_log(&kit), 0);
            kit.assert_untouched(&project_before, &root_before);
        }
    }

    // The same machine with the setting removed does find and start a browser: the fallback was there to take.
    let ran = kit.bare(&path, Path::new(""), &[]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert!(found_marker.exists());
}

#[test]
fn t040_c5_a_render_proceeds_once_a_browser_is_installed_or_named() {
    assert_no_browser_in_applications();
    let kit = kit("t040_c5");
    let nothing = Path::new("");
    let first = kit.bare(&[&kit.tools], nothing, &[]);
    assert_eq!((first.code, first.stderr.as_str()), (1, NO_BROWSER));

    // The setting names something wrong; then it names a working browser.
    let wrong = kit.bare(&[&kit.tools], &kit.root.join("wrong"), &[]);
    assert_eq!(wrong.code, 1, "{}", wrong.all());
    let ran = kit.bare(&[&kit.tools], &browser(), &["--output", text(&kit.root.join("named.mp4"))]);
    assert_rendered(&ran, &kit.root.join("named.mp4"));

    // Or a browser is installed where the search path looks.
    let (on_path, _) = browser_on_path(&kit);
    let ran = kit.bare(
        &[&on_path, &kit.tools],
        nothing,
        &["--output", text(&kit.root.join("installed.mp4"))],
    );
    assert_rendered(&ran, &kit.root.join("installed.mp4"));
}

#[test]
fn t041_c1_no_ffmpeg_stops_before_a_browser_starts() {
    let kit = kit("t041_c1");
    let browser_dir = kit.root.join("named");
    std::fs::create_dir_all(&browser_dir).unwrap();
    let (named, browser_started) = spy(&browser_dir, "browser-spy", &browser());
    let project_before = tree(&kit.project);
    let root_before = kit.entries(&kit.root);
    let ran = kit.bare(&[&kit.empty], &named, &[]);
    assert_eq!(ran.code, 1, "{}", ran.all());
    assert_eq!(ran.stderr, NO_FFMPEG);
    assert_eq!(ran.stdout, "");
    assert!(!browser_started.exists(), "a browser was started");
    assert_eq!(stages_in_log(&kit), 0);
    kit.assert_untouched(&project_before, &root_before);
}

#[test]
fn t041_c2_both_missing_are_named_in_one_run_each_with_its_fix() {
    assert_no_browser_in_applications();
    let kit = kit("t041_c2");
    let project_before = tree(&kit.project);
    let root_before = kit.entries(&kit.root);
    let ran = kit.bare(&[&kit.empty], Path::new(""), &[]);
    assert_eq!(ran.code, 1, "{}", ran.all());
    assert_eq!(ran.stderr, format!("{NO_BROWSER}{NO_FFMPEG}"));
    assert_eq!(ran.stdout, "");
    kit.assert_untouched(&project_before, &root_before);

    // The same in the JSON document: the first is `error`, and `errors` holds both with their fixes.
    let json = kit.bare(&[&kit.empty], Path::new(""), &["--json"]);
    assert_eq!(json.code, 1, "{}", json.all());
    let document = document(&json);
    assert_eq!(document["outcome"], "failed");
    assert_eq!(document["error"]["code"], "FS-R001");
    let errors = document["errors"].as_array().unwrap();
    let codes: Vec<&str> = errors.iter().map(|e| e["code"].as_str().unwrap()).collect();
    assert_eq!(codes, ["FS-R001", "FS-R003"]);
    for error in errors {
        assert!(error["remedy"].as_str().unwrap().starts_with("Install "), "{error}");
    }
}

#[test]
fn t041_c3_check_passes_while_ffmpeg_is_missing() {
    // The shared check test `t007_c2_t041_c3_check_needs_no_browser_and_no_ffmpeg` in check_command.rs
    // proves the same with nothing at all on the machine; this one leaves a working browser and takes away ffmpeg.
    let kit = kit("t041_c3");
    let (named, browser_started) = spy(&kit.root, "browser-spy", &browser());
    let check_in = |path: &Path| {
        run_in(
            &kit.root,
            &["check", text(&kit.project)],
            &[
                ("PATH", path.as_os_str()),
                ("FRAMESMITH_BROWSER", named.as_os_str()),
                ("FRAMESMITH_LOG_DIR", kit.logs.as_os_str()),
            ],
        )
    };
    let with = check_in(&kit.tools);
    let without = check_in(&kit.empty);
    assert_eq!(with.code, 0, "{}", with.all());
    assert_eq!((without.code, &without.stdout, &without.stderr), (0, &with.stdout, &with.stderr));
    assert!(without.stdout.starts_with("check passed\n"), "{}", without.stdout);
    // Rendering on the same machine is refused: the check really ran without ffmpeg.
    let render = run_in(
        &kit.root,
        &["render", text(&kit.project)],
        &[
            ("PATH", kit.empty.as_os_str()),
            ("FRAMESMITH_BROWSER", named.as_os_str()),
            ("FRAMESMITH_LOG_DIR", kit.logs.as_os_str()),
        ],
    );
    assert_eq!((render.code, render.stderr.as_str()), (1, NO_FFMPEG));
    assert!(!browser_started.exists() && !kit.ffmpeg_started.exists());
}

#[test]
fn t041_c4_a_render_proceeds_once_ffmpeg_is_on_the_search_path() {
    let kit = kit("t041_c4");
    let (named, browser_started) = spy(&kit.root, "browser-spy", &browser());
    let first = kit.bare(&[&kit.empty], &named, &[]);
    assert_eq!((first.code, first.stderr.as_str()), (1, NO_FFMPEG));
    assert!(!browser_started.exists());
    let ran = kit.bare(&[&kit.empty, &kit.tools], &named, &[]);
    assert_rendered(&ran, &kit.project.join("out/tiny.mp4"));
    assert!(browser_started.exists() && kit.ffmpeg_started.exists());
}
