//! Sprint 7 motion (T008): path travel validates — `path` and
//! `path-to` parse as SVG-subset data, `orient` is boolean, morphs
//! need the same command structure, and every refusal names the
//! command with file, line, column, code, and fix.
//!
//! Validates through the public [`validate_folder`] API only.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use validate::validate::{validate_folder, Status};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-validate-n07-paths-{tag}-{}-{}",
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

fn problems(dir: &Path) -> Vec<(String, usize, String, String)> {
    validate_folder(dir)
        .diagnostics
        .iter()
        .map(|d| (d.code.to_string(), d.line, d.message.clone(), d.fix.clone()))
        .collect()
}

// --- valid path motion -----------------------------------------------------

#[test]
fn path_tween_validates_with_travel_fields() {
    let dir = tmp("tween");
    write_comp(
        &dir,
        &comp_with(
            "    tween target=0 path=\"M10,40 L150,40\" start=0 duration=2 ease=\"linear\"\n",
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    let comp = v.composition.expect("valid composition");
    assert_eq!(comp.tweens.len(), 1);
    let tween = &comp.tweens[0];
    assert_eq!(tween.path.as_deref(), Some("M10,40 L150,40"));
    assert_eq!(tween.path_to, None);
    assert!(!tween.orient);
}

#[test]
fn path_tween_with_morph_orient_and_aliases_validates() {
    let dir = tmp("morph");
    write_comp(
        &dir,
        &comp_with(
            "    tween target=0 path=\"M10,40 L150,40\" path_to=\"M10,80 L150,80\" orient=true start=0 duration=2 stagger=0.1 repeat=2\n",
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    let comp = v.composition.expect("valid composition");
    let tween = &comp.tweens[0];
    assert_eq!(tween.path_to.as_deref(), Some("M10,80 L150,80"));
    assert!(tween.orient);
    assert_eq!(tween.repeat, 2);
}

#[test]
fn path_key_with_orient_validates() {
    let dir = tmp("key");
    write_comp(
        &dir,
        &comp_with(
            "    key target=0 orient=true {\n        frame at=0 path=\"M10,40 L150,40\"\n        frame at=2 path=\"M10,80 L150,80\"\n    }\n",
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    let comp = v.composition.expect("valid composition");
    assert_eq!(comp.keys.len(), 1);
    let key = &comp.keys[0];
    assert!(key.orient);
    assert_eq!(key.stops.len(), 2);
    assert_eq!(key.stops[0].path.as_deref(), Some("M10,40 L150,40"));
}

// --- unparseable paths name the command ------------------------------------

#[test]
fn unsupported_commands_fail_naming_the_letter() {
    for (data, letter) in [
        ("M10,40 H90", 'H'),
        ("M10,40 S20,20 30,30", 'S'),
        ("M10,40 T90,40", 'T'),
        ("M10,40 V90", 'V'),
    ] {
        let dir = tmp("unsupported");
        write_comp(
            &dir,
            &comp_with(&format!(
                "    tween target=0 path=\"{data}\" start=0 duration=1\n"
            )),
        );
        let found = problems(&dir);
        assert_eq!(found.len(), 1, "{data}: {found:?}");
        assert_eq!(found[0].0, "E_BAD_MOTION");
        assert!(found[0].1 > 0, "refusals carry a line");
        assert!(
            found[0].2.contains(&format!("`{letter}`")),
            "{data}: {:?}",
            found[0].2
        );
        assert!(!found[0].3.is_empty(), "refusals carry a fix");
    }
}

#[test]
fn malformed_path_data_fails_naming_the_command() {
    let cases = [
        ("M10", "`M`"),
        ("M10,xx", "`M`"),
        ("M10,40 C0,0 0,0", "`C`"),
        ("M10,40 A10,10 0 2 0 20,20", "`A`"),
        ("", "path"),
        ("10,10", "command"),
    ];
    for (data, named) in cases {
        let dir = tmp("malformed");
        write_comp(
            &dir,
            &comp_with(&format!(
                "    key target=0 {{\n        frame at=0 path=\"{data}\"\n        frame at=1 path=\"M10,40 L150,40\"\n    }}\n"
            )),
        );
        let found = problems(&dir);
        assert!(
            found.iter().any(|(c, line, m, fix)| c == "E_BAD_MOTION"
                && *line > 0
                && m.contains(named)
                && !fix.is_empty()),
            "{data:?} must refuse naming {named}: {found:?}"
        );
    }
}

// --- morphs ------------------------------------------------------------------

#[test]
fn cross_structure_tween_morph_fails_naming_the_command() {
    let dir = tmp("morph-refuse");
    write_comp(
        &dir,
        &comp_with(
            "    tween target=0 path=\"M10,40 L150,40\" path-to=\"M10,40 C60,0 90,80 150,40\" start=0 duration=1\n",
        ),
    );
    let found = problems(&dir);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].0, "E_BAD_MOTION");
    assert!(found[0].2.contains("command 2"), "{:?}", found[0].2);
    assert!(found[0].2.contains("`L`"), "{:?}", found[0].2);
    assert!(found[0].2.contains("`C`"), "{:?}", found[0].2);
}

#[test]
fn cross_structure_key_morph_fails_naming_the_stop() {
    let dir = tmp("key-morph-refuse");
    write_comp(
        &dir,
        &comp_with(
            "    key target=0 {\n        frame at=0 path=\"M10,40 L150,40\"\n        frame at=1 path=\"M10,40 Q80,0 150,40\"\n    }\n",
        ),
    );
    let found = problems(&dir);
    assert!(
        found.iter().any(|(c, _, m, _)| c == "E_BAD_MOTION"
            && m.contains("stop 2")
            && m.contains("`L`")
            && m.contains("`Q`")),
        "{found:?}"
    );
}

#[test]
fn path_to_without_path_fails() {
    let dir = tmp("path-to-alone");
    write_comp(
        &dir,
        &comp_with("    tween target=0 path-to=\"M10,40 L150,40\" start=0 duration=1\n"),
    );
    let found = problems(&dir);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].0, "E_BAD_MOTION");
    assert!(found[0].2.contains("`path-to`"), "{:?}", found[0].2);
}

// --- shape conflicts -----------------------------------------------------------

#[test]
fn numeric_keys_conflict_with_path_travel() {
    let dir = tmp("conflict");
    write_comp(
        &dir,
        &comp_with(
            "    tween target=0 property=\"x\" kind=\"to\" from=1 to=2 path=\"M10,40 L150,40\" start=0 duration=1\n",
        ),
    );
    let found = problems(&dir);
    for key in ["property", "kind", "from", "to"] {
        assert!(
            found
                .iter()
                .any(|(c, _, m, _)| c == "E_BAD_MOTION" && m.contains(&format!("`{key}`"))),
            "`{key}` must conflict with `path`: {found:?}"
        );
    }
}

#[test]
fn orient_without_path_fails() {
    for body in [
        "    tween target=0 property=\"x\" kind=\"to\" to=10 orient=true start=0 duration=1\n",
        "    key target=0 property=\"x\" orient=false {\n        frame at=0 value=0\n        frame at=1 value=10\n    }\n",
    ] {
        let dir = tmp("orient-alone");
        write_comp(&dir, &comp_with(body));
        let found = problems(&dir);
        assert!(
            found
                .iter()
                .any(|(c, _, m, _)| c == "E_BAD_MOTION" && m.contains("`orient`")),
            "{body}: {found:?}"
        );
    }
}

#[test]
fn bad_orient_shape_fails() {
    let dir = tmp("orient-shape");
    write_comp(
        &dir,
        &comp_with(
            "    tween target=0 path=\"M10,40 L150,40\" orient=\"yes\" start=0 duration=1\n",
        ),
    );
    let found = problems(&dir);
    assert!(
        found
            .iter()
            .any(|(c, _, m, _)| c == "E_BAD_MOTION" && m.contains("`orient`")),
        "{found:?}"
    );
}

#[test]
fn value_stops_conflict_with_path_stops() {
    let dir = tmp("mixed-stops");
    write_comp(
        &dir,
        &comp_with(
            "    key target=0 property=\"x\" {\n        frame at=0 path=\"M10,40 L150,40\"\n        frame at=1 value=10\n        frame at=2 path=\"M10,80 L150,80\"\n    }\n",
        ),
    );
    let found = problems(&dir);
    assert!(
        found.iter().any(|(c, _, m, _)| c == "E_BAD_MOTION"
            && m.contains("stop 2")
            && m.contains("`value`")),
        "{found:?}"
    );
    assert!(
        found
            .iter()
            .any(|(c, _, m, _)| c == "E_BAD_MOTION" && m.contains("`property`")),
        "{found:?}"
    );
}

#[test]
fn element_level_path_on_key_points_at_stops() {
    let dir = tmp("element-path");
    write_comp(
        &dir,
        &comp_with(
            "    key target=0 path=\"M10,40 L150,40\" {\n        frame at=0 value=0\n        frame at=1 value=10\n    }\n",
        ),
    );
    let found = problems(&dir);
    assert!(
        found.iter().any(|(c, _, m, _)| c == "E_BAD_MOTION"
            && m.contains("`frame`")
            && m.contains("`path`")),
        "{found:?}"
    );
}

#[test]
fn set_with_path_fails_as_unknown_key() {
    let dir = tmp("set-path");
    write_comp(
        &dir,
        &comp_with(
            "    set target=0 property=\"x\" path=\"M10,40 L150,40\" start=0 duration=1 value=10\n",
        ),
    );
    let found = problems(&dir);
    assert!(
        found
            .iter()
            .any(|(c, _, m, _)| c == "E_UNKNOWN_KEY" && m.contains("\"path\"")),
        "{found:?}"
    );
}

#[test]
fn every_path_problem_reports_together() {
    let dir = tmp("together");
    write_comp(
        &dir,
        &comp_with(
            "    tween target=0 path=\"M10,40 H90\" start=0 duration=1\n    tween target=0 path=\"M0,0 L1,0\" path-to=\"M0,0 C1,1 2,2 3,3\" start=0 duration=1\n    key target=0 {\n        frame at=0 path=\"M10,40 L150,40\"\n        frame at=1 path=\"M10,40 V90\"\n    }\n",
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Invalid);
    assert!(v.composition.is_none());
    let found = problems(&dir);
    assert!(
        found.iter().any(|(_, _, m, _)| m.contains("`H`")),
        "{found:?}"
    );
    assert!(
        found.iter().any(|(_, _, m, _)| m.contains("`C`")),
        "{found:?}"
    );
    assert!(
        found.iter().any(|(_, _, m, _)| m.contains("`V`")),
        "{found:?}"
    );
}
