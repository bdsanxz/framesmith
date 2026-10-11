//! The seek measurement's summary and verdict (T047 c1, c2).

use bench::seek::{json, report, summarise, LIMIT_MS};

#[test]
fn t047_c1_the_report_lists_every_frame_and_the_largest_and_typical_values() {
    let text = report(&[0.1, 0.4, 0.2, 1.25, 0.2]);
    assert_eq!(
        text,
        "seek measurement: real time inside the page for each seek, in milliseconds\n\
         \x20 frame    0  0.100\n\
         \x20 frame    1  0.400\n\
         \x20 frame    2  0.200\n\
         \x20 frame    3  1.250\n\
         \x20 frame    4  0.200\n\
         summary: 5 frames, largest 1.250 ms (frame 3), typical 0.200 ms, limit under 2 ms\n\
         result: pass\n"
    );
}

#[test]
fn t047_c2_under_two_milliseconds_passes_and_two_or_more_fails() {
    assert_eq!(LIMIT_MS, 2.0);
    assert!(summarise(&[0.3, 1.999, 0.2]).unwrap().pass);
    // Exactly at the limit is a failure.
    let at = summarise(&[0.3, 2.0, 0.2]).unwrap();
    assert!(!at.pass && at.largest == 2.0 && at.largest_frame == 1);
    assert!(report(&[0.3, 2.0, 0.2]).ends_with("result: fail (a seek took 2 milliseconds or longer)\n"));
    assert!(!summarise(&[0.1, 0.1, 7.5]).unwrap().pass);
    // One slow frame among many fast ones still fails: the largest value decides, not the typical one.
    let mut mostly_fast = vec![0.1; 500];
    mostly_fast[250] = 2.1;
    let summary = summarise(&mostly_fast).unwrap();
    assert!(!summary.pass && summary.typical == 0.1 && summary.largest_frame == 250);
    // Nothing measured is not a pass.
    assert_eq!(summarise(&[]), None);
    assert!(report(&[]).ends_with("result: fail (nothing to judge)\n"));
}

#[test]
fn the_json_form_carries_the_same_facts() {
    let document: serde_json::Value = serde_json::from_str(&json(&[0.5, 2.5])).unwrap();
    assert_eq!(document["result"], "fail");
    assert_eq!(document["largest"], 2.5);
    assert_eq!(document["largest_frame"], 1);
    assert_eq!(document["frames"].as_array().unwrap().len(), 2);
    assert_eq!(serde_json::from_str::<serde_json::Value>(&json(&[0.5])).unwrap()["result"], "pass");
}

#[test]
fn a_wrong_call_and_a_run_that_cannot_be_made_never_say_pass() {
    let args = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<String>>();
    assert_eq!(bench::run(&args(&[])).1, bench::EXIT_USAGE);
    assert_eq!(bench::run(&args(&["seek"])).1, bench::EXIT_USAGE);
    assert_eq!(bench::run(&args(&["seek", "a", "--fast"])).1, bench::EXIT_USAGE);
    let work = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("bench-not-run");
    let (text, code) = bench::run(&args(&[
        "seek",
        "a",
        "--framesmith",
        "/nowhere/framesmith",
        "--work",
        work.to_str().unwrap(),
    ]));
    assert_eq!(code, bench::EXIT_NOT_RUN);
    assert!(text.ends_with("result: not run\n") && !text.contains("pass"), "{text}");
}
