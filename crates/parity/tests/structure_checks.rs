//! The four structural checks on facts alone, where no video is needed: each fails on its own and
//! shows both values (`docs/dev/comparison.md`, section 3).

use measure::VideoFacts;
use parity::report::Verdict;
use parity::structure::{checks, same_length, same_rate};

fn facts() -> VideoFacts {
    VideoFacts {
        container: "mp4".into(),
        codec: "h264".into(),
        width: 320,
        height: 180,
        frames: 60,
        rate: (30, 1),
        ticks: 60_000,
        timescale: 30_000,
    }
}

fn verdicts(ours: &VideoFacts, theirs: &VideoFacts) -> Vec<(String, Verdict)> {
    checks(ours, theirs).into_iter().map(|c| (c.name, c.verdict)).collect()
}

fn only_failing(ours: &VideoFacts, theirs: &VideoFacts) -> Vec<String> {
    verdicts(ours, theirs)
        .into_iter()
        .filter(|(_, v)| *v == Verdict::Fail)
        .map(|(n, _)| n)
        .collect()
}

#[test]
fn equal_facts_pass_each_of_the_four_in_order() {
    let all = verdicts(&facts(), &facts());
    let names: Vec<&str> = all.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, ["frame count", "frame rate", "size", "length"]);
    assert!(all.iter().all(|(_, v)| *v == Verdict::Pass));
}

#[test]
fn a_different_frame_count_alone_fails_the_count_with_both_values() {
    let mut theirs = facts();
    theirs.frames = 45;
    assert_eq!(only_failing(&facts(), &theirs), ["frame count"]);
    assert_eq!(checks(&facts(), &theirs)[0].detail, "ours 60, reference 45");
}

#[test]
fn a_different_frame_rate_alone_fails_the_rate_with_both_values() {
    let mut theirs = facts();
    theirs.rate = (24, 1);
    assert_eq!(only_failing(&facts(), &theirs), ["frame rate"]);
    assert_eq!(checks(&facts(), &theirs)[1].detail, "ours 30/1, reference 24/1");
}

#[test]
fn a_different_size_alone_fails_the_size_with_both_values() {
    let mut theirs = facts();
    theirs.width = 640;
    assert_eq!(only_failing(&facts(), &theirs), ["size"]);
    assert_eq!(checks(&facts(), &theirs)[2].detail, "ours 320x180, reference 640x180");
    let mut taller = facts();
    taller.height = 181;
    assert_eq!(only_failing(&facts(), &taller), ["size"]);
}

#[test]
fn a_different_length_alone_fails_the_length_with_both_values() {
    let mut theirs = facts();
    theirs.ticks = 61_000;
    assert_eq!(only_failing(&facts(), &theirs), ["length"]);
    let detail = &checks(&facts(), &theirs)[3].detail;
    assert_eq!(detail, "ours 2.000000 s (60000/30000), reference 2.033333 s (61000/30000)");
}

#[test]
fn the_same_rate_and_the_same_length_written_with_other_clocks_are_the_same() {
    assert!(same_rate((30, 1), (60_000, 2_000)));
    assert!(!same_rate((30_000, 1_001), (30, 1)));
    assert!(!same_rate((30, 1), (24, 1)));
    let mut theirs = facts();
    theirs.ticks = 600;
    theirs.timescale = 300;
    assert!(same_length(&facts(), &theirs));
    theirs.rate = (60_000, 2_000);
    assert_eq!(only_failing(&facts(), &theirs), Vec::<String>::new());
}
