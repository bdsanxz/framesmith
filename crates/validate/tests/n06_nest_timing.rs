//! Sprint 6 T007: nest compositions with per-use timing.
//!
//! Validates through the public [`validate_folder`] API only: a nest
//! plays another composition's timeline with per-use `trim-start`,
//! `trim-end`, and `rate`; past its content the picture holds while
//! sound goes silent (render-side); chains deeper than 8 levels fail
//! naming the limit; circular chains fail naming the chain; unbounded
//! and unknown nests fail before frame one.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use validate::validate::{validate_folder, Status};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-validate-n06t007-{tag}-{}-{}",
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

fn codes(dir: &Path) -> Vec<String> {
    validate_folder(dir)
        .diagnostics
        .iter()
        .map(|d| d.code.to_string())
        .collect()
}

const BUMPER: &str =
    "composition name=\"bumper\" width=64 height=64 duration=2 background=\"#000\" version=1 {\n\
     box x=0 y=0 width=64 height=64 fill=\"#ff0000\" start=0 duration=1\n\
     box x=0 y=0 width=64 height=64 fill=\"#00ff00\" start=1 duration=1\n\
     }\n";

fn main_with(nest: &str) -> String {
    format!(
        "composition name=\"main\" root=true width=64 height=64 duration=2 background=\"#000\" version=1 {{\n\
         {nest}\n\
         }}\n{BUMPER}"
    )
}

// --- per-use timing ------------------------------------------------------

