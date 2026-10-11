//! T038: a render interrupted in any of its five stages says so, exits 130,
//! and leaves no file and no program behind.

mod common;

use common::live::{alive, descendants, running_with, script, still_alive, Running, UserBrowser};
use common::*;
use measure::Ran;
use std::ffi::OsStr;
use std::path::PathBuf;
use std::time::{Duration, Instant};

const STAGES: [&str; 5] = ["compile", "probe", "capture", "encode", "commit"];
const EARLIER: &[u8] = b"an earlier video: these bytes must not change";

/// One test's place: the folder the command runs in, the project `demo` in
/// it, and a temporary folder of its own for the run's browser profile.
struct Place {
    root: PathBuf,
    project: PathBuf,
    tmp: PathBuf,
}

fn place(test: &str) -> Place {
    // 10 frames: 1 s at 10 fps.
    let project = project(
        test,
        "demo",
        &page(
            "data-fs-size=\"64x36\" data-fs-fps=\"10\" data-fs-duration=\"1\"",
            "",
            "<p>x</p>",
            "",
        ),
    );
    let root = project.parent().unwrap().to_path_buf();
    let tmp = root.join("tmp");
    std::fs::create_dir_all(&tmp).unwrap();
    Place { root, project, tmp }
}

impl Place {
    fn names(&self, dir: &str) -> Vec<String> {
        let mut found: Vec<String> = std::fs::read_dir(self.project.join(dir))
            .map(|entries| entries.map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect())
            .unwrap_or_default();
        found.sort();
        found
    }

    /// Programs still running that this place's run started: the browser
    /// carries the profile folder on its command line, `ffmpeg` the staging file.
    fn left_running(&self) -> Vec<String> {
        running_with(&format!("{}/", self.root.display()))
    }

    /// Starts a render that waits at the start of `stage`.
    fn held_in(&self, stage: &str, env: &[(&str, &OsStr)]) -> Running {
        let mut all: Vec<(&str, &OsStr)> = vec![("FRAMESMITH_DEBUG_HOLD", OsStr::new(stage)), ("TMPDIR", self.tmp.as_os_str())];
        all.extend_from_slice(env);
        let mut running = Running::start(&self.root, &["render", "demo"], &all);
        running.wait_for(&format!("holding in {stage}"));
        running
    }

    fn interrupted_in(&self, stage: &str) -> Ran {
        let running = self.held_in(stage, &[]);
        running.signal("INT");
        running.finish()
    }

    fn render(&self) -> Ran {
        run_in(&self.root, &["render", "demo"], &[("TMPDIR", self.tmp.as_os_str())])
    }
}

fn last_line(ran: &Ran) -> &str {
    ran.stdout.lines().last().unwrap_or_default()
}

