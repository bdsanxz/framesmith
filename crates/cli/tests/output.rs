//! T034, T035, T036: where the video goes, that nothing partial is ever
//! found at the output path, and that an output which cannot be written
//! is reported before anything starts.

mod common;

use common::*;
use measure::Ran;
use std::ffi::OsStr;
use std::io::{BufRead, BufReader, Read};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------

/// A small composition of `seconds` at 30 fps.
fn small_page(seconds: &str) -> String {
    page(
        &format!("data-fs-size=\"64x36\" data-fs-duration=\"{seconds}\""),
        "",
        "<p>x</p>",
        "",
    )
}

fn small(test: &str, folder: &str) -> PathBuf {
    project(test, folder, &small_page("0.2"))
}

/// A composition that takes a few seconds to capture: 150 frames at 320x180.
fn slow_page() -> String {
    page(
        "data-fs-size=\"320x180\" data-fs-fps=\"30\" data-fs-duration=\"5\"",
        "",
        "<p>x</p>",
        "",
    )
}

fn names(dir: &Path) -> Vec<String> {
    let mut found: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    found.sort();
    found
}

/// Whether the folder holds a staging file for the given output name.
fn staging_in(dir: &Path, output_name: &str) -> bool {
    names(dir)
        .iter()
        .any(|name| name.starts_with(&format!(".{output_name}.")) && name.ends_with(".part"))
}

fn decodes_cleanly(video: &Path, dir: &Path) -> bool {
    let ran = measure::run(&ffmpeg(), &["-v", "error", "-i", text(video), "-f", "null", "-"], dir, &[]);
    ran.code == 0 && ran.stderr.trim().is_empty()
}

/// Starts a render that is left running; its output is discarded.
fn start(args: &[&str], cwd: &Path, env: &[(&str, &OsStr)]) -> Child {
    let mut command = Command::new(BIN);
    command.args(args).current_dir(cwd).stdout(Stdio::null()).stderr(Stdio::null());
    for (key, value) in env {
        command.env(key, value);
    }
    command.spawn().unwrap()
}

/// Gives a folder its write permission back when the test ends, even by a panic.
struct Restore(PathBuf);

impl Drop for Restore {
    fn drop(&mut self) {
        let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o755));
    }
}

fn read_only(dir: &Path) -> Restore {
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o555)).unwrap();
    Restore(dir.to_path_buf())
}

/// A render with stand-ins for the browser and `ffmpeg` that note whether they were started.
struct Spied {
    ran: Ran,
    browser_started: bool,
    ffmpeg_started: bool,
}

fn spied_render(dir: &Path, args: &[&str]) -> Spied {
    let tools = dir.parent().unwrap().join("tools");
    std::fs::create_dir_all(&tools).unwrap();
    let (spy_browser, browser_marker) = spy(&tools, "browser-spy", &browser());
    let (_, ffmpeg_marker) = spy(&tools, "ffmpeg", &ffmpeg());
    let path = format!("{}:{}", tools.display(), std::env::var("PATH").unwrap());
    let mut all = vec!["render", text(dir)];
    all.extend_from_slice(args);
    let ran = run_in(
        dir.parent().unwrap(),
        &all,
        &[("FRAMESMITH_BROWSER", spy_browser.as_os_str()), ("PATH", OsStr::new(&path))],
    );
    Spied {
        ran,
        browser_started: browser_marker.exists(),
        ffmpeg_started: ffmpeg_marker.exists(),
    }
}

// ---------------------------------------------------------------------
// T034: choose where the video goes
// ---------------------------------------------------------------------

