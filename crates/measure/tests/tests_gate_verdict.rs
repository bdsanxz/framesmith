//! The verdict of the tests gate: it fails when any of its three runs failed (the plain test run, the
//! run of the tests marked "reference machine only" and the run of the yardstick tests) and names each
//! failure, in the whole run of the gates and in a run narrowed to the gate (`docs/dev/gates.md`,
//! sections 2, 4 and 5).
//!
//! Each case is a small workspace of its own with a copy of `scripts/gates.sh`, cut off from the real
//! runner. The "yardstick" and "reference machine only" tests of the workspace are ordinary tests
//! carrying the marks the gate looks for, so that no program is started from outside.

mod common;

use common::scratch::Scratch;

/// What each of the three runs of the workspace does: a test of it passes or fails.
struct Workspace {
    plain: bool,
    reference: bool,
    yardstick: bool,
}

const ALL_PASS: Workspace = Workspace {
    plain: true,
    reference: true,
    yardstick: true,
};

fn expected(passes: bool) -> &'static str {
    if passes {
        "2"
    } else {
        "3"
    }
}

fn workspace(name: &str, parts: &Workspace) -> Scratch {
    let scratch = Scratch::new(name);
    scratch.write(
        "crates/user/tests/plain.rs",
        &format!(
            "#[test]\nfn a_test_that_breaks() {{\n    assert_eq!(1 + 1, {});\n}}\n",
            expected(parts.plain)
        ),
    );
    scratch.write(
        "crates/user/tests/marked.rs",
        &format!(
            "#[test]\n#[ignore = \"reference machine only: needs a card, which this scratch workspace pretends to have\"]\n\
             fn reference_machine_only_a_card_draws_a_frame() {{\n    assert_eq!(1 + 1, {});\n}}\n\n\
             #[test]\n#[ignore = \"yardstick: compares with a program the scratch workspace pretends to have\"]\n\
             fn yardstick_a_comparison_agrees() {{\n    assert_eq!(1 + 1, {});\n}}\n",
            expected(parts.reference),
            expected(parts.yardstick)
        ),
    );
    scratch
}

/// What the script printed under its heading "== summary": one verdict line for each gate, the notes,
/// and the closing line.
fn summary(text: &str) -> &str {
    text.rsplit("\n== summary\n").next().expect("the output has a summary")
}

fn verdict<'a>(summary: &'a str, gate: &str) -> &'a str {
    summary
        .lines()
        .find(|line| line.starts_with(&format!("PASS {gate}")) || line.starts_with(&format!("FAIL {gate}")))
        .unwrap_or_else(|| panic!("the summary has no verdict for {gate}: {summary}"))
}

/// The line of the whole output that begins the report of a failed gate, with what it names under it.
fn what_failed(text: &str) -> String {
    let at = text
        .find("FAIL tests (")
        .unwrap_or_else(|| panic!("no failure report for the tests: {text}"));
    let report = &text[at..];
    let end = report.find("\n\n").unwrap_or(report.len());
    report[..end].to_string()
}

#[test]
fn a_broken_plain_test_fails_the_tests_gate_though_the_marked_tests_pass() {
    let scratch = workspace("verdict-plain-broken", &Workspace { plain: false, ..ALL_PASS });

    let narrowed = scratch.gates(&["--gate", "test"], &[]);
    assert_eq!(
        narrowed.code, 1,
        "a narrowed run whose part failed ends with 1, not 3: {}",
        narrowed.text
    );
    assert!(narrowed.text.contains("test a_test_that_breaks ... FAILED"), "{}", narrowed.text);
    assert!(
        narrowed.text.contains("reference machine only: 1 marked, 1 ran and passed")
            && narrowed.text.contains("yardstick: 1 marked, 1 ran and passed, 0 skipped"),
        "the later runs passed, which is what this case is about: {}",
        narrowed.text
    );
    let verdict_line = verdict(summary(&narrowed.text), "tests").to_string();
    assert!(
        verdict_line.starts_with("FAIL tests: test a_test_that_breaks failed (crates/user/tests/plain.rs:"),
        "{verdict_line}"
    );
    assert!(
        narrowed
            .text
            .trim_end()
            .ends_with("NARROWED RUN (gates: test): a part failed. This is not a pass of the gates."),
        "{}",
        narrowed.text
    );

    let whole = scratch.gates(&[], &[]);
    assert_eq!(whole.code, 1, "{}", whole.text);
    assert!(!summary(&whole.text).contains("NARROWED"), "{}", whole.text);
    let verdict_line = verdict(summary(&whole.text), "tests").to_string();
    assert!(
        verdict_line.starts_with("FAIL tests: test a_test_that_breaks failed (crates/user/tests/plain.rs:"),
        "{verdict_line}"
    );
    assert!(
        what_failed(&whole.text).contains("    test a_test_that_breaks failed (crates/user/tests/plain.rs:"),
        "{}",
        whole.text
    );
}

