//! Sprint 11 transitions (T006) validation: a composition-level
//! `transition` bridges two clips by id over `duration` seconds with
//! one of twenty kinds, borrowing handles inside both clips without
//! moving any range.
//!
//! Validates through the public [`validate_folder`] API only: the
//! element lands on the composition with its fields; every kind
//! validates; unknown clip ids, self-bridges, overlapping bridges on
//! one boundary, short handles, zero durations, unknown kinds, and
//! malformed elements fail naming the element and the fix; ranges
//! never move under a transition.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use validate::validate::{validate_folder, Status};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-validate-n11-transition-{tag}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_comp(dir: &Path, body: &str) {
    std::fs::write(dir.join("composition.kdl"), body).unwrap();
}

fn codes_messages(dir: &Path) -> Vec<(String, String, String)> {
    validate_folder(dir)
        .diagnostics
        .iter()
        .map(|d| (d.code.to_string(), d.message.clone(), d.fix.clone()))
        .collect()
}

fn has(dir: &Path, code: &str, needles: &[&str]) -> bool {
    codes_messages(dir)
        .iter()
        .any(|(c, m, f)| c == code && needles.iter().all(|n| m.contains(n) || f.contains(n)))
}

/// Two abutting one-second boxes plus a bridge of `duration`/`kind`.
fn body(bridge: &str) -> String {
    format!(
        "composition name=\"main\" root=true width=64 height=64 duration=3 background=\"#000\" version=1 {{\n box x=0 y=0 width=64 height=64 fill=\"#ff0000\" start=0 duration=1 id=\"a\"\n box x=0 y=0 width=64 height=64 fill=\"#0000ff\" start=1 duration=1 id=\"b\"\n box x=0 y=0 width=64 height=64 fill=\"#00ff00\" start=2 duration=1 id=\"c\"\n {bridge}\n}}\n"
    )
}

// --- T006: valid bridges -----------------------------------------------------