#[test]
fn t038_c1_c2_c3_an_interrupt_in_each_of_the_five_stages_says_cancelled_exits_130_and_leaves_nothing() {
    for stage in STAGES {
        let place = place(&format!("t038_{stage}"));
        let running = place.held_in(stage, &[]);
        // While the run waits: a staging file is beside the output path. Its
        // browser is running from the capture on and its ffmpeg from the
        // encode on; by the commit both have ended by themselves.
        let staged = place.names("out");
        assert!(staged.len() == 1 && staged[0].ends_with(".part"), "{stage}: {staged:?}");
        let during = place.left_running();
        let has = |word: &str| during.iter().any(|line| line.contains(word));
        match stage {
            "compile" | "probe" | "commit" => assert!(during.is_empty(), "{stage}: {during:?}"),
            "capture" => assert!(has("--user-data-dir=") && !has("ffmpeg"), "{stage}: {during:?}"),
            _ => assert!(has("--user-data-dir=") && has("ffmpeg"), "{stage}: {during:?}"),
        }
        // Everything the run has started, the browser's helper processes included.
        let started = descendants(running.pid());
        match stage {
            "compile" | "probe" | "commit" => assert!(started.is_empty(), "{stage}: {started:?}"),
            "capture" => assert!(started.len() > 1, "{stage}: a browser and its helpers, {started:?}"),
            _ => assert!(started.len() > 2, "{stage}: a browser, its helpers and ffmpeg, {started:?}"),
        }
        running.signal("INT");
        let ran = running.finish();

        // c1: the words and the exit code.
        assert_eq!(ran.code, 130, "{stage}: {}", ran.all());
        assert_eq!(last_line(&ran), format!("Render cancelled (in {stage})"), "{stage}: {}", ran.all());
        assert!(
            !ran.stdout.contains("Rendered") && !ran.stderr.contains("error"),
            "{stage}: {}",
            ran.all()
        );
        // The run had got as far as that stage.
        let reached = |line: &str| ran.stdout.lines().any(|l| l.starts_with(line));
        match stage {
            "compile" => assert!(!reached("compile   done")),
            "probe" => assert!(reached("compile   done") && !reached("probe     done")),
            "capture" => assert!(reached("probe     done") && !reached("capture")),
            "encode" => assert!(reached("capture   10/10") && !reached("encode    done")),
            _ => assert!(reached("encode    done") && !reached("commit")),
        }
        // c2: no video, no staging file, no output folder: the project is as it was.
        let files: Vec<String> = tree(&place.project).keys().map(|p| p.to_string_lossy().into_owned()).collect();
        assert_eq!(files, ["index.html"], "{stage}");
        assert!(!place.project.join("out").exists(), "{stage}: the output folder was left");
        // c3: no browser and no ffmpeg of the run is still running, and its profile folder is gone.
        assert_eq!(place.left_running(), Vec::<String>::new(), "{stage}");
        assert_eq!(still_alive(&started), Vec::<u32>::new(), "{stage}: of {started:?}");
        assert_eq!(
            std::fs::read_dir(&place.tmp).unwrap().count(),
            0,
            "{stage}: the profile folder was left"
        );
    }
}

#[test]
fn t038_c1_a_terminate_signal_from_a_calling_program_is_a_cancel_too() {
    let place = place("t038_term");
    let running = place.held_in("capture", &[]);
    running.signal("TERM");
    let ran = running.finish();
    assert_eq!(ran.code, 130, "{}", ran.all());
    assert_eq!(last_line(&ran), "Render cancelled (in capture)");
    assert!(!place.project.join("out").exists());
    assert_eq!(place.left_running(), Vec::<String>::new());
}

#[test]
fn t038_c4_an_earlier_video_is_byte_for_byte_unchanged_after_a_cancel_in_any_stage() {
    for stage in STAGES {
        let place = place(&format!("t038_c4_{stage}"));
        write(&place.project, "out/demo.mp4", EARLIER);
        let ran = place.interrupted_in(stage);
        assert_eq!(ran.code, 130, "{stage}: {}", ran.all());
        assert_eq!(std::fs::read(place.project.join("out/demo.mp4")).unwrap(), EARLIER, "{stage}");
        assert_eq!(
            place.names("out"),
            ["demo.mp4"],
            "{stage}: something was left beside the earlier video"
        );
    }
}

#[test]
fn t038_c5_the_users_own_browser_is_still_running_with_its_windows_unchanged() {
    let place = place("t038_c5");
    let user = UserBrowser::start(&place.root);
    let before = user.windows().expect("the user's browser answers");
    assert_eq!(before.len(), 1, "{before:?}");
    assert!(before[0].ends_with("mine.html a page of the user"), "{before:?}");
    for stage in ["probe", "capture", "encode"] {
        let ran = place.interrupted_in(stage);
        assert_eq!(ran.code, 130, "{stage}: {}", ran.all());
        assert!(alive(user.pid()), "{stage}: the user's browser was stopped");
        assert_eq!(user.windows().as_ref(), Some(&before), "{stage}");
    }
    // Only the user's own browser is left of everything started in this place.
    let left = place.left_running();
    assert!(
        !left.is_empty() && left.iter().all(|line| line.contains("the-users-own-profile")),
        "{left:?}"
    );
}