#[test]
fn nest_with_trim_and_rate_validates_and_stores() {
    let dir = tmp("timing-ok");
    write_comp(
        &dir,
        &main_with("nest nest=\"bumper\" start=0 duration=2 trim-start=0.5 trim-end=1.5 rate=2"),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    let comp = v.composition.expect("valid verdict carries the root");
    assert_eq!(comp.clips.len(), 1);
    match &comp.clips[0] {
        notation::model::Clip::Nest(n) => {
            assert_eq!(n.target, "bumper");
            assert_eq!((n.trim_start, n.trim_end, n.rate), (0.5, Some(1.5), 2.0));
            assert_eq!(n.order, 0);
        }
        other => panic!("expected a nest clip, found {other:?}"),
    }
}

#[test]
fn nest_underscore_spellings_and_order_validate() {
    let dir = tmp("timing-underscore");
    write_comp(
        &dir,
        &main_with(
            "nest nest=\"bumper\" start=0 duration=2 trim_start=0 trim_end=2 rate=0.5 order=3",
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    match &v.composition.expect("root").clips[0] {
        notation::model::Clip::Nest(n) => {
            assert_eq!(
                (n.trim_start, n.trim_end, n.rate, n.order),
                (0.0, Some(2.0), 0.5, 3)
            );
        }
        other => panic!("expected a nest clip, found {other:?}"),
    }
}

#[test]
fn nest_defaults_are_unity_window_and_zero_order() {
    let dir = tmp("timing-defaults");
    write_comp(&dir, &main_with("nest nest=\"bumper\" start=0 duration=2"));
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    match &v.composition.expect("root").clips[0] {
        notation::model::Clip::Nest(n) => {
            assert_eq!(
                (n.trim_start, n.trim_end, n.rate, n.order),
                (0.0, None, 1.0, 0)
            );
        }
        other => panic!("expected a nest clip, found {other:?}"),
    }
}

#[test]
fn nest_target_may_name_the_composition_id() {
    let dir = tmp("timing-target-id");
    write_comp(
        &dir,
        "composition name=\"main\" root=true width=64 height=64 duration=2 background=\"#000\" version=1 {\n\
         nest nest=\"b-1\" start=0 duration=2\n\
         }\n\
         composition name=\"bumper\" id=\"b-1\" width=64 height=64 duration=2 background=\"#000\" version=1 {\n\
         box x=0 y=0 width=64 height=64 fill=\"#ff0000\" start=0 duration=2\n\
         }\n",

    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
}

// --- trim and rate refusals ------------------------------------------------

#[test]
fn nest_backward_window_refuses() {
    let dir = tmp("timing-backward");
    write_comp(
        &dir,
        &main_with("nest nest=\"bumper\" start=0 duration=2 trim-start=1.5 trim-end=0.5"),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
    assert_eq!(codes(&dir), vec!["E_BAD_TIMING"]);
    assert!(
        v.diagnostics[0].message.contains("backward"),
        "{}",
        v.diagnostics[0]
    );
}

#[test]
fn nest_empty_window_refuses() {
    let dir = tmp("timing-empty");
    write_comp(
        &dir,
        &main_with("nest nest=\"bumper\" start=0 duration=2 trim-start=1 trim-end=1"),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
    assert_eq!(codes(&dir), vec!["E_BAD_TIMING"]);
    assert!(
        v.diagnostics[0].message.contains("empty"),
        "{}",
        v.diagnostics[0]
    );
}

#[test]
fn nest_negative_trim_refuses_at_the_key() {
    let dir = tmp("timing-negative");
    write_comp(
        &dir,
        &main_with("nest nest=\"bumper\" start=0 duration=2 trim-start=-1"),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
    assert_eq!(codes(&dir), vec!["E_BAD_TIMING"]);
    let d = &v.diagnostics[0];
    assert!(d.message.contains("trim-start"), "{d}");
    assert_eq!((d.line, d.column), (2, 50), "{d}");
}

#[test]
fn nest_rate_outside_point_one_to_ten_refuses() {
    for (tag, rate) in [("low", "0.05"), ("high", "20"), ("zero", "0")] {
        let dir = tmp(&format!("timing-rate-{tag}"));
        write_comp(
            &dir,
            &main_with(&format!(
                "nest nest=\"bumper\" start=0 duration=2 rate={rate}"
            )),
        );
        let v = validate_folder(&dir);
        assert_eq!(v.status, Status::Invalid, "{tag}: {:?}", v.diagnostics);
        assert_eq!(codes(&dir), vec!["E_BAD_TIMING"], "{tag}");
        assert!(
            v.diagnostics[0].message.contains("0.1 to 10"),
            "{tag}: {}",
            v.diagnostics[0]
        );
    }
}

#[test]
fn nest_trim_start_past_child_end_refuses() {
    let dir = tmp("timing-past-end");
    write_comp(
        &dir,
        &main_with("nest nest=\"bumper\" start=0 duration=2 trim-start=2"),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
    assert_eq!(codes(&dir), vec!["E_BAD_TIMING"]);
    let d = &v.diagnostics[0];
    assert!(
        d.message.contains("bumper") && d.message.contains('2'),
        "{d}"
    );
    assert!(!d.fix.is_empty());
}

#[test]
fn nest_trim_end_past_child_end_clamps_silently() {
    let dir = tmp("timing-clamp");
    write_comp(
        &dir,
        &main_with("nest nest=\"bumper\" start=0 duration=2 trim-start=1 trim-end=99"),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
}

// --- unbounded and unknown -------------------------------------------------

#[test]
fn nest_without_duration_refuses_until_bounded() {
    let dir = tmp("timing-unbounded");
    write_comp(&dir, &main_with("nest nest=\"bumper\" start=0"));
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
    assert_eq!(codes(&dir), vec!["E_BAD_TIMING"]);
    assert!(
        v.diagnostics[0].message.contains("unbounded"),
        "{}",
        v.diagnostics[0]
    );
}

#[test]
fn nest_naming_a_ghost_refuses() {
    let dir = tmp("timing-ghost");
    write_comp(&dir, &main_with("nest nest=\"ghost\" start=0 duration=2"));
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
    assert_eq!(codes(&dir), vec!["E_UNKNOWN_REF"]);
}

#[test]
fn nest_naming_a_clip_id_refuses() {
    let dir = tmp("timing-clip-target");
    write_comp(
        &dir,
        &format!(
            "composition name=\"main\" root=true width=64 height=64 duration=2 background=\"#000\" version=1 {{\n\
             box x=0 y=0 width=1 height=1 fill=\"#fff\" start=0 duration=1 id=\"clip-a\"\n\
             nest nest=\"clip-a\" start=1 duration=1\n\
             }}\n{BUMPER}"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
    assert_eq!(codes(&dir), vec!["E_UNKNOWN_REF"]);
}

// --- cycles and depth ------------------------------------------------------

#[test]
fn circular_nest_refuses_naming_the_chain() {
    let dir = tmp("graph-cycle");
    write_comp(
        &dir,
        "composition name=\"a\" root=true width=64 height=64 duration=2 background=\"#000\" version=1 {\n\
         nest nest=\"b\" start=0 duration=1\n\
         }\n\
         composition name=\"b\" width=64 height=64 duration=2 background=\"#000\" version=1 {\n\
         nest nest=\"a\" start=0 duration=1\n\
         }\n",
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
    assert_eq!(codes(&dir), vec!["E_BAD_TIMING"]);
    let d = &v.diagnostics[0];
    assert!(d.message.contains("\"a\" -> \"b\" -> \"a\""), "{d}");
    assert!(!d.fix.is_empty());
}

#[test]
fn self_nest_refuses_naming_the_chain() {
    let dir = tmp("graph-self");
    write_comp(
        &dir,
        "composition name=\"a\" width=64 height=64 duration=2 background=\"#000\" version=1 {\n\
         nest nest=\"a\" start=0 duration=1\n\
         }\n",
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
    assert_eq!(codes(&dir), vec!["E_BAD_TIMING"]);
    assert!(
        v.diagnostics[0].message.contains("\"a\" -> \"a\""),
        "{}",
        v.diagnostics[0]
    );
}

#[test]
fn three_cycle_names_the_whole_chain() {
    let dir = tmp("graph-3cycle");
    write_comp(
        &dir,
        "composition name=\"a\" root=true width=64 height=64 duration=2 background=\"#000\" version=1 {\n\
         nest nest=\"b\" start=0 duration=1\n\
         }\n\
         composition name=\"b\" width=64 height=64 duration=2 background=\"#000\" version=1 {\n\
         nest nest=\"c\" start=0 duration=1\n\
         }\n\
         composition name=\"c\" width=64 height=64 duration=2 background=\"#000\" version=1 {\n\
         nest nest=\"a\" start=0 duration=1\n\
         }\n",
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
    assert_eq!(codes(&dir), vec!["E_BAD_TIMING"]);
    assert!(
        v.diagnostics[0]
            .message
            .contains("\"a\" -> \"b\" -> \"c\" -> \"a\""),
        "{}",
        v.diagnostics[0]
    );
}

fn chain_project(depth_edges: usize) -> String {
    // `depth_edges + 1` compositions c0..cn, each nesting the next but
    // the last (which holds a box). c0 is the root.
    let mut parts = Vec::new();
    for i in 0..=depth_edges {
        let use_line = if i < depth_edges {
            format!("nest nest=\"c{}\" start=0 duration=1", i + 1)
        } else {
            "box x=0 y=0 width=1 height=1 fill=\"#fff\" start=0 duration=1".to_string()
        };
        let root = if i == 0 { " root=true" } else { "" };
        parts.push(format!(
            "composition name=\"c{i}\"{root} width=64 height=64 duration=2 background=\"#000\" version=1 {{\n{use_line}\n}}\n"
        ));
    }
    parts.join("")
}

#[test]
fn eight_levels_validate_nine_refuse() {
    let dir = tmp("graph-depth-8");
    write_comp(&dir, &chain_project(8));
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);

    let dir = tmp("graph-depth-9");
    write_comp(&dir, &chain_project(9));
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid, "{:?}", v.diagnostics);
    assert_eq!(codes(&dir), vec!["E_BAD_TIMING"]);
    let d = &v.diagnostics[0];
    assert!(
        d.message.contains("8-level limit") || d.message.contains("8"),
        "{d}"
    );
    assert!(
        d.message.contains("\"c0\"") && d.message.contains("\"c9\""),
        "{d}"
    );
    assert!(!d.fix.is_empty());
}

// --- nesting with the rest of Sprint 6 -------------------------------------

#[test]
fn nest_covers_a_child_with_relative_starts() {
    // The child resolves its own `start-after` chain and infers its own
    // end; the use stays valid and the parent still ends at the latest
    // finite clip end.
    let dir = tmp("timing-child-relative");
    write_comp(
        &dir,
        "composition name=\"main\" root=true width=64 height=64 background=\"#000\" version=1 {\n\
         nest nest=\"bumper\" start=0 duration=4 id=\"use1\"\n\
         box x=0 y=0 width=1 height=1 fill=\"#fff\" duration=1 start-after=\"use1\"\n\
         }\n\
         composition name=\"bumper\" width=64 height=64 background=\"#000\" version=1 {\n\
         box x=0 y=0 width=64 height=64 fill=\"#ff0000\" start=0 duration=1 id=\"first\"\n\
         box x=0 y=0 width=64 height=64 fill=\"#00ff00\" duration=1 start-after=\"first\"\n\
         }\n",
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    let child = v
        .compositions
        .iter()
        .find(|c| c.name == "bumper")
        .expect("bumper");
    assert_eq!(child.duration, 2.0);
    // The parent omits `duration`: it ends at the latest finite clip
    // end — the follower at 5, past the nest end at 4.
    let root = v.composition.expect("root");
    assert_eq!(root.duration, 5.0);
}

#[test]
fn nest_with_start_after_resolves_in_the_parent() {
    let dir = tmp("timing-nest-relative");
    write_comp(
        &dir,
        &format!(
            "composition name=\"main\" root=true width=64 height=64 duration=4 background=\"#000\" version=1 {{\n\
             box x=0 y=0 width=1 height=1 fill=\"#fff\" start=0 duration=1 id=\"lead\"\n\
             nest nest=\"bumper\" duration=2 start-after=\"lead\"\n\
             }}\n{BUMPER}"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    match &v.composition.expect("root").clips[1] {
        notation::model::Clip::Nest(n) => assert_eq!(n.start, 1.0),
        other => panic!("expected a nest clip, found {other:?}"),
    }
}