#[test]
fn transition_lands_with_its_fields() {
    let dir = tmp("ok");
    write_comp(
        &dir,
        &body("transition from=\"a\" to=\"b\" duration=0.5 kind=\"fade\""),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    let comp = v.composition.expect("valid composition");
    assert_eq!(comp.transitions.len(), 1);
    let t = &comp.transitions[0];
    assert_eq!(t.from, "a");
    assert_eq!(t.to, "b");
    assert_eq!(t.duration, 0.5);
    assert_eq!(t.kind, notation::model::TransitionKind::Fade);
}

#[test]
fn every_kind_bridges() {
    // All fourteen natives plus the six-piece CSS gallery validate.
    for kind in notation::model::TransitionKind::ACCEPTED {
        let dir = tmp("kinds");
        write_comp(
            &dir,
            &body(&format!(
                "transition from=\"a\" to=\"b\" duration=1 kind=\"{kind}\""
            )),
        );
        let v = validate_folder(&dir);
        assert_eq!(v.status, Status::Valid, "kind {kind}: {:?}", v.diagnostics);
        let comp = v.composition.expect("valid composition");
        assert_eq!(comp.transitions.len(), 1);
        assert_eq!(comp.transitions[0].kind.name(), kind);
    }
}

#[test]
fn bridge_as_long_as_the_shorter_handle_is_valid() {
    // Exactly the whole of both one-second clips: every borrowed
    // frame used once.
    let dir = tmp("full-handle");
    write_comp(
        &dir,
        &body("transition from=\"a\" to=\"b\" duration=1 kind=\"wipe-left\""),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
}

#[test]
fn disjoint_bridges_sharing_a_clip_are_valid() {
    // a->b over [1,1.4) and b->c over [2,2.4): shared clip `b` but
    // disjoint windows, so both bridges validate.
    let dir = tmp("disjoint");
    write_comp(
        &dir,
        "composition name=\"main\" root=true width=64 height=64 duration=3 background=\"#000\" version=1 {\n box x=0 y=0 width=64 height=64 fill=\"#ff0000\" start=0 duration=1 id=\"a\"\n box x=0 y=0 width=64 height=64 fill=\"#0000ff\" start=1 duration=1 id=\"b\"\n box x=0 y=0 width=64 height=64 fill=\"#00ff00\" start=2 duration=1 id=\"c\"\n transition from=\"a\" to=\"b\" duration=0.4 kind=\"fade\"\n transition from=\"b\" to=\"c\" duration=0.4 kind=\"dissolve\"\n}\n",
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    let comp = v.composition.expect("valid composition");
    assert_eq!(comp.transitions.len(), 2);
}

#[test]
fn ranges_never_move_under_a_transition() {
    // The bridge borrows handles; it must not shift any clip or the
    // composition length.
    let plain = body("");
    let bridged = body("transition from=\"a\" to=\"b\" duration=1 kind=\"push\"");
    let dir_plain = tmp("plain");
    write_comp(&dir_plain, &plain);
    let dir_bridged = tmp("bridged");
    write_comp(&dir_bridged, &bridged);
    let vp = validate_folder(&dir_plain);
    let vb = validate_folder(&dir_bridged);
    assert_eq!(vp.status, Status::Valid, "{:?}", vp.diagnostics);
    assert_eq!(vb.status, Status::Valid, "{:?}", vb.diagnostics);
    let (cp, cb) = (
        vp.composition.expect("plain"),
        vb.composition.expect("bridged"),
    );
    assert_eq!(cp.duration, cb.duration);
    assert_eq!(cp.clips.len(), cb.clips.len());
    for (p, b) in cp.clips.iter().zip(cb.clips.iter()) {
        assert_eq!(p.start(), b.start(), "start moves under a transition");
        assert_eq!(
            p.duration(),
            b.duration(),
            "duration moves under a transition"
        );
    }
}

// --- T006: refusals ----------------------------------------------------------

#[test]
fn unknown_clip_ids_fail_naming_the_transition_and_the_id() {
    let dir = tmp("unknown-from");
    write_comp(
        &dir,
        &body("transition from=\"ghost\" to=\"b\" duration=0.5 kind=\"fade\""),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
    assert!(
        has(
            &dir,
            "E_UNKNOWN_REF",
            &["transition", "`from`", "\"ghost\""]
        ),
        "{:?}",
        codes_messages(&dir)
    );

    let dir = tmp("unknown-to");
    write_comp(
        &dir,
        &body("transition from=\"a\" to=\"ghost\" duration=0.5 kind=\"fade\""),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
    assert!(
        has(&dir, "E_UNKNOWN_REF", &["transition", "`to`", "\"ghost\""]),
        "{:?}",
        codes_messages(&dir)
    );
}

#[test]
fn self_bridge_fails() {
    let dir = tmp("self");
    write_comp(
        &dir,
        &body("transition from=\"a\" to=\"a\" duration=0.5 kind=\"fade\""),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
    assert!(
        has(&dir, "E_BAD_TRANSITION", &["transition", "itself"]),
        "{:?}",
        codes_messages(&dir)
    );
}

#[test]
fn overlapping_bridges_on_one_boundary_fail_naming_both() {
    // Two bridges into `b` whose windows overlap share the boundary.
    let dir = tmp("overlap");
    write_comp(
        &dir,
        "composition name=\"main\" root=true width=64 height=64 duration=3 background=\"#000\" version=1 {\n box x=0 y=0 width=64 height=64 fill=\"#ff0000\" start=0 duration=2 id=\"a\"\n box x=0 y=0 width=64 height=64 fill=\"#0000ff\" start=1 duration=2 id=\"b\"\n transition from=\"a\" to=\"b\" duration=1 kind=\"fade\"\n transition from=\"a\" to=\"b\" duration=0.5 kind=\"dissolve\"\n}\n",
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
    assert!(
        has(
            &dir,
            "E_BAD_TRANSITION",
            &["overlaps", "fade", "dissolve", "one transition"]
        ),
        "{:?}",
        codes_messages(&dir)
    );
}

#[test]
fn chained_bridges_with_overlapping_windows_fail() {
    // a->b borrows [1,2), b->c borrows [1.5,2.5): they share `b` and
    // overlap, so the second names both.
    let dir = tmp("chain-overlap");
    write_comp(
        &dir,
        "composition name=\"main\" root=true width=64 height=64 duration=3 background=\"#000\" version=1 {\n box x=0 y=0 width=64 height=64 fill=\"#ff0000\" start=0 duration=1 id=\"a\"\n box x=0 y=0 width=64 height=64 fill=\"#0000ff\" start=1 duration=1 id=\"b\"\n box x=0 y=0 width=64 height=64 fill=\"#00ff00\" start=1.5 duration=1 id=\"c\"\n transition from=\"a\" to=\"b\" duration=1 kind=\"fade\"\n transition from=\"b\" to=\"c\" duration=1 kind=\"wipe-left\"\n}\n",
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
    assert!(
        has(
            &dir,
            "E_BAD_TRANSITION",
            &["overlaps", "\"b\"", "fade", "wipe-left"]
        ),
        "{:?}",
        codes_messages(&dir)
    );
}

#[test]
fn short_handles_fail_naming_the_clip_and_the_shortfall_frames() {
    // Both clips run 1s; a 1.5s bridge is short half a second on each
    // side: 15 frames at 30fps.
    let dir = tmp("short");
    write_comp(
        &dir,
        &body("transition from=\"a\" to=\"b\" duration=1.5 kind=\"fade\""),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
    assert!(
        has(
            &dir,
            "E_BAD_TRANSITION",
            &["transition", "\"a\"", "15 frame", "30fps"]
        ),
        "{:?}",
        codes_messages(&dir)
    );
    assert!(
        has(
            &dir,
            "E_BAD_TRANSITION",
            &["transition", "\"b\"", "15 frame", "30fps"]
        ),
        "{:?}",
        codes_messages(&dir)
    );
}

#[test]
fn zero_and_negative_durations_fail_naming_the_rule() {
    for duration in ["0", "-1"] {
        let dir = tmp("zero");
        write_comp(
            &dir,
            &body(&format!(
                "transition from=\"a\" to=\"b\" duration={duration} kind=\"fade\""
            )),
        );
        let v = validate_folder(&dir);
        assert_eq!(
            v.status,
            Status::Invalid,
            "duration {duration}: {:?}",
            v.diagnostics
        );
        assert!(
            has(&dir, "E_BAD_TRANSITION", &["duration", "positive"]),
            "{:?}",
            codes_messages(&dir)
        );
    }
}

#[test]
fn unknown_kind_fails_naming_the_accepted_set() {
    let dir = tmp("kind");
    write_comp(
        &dir,
        &body("transition from=\"a\" to=\"b\" duration=0.5 kind=\"morph\""),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
    assert!(
        has(
            &dir,
            "E_BAD_TRANSITION",
            &["transition", "morph", "wipe-left", "css-zoom-in"]
        ),
        "{:?}",
        codes_messages(&dir)
    );
}

#[test]
fn malformed_transitions_fail_with_a_fix() {
    // Missing `kind`.
    let dir = tmp("missing-kind");
    write_comp(&dir, &body("transition from=\"a\" to=\"b\" duration=0.5"));
    assert_eq!(validate_folder(&dir).status, Status::Invalid);
    assert!(
        has(&dir, "E_BAD_TRANSITION", &["transition", "`kind`"]),
        "{:?}",
        codes_messages(&dir)
    );

    // Missing `from`.
    let dir = tmp("missing-from");
    write_comp(
        &dir,
        &body("transition to=\"b\" duration=0.5 kind=\"fade\""),
    );
    assert_eq!(validate_folder(&dir).status, Status::Invalid);
    assert!(
        has(&dir, "E_BAD_TRANSITION", &["transition", "`from`"]),
        "{:?}",
        codes_messages(&dir)
    );

    // Unknown key.
    let dir = tmp("bad-key");
    write_comp(
        &dir,
        &body("transition from=\"a\" to=\"b\" duration=0.5 kind=\"fade\" ease=\"linear\""),
    );
    assert_eq!(validate_folder(&dir).status, Status::Invalid);
    assert!(
        has(&dir, "E_UNKNOWN_KEY", &["transition", "ease"]),
        "{:?}",
        codes_messages(&dir)
    );

    // Children are refused: a transition carries attributes only.
    let dir = tmp("child");
    write_comp(
        &dir,
        &body(
            "transition from=\"a\" to=\"b\" duration=0.5 kind=\"fade\" {\n grade preset=\"noir\"\n}",
        ),
    );
    assert_eq!(validate_folder(&dir).status, Status::Invalid);
    assert!(
        has(&dir, "E_BAD_TRANSITION", &["transition", "grade"]),
        "{:?}",
        codes_messages(&dir)
    );
}
