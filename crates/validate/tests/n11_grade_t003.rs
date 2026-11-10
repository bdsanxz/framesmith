//! Sprint 11 LUT import (T003) validation: the `lut` child of
//! `grade` on clips, lane groups, and the composition, with
//! project-folder-relative `.cube` files checked at check time.
//!
//! Validates through the public [`validate_folder`] API only: every
//! level lands where written and merges piece by piece; a missing
//! file, an unparsable cube, an oversized grid, an out-of-range
//! amount, duplicated stages, and unknown stage keys or children fail
//! naming the file or the value and the fix.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use validate::validate::{validate_folder, Status};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-validate-n11-lut-{tag}-{}-{}",
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

/// One size-2 identity `.cube`: every lattice point maps to itself.
fn write_identity_cube(dir: &Path, name: &str) {
    let mut s = String::from("TITLE \"identity\"\nLUT_3D_SIZE 2\n");
    for b in 0..2 {
        for g in 0..2 {
            for r in 0..2 {
                s.push_str(&format!("{r} {g} {b}\n"));
            }
        }
    }
    std::fs::write(dir.join(name), s).unwrap();
}

const OPEN: &str =
    "composition name=\"main\" root=true width=160 height=120 duration=4 background=\"#000\" version=1 {\n";

fn codes_messages(dir: &Path) -> Vec<(String, String, String)> {
    validate_folder(dir)
        .diagnostics
        .iter()
        .map(|d| (d.code.to_string(), d.message.clone(), d.fix.clone()))
        .collect()
}

fn codes(dir: &Path) -> Vec<String> {
    codes_messages(dir)
        .iter()
        .map(|(c, _, _)| c.clone())
        .collect()
}

// --- T003: lut validates on every level --------------------------------------

