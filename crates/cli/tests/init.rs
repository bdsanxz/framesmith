//! T046: `init` makes a starter project that checks and renders at once.

mod common;

use common::*;
use std::ffi::OsStr;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

const NEXT: &str = "Next:\n  framesmith check demo\n  framesmith render demo\n";
const TITLE_TEXT: [u8; 3] = [0xf4, 0xf1, 0xea];

/// A scratch folder for the test with `init demo` already run in it.
fn made(test: &str) -> (PathBuf, PathBuf) {
    let root = scratch(test);
    let ran = run_in(&root, &["init", "demo"], &[]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    let demo = root.join("demo");
    (root, demo)
}

fn names(dir: &Path) -> Vec<String> {
    let mut found: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    found.sort();
    found
}

#[test]
fn t046_c1_init_makes_the_folder_and_prints_the_two_next_commands() {
    let root = scratch("t046_c1");
    let ran = run_in(&root, &["init", "demo"], &[]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert_eq!(ran.stdout, format!("Created demo\n{NEXT}"));
    assert_eq!(ran.stderr, "");
    assert_eq!(names(&root.join("demo")), ["index.html", "style.css"]);
    let html = std::fs::read_to_string(root.join("demo/index.html")).unwrap();
    assert!(html.contains("data-fs-composition") && html.contains("application/framesmith-motion+json"));

    // The path is printed as it was given, and missing parents are made.
    let deep = root.join("a/b/c");
    let ran = run_in(&root, &["init", text(&deep)], &[]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    let shown = deep.display();
    assert_eq!(
        ran.stdout,
        format!("Created {shown}\nNext:\n  framesmith check {shown}\n  framesmith render {shown}\n")
    );
    assert_eq!(names(&deep), ["index.html", "style.css"]);

    // An existing empty folder is used.
    std::fs::create_dir_all(root.join("empty")).unwrap();
    let ran = run_in(&root, &["init", "empty"], &[]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert_eq!(names(&root.join("empty")), ["index.html", "style.css"]);
}

#[test]
fn t046_c2_check_passes_on_the_untouched_starter_with_clips_and_tweens() {
    let (root, demo) = made("t046_c2");
    let before = tree(&demo);
    // The command the starter printed, as printed.
    let ran = run_in(&root, &["check", "demo"], &[]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert_eq!(
        ran.stdout,
        "check passed\n  composition  main\n  size         1280x720\n  rate         30 fps\n  length       5 s\n  frames       150\n  clips        3\n  tweens       5\n"
    );
    assert_eq!(ran.stderr, "");
    assert_eq!(tree(&demo), before, "check changed a file");
}

/// The title's pixels in a picture: the part of the picture the title can be in,
/// matched on its colour (wide enough to count a half-faded edge).
fn title(frame: &measure::Frame) -> (Option<u32>, usize) {
    let area = frame.crop(100, 270, 900, 150);
    (area.bounds(TITLE_TEXT, 150).map(|b| b.top), area.count(TITLE_TEXT, 150))
}

#[test]
fn t046_c3_render_of_the_untouched_starter_makes_a_playable_video_in_which_text_enters_with_motion() {
    // A tenth of the usual frame rate keeps this test short; the starter is otherwise untouched.
    let (root, demo) = made("t046_c3_motion");
    let (ran, frames) = render_frames(&demo, &["--fps", "10"], &[]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert_eq!(frames.len(), 50);
    let shown: Vec<(Option<u32>, usize)> = frames.iter().map(title).collect();

    // Before its clip starts (0.5 s) the title is not in the picture; in the clip's first picture it is still clear.
    for (number, (top, count)) in shown.iter().enumerate().take(6) {
        assert_eq!((*top, *count), (None, 0), "the title is already there in picture {number}");
    }
    // Then it comes in: each picture has it higher than the one before, and more of it, until it settles.
    for number in 6..10 {
        let (now, then) = (&shown[number], &shown[number + 1]);
        assert!(
            now.0.unwrap() > then.0.unwrap(),
            "picture {number} -> {}: {now:?} {then:?}",
            number + 1
        );
        assert!(now.1 < then.1, "picture {number} -> {}: {now:?} {then:?}", number + 1);
    }
    let (start, settled) = (shown[6], shown[11]);
    assert!(
        start.0.unwrap() >= settled.0.unwrap() + 30,
        "it moved up by {} px only",
        start.0.unwrap() - settled.0.unwrap()
    );
    assert!(start.1 < settled.1, "it did not fade in: {start:?} {settled:?}");
    // (From 1.2 s the subtitle comes in, so only the title's top edge is compared.)
    assert_eq!(
        (shown[11].0, shown[30].0, shown[40].0),
        (shown[11].0, shown[11].0, shown[11].0),
        "it stays where it settled"
    );
    assert_eq!(shown[30], shown[40]);
    // It leaves again at the end, and is gone with its clip.
    assert!(shown[47].1 < shown[30].1);
    assert_eq!(shown[49], (None, 0));

    // The same is seen in the file that was made, within what the encoder changes.
    let video = demo.join("out/demo.mp4");
    let facts = facts(&video);
    assert_eq!(
        (facts.width, facts.height, facts.frames, facts.frame_rate.as_str()),
        (1280, 720, 50, "10/1")
    );
    let pictures = measure::frames_from_video(&ffmpeg(), &video, 1280, 720).unwrap();
    let seen: Vec<(Option<u32>, usize)> = pictures.iter().map(title).collect();
    // A few stray pixels of encoder noise are not a title (the title has more than 10000 of them).
    for (number, (_, count)) in seen.iter().enumerate().take(5) {
        assert!(*count < 50, "the title is in the video's picture {number}: {count} pixels");
    }
    assert!(seen[6].0.unwrap() >= seen[11].0.unwrap() + 30, "{:?} {:?}", seen[6], seen[11]);
    assert!(seen[6].1 < seen[11].1);
    let _ = root;
}

#[test]
fn t046_c3_render_of_the_untouched_starter_at_its_own_rate_makes_a_file_that_decodes_to_the_end() {
    let (root, demo) = made("t046_c3_full");
    // The command the starter printed, as printed.
    let ran = run_in(&root, &["render", "demo"], &[]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    let video = demo.join("out/demo.mp4");
    let facts = facts(&video);
    assert_eq!((facts.video_streams, facts.audio_streams), (1, 0));
    assert_eq!((facts.codec.as_str(), facts.pixel_format.as_str()), ("h264", "yuv420p"));
    assert_eq!(
        (facts.width, facts.height, facts.frame_rate.as_str(), facts.frames),
        (1280, 720, "30/1", 150)
    );
    assert!((facts.duration - 5.0).abs() < 0.001, "{}", facts.duration);
    // Every picture decodes without a complaint.
    let decoded = measure::run(&ffmpeg(), &["-v", "error", "-i", text(&video), "-f", "null", "-"], &root, &[]);
    assert_eq!((decoded.code, decoded.stderr.trim()), (0, ""));
}

#[test]
fn t046_c4_the_starter_holds_only_files_we_wrote_and_no_outside_reference() {
    let (_, demo) = made("t046_c4");
    // Only the two files, the same as the ones kept in the source of the command, and nothing hidden or nested.
    assert_eq!(names(&demo), ["index.html", "style.css"]);
    let starter = Path::new(env!("CARGO_MANIFEST_DIR")).join("starter");
    assert_eq!(names(&starter), ["index.html", "style.css"]);
    for name in ["index.html", "style.css"] {
        assert_eq!(
            std::fs::read(demo.join(name)).unwrap(),
            std::fs::read(starter.join(name)).unwrap(),
            "{name}"
        );
        assert!(demo.join(name).is_file());
    }

    let forbidden = [
        "http:",
        "https:",
        "//",
        "@import",
        "<script src",
        " src=",
        "url(",
        "@font-face",
        "<img",
        "<video",
        "<audio",
        "<iframe",
        "<source",
        "<object",
        "<embed",
        "<svg",
        "@namespace",
        "ftp:",
        "data:",
        "font:",
    ];
    for name in ["index.html", "style.css"] {
        let content = std::fs::read_to_string(demo.join(name)).unwrap().to_lowercase();
        for word in forbidden {
            assert!(!content.contains(word), "{name} holds {word:?}");
        }
        // No unusual bytes either: it is plain text.
        assert!(content.is_ascii(), "{name} is not plain ASCII");
    }

    // The one `link` points at the style sheet, and the only script is the motion block.
    let html = std::fs::read_to_string(demo.join("index.html")).unwrap();
    let links: Vec<&str> = html
        .match_indices("<link")
        .map(|(at, _)| &html[at..at + html[at..].find('>').unwrap() + 1])
        .collect();
    assert_eq!(links, ["<link rel=\"stylesheet\" href=\"style.css\">"]);
    let scripts: Vec<&str> = html
        .match_indices("<script")
        .map(|(at, _)| &html[at..at + html[at..].find('>').unwrap() + 1])
        .collect();
    assert_eq!(scripts, ["<script type=\"application/framesmith-motion+json\">"]);

    // The fonts named are ones every machine has: common system families and the generic ones.
    let allowed = [
        "helvetica neue",
        "helvetica",
        "arial",
        "sans-serif",
        "serif",
        "monospace",
        "system-ui",
    ];
    let mut stacks = 0;
    for name in ["index.html", "style.css"] {
        let content = std::fs::read_to_string(demo.join(name)).unwrap();
        for (at, _) in content.match_indices("font-family:") {
            let value = content[at + "font-family:".len()..].split([';', '}']).next().unwrap();
            for family in value.split(',') {
                let family = family.trim().trim_matches('"').trim_matches('\'').to_lowercase();
                assert!(allowed.contains(&family.as_str()), "{name} names the font {family:?}");
            }
            stacks += 1;
        }
    }
    assert!(stacks >= 1, "the starter names no font at all");
}

#[test]
fn t046_c5_a_folder_that_exists_and_is_not_empty_is_refused_and_left_as_it_was() {
    let root = scratch("t046_c5");
    let cases: [(&str, &[&str]); 4] = [
        ("with-a-file", &["notes.txt"]),
        ("with-a-hidden-file", &[".hidden"]),
        ("with-a-folder", &["inside/deeper.txt"]),
        ("with-a-page", &["index.html"]),
    ];
    for (folder, files) in cases {
        for file in files {
            write(&root, &format!("{folder}/{file}"), "mine");
        }
        let before = tree(&root.join(folder));
        let ran = run_in(&root, &["init", folder], &[]);
        assert_eq!(ran.code, 1, "{folder}: {}", ran.all());
        assert_eq!(
            ran.stderr,
            format!(
                "error FS-R014\n  The folder {folder} already exists and is not empty; nothing was written.\n  fix: Choose a folder name that does not exist yet, or empty the folder first.\n"
            ),
            "{folder}"
        );
        assert_eq!(ran.stdout, "", "no next steps are offered");
        assert_eq!(tree(&root.join(folder)), before, "{folder} changed");
    }

    // Running init twice on the same name: the second run changes nothing in what the first made.
    let (root, demo) = made("t046_c5_twice");
    let before = tree(&demo);
    let again = run_in(&root, &["init", "demo"], &[]);
    assert_eq!(again.code, 1, "{}", again.all());
    assert!(again.stderr.contains("error FS-R014"), "{}", again.stderr);
    assert!(
        again.stderr.contains("already exists and is not empty; nothing was written."),
        "{}",
        again.stderr
    );
    assert_eq!(tree(&demo), before);
}

#[test]
fn t046_c6_a_folder_that_cannot_be_created_is_named_with_the_reason() {
    let root = scratch("t046_c6");
    let parent = root.join("locked");
    std::fs::create_dir_all(&parent).unwrap();
    std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o555)).unwrap();
    let target = parent.join("demo");
    let ran = run_in(&root, &["init", text(&target)], &[]);
    let nested = run_in(&root, &["init", text(&parent.join("one/two"))], &[]);
    let left = names(&parent);
    std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o755)).unwrap();

    assert_eq!(ran.code, 1, "{}", ran.all());
    assert_eq!(
        ran.stderr,
        format!(
            "error FS-R015\n  The folder {} cannot be created: Permission denied (os error 13).\n  fix: Choose a folder in a place you can write to.\n",
            target.display()
        )
    );
    assert_eq!(ran.stdout, "");
    assert_eq!(nested.code, 1, "{}", nested.all());
    assert!(nested.stderr.contains("error FS-R015"), "{}", nested.stderr);
    assert!(nested.stderr.contains("Permission denied (os error 13)"), "{}", nested.stderr);
    assert!(left.is_empty(), "something was made in the locked folder: {left:?}");

    // Something that is not a folder is in the way.
    write(&root, "a-file", "mine");
    for (path, reason) in [
        (root.join("a-file"), "something that is not a folder is already there"),
        (root.join("a-file/demo"), "Not a directory (os error 20)"),
    ] {
        let ran = run_in(&root, &["init", text(&path)], &[]);
        assert_eq!(ran.code, 1, "{}", ran.all());
        assert!(
            ran.stderr.contains(&format!(
                "error FS-R015\n  The folder {} cannot be created: {reason}",
                path.display()
            )),
            "{}",
            ran.stderr
        );
    }
    assert_eq!(std::fs::read_to_string(root.join("a-file")).unwrap(), "mine");
}

#[test]
fn t046_c7_the_log_has_the_command_its_input_and_its_result_under_one_run_id() {
    let root = scratch("t046_c7");
    let logs = root.join("logs");
    let env: [(&str, &OsStr); 1] = [("FRAMESMITH_LOG_DIR", logs.as_os_str())];
    let ran = run_in(&root, &["init", "demo"], &env);
    assert_eq!(ran.code, 0, "{}", ran.all());
    let log = std::fs::read_to_string(logs.join("framesmith.log")).unwrap();
    let lines: Vec<&str> = log.lines().collect();
    assert_eq!(lines.len(), 2, "{lines:?}");
    let word = |line: &str, at: usize| line.split(' ').filter(|w| !w.is_empty()).nth(at).unwrap().to_string();
    let run = word(lines[0], 3);
    assert!(run.starts_with("run=") && run.len() == 16, "{run}");
    assert_eq!(word(lines[1], 3), run, "one run id for both lines");
    assert_eq!((word(lines[0], 1).as_str(), word(lines[0], 2).as_str()), ("INFO", "cli"));
    assert!(lines[0].contains("command started command=\"init\" folder=demo"), "{}", lines[0]);
    assert_eq!(word(lines[1], 1), "INFO");
    assert!(lines[1].ends_with("command finished command=\"init\" exit=0"), "{}", lines[1]);

    // A refusal is told the same way: the input, an error level, and the code of the refusal, under a new run id.
    let again = run_in(&root, &["init", "demo"], &env);
    assert_eq!(again.code, 1, "{}", again.all());
    let log = std::fs::read_to_string(logs.join("framesmith.log")).unwrap();
    let lines: Vec<&str> = log.lines().collect();
    assert_eq!(lines.len(), 4, "{lines:?}");
    let second = word(lines[2], 3);
    assert_ne!(second, run);
    assert_eq!(word(lines[3], 3), second);
    assert!(lines[2].contains("command started command=\"init\" folder=demo"), "{}", lines[2]);
    assert_eq!(word(lines[3], 1), "ERROR");
    assert!(
        lines[3].ends_with("command finished command=\"init\" exit=1 code=FS-R014"),
        "{}",
        lines[3]
    );
}
