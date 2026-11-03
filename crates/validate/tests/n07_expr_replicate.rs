//! Sprint 7 motion (T010): pure seeded expressions validate —
//! `expr` tweens read `t`, `i`, and declared `input` names with
//! seeded `random`, impure/unknown/unseeded expressions fail, and
//! `replicate` clones one template exposing `i` with non-positive
//! counts failing. Every refusal carries file, line, column, code,
//! and fix.
//!
//! Validates through the public [`validate_folder`] API only.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use validate::validate::{validate_folder, Status};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-validate-n07-expr-{tag}-{}-{}",
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

const OPEN: &str =
    "composition name=\"main\" width=160 height=120 duration=4 background=\"#000\" version=1 {\n";
const CLIP: &str = "    box x=10 y=40 width=40 height=40 fill=\"#ff0000\" start=0 duration=4\n";

fn comp_with(body: &str) -> String {
    format!("{OPEN}{CLIP}{body}}}\n")
}

fn problems(dir: &Path) -> Vec<(String, usize, usize, String, String)> {
    validate_folder(dir)
        .diagnostics
        .iter()
        .map(|d| {
            (
                d.code.to_string(),
                d.line,
                d.column,
                d.message.clone(),
                d.fix.clone(),
            )
        })
        .collect()
}

// --- valid expressions -----------------------------------------------------