#[test]
fn a_broken_reference_machine_only_test_fails_the_tests_gate_though_the_yardstick_tests_pass() {
    let scratch = workspace(
        "verdict-reference-broken",
        &Workspace {
            reference: false,
            ..ALL_PASS
        },
    );

    for (label, args, code) in [("narrowed", vec!["--gate", "test"], 1), ("whole", vec![], 1)] {
        let run = scratch.gates(&args, &[]);
        assert_eq!(run.code, code, "{label}: {}", run.text);
        assert!(
            run.text.contains("yardstick: 1 marked, 1 ran and passed, 0 skipped"),
            "{label}: the yardstick run passed, which is what this case is about: {}",
            run.text
        );
        let verdict_line = verdict(summary(&run.text), "tests").to_string();
        assert!(
            verdict_line.starts_with("FAIL tests: reference machine only test failed: reference_machine_only_a_card_draws_a_frame"),
            "{label}: {verdict_line}"
        );
    }
}

#[test]
fn a_broken_yardstick_test_fails_the_tests_gate() {
    let scratch = workspace(
        "verdict-yardstick-broken",
        &Workspace {
            yardstick: false,
            ..ALL_PASS
        },
    );

    for (label, args) in [("narrowed", vec!["--gate", "test"]), ("whole", vec![])] {
        let run = scratch.gates(&args, &[]);
        assert_eq!(run.code, 1, "{label}: {}", run.text);
        let verdict_line = verdict(summary(&run.text), "tests").to_string();
        assert!(
            verdict_line.starts_with("FAIL tests: yardstick test failed: yardstick_a_comparison_agrees"),
            "{label}: {verdict_line}"
        );
    }
}

#[test]
fn every_failed_run_of_the_tests_gate_is_named() {
    let scratch = workspace(
        "verdict-all-broken",
        &Workspace {
            plain: false,
            reference: false,
            yardstick: false,
        },
    );

    let run = scratch.gates(&["--gate", "test"], &[]);
    assert_eq!(run.code, 1, "{}", run.text);
    let verdict_line = verdict(summary(&run.text), "tests").to_string();
    assert!(
        verdict_line.starts_with("FAIL tests: test a_test_that_breaks failed (crates/user/tests/plain.rs:"),
        "{verdict_line}"
    );
    let report = what_failed(&run.text);
    for named in [
        "    test a_test_that_breaks failed (crates/user/tests/plain.rs:",
        "    reference machine only test failed: reference_machine_only_a_card_draws_a_frame",
        "    yardstick test failed: yardstick_a_comparison_agrees",
    ] {
        assert!(report.contains(named), "the report does not hold {named:?}: {report}");
    }
}

#[test]
fn a_workspace_whose_tests_all_pass_gets_a_pass_for_the_tests_gate() {
    let scratch = workspace("verdict-all-pass", &ALL_PASS);

    let narrowed = scratch.gates(&["--gate", "test"], &[]);
    assert_eq!(narrowed.code, 3, "a narrowed run whose part passed ends with 3: {}", narrowed.text);
    assert!(
        verdict(summary(&narrowed.text), "tests").starts_with("PASS tests"),
        "{}",
        narrowed.text
    );
    assert!(
        narrowed.text.contains("reference machine only: 1 marked, 1 ran and passed")
            && narrowed.text.contains("yardstick: 1 marked, 1 ran and passed, 0 skipped"),
        "{}",
        narrowed.text
    );

    let whole = scratch.gates(&[], &[]);
    assert!(verdict(summary(&whole.text), "tests").starts_with("PASS tests"), "{}", whole.text);
}
