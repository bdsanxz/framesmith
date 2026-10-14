//! What the run log never holds: the content of a composition, a value of the environment, and the
//! name of another user's home folder (`docs/spec/run-log.md` section 6).

mod common;

use std::ffi::OsStr;
use std::fs;

use cli::runlog::{input_word, word_list};
use common::*;

const GROUP: &str = "run-log-content";

/// A composition whose every kind of content carries a word that must not reach the log.
const WITH_CONTENT: &str = r#"<composition version="1" id="needle-id" width="needle-width" height="720" duration="5" background="needle-colour" class="needle-class">
  <text class="needle-style">needle-text spoken by nobody</text>
  <!-- needle-comment -->
</composition>
"#;

const NEEDLES: [&str; 7] = [
    "needle-id",
    "needle-width",
    "needle-colour",
    "needle-class",
    "needle-style",
    "needle-text",
    "needle-comment",
];

#[test]
fn the_log_of_a_run_holds_no_content_of_the_composition() {
    let (project, cwd) = project(GROUP, "composition-content", WITH_CONTENT);
    let mut printed = String::new();
    for level in ["trace", "debug", "info"] {
        let output = framesmith(&cwd, [OsStr::new("check"), project.as_os_str()], &[("FRAMESMITH_LOG", level)]);
        printed.push_str(&out(&output));
    }
    // The control: the command itself shows the value it found wrong, so the words are in play.
    assert!(printed.contains("needle-width"), "{printed}");

    let log = log_text(&cwd);
    assert!(log.lines().count() >= 3 * 2, "{log}");
    for needle in NEEDLES {
        assert!(!log.contains(needle), "{needle} is in the log:\n{log}");
    }
    // What the log does hold of the problem is its code and its line.
    assert!(log.contains(r#"code="FS-0007" line=1"#), "{log}");
}

#[test]
fn the_log_of_a_run_holds_no_value_from_the_environment() {
    let (project, cwd) = project(GROUP, "environment", VALID);
    let env = [
        ("FRAMESMITH_LOG", "needle-level"),
        ("FRAMESMITH_OTHER", "needle-variable"),
        ("USER", "needle-user"),
        ("LOGNAME", "needle-logname"),
        ("HOME", "/Users/needle-home"),
        ("TMPDIR", "/needle-tmp/"),
        ("LANG", "needle-lang"),
        ("TERM", "needle-term"),
    ];
    // Once with the level unreadable (its value is the one to hide), and once at the most detail.
    framesmith(&cwd, [OsStr::new("check"), project.as_os_str()], &env);
    let mut detailed = env;
    detailed[0] = ("FRAMESMITH_LOG", "trace");
    framesmith(&cwd, [OsStr::new("check"), project.as_os_str()], &detailed);
    // And with a project that does not exist, as a path inside the pretend home folder.
    framesmith(&cwd, [OsStr::new("check"), OsStr::new("/Users/needle-home/demo")], &detailed);

    let log = log_text(&cwd);
    assert!(log.lines().count() >= 6, "{log}");
    assert!(!log.contains("needle"), "{log}");
    // The home folder of the user running the command is written as `~`.
    assert!(log.contains(r#"args=["check","~/demo"]"#), "{log}");
}

#[test]
fn the_log_of_a_run_holds_no_home_folder_name_of_another_user() {
    let (project, cwd) = project(GROUP, "another-user", VALID);
    let real_home = std::env::var("HOME").unwrap();
    let real_user = std::env::var("USER").unwrap_or_default();
    // Words that name another user's home folder, and one that names the user's own.
    framesmith(&cwd, [OsStr::new("check"), OsStr::new("/Users/needle-alice/demo")], &[]);
    framesmith(&cwd, [OsStr::new("check"), OsStr::new("/home/needle-bob/projects/demo")], &[]);
    framesmith(
        &cwd,
        [OsStr::new("check"), OsStr::new("/Users/needle-alice")],
        &[("FRAMESMITH_LOG", "trace")],
    );
    // The project itself, given by a full path that lies under this user's home folder.
    framesmith(&cwd, [OsStr::new("check"), project.as_os_str()], &[]);

    let log = log_text(&cwd);
    assert!(log.lines().count() >= 8, "{log}");
    assert!(!log.contains("needle-alice") && !log.contains("needle-bob"), "{log}");
    assert!(log.contains(r#"args=["check","~other/demo"]"#), "{log}");
    assert!(log.contains(r#"args=["check","~other/projects/demo"]"#), "{log}");
    assert!(log.contains(r#"args=["check","~other"]"#), "{log}");
    // The user's own home folder and name are not in it either.
    assert!(!log.contains(&real_home), "{log}");
    if real_user.len() > 2 && real_home.contains(&real_user) {
        // A short name can be in a time or a run id by chance, so only the messages and fields are searched.
        assert!(!after_run_id(&log).contains(&real_user), "{log}");
    }
}

#[test]
fn the_working_folder_is_not_written_in_the_log() {
    let (project, cwd) = project(GROUP, "working-folder", VALID);
    framesmith(&cwd, [OsStr::new("check"), project.as_os_str()], &[("FRAMESMITH_LOG", "trace")]);
    let log = log_text(&cwd);
    assert!(!log.contains(cwd.to_str().unwrap()), "{log}");
    assert!(!log.contains("/cwd"), "{log}");
}

#[test]
fn a_path_under_a_home_folder_has_the_user_name_replaced_and_any_other_word_is_left_as_it_is() {
    let home = Some("/Users/me");
    let cases = [
        ("/Users/me/demo", "~/demo"),
        ("/Users/me", "~"),
        ("/Users/me/", "~/"),
        ("/Users/someone/demo", "~other/demo"),
        ("/Users/megan/demo", "~other/demo"),
        ("/home/someone/demo", "~other/demo"),
        ("--out=/Users/someone/x", "--out=~other/x"),
        ("/Users/someone/a:/Users/me/b", "~other/a:~/b"),
        ("/Users/", "~other"),
        ("demo", "demo"),
        ("./demo/Users", "./demo/Users"),
        ("/tmp/demo", "/tmp/demo"),
        ("", ""),
    ];
    for (word, written) in cases {
        assert_eq!(input_word(word, home), written, "{word}");
    }
    // With no home folder known, every user is another user.
    assert_eq!(input_word("/Users/me/demo", None), "~other/demo");
}

#[test]
fn the_words_of_a_command_line_are_written_as_one_quoted_list() {
    assert_eq!(word_list(&["check", "de mo", "a\"b", "c\nd"]), r#"["check","de mo","a\"b","c\nd"]"#);
    assert_eq!(word_list::<&str>(&[]), "[]");
}

#[test]
fn a_log_line_is_one_line_whatever_the_command_line_holds() {
    let (_, cwd) = project(GROUP, "one-line", VALID);
    framesmith(&cwd, [OsStr::new("check"), OsStr::new("a\nb\tc\u{1b}[31m")], &[]);
    let log = log_text(&cwd);
    assert_eq!(log.lines().count(), 2, "{log:?}");
    assert!(!log.bytes().any(|b| b == 0x1b || (b < 0x20 && b != b'\n')), "{log:?}");
    assert!(log.contains(r#"args=["check","a\nb\tc\u{1b}[31m"]"#), "{log:?}");
}

#[test]
fn the_log_file_holds_only_what_the_command_wrote_no_part_of_the_project_is_copied() {
    let (project, cwd) = project(GROUP, "no-copy", WITH_CONTENT);
    fs::write(project.join("framesmith.yaml"), "name: needle-yaml\n").unwrap();
    framesmith(&cwd, [OsStr::new("check"), project.as_os_str()], &[("FRAMESMITH_LOG", "trace")]);
    assert!(!log_text(&cwd).contains("needle"));
}