#[test]
fn t034_c1_a_path_in_an_existing_folder_gets_the_video_and_the_default_path_gets_nothing() {
    let dir = small("t034_c1", "demo");
    // Inside the project, in a folder that exists, then outside the project.
    std::fs::create_dir_all(dir.join("renders")).unwrap();
    let inside = dir.join("renders/mine.mp4");
    let ran = framesmith(&["render", text(&dir), "--output", text(&inside)]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert_eq!(facts(&inside).frames, 6);
    assert!(!dir.join("out").exists(), "nothing at the default path");

    let outside_folder = dir.parent().unwrap().join("elsewhere");
    std::fs::create_dir_all(&outside_folder).unwrap();
    let outside = outside_folder.join("other.mp4");
    let ran = framesmith(&["render", text(&dir), "--output", text(&outside)]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert_eq!(facts(&outside).frames, 6);
    assert!(!dir.join("out").exists());
    assert_eq!(names(&outside_folder), ["other.mp4"]);
}

#[test]
fn t034_c1_a_relative_path_is_taken_from_the_folder_the_command_runs_in_and_is_shown_as_typed() {
    let dir = small("t034_c1_relative", "demo");
    let cwd = dir.parent().unwrap();
    std::fs::create_dir_all(cwd.join("here")).unwrap();
    let ran = run_in(cwd, &["render", text(&dir), "--output", "here/v.mp4"], &[]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert!(cwd.join("here/v.mp4").is_file());
    assert!(ran.stdout.lines().any(|line| line == "commit    here/v.mp4"), "{}", ran.stdout);
    assert!(!dir.join("out").exists());
}

#[test]
fn t034_c2_missing_parent_folders_are_created() {
    let dir = small("t034_c2", "demo");
    let output = dir.parent().unwrap().join("a/b/c/video.mp4");
    assert!(!dir.parent().unwrap().join("a").exists());
    let ran = framesmith(&["render", text(&dir), "--output", text(&output)]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert_eq!(facts(&output).frames, 6);
    assert_eq!(names(output.parent().unwrap()), ["video.mp4"]);
    assert!(!dir.join("out").exists());
    // The same into a new folder inside the project.
    let inside = dir.join("new/deeper/video.mp4");
    let ran = framesmith(&["render", text(&dir), "--output", text(&inside)]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert!(inside.is_file());
}

#[test]
fn t034_c3_spaces_and_other_scripts_in_the_path_are_kept_exactly() {
    let dir = small("t034_c3", "demo");
    let output = dir.parent().unwrap().join("my videos/été 日本/clip ✓ one.mp4");
    let ran = framesmith(&["render", text(&dir), "--output", text(&output)]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert_eq!(facts(&output).frames, 6);
    assert_eq!(names(&dir.parent().unwrap().join("my videos")), ["été 日本"]);
    assert_eq!(names(output.parent().unwrap()), ["clip ✓ one.mp4"]);
    assert!(ran.stdout.contains(&format!("commit    {}", output.display())), "{}", ran.stdout);
}

#[test]
fn t034_c4_without_output_the_video_is_at_out_and_the_project_folder_name() {
    let dir = small("t034_c4", "my project");
    let ran = framesmith(&["render", text(&dir)]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert_eq!(names(&dir.join("out")), ["my project.mp4"]);
    assert_eq!(facts(&dir.join("out/my project.mp4")).frames, 6);
    assert_eq!(names(&dir), ["index.html", "out"]);
}

#[test]
fn t034_c5_an_output_outside_the_project_leaves_the_project_folder_unchanged() {
    let dir = fixture("t034_c5", "still");
    let before = tree(&dir);
    let listing = names(&dir);
    let output = dir.parent().unwrap().join("collected/still.mp4");
    let ran = framesmith(&["render", text(&dir), "--output", text(&output), "--fps", "2"]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert!(output.is_file());
    assert_eq!(tree(&dir), before);
    assert_eq!(names(&dir), listing);
}

// ---------------------------------------------------------------------
// T035: never find a partial video at the output path
// ---------------------------------------------------------------------

/// Renders the small page to `output`, then makes the project the slow page.
fn project_with_a_video(test: &str) -> (PathBuf, PathBuf, Vec<u8>) {
    let dir = small(test, "demo");
    let output = dir.join("out/demo.mp4");
    let ran = framesmith(&["render", text(&dir)]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    let old = std::fs::read(&output).unwrap();
    write(&dir, "index.html", slow_page());
    (dir, output, old)
}

#[test]
fn t035_c1_while_a_render_runs_the_output_path_holds_the_previous_video_or_nothing() {
    // With a previous video: every look during the run finds exactly those bytes.
    let (dir, output, old) = project_with_a_video("t035_c1_previous");
    let folder = output.parent().unwrap().to_path_buf();
    let mut child = start(&["render", text(&dir)], dir.parent().unwrap(), &[]);
    let (mut looks, mut staging_seen, mut previous_seen) = (Vec::new(), false, 0);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        let bytes = std::fs::read(&output).unwrap_or_default();
        previous_seen += usize::from(bytes == old);
        looks.push(bytes);
        staging_seen |= staging_in(&folder, "demo.mp4");
        std::thread::sleep(Duration::from_millis(15));
    };
    assert!(status.success());
    let new = std::fs::read(&output).unwrap();
    assert_ne!(new, old);
    assert!(staging_seen, "the run was watched while its staging file existed");
    assert!(previous_seen >= 20, "only {previous_seen} looks found the previous video");
    // Every look found the previous video, or (only at the very end) the finished new one.
    for (number, bytes) in looks.iter().enumerate() {
        assert!(
            *bytes == old || *bytes == new,
            "look {number} found {} bytes that are neither video",
            bytes.len()
        );
    }
    assert_eq!(facts(&output).frames, 150);

    // With nothing there before: every look finds no file, then the whole file.
    let dir = project("t035_c1_nothing", "demo", &slow_page());
    let output = dir.join("out/demo.mp4");
    let mut child = start(&["render", text(&dir)], dir.parent().unwrap(), &[]);
    let (mut looks, mut nothing_seen) = (Vec::new(), 0);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        let bytes = std::fs::read(&output).unwrap_or_default();
        nothing_seen += usize::from(bytes.is_empty());
        looks.push(bytes);
        std::thread::sleep(Duration::from_millis(15));
    };
    assert!(status.success());
    let new = std::fs::read(&output).unwrap();
    assert!(nothing_seen >= 20, "only {nothing_seen} looks found nothing");
    assert!(looks.iter().all(|bytes| bytes.is_empty() || *bytes == new));
}

#[test]
fn t035_c1_at_the_start_of_the_commit_stage_the_output_is_still_the_previous_video() {
    let (dir, output, old) = project_with_a_video("t035_c1_commit");
    let folder = output.parent().unwrap().to_path_buf();
    let mut child = Command::new(BIN)
        .args(["render", text(&dir)])
        .current_dir(dir.parent().unwrap())
        .env("FRAMESMITH_DEBUG_HOLD", "commit")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut lines = BufReader::new(child.stderr.take().unwrap());
    let started = Instant::now();
    let mut line = String::new();
    loop {
        line.clear();
        assert!(lines.read_line(&mut line).unwrap() > 0, "the run ended before it held in commit");
        if line.trim() == "holding in commit" {
            break;
        }
        assert!(started.elapsed() < Duration::from_secs(60));
    }
    // Everything is encoded, yet the output path still holds the previous video.
    assert_eq!(std::fs::read(&output).unwrap(), old);
    let staging: Vec<String> = names(&folder).into_iter().filter(|name| name.ends_with(".part")).collect();
    assert_eq!(staging.len(), 1, "{:?}", names(&folder));
    assert!(staging[0].starts_with(".demo.mp4."), "{}", staging[0]);
    assert_eq!(facts(&folder.join(&staging[0])).frames, 150);
    // Cancelling there leaves the previous video and no staging file.
    let killed = measure::run(Path::new("/bin/kill"), &["-INT", &child.id().to_string()], Path::new("/"), &[]);
    assert_eq!(killed.code, 0);
    let mut rest = String::new();
    lines.read_to_string(&mut rest).unwrap();
    assert_eq!(child.wait().unwrap().code(), Some(130));
    assert_eq!(std::fs::read(&output).unwrap(), old);
    assert_eq!(names(&folder), ["demo.mp4"]);
}

#[test]
fn t035_c2_a_new_video_replaces_the_old_one_and_the_old_one_is_gone() {
    let (dir, output, old) = project_with_a_video("t035_c2");
    assert_eq!(facts(&output).frames, 6);
    let ran = framesmith(&["render", text(&dir)]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    let new = std::fs::read(&output).unwrap();
    assert_ne!(new, old);
    assert_eq!(facts(&output).frames, 150);
    // Nothing of the old video is left beside it.
    assert_eq!(names(output.parent().unwrap()), ["demo.mp4"]);
    assert!(decodes_cleanly(&output, &dir));
}

#[test]
fn t035_c3_the_previous_video_plays_normally_while_a_new_render_runs() {
    let (dir, output, old) = project_with_a_video("t035_c3");
    let folder = output.parent().unwrap().to_path_buf();
    // A player that opened the file before the run keeps its handle.
    let mut open = std::fs::File::open(&output).unwrap();
    let mut child = start(&["render", text(&dir)], dir.parent().unwrap(), &[]);
    let started = Instant::now();
    while !staging_in(&folder, "demo.mp4") {
        assert!(started.elapsed() < Duration::from_secs(30), "the render never started");
        assert!(child.try_wait().unwrap().is_none(), "the render ended too early");
        std::thread::sleep(Duration::from_millis(10));
    }
    // The run is in progress: decode the previous video from start to end, and take a frame from it.
    let facts_during = facts(&output);
    assert_eq!((facts_during.frames, facts_during.width, facts_during.height), (6, 64, 36));
    assert!(
        decodes_cleanly(&output, &dir),
        "the previous video does not decode while the render runs"
    );
    let picture = folder.parent().unwrap().join("old-frame.png");
    let frame = measure::run(
        &ffmpeg(),
        &[
            "-v",
            "error",
            "-y",
            "-i",
            text(&output),
            "-vf",
            "select=eq(n\\,3)",
            "-frames:v",
            "1",
            text(&picture),
        ],
        &dir,
        &[],
    );
    assert_eq!((frame.code, frame.stderr.trim()), (0, ""));
    assert!(picture.metadata().unwrap().len() > 0);
    assert!(child.try_wait().unwrap().is_none(), "the render had already ended");
    assert!(child.wait().unwrap().success());
    // After the replacement the handle opened earlier still reads the old video in full.
    let mut bytes = Vec::new();
    open.read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes, old);
    assert_eq!(facts(&output).frames, 150);
}

#[test]
fn t035_c4_a_succeeded_render_leaves_no_temporary_or_staging_file() {
    let dir = small("t035_c4", "demo");
    let elsewhere = dir.parent().unwrap().join("elsewhere");
    for (output, folder) in [
        (dir.join("out/demo.mp4"), dir.join("out")),
        (elsewhere.join("x.mp4"), elsewhere.clone()),
    ] {
        let ran = framesmith(&["render", text(&dir), "--output", text(&output)]);
        assert_eq!(ran.code, 0, "{}", ran.all());
        // Hidden files are counted too.
        assert_eq!(names(&folder), [output.file_name().unwrap().to_string_lossy().into_owned()]);
        // Replacing an existing video leaves nothing either.
        let ran = framesmith(&["render", text(&dir), "--output", text(&output)]);
        assert_eq!(ran.code, 0, "{}", ran.all());
        assert_eq!(names(&folder).len(), 1, "{:?}", names(&folder));
    }
    assert_eq!(names(&dir), ["index.html", "out"]);
}

#[test]
fn t035_c5_no_source_file_of_the_project_changes_in_any_render() {
    let dir = fixture("t035_c5", "pair-move");
    write(&dir, "notes/extra.css", ".a { color: red; }\n");
    write(&dir, "data/blob.bin", [0u8, 1, 2, 3, 4, 5]);
    let before = tree(&dir);
    let sources = before.len();
    let ran = framesmith(&["render", text(&dir), "--fps", "2"]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    let mut after = tree(&dir);
    assert!(after.remove(Path::new("out/pair-move.mp4")).is_some());
    assert_eq!(after, before);
    assert_eq!(after.len(), sources);

    // Rendering again over the video, to another path in the project, and a refused render change no source.
    let again = dir.join("renders/again.mp4");
    for args in [
        vec!["--fps", "2"],
        vec!["--fps", "2", "--output", text(&again)],
        vec!["--fps", "999"],
    ] {
        let mut all = vec!["render", text(&dir)];
        all.extend_from_slice(&args);
        framesmith(&all);
        let mut now = tree(&dir);
        now.retain(|path, _| !path.starts_with("out") && !path.starts_with("renders"));
        assert_eq!(now, before, "after {args:?}");
    }
    // A refused project and a failed one leave the files as they were.
    let refused = project(
        "t035_c5_refused",
        "refused",
        &page(
            "data-fs-size=\"64x36\" data-fs-duration=\"0.2\"",
            "",
            "<p>x</p>",
            "<script>\nwindow[[\"frame\", \"smith\"].join(\"\")].timeline();\n</script>",
        ),
    );
    let before = tree(&refused);
    assert_eq!(framesmith(&["render", text(&refused)]).code, 2);
    assert_eq!(tree(&refused), before);
}

// ---------------------------------------------------------------------
// T036: learn early that the output cannot be written
// ---------------------------------------------------------------------

const EARLIER: &str = "an earlier video";

#[test]
fn t036_c1_c4_c5_a_read_only_folder_is_reported_before_anything_starts_and_a_writable_path_then_works() {
    let dir = small("t036_c1", "demo");
    let locked_folder = dir.parent().unwrap().join("locked");
    write(&locked_folder, "video.mp4", EARLIER);
    let output = locked_folder.join("video.mp4");
    let _restore = read_only(&locked_folder);
    let spied = spied_render(&dir, &["--output", text(&output)]);
    let ran = &spied.ran;
    assert_eq!(ran.code, 1, "{}", ran.all());
    // c1: the path, the folder and the reason are named.
    let message = format!(
        "The output cannot be written at {}: a file cannot be made in the folder {} (Permission denied (os error 13)).",
        output.display(),
        locked_folder.display()
    );
    assert!(ran.stderr.contains("error FS-R005"), "{}", ran.all());
    assert!(ran.stderr.contains(&message), "{}", ran.all());
    // c1, c4: neither the browser nor ffmpeg was started, so no frame was captured.
    assert!(!spied.browser_started && !spied.ffmpeg_started);
    assert!(!ran.stdout.contains("probe") && !ran.stdout.contains("capture"), "{}", ran.stdout);
    // c4: the earlier video is unchanged and nothing else is in the folder.
    assert_eq!(std::fs::read_to_string(&output).unwrap(), EARLIER);
    assert_eq!(names(&locked_folder), ["video.mp4"]);
    assert!(!dir.join("out").exists());
    // c5: a writable path then renders.
    let good = dir.parent().unwrap().join("good/demo.mp4");
    let ran = framesmith(&["render", text(&dir), "--output", text(&good)]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert_eq!(facts(&good).frames, 6);
}

#[test]
fn t036_c2_c4_c5_a_parent_folder_that_cannot_be_made_is_reported_before_anything_starts() {
    let dir = small("t036_c2", "demo");
    let base = dir.parent().unwrap().to_path_buf();
    // A folder that is really a file, and a folder without the right to add to it.
    write(&base, "a file", "not a folder");
    let locked_folder = base.join("locked");
    std::fs::create_dir_all(&locked_folder).unwrap();
    let _restore = read_only(&locked_folder);
    for (name, output, blocked) in [
        ("under a file", base.join("a file/inner/video.mp4"), base.join("a file")),
        ("read-only", locked_folder.join("sub/deeper/video.mp4"), locked_folder.join("sub")),
    ] {
        let spied = spied_render(&dir, &["--output", text(&output)]);
        let ran = &spied.ran;
        assert_eq!(ran.code, 1, "{name}: {}", ran.all());
        assert!(ran.stderr.contains("error FS-R005"), "{name}: {}", ran.all());
        let start = format!("The output cannot be written at {}: the folder ", output.display());
        assert!(ran.stderr.contains(&start), "{name}: {}", ran.all());
        assert!(ran.stderr.contains(" could not be made ("), "{name}: {}", ran.all());
        assert!(ran.stderr.contains(&blocked.display().to_string()), "{name}: {}", ran.all());
        assert!(!spied.browser_started && !spied.ffmpeg_started, "{name}");
        assert!(!ran.stdout.contains("capture"), "{name}: {}", ran.stdout);
        assert!(!output.exists());
    }
    assert!(!locked_folder.join("sub").exists());
    assert!(!dir.join("out").exists());
    // c5: a writable path works afterwards.
    let good = base.join("good/demo.mp4");
    let ran = framesmith(&["render", text(&dir), "--output", text(&good)]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert!(good.is_file());
}

/// A small disk image mounted in the test's scratch folder; it is detached again when dropped.
struct Volume {
    image: PathBuf,
    mount: PathBuf,
}

impl Volume {
    fn new(test: &str) -> Volume {
        let folder = scratch(test);
        let volume = Volume {
            image: folder.join("disk.dmg"),
            mount: folder.join("volume"),
        };
        std::fs::create_dir_all(&volume.mount).unwrap();
        let tool = Path::new("/usr/bin/hdiutil");
        let name = format!("fs{}", std::process::id());
        let made = measure::run(
            tool,
            &["create", "-size", "2m", "-fs", "HFS+", "-volname", &name, "-o", text(&volume.image)],
            &folder,
            &[],
        );
        assert_eq!(made.code, 0, "{}", made.all());
        let attached = measure::run(
            tool,
            &["attach", text(&volume.image), "-mountpoint", text(&volume.mount), "-nobrowse"],
            &folder,
            &[],
        );
        assert_eq!(attached.code, 0, "{}", attached.all());
        volume
    }

    /// Kilobytes still free on the volume.
    fn free_kb(&self) -> u64 {
        let df = measure::run(Path::new("/bin/df"), &["-k", text(&self.mount)], Path::new("/"), &[]);
        let line = df.stdout.lines().nth(1).unwrap_or_else(|| panic!("{}", df.all()));
        line.split_whitespace().nth(3).unwrap().parse().unwrap()
    }

    /// Writes a filler file so that only `leave_kb` kilobytes stay free.
    fn fill_to(&self, leave_kb: u64) {
        let free = self.free_kb();
        assert!(free > leave_kb);
        std::fs::write(self.mount.join("filler"), vec![0u8; ((free - leave_kb) * 1024) as usize]).unwrap();
    }
}

impl Drop for Volume {
    fn drop(&mut self) {
        for _ in 0..5 {
            let ran = measure::run(
                Path::new("/usr/bin/hdiutil"),
                &["detach", text(&self.mount), "-force"],
                Path::new("/"),
                &[],
            );
            if ran.code == 0 {
                break;
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        let _ = std::fs::remove_file(&self.image);
    }
}

/// What a render onto an almost full disk did.
struct FullDisk {
    ran: Ran,
    run_id: String,
    output: PathBuf,
    folder_names: Vec<String>,
    earlier_kept: bool,
    ffmpeg_left: bool,
    browser_left: bool,
    free_kb: u64,
    real_size: u64,
    good_frames: u64,
}

/// Renders a composition with a lot of changing noise onto a disk that has a few kilobytes free.
fn render_onto_a_full_disk(test: &str) -> FullDisk {
    let volume = Volume::new(test);
    let dir = scratch(&format!("{test}_project")).join("noisy");
    write(
        &dir,
        "index.html",
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<title>Noise</title>\n\
<style>#n { position: absolute; left: 0; top: 0; width: 320px; height: 180px; }</style>\n</head>\n<body>\n\
<div data-fs-composition=\"main\" data-fs-size=\"320x180\" data-fs-fps=\"10\" data-fs-duration=\"2\"><img id=\"n\" src=\"noise.png\"></div>\n\
<script type=\"application/framesmith-motion+json\">\n\
[{\"target\": \"#n\", \"from\": {\"rotation\": 0, \"scale\": 1.6}, \"to\": {\"rotation\": 360, \"scale\": 1.6}, \"at\": 0, \"duration\": 2}]\n\
</script>\n</body>\n</html>\n",
    );
    let noise = measure::run(
        &ffmpeg(),
        &[
            "-v",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "nullsrc=s=320x180,format=rgb24,geq=r='random(1)*255':g='random(2)*255':b='random(3)*255'",
            "-frames:v",
            "1",
            text(&dir.join("noise.png")),
        ],
        &dir,
        &[],
    );
    assert_eq!(noise.code, 0, "{}", noise.all());

    // An earlier video on the volume, then the volume is filled up to a few kilobytes.
    let output = volume.mount.join("video.mp4");
    std::fs::write(&output, EARLIER).unwrap();
    volume.fill_to(16);
    let free_kb = volume.free_kb();

    let ran = run_in(
        dir.parent().unwrap(),
        &["render", text(&dir), "--output", text(&output), "--json"],
        &[],
    );
    let run_id = serde_json::from_str::<serde_json::Value>(ran.stdout.trim())
        .ok()
        .and_then(|value| value["run_id"].as_str().map(str::to_string))
        .unwrap_or_default();
    // The system's own bookkeeping folders on a volume are not Framesmith's.
    let mut folder_names = names(&volume.mount);
    folder_names.retain(|name| {
        !matches!(
            name.as_str(),
            ".fseventsd" | ".Trashes" | ".Spotlight-V100" | ".metadata_never_index"
        )
    });
    let earlier_kept = std::fs::read_to_string(&output).map(|text| text == EARLIER).unwrap_or(false);
    let ffmpeg_left = measure::run(Path::new("/usr/bin/pgrep"), &["-f", text(&volume.mount)], Path::new("/"), &[]).code == 0;
    let browser_left = browser_left(&run_id);

    // The same render onto a disk with room: how big the video really is, and that it completes.
    let good = dir.parent().unwrap().join("good/noisy.mp4");
    let ok = run_in(dir.parent().unwrap(), &["render", text(&dir), "--output", text(&good)], &[]);
    assert_eq!(ok.code, 0, "{}", ok.all());
    FullDisk {
        ran,
        run_id,
        output,
        folder_names,
        earlier_kept,
        ffmpeg_left,
        browser_left,
        free_kb,
        real_size: good.metadata().unwrap().len(),
        good_frames: facts(&good).frames,
    }
}

#[test]
fn t036_c3_c4_c5_a_full_disk_stops_the_run_with_exit_1_and_leaves_no_partial_file() {
    let full = render_onto_a_full_disk("t036_c3_full");
    let ran = &full.ran;
    // The video is much larger than the space that was left.
    assert!(
        full.real_size > 20 * full.free_kb * 1024,
        "{} bytes against {} KB free",
        full.real_size,
        full.free_kb
    );
    assert_eq!(ran.code, 1, "{}", ran.all());
    assert!(!full.run_id.is_empty(), "{}", ran.all());
    assert!(ran.stderr.contains("error FS-R005"), "{}", ran.all());
    assert!(ran.stderr.contains("no space left"), "{}", ran.all());
    // No partial file and no staging file on the volume; the earlier video is unchanged.
    assert_eq!(full.folder_names, ["filler", "video.mp4"]);
    assert!(full.earlier_kept);
    // c4: nothing started by the run is left running.
    assert!(!full.ffmpeg_left, "ffmpeg is still running");
    assert!(!full.browser_left, "the browser is still running");
    // c5: a writable path then renders.
    assert_eq!(full.good_frames, 20);
}

#[test]
fn t036_c3_a_full_disk_message_names_the_path_and_the_reason() {
    let full = render_onto_a_full_disk("t036_c3_names");
    assert_eq!(full.ran.code, 1, "{}", full.ran.all());
    assert!(full.ran.stderr.contains("error FS-R005"), "{}", full.ran.all());
    assert!(
        full.ran.stderr.contains(&full.output.display().to_string()),
        "the path is not named: {}",
        full.ran.all()
    );
    assert!(full.ran.stderr.contains("no space left"), "{}", full.ran.all());
}
