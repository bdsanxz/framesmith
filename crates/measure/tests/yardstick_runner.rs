//! The runner of the yardstick tests (`docs/dev/gates.md`, section 5): a comparison whose program is
//! absent is reported as skipped, by name, and never as passed; a plain run lists the tests as
//! ignored and starts none. The file is named `yardstick*` because it spells the programs' names; the
//! tests in it are core tests and start no program.

use measure::yardstick::{self, Needs, Test};
use std::sync::atomic::{AtomicUsize, Ordering};

static FIRST_RAN: AtomicUsize = AtomicUsize::new(0);
static SECOND_RAN: AtomicUsize = AtomicUsize::new(0);
static ABSENT_RAN: AtomicUsize = AtomicUsize::new(0);
static LISTED_RAN: AtomicUsize = AtomicUsize::new(0);
static PRESENT_RAN: AtomicUsize = AtomicUsize::new(0);

fn first() {
    FIRST_RAN.fetch_add(1, Ordering::SeqCst);
}
fn second() {
    SECOND_RAN.fetch_add(1, Ordering::SeqCst);
}
fn absent_one() {
    ABSENT_RAN.fetch_add(1, Ordering::SeqCst);
}
fn absent_two() {
    ABSENT_RAN.fetch_add(1, Ordering::SeqCst);
}
fn listed() {
    LISTED_RAN.fetch_add(1, Ordering::SeqCst);
}
fn present_one() {
    PRESENT_RAN.fetch_add(1, Ordering::SeqCst);
}
fn present_two() {
    PRESENT_RAN.fetch_add(1, Ordering::SeqCst);
}
fn breaks() {
    panic!("a limit was missed");
}

fn present() -> Result<(), String> {
    Ok(())
}
fn absent() -> Result<(), String> {
    Err("ffmpeg was not found in any folder of the search path".to_string())
}

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|a| (*a).to_string()).collect()
}

fn test(name: &'static str, run: fn()) -> Test {
    Test { name, run }
}

/// Runs `tests` as `list` asks and gives the exit code and what was printed.
fn run(list: &[&str], needs: &Needs, tests: &[Test]) -> (i32, String) {
    let mut out = Vec::new();
    let code = yardstick::run(&args(list), needs, tests, &mut out);
    (code, String::from_utf8(out).unwrap())
}

#[test]
fn with_the_program_absent_each_test_is_skipped_by_name_and_none_is_run_or_passed() {
    let needs = Needs {
        program: "ffmpeg and ffprobe",
        check: absent,
    };
    let tests = [test("yardstick_b_second", absent_two), test("yardstick_a_first", absent_one)];
    let (code, text) = run(&["--ignored", "yardstick_"], &needs, &tests);
    assert_eq!(code, 0, "{text}");
    assert!(
        text.contains("test yardstick_a_first ... skipped, ffmpeg was not found in any folder of the search path; nothing was compared\n"),
        "{text}"
    );
    assert!(
        text.contains("test yardstick_b_second ... skipped, ffmpeg was not found in any folder of the search path; nothing was compared\n"),
        "{text}"
    );
    assert!(text.contains("test result: skipped. 0 passed; 0 failed; 2 skipped;"), "{text}");
    assert!(!text.contains("... ok"), "{text}");
    assert!(!text.contains("test result: ok"), "{text}");
    assert_eq!(ABSENT_RAN.load(Ordering::SeqCst), 0, "no test body ran");
    let a = text.find("yardstick_a_first").unwrap();
    let b = text.find("yardstick_b_second").unwrap();
    assert!(a < b, "the tests are in the order of their names");
}

#[test]
fn with_the_program_present_each_test_runs_and_is_reported_ok() {
    let needs = Needs {
        program: "ffmpeg and ffprobe",
        check: present,
    };
    let tests = [test("yardstick_one", present_one), test("yardstick_two", present_two)];
    let (code, text) = run(&["--ignored", "yardstick_"], &needs, &tests);
    assert_eq!(code, 0, "{text}");
    assert!(
        text.contains("test yardstick_one ... ok\n") && text.contains("test yardstick_two ... ok\n"),
        "{text}"
    );
    assert!(text.contains("test result: ok. 2 passed; 0 failed; 0 skipped;"), "{text}");
    assert!(!text.contains("skipped,"), "{text}");
    assert_eq!(PRESENT_RAN.load(Ordering::SeqCst), 2);
}

#[test]
fn a_test_that_misses_a_limit_is_reported_failed_by_name_and_the_run_exits_101() {
    let needs = Needs {
        program: "ffmpeg and ffprobe",
        check: present,
    };
    let tests = [test("yardstick_misses_a_limit", breaks)];
    let (code, text) = run(&["--include-ignored"], &needs, &tests);
    assert_eq!(code, 101, "{text}");
    assert!(text.contains("test yardstick_misses_a_limit ... FAILED\n"), "{text}");
    assert!(text.contains("failures:\n\n    yardstick_misses_a_limit\n"), "{text}");
    assert!(text.contains("test result: FAILED. 0 passed; 1 failed;"), "{text}");
}