#[test]
fn expr_tween_validates_with_source_stored() {
    // T010: `expr` replaces from/to on a numeric tween and validates.
    let dir = tmp("tween");
    write_comp(
        &dir,
        &comp_with("    tween target=0 property=\"x\" expr=\"t*30+10\" start=0 duration=2\n"),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    let comp = v.composition.expect("valid composition");
    assert_eq!(comp.tweens.len(), 1);
    assert_eq!(comp.tweens[0].expr.as_deref(), Some("t*30+10"));
    assert_eq!(comp.tweens[0].from, None);
    assert_eq!(comp.tweens[0].to, None);
}

#[test]
fn expr_with_inputs_and_seeded_random_validates() {
    // T010: declared inputs resolve by name and seeded random parses.
    let dir = tmp("inputs");
    write_comp(
        &dir,
        &comp_with(
            "    input name=\"amp\" value=20\n    tween target=0 property=\"y\" expr=\"amp*sin(t)+random(7,-5,5)\" start=0 duration=4\n",
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    let comp = v.composition.expect("valid composition");
    assert_eq!(comp.inputs.len(), 1);
    assert_eq!(comp.inputs[0].name, "amp");
    assert_eq!(comp.inputs[0].value, 20.0);
}

#[test]
fn replicate_expands_to_indexed_clones() {
    // T010: `replicate count=3` clones the template; clones expose
    // `i` through the group table in document order.
    let dir = tmp("replicate");
    write_comp(
        &dir,
        &comp_with(
            "    replicate count=3 {\n        box x=10 y=40 width=20 height=20 fill=\"#ff0000\" start=0 duration=4\n    }\n    tween target=1 property=\"x\" expr=\"10+i*30\" start=0 duration=4\n",
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    let comp = v.composition.expect("valid composition");
    // One template box plus three clones.
    assert_eq!(comp.clips.len(), 4);
    assert_eq!(comp.replicates.len(), 1);
    assert_eq!(comp.replicates[0].first, 1);
    assert_eq!(comp.replicates[0].count, 3);
    assert_eq!(comp.replicate_index(1), 0);
    assert_eq!(comp.replicate_index(2), 1);
    assert_eq!(comp.replicate_index(3), 2);
    assert_eq!(comp.replicate_index(0), 0);
}

// --- expression refusals ---------------------------------------------------

#[test]
fn impure_unknown_and_unseeded_expressions_fail_together() {
    // T010/REC-004: side effects, I/O, unknown names, and unseeded
    // randomness all fail with E_BAD_MOTION in one verdict.
    let dir = tmp("impure");
    write_comp(
        &dir,
        &comp_with(
            "    tween target=0 property=\"x\" expr=\"open(1)\" start=0 duration=1\n    tween target=0 property=\"y\" expr=\"t+zz\" start=1 duration=1\n    tween target=0 property=\"scale\" expr=\"random()\" start=2 duration=1\n",
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid);
    let ps = problems(&dir);
    assert_eq!(ps.len(), 3, "every bad expression reports, got {ps:?}");
    assert!(ps.iter().all(|p| p.0 == "E_BAD_MOTION"));
    assert!(ps[0].3.contains("impure"), "got {:?}", ps[0].3);
    assert!(ps[1].3.contains("unknown name `zz`"), "got {:?}", ps[1].3);
    assert!(ps[2].3.contains("seed"), "got {:?}", ps[2].3);
    // Every diagnostic carries location and fix.
    for (_, line, column, _, fix) in &ps {
        assert!(*line >= 3, "line {line}");
        assert!(*column >= 1, "column {column}");
        assert!(!fix.is_empty());
    }
}

#[test]
fn expr_conflicts_with_endpoints_paths_colors_and_windows() {
    // T010: `expr` conflicts with from/to, paths, colors, kinds,
    // easings, and cascade/replay keys, each naming the fix.
    for (tween, needle) in [
        (
            "tween target=0 property=\"x\" expr=\"t\" from=0 start=0 duration=1",
            "`from` conflicts with `expr`",
        ),
        (
            "tween target=0 property=\"x\" expr=\"t\" to=5 start=0 duration=1",
            "`to` conflicts with `expr`",
        ),
        (
            "tween target=0 expr=\"t\" path=\"M0,0 L10,0\" start=0 duration=1",
            "conflicts with `path`",
        ),
        (
            "tween target=0 property=\"fill\" expr=\"t\" start=0 duration=1",
            "holds a color",
        ),
        (
            "tween target=0 property=\"x\" expr=\"t\" kind=\"from-to\" from=0 to=1 start=0 duration=1",
            "`kind` conflicts with `expr`",
        ),
        (
            "tween target=0 property=\"x\" expr=\"t\" ease=\"in-quad\" start=0 duration=1",
            "`ease` conflicts with `expr`",
        ),
        (
            "tween target=0 property=\"x\" expr=\"t\" stagger=0.5 start=0 duration=1",
            "`stagger` conflicts with `expr`",
        ),
        (
            "tween target=0 property=\"x\" expr=\"t\" repeat=2 start=0 duration=1",
            "`repeat` conflicts with `expr`",
        ),
    ] {
        let dir = tmp("conflict");
        write_comp(&dir, &comp_with(&format!("    {tween}\n")));
        let v = validate_folder(&dir);
        assert_eq!(v.status, Status::Invalid, "for {tween:?}");
        let ps = problems(&dir);
        assert!(
            ps.iter().any(|p| p.0 == "E_BAD_MOTION" && p.3.contains(needle)),
            "expected {needle:?} for {tween:?}, got {ps:?}"
        );
    }
}

#[test]
fn out_of_range_and_nonfinite_expressions_name_the_instant() {
    // T010: sampled values outside the property range (or non-finite)
    // refuse with the instant named.
    for (property, source, needle) in [
        ("opacity", "t", "outside 0 (transparent)"),
        ("width", "40-t*30", "outside"),
        ("x", "1/(t-1)", "t=1.000000s"),
    ] {
        let dir = tmp("range");
        let clip = if property == "width" {
            CLIP
        } else if property == "opacity" {
            "    box x=10 y=40 width=40 height=40 fill=\"#ff0000\" opacity=0.5 start=0 duration=4\n"
        } else {
            CLIP
        };
        write_comp(
            &dir,
            &format!(
                "{OPEN}{clip}    tween target=0 property=\"{property}\" expr=\"{source}\" start=0 duration=2\n}}\n"
            ),
        );
        let v = validate_folder(&dir);
        assert_eq!(v.status, Status::Invalid, "for {property}={source}");
        let ps = problems(&dir);
        assert!(
            ps.iter()
                .any(|p| p.0 == "E_BAD_MOTION" && p.3.contains(needle)),
            "expected {needle:?} for {property}={source}, got {ps:?}"
        );
    }
}

// --- replicate and input refusals ------------------------------------------

#[test]
fn non_positive_replicate_counts_fail() {
    // T010/REC-004: zero, negative, fractional, and missing counts
    // fail with E_BAD_MOTION.
    for count in ["count=0", "count=-2", "count=2.5", ""] {
        let dir = tmp("count");
        let rep = if count.is_empty() {
            "    replicate {\n        box x=10 y=40 width=20 height=20 fill=\"#ff0000\" start=0 duration=4\n    }\n"
                .to_string()
        } else {
            format!(
                "    replicate {count} {{\n        box x=10 y=40 width=20 height=20 fill=\"#ff0000\" start=0 duration=4\n    }}\n"
            )
        };
        write_comp(&dir, &comp_with(&rep));
        let v = validate_folder(&dir);
        assert_eq!(v.status, Status::Invalid, "for {count:?}");
        let ps = problems(&dir);
        assert!(
            ps.iter().any(|p| p.0 == "E_BAD_MOTION"),
            "expected E_BAD_MOTION for {count:?}, got {ps:?}"
        );
    }
}

#[test]
fn replicate_template_rules_name_the_input() {
    // T010: missing/extra templates, non-picture templates, nested
    // motion, and template ids all fail.
    for (body, needle) in [
        ("    replicate count=2\n", "no template clip"),
        (
            "    replicate count=2 {\n        box x=1 y=1 width=2 height=2 fill=\"#ff0000\" start=0 duration=1\n        box x=1 y=1 width=2 height=2 fill=\"#ff0000\" start=0 duration=1\n    }\n",
            "exactly one child",
        ),
        (
            "    replicate count=2 {\n        audio path=\"s.wav\" start=0 duration=1\n    }\n",
            "one picture clip",
        ),
        (
            "    replicate count=2 {\n        tween target=0 property=\"x\" to=5 start=0 duration=1\n    }\n",
            "never motion",
        ),
        (
            "    replicate count=2 {\n        replicate count=2 {\n            box x=1 y=1 width=2 height=2 fill=\"#ff0000\" start=0 duration=1\n        }\n    }\n",
            "never motion",
        ),
        (
            "    replicate count=2 {\n        box id=\"b\" x=1 y=1 width=2 height=2 fill=\"#ff0000\" start=0 duration=1\n    }\n",
            "carries `id`",
        ),
    ] {
        let dir = tmp("template");
        write_comp(&dir, &comp_with(body));
        let v = validate_folder(&dir);
        assert_eq!(v.status, Status::Invalid, "for {body:?}");
        let ps = problems(&dir);
        assert!(
            ps.iter().any(|p| p.0 == "E_BAD_MOTION" && p.3.contains(needle)),
            "expected {needle:?} for {body:?}, got {ps:?}"
        );
    }
}

#[test]
fn bad_inputs_fail_naming_the_input() {
    // T010: reserved/duplicate/malformed names and values fail.
    for (body, code, needle) in [
        ("    input name=\"t\" value=1\n", "E_BAD_MOTION", "reserved"),
        (
            "    input name=\"1abc\" value=1\n",
            "E_BAD_ID",
            "must be 1-64",
        ),
        (
            "    input name=\"amp\"\n",
            "E_BAD_MOTION",
            "missing required `value`",
        ),
        (
            "    input name=\"amp\" value=1\n    input name=\"amp\" value=2\n",
            "E_DUP_ID",
            "duplicate input",
        ),
        (
            "    input name=\"amp\" value=\"loud\"\n",
            "E_BAD_MOTION",
            "not a readable number",
        ),
    ] {
        let dir = tmp("input");
        write_comp(&dir, &comp_with(body));
        let v = validate_folder(&dir);
        assert_eq!(v.status, Status::Invalid, "for {body:?}");
        let ps = problems(&dir);
        assert!(
            ps.iter()
                .any(|p| p.0 == code && (p.3.contains(needle) || p.4.contains(needle))),
            "expected {code} {needle:?} for {body:?}, got {ps:?}"
        );
    }
}

#[test]
fn undeclared_names_fail_while_declared_pass() {
    // T010: an `expr` reading an undeclared name fails; declaring it
    // turns the same project green.
    let dir = tmp("undeclared");
    write_comp(
        &dir,
        &comp_with("    tween target=0 property=\"x\" expr=\"t+loud\" start=0 duration=1\n"),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid);
    assert!(
        problems(&dir)
            .iter()
            .any(|p| p.0 == "E_BAD_MOTION" && p.3.contains("unknown name `loud`")),
        "got {:?}",
        v.diagnostics
    );
    let dir = tmp("declared");
    write_comp(
        &dir,
        &comp_with(
            "    input name=\"loud\" value=3\n    tween target=0 property=\"x\" expr=\"t+loud\" start=0 duration=1\n",
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
}
