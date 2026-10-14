//! Scratch only (decision B7 of DECISIONS.md): a test that fails on purpose, to read how the hosted
//! run reports a failing gate. This file is never merged.

#[test]
fn fails_on_purpose_to_show_how_a_failing_gate_is_reported() {
    assert_eq!(1 + 1, 3, "this test fails on purpose");
}