#[test]
fn a_plain_run_lists_each_test_as_ignored_with_its_reason_and_starts_none() {
    let needs = Needs {
        program: "the reference",
        check: present,
    };
    let tests = [test("yardstick_reference_first", first), test("yardstick_reference_second", second)];
    let (code, text) = run(&[], &needs, &tests);
    assert_eq!(code, 0, "{text}");
    for name in ["yardstick_reference_first", "yardstick_reference_second"] {
        assert!(
            text.contains(&format!(
                "test {name} ... ignored, yardstick: needs the reference, which it starts from outside\n"
            )),
            "{text}"
        );
    }
    assert!(text.contains("test result: ok. 0 passed; 0 failed; 2 ignored;"), "{text}");
    assert_eq!(FIRST_RAN.load(Ordering::SeqCst) + SECOND_RAN.load(Ordering::SeqCst), 0);
}

#[test]
fn names_to_match_exact_skip_and_list_choose_the_tests_like_the_rust_harness() {
    let needs = Needs {
        program: "ffmpeg and ffprobe",
        check: absent,
    };
    let tests = [
        test("yardstick_alpha", listed),
        test("yardstick_beta", listed),
        test("yardstick_alphabet", listed),
        test("gamma_is_not_one", listed),
    ];
    let (_, text) = run(&["--ignored", "yardstick_"], &needs, &tests);
    assert!(!text.contains("gamma_is_not_one"), "{text}");
    let (_, text) = run(&["--ignored", "yardstick_alpha", "--exact"], &needs, &tests);
    assert!(
        text.contains("yardstick_alpha ... skipped") && !text.contains("yardstick_alphabet"),
        "{text}"
    );
    let (_, text) = run(&["--ignored", "yardstick_", "--skip", "alpha"], &needs, &tests);
    assert!(text.contains("yardstick_beta ... skipped") && !text.contains("alpha"), "{text}");
    let (code, text) = run(&["--list", "yardstick_"], &needs, &tests);
    assert_eq!(code, 0);
    assert_eq!(
        text,
        "yardstick_alpha: test\nyardstick_alphabet: test\nyardstick_beta: test\n\n3 tests, 0 benchmarks\n"
    );
    assert_eq!(LISTED_RAN.load(Ordering::SeqCst), 0);
    // A name that matches nothing runs nothing, and says so as the harness does: it asks nothing of
    // the program, so it is an ordinary empty run and not a skipped one.
    let (code, text) = run(&["--ignored", "nothing_matches_this"], &needs, &tests);
    assert_eq!(code, 0);
    assert!(
        text.contains("running 0 tests") && text.contains("test result: ok. 0 passed; 0 failed; 0 skipped;"),
        "{text}"
    );
}

#[test]
fn ffmpeg_is_absent_when_either_program_is_missing_from_the_search_path() {
    let base = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("yardstick-runner");
    let make = |name: &str, programs: &[&str]| {
        let dir = base.join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for program in programs {
            std::fs::write(dir.join(program), "").unwrap();
        }
        dir
    };
    let both = make("both", &["ffmpeg", "ffprobe"]);
    let only_ffmpeg = make("only-ffmpeg", &["ffmpeg"]);
    let neither = make("neither", &[]);
    let search = |dirs: &[&std::path::Path]| std::env::join_paths(dirs).unwrap();
    assert!(yardstick::missing_on(&search(&[&both]), &["ffmpeg", "ffprobe"]).is_empty());
    assert_eq!(
        yardstick::missing_on(&search(&[&only_ffmpeg]), &["ffmpeg", "ffprobe"]),
        vec!["ffprobe".to_string()]
    );
    assert_eq!(
        yardstick::missing_on(&search(&[&neither]), &["ffmpeg", "ffprobe"]),
        vec!["ffmpeg".to_string(), "ffprobe".to_string()]
    );
    // The two programs may lie in two folders of the path.
    assert!(yardstick::missing_on(
        &search(&[&only_ffmpeg, &make("only-ffprobe", &["ffprobe"])]),
        &["ffmpeg", "ffprobe"]
    )
    .is_empty());
    // A folder with a directory of that name is not the program.
    let tricky = make("tricky", &[]);
    std::fs::create_dir_all(tricky.join("ffmpeg")).unwrap();
    assert_eq!(yardstick::missing_on(&search(&[&tricky]), &["ffmpeg"]), vec!["ffmpeg".to_string()]);
}