#[test]
fn lut_validates_on_clip_lane_and_composition() {
    let dir = tmp("levels-ok");
    write_identity_cube(&dir, "look.cube");
    write_comp(
        &dir,
        &format!(
            "{OPEN}\
             grade preset=\"filmic\" {{\n lut src=\"look.cube\" amount=0.8\n}}\n\
             lane track=\"video\" {{\n grade {{\n lut src=\"look.cube\"\n}}\n}}\n\
             box x=0 y=0 width=20 height=12 fill=\"#ff0000\" start=0 duration=4 {{\n grade {{\n lut src=\"look.cube\" amount=1\n}}\n}}\n\
             }}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    let comp = v.composition.expect("valid composition");
    assert_eq!(comp.grade.lut.src, Some("look.cube".to_string()));
    assert_eq!(comp.grade.lut.amount, Some(0.8));
    assert_eq!(comp.lane_grades.len(), 1);
    assert_eq!(
        comp.lane_grades[0].grade.lut.src,
        Some("look.cube".to_string())
    );
    assert_eq!(comp.lane_grades[0].grade.lut.amount, None);
    use notation::model::Clip;
    match &comp.clips[0] {
        Clip::Box(c) => {
            assert_eq!(c.grade.lut.src, Some("look.cube".to_string()));
            assert_eq!(c.grade.lut.amount, Some(1.0));
        }
        other => panic!("expected box, got {other:?}"),
    }
}

#[test]
fn bare_and_partial_luts_are_identity_or_inherit() {
    // A bare `lut` validates as the identity; an amount-only `lut`
    // validates and inherits the file from the outer level.
    let dir = tmp("partial-ok");
    write_identity_cube(&dir, "look.cube");
    write_comp(
        &dir,
        &format!(
            "{OPEN}\
             grade {{\n lut src=\"look.cube\"\n}}\n\
             box x=0 y=0 width=20 height=12 fill=\"#ff0000\" start=0 duration=4 {{\n grade {{\n lut amount=0.5\n}}\n}}\n\
             box x=0 y=0 width=4 height=4 fill=\"#00ff00\" start=0 duration=4 {{\n grade {{\n lut\n}}\n}}\n\
             }}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    let comp = v.composition.expect("valid composition");
    use notation::model::Clip;
    match &comp.clips[0] {
        Clip::Box(c) => {
            assert_eq!(c.grade.lut.src, None);
            assert_eq!(c.grade.lut.amount, Some(0.5));
        }
        other => panic!("expected box, got {other:?}"),
    }
    match &comp.clips[1] {
        Clip::Box(c) => {
            assert!(c.grade.lut.is_empty());
        }
        other => panic!("expected box, got {other:?}"),
    }
    // Boundary amounts validate: 0 passes through, 1 applies fully.
    for amount in ["0", "1", "0.0", "1.0"] {
        let dir = tmp("boundary-ok");
        write_identity_cube(&dir, "look.cube");
        write_comp(
            &dir,
            &format!(
                "{OPEN}\
                 box x=0 y=0 width=4 height=4 fill=\"#00ff00\" start=0 duration=4 {{\n grade {{\n lut src=\"look.cube\" amount={amount}\n}}\n}}\n\
                 }}\n"
            ),
        );
        let v = validate_folder(&dir);
        assert_eq!(
            v.status,
            Status::Valid,
            "amount={amount}: {:?}",
            v.diagnostics
        );
    }
}

// --- T003: LUT refusals --------------------------------------------------------

#[test]
fn lut_missing_file_refuses_naming_the_file() {
    let dir = tmp("missing");
    write_comp(
        &dir,
        &format!(
            "{OPEN}\
             box x=0 y=0 width=20 height=12 fill=\"#ff0000\" start=0 duration=4 {{\n grade {{\n lut src=\"gone.cube\" amount=1\n}}\n}}\n\
             }}\n"
        ),
    );
    let all = codes_messages(&dir);
    assert!(all.iter().any(|(c, _, _)| c == "E_MISSING_FILE"), "{all:?}");
    let hit = all
        .iter()
        .find(|(c, _, _)| c == "E_MISSING_FILE")
        .expect("missing LUT names its code");
    assert!(hit.1.contains("gone.cube"), "{hit:?}");
}

#[test]
fn lut_unparsable_refuses_naming_the_file_and_the_problem() {
    let dir = tmp("unparsable");
    std::fs::write(dir.join("bad.cube"), "LUT_3D_SIZE 2\n0 0\n").unwrap();
    write_comp(
        &dir,
        &format!(
            "{OPEN}\
             box x=0 y=0 width=20 height=12 fill=\"#ff0000\" start=0 duration=4 {{\n grade {{\n lut src=\"bad.cube\"\n}}\n}}\n\
             }}\n"
        ),
    );
    let all = codes_messages(&dir);
    assert_eq!(
        all.iter().map(|(c, _, _)| c.clone()).collect::<Vec<_>>(),
        vec!["E_BAD_GRADE"],
        "{all:?}"
    );
    assert!(all[0].1.contains("bad.cube"), "{all:?}");
    assert!(all[0].1.contains("2"), "{all:?}");
}

#[test]
fn lut_oversized_grid_refuses_naming_the_limit() {
    let dir = tmp("oversized");
    std::fs::write(dir.join("big.cube"), "LUT_3D_SIZE 64\n").unwrap();
    write_comp(
        &dir,
        &format!(
            "{OPEN}\
             box x=0 y=0 width=20 height=12 fill=\"#ff0000\" start=0 duration=4 {{\n grade {{\n lut src=\"big.cube\"\n}}\n}}\n\
             }}\n"
        ),
    );
    let all = codes_messages(&dir);
    assert_eq!(
        all.iter().map(|(c, _, _)| c.clone()).collect::<Vec<_>>(),
        vec!["E_BAD_GRADE"],
        "{all:?}"
    );
    assert!(all[0].1.contains("big.cube"), "{all:?}");
    assert!(all[0].1.contains("64"), "{all:?}");
    assert!(all[0].1.contains("33"), "{all:?}");
}

#[test]
fn lut_amount_out_of_range_refuses_naming_the_range() {
    for amount in ["-0.5", "1.5", "2"] {
        let dir = tmp("amount-bad");
        write_identity_cube(&dir, "look.cube");
        write_comp(
            &dir,
            &format!(
                "{OPEN}\
                 box x=0 y=0 width=20 height=12 fill=\"#ff0000\" start=0 duration=4 {{\n grade {{\n lut src=\"look.cube\" amount={amount}\n}}\n}}\n\
                 }}\n"
            ),
        );
        let all = codes_messages(&dir);
        assert_eq!(
            all.iter().map(|(c, _, _)| c.clone()).collect::<Vec<_>>(),
            vec!["E_BAD_GRADE"],
            "amount={amount}: {all:?}"
        );
        assert!(all[0].1.contains("amount"), "amount={amount}: {all:?}");
        assert!(all[0].1.contains("0 to 1"), "amount={amount}: {all:?}");
    }
}

#[test]
fn lut_duplicate_unknown_child_and_key_refused() {
    // Two `lut` children on one grade.
    let dir = tmp("dup");
    write_identity_cube(&dir, "look.cube");
    write_comp(
        &dir,
        &format!(
            "{OPEN}\
             box x=0 y=0 width=20 height=12 fill=\"#ff0000\" start=0 duration=4 {{\n grade {{\n lut src=\"look.cube\"\n lut src=\"look.cube\"\n}}\n}}\n\
             }}\n"
        ),
    );
    assert_eq!(
        codes(&dir),
        vec!["E_BAD_GRADE"],
        "{:?}",
        codes_messages(&dir)
    );
    // A non-lut child of `lut`.
    let dir = tmp("child");
    write_identity_cube(&dir, "look.cube");
    write_comp(
        &dir,
        &format!(
            "{OPEN}\
             box x=0 y=0 width=20 height=12 fill=\"#ff0000\" start=0 duration=4 {{\n grade {{\n lut src=\"look.cube\" {{\n wheel\n}}\n}}\n}}\n\
             }}\n"
        ),
    );
    assert_eq!(
        codes(&dir),
        vec!["E_BAD_GRADE"],
        "{:?}",
        codes_messages(&dir)
    );
    // An unknown key on `lut`.
    let dir = tmp("key");
    write_identity_cube(&dir, "look.cube");
    write_comp(
        &dir,
        &format!(
            "{OPEN}\
             box x=0 y=0 width=20 height=12 fill=\"#ff0000\" start=0 duration=4 {{\n grade {{\n lut src=\"look.cube\" strength=1\n}}\n}}\n\
             }}\n"
        ),
    );
    assert_eq!(
        codes(&dir),
        vec!["E_UNKNOWN_KEY"],
        "{:?}",
        codes_messages(&dir)
    );
}