#[test]
fn t038_c6_children_that_ignore_the_request_to_stop_are_forced_to_stop() {
    let place = place("t038_c6");
    let tools = place.root.join("tools");
    std::fs::create_dir_all(&tools).unwrap();
    let notes = place.root.join("notes.txt");
    // A browser that stays after it was asked to close: the real browser
    // does close, but the program Framesmith started ignores every signal
    // it can and never ends by itself.
    let stubborn_browser = tools.join("stubborn-browser");
    script(
        &stubborn_browser,
        &format!(
            "trap 'echo \"browser: asked to stop, staying\" >> \"{notes}\"' TERM INT HUP\n\"{real}\" \"$@\" &\nwait $!\n\
echo \"browser: the real browser closed, staying\" >> \"{notes}\"\nwhile :; do sleep 1; done",
            notes = notes.display(),
            real = browser().display()
        ),
    );
    // An ffmpeg that ignores the request to stop in the same way.
    script(
        &tools.join("ffmpeg"),
        &format!(
            "trap 'echo \"ffmpeg: asked to stop, staying\" >> \"{notes}\"' TERM INT HUP\n\"{real}\" \"$@\" <&0 &\nwait $!\n\
echo \"ffmpeg: the real ffmpeg ended, staying\" >> \"{notes}\"\nwhile :; do sleep 1; done",
            notes = notes.display(),
            real = ffmpeg().display()
        ),
    );
    let path = format!("{}:{}", tools.display(), std::env::var("PATH").unwrap());
    let env = [("FRAMESMITH_BROWSER", stubborn_browser.as_os_str()), ("PATH", OsStr::new(&path))];
    // In the encode stage both children are running.
    let running = place.held_in("encode", &env);
    let during = place.left_running();
    assert!(during.iter().any(|line| line.contains("stubborn-browser")), "{during:?}");
    assert!(during.iter().any(|line| line.contains("tools/ffmpeg")), "{during:?}");
    let started = descendants(running.pid());
    let asked = Instant::now();
    running.signal("INT");
    let ran = running.finish();
    let took = asked.elapsed();
    assert_eq!(ran.code, 130, "{}", ran.all());
    assert_eq!(last_line(&ran), "Render cancelled (in encode)");
    assert!(took < Duration::from_secs(15), "the command took {took:?} to stop");
    // Both were asked and stayed; then they were forced. Nothing is left.
    let noted = std::fs::read_to_string(&notes).unwrap_or_default();
    assert!(noted.contains("browser: the real browser closed, staying"), "{noted}");
    assert!(noted.contains("ffmpeg: asked to stop, staying"), "{noted}");
    assert_eq!(place.left_running(), Vec::<String>::new());
    assert_eq!(still_alive(&started), Vec::<u32>::new(), "of {started:?}");
    assert!(!place.project.join("out").exists());
    println!(
        "forced stop took {:.1} s; the stubborn programs noted:\n{noted}",
        took.as_secs_f64()
    );
}

#[test]
fn t038_c7_a_render_after_a_cancel_starts_from_the_first_frame_and_completes() {
    let place = place("t038_c7");
    let ran = place.interrupted_in("encode");
    assert_eq!(ran.code, 130, "{}", ran.all());
    assert!(
        ran.stdout.contains("capture   10/10"),
        "all ten frames had been captured: {}",
        ran.all()
    );
    let again = place.render();
    assert_eq!(again.code, 0, "{}", again.all());
    // The counter starts at 0 again: nothing of the cancelled run is reused.
    let captures: Vec<&str> = again.stdout.lines().filter(|line| line.starts_with("capture")).collect();
    assert_eq!(captures.first(), Some(&"capture   0/10"), "{}", again.stdout);
    assert_eq!(captures.last(), Some(&"capture   10/10"), "{}", again.stdout);
    assert!(
        last_line(&again).starts_with("Rendered 10 frames, 64x36, 10 fps, in "),
        "{}",
        again.stdout
    );
    let video = facts(&place.project.join("out/demo.mp4"));
    assert_eq!((video.frames, video.width, video.height), (10, 64, 36));
    assert_eq!(place.names("out"), ["demo.mp4"]);
}
