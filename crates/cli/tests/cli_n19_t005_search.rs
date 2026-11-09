//! Sprint 19 T005: `framesmith catalog search|show|report-miss`
//! end to end over a fixture registry.
//!
//! Word search ranks weighted name/title/description/tags matches
//! with stemming and compound splitting, fully offline; the human
//! picker listing, `--json`, and the Studio panel share one merged
//! index and agree on order. No-hit searches offer miss reporting;
//! an empty index reports the empty registry. Every fixture is our
//! own clean-room content.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-n19-t005-cli-{tag}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_framesmith"))
}

fn run(args: &[&str], cwd: &Path) -> (i32, String) {
    let out = Command::new(bin())
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("run framesmith binary");
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    (out.status.code().unwrap_or(-1), text)
}

fn manifest(name: &str, kind: &str, title: &str, description: &str, tags: &[&str], dir: &Path) {
    let tags_json = tags
        .iter()
        .map(|t| format!("{t:?}"))
        .collect::<Vec<_>>()
        .join(",");
    let dims = if kind == "component" {
        String::new()
    } else {
        r#","width": 1280, "height": 720, "duration": 4"#.to_string()
    };
    let text = format!(
        r#"{{"name": {name:?}, "type": {kind:?}, "title": {title:?}, "description": {description:?},
            "files": [{{"src": "composition.kdl", "target": "composition.kdl"}}]{dims}, "tags": [{tags_json}]}}"#
    );
    let folder = dir.join(name);
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("manifest.json"), text).unwrap();
}

/// Fixture registry mirroring the catalog-crate sample: `sunset`
/// matches in name, title, tags, and description in that order.
fn sample_registry(root: &Path) -> String {
    let dir = root.join("registry");
    std::fs::create_dir_all(&dir).unwrap();
    manifest(
        "sunset-glow",
        "block",
        "Harbor Evening",
        "An original amber composition over still water.",
        &["evenings"],
        &dir,
    );
    manifest(
        "dawn-chorus",
        "component",
        "Sunset Accents",
        "A short snippet with soft edges.",
        &["accents"],
        &dir,
    );
    manifest(
        "meadow-walk",
        "block",
        "Meadow Walk",
        "A gentle stroll past painted fences.",
        &["sunset"],
        &dir,
    );
    manifest(
        "pebble-sort",
        "component",
        "Pebble Sort",
        "A quiet sunset shimmer for backgrounds.",
        &["shimmer"],
        &dir,
    );
    dir.to_string_lossy().into_owned()
}

fn picker_order(out: &str) -> Vec<String> {
    out.lines()
        .filter_map(|l| l.split_once(". "))
        .map(|(_, rest)| rest.split_whitespace().next().unwrap().to_string())
        .collect()
}

#[test]
fn search_picker_ranks_weighted_fields_offline() {
    let root = tmp("rank");
    let reg = sample_registry(&root);
    let (code, out) = run(&["catalog", "search", "sunset", "--registry", &reg], &root);
    assert_eq!(code, 0, "{out}");
    assert_eq!(
        picker_order(&out),
        vec!["sunset-glow", "dawn-chorus", "meadow-walk", "pebble-sort"],
        "picker ranks name > title > tags > description:\n{out}"
    );
    assert!(out.contains("framesmith catalog install"), "{out}");
}

#[test]
fn search_stems_and_splits_compounds() {
    let root = tmp("stems");
    let reg = sample_registry(&root);
    let (code, out) = run(&["catalog", "search", "evening", "--registry", &reg], &root);
    assert_eq!(code, 0, "{out}");
    assert_eq!(picker_order(&out), vec!["sunset-glow"], "{out}");
    // A joined compound splits and ranks its item first.
    let (code, out) = run(
        &["catalog", "search", "sunsetglow", "--registry", &reg],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    assert_eq!(picker_order(&out)[0], "sunset-glow", "{out}");
    // A compound unique to one item isolates it.
    let (code, out) = run(
        &["catalog", "search", "dawnchorus", "--registry", &reg],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    assert_eq!(picker_order(&out), vec!["dawn-chorus"], "{out}");
}

#[test]
fn search_json_matches_picker_order() {
    let root = tmp("json");
    let reg = sample_registry(&root);
    let (code, out) = run(
        &["catalog", "search", "sunset", "--json", "--registry", &reg],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    let value: serde_json::Value = serde_json::from_str(&out).unwrap();
    let order: Vec<String> = value["hits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h["name"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        order,
        vec!["sunset-glow", "dawn-chorus", "meadow-walk", "pebble-sort"],
        "{out}"
    );
    assert_eq!(value["empty"], false);
}

#[test]
fn no_hit_search_offers_miss_reporting() {
    let root = tmp("miss");
    let reg = sample_registry(&root);
    let (code, out) = run(
        &["catalog", "search", "xylophone", "--registry", &reg],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("no items match"), "{out}");
    assert!(out.contains("report-miss"), "{out}");
    let (code, out) = run(
        &[
            "catalog",
            "search",
            "xylophone",
            "--json",
            "--registry",
            &reg,
        ],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    let value: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(value["hits"].as_array().unwrap().len(), 0);
    assert!(value["report_offer"]
        .as_str()
        .unwrap()
        .contains("report-miss"));
}

#[test]
fn empty_index_reports_the_empty_registry() {
    let root = tmp("empty");
    let missing = root.join("no-such-registry");
    let missing_arg = missing.to_string_lossy().into_owned();
    let (code, out) = run(
        &["catalog", "search", "sunset", "--registry", &missing_arg],
        &root,
    );
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("holds no items"), "{out}");
    assert!(out.contains(&missing_arg), "{out}");
    let (code, out) = run(
        &[
            "catalog",
            "search",
            "sunset",
            "--json",
            "--registry",
            &missing_arg,
        ],
        &root,
    );
    assert_eq!(code, 1, "{out}");
    let value: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(value["empty"], true);
    // An existing but item-free directory reports the same way.
    let bare = root.join("bare");
    std::fs::create_dir_all(&bare).unwrap();
    let bare_arg = bare.to_string_lossy().into_owned();
    let (code, out) = run(
        &["catalog", "search", "sunset", "--registry", &bare_arg],
        &root,
    );
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("holds no items"), "{out}");
}

#[test]
fn show_prints_item_page_and_refuses_unknown_names() {
    let root = tmp("show");
    let reg = sample_registry(&root);
    let (code, out) = run(
        &["catalog", "show", "sunset-glow", "--registry", &reg],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("Harbor Evening"), "{out}");
    assert!(out.contains("framesmith catalog install"), "{out}");
    let (code, out) = run(
        &["catalog", "show", "no-such-item", "--registry", &reg],
        &root,
    );
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("unknown item"), "{out}");
    assert!(out.contains("sunset-glow"), "{out}");
    // Showing from an empty registry reports the empty registry.
    let missing = root.join("missing");
    let missing_arg = missing.to_string_lossy().into_owned();
    let (code, out) = run(
        &["catalog", "show", "sunset-glow", "--registry", &missing_arg],
        &root,
    );
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("holds no items"), "{out}");
}

#[test]
fn report_miss_stores_and_says_so_when_it_cannot() {
    let root = tmp("report");
    let reg = sample_registry(&root);
    let (code, out) = run(
        &["catalog", "report-miss", "xylophone", "--registry", &reg],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("recorded the miss"), "{out}");
    let log = std::fs::read_to_string(root.join("registry").join(".miss-reports.jsonl")).unwrap();
    assert!(log.contains("\"xylophone\""), "{log}");
    // A registry that cannot take the log says so and keeps the result.
    let missing = root.join("no-such-registry");
    let missing_arg = missing.to_string_lossy().into_owned();
    let (code, out) = run(
        &[
            "catalog",
            "report-miss",
            "xylophone",
            "--registry",
            &missing_arg,
        ],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("cannot store"), "{out}");
    assert!(out.contains("search result stands"), "{out}");
}

#[test]
fn catalog_usage_errors_exit_2_and_change_nothing() {
    let root = tmp("usage");
    let reg = sample_registry(&root);
    for args in [
        vec!["catalog"],
        vec!["catalog", "browse"],
        vec!["catalog", "search"],
        vec![
            "catalog",
            "search",
            "sunset",
            "--json",
            "--json",
            "--registry",
            &reg,
        ],
        vec!["catalog", "search", "sunset", "--unknown"],
        vec!["catalog", "show"],
        vec!["catalog", "show", "a", "b", "--registry", &reg],
        vec!["catalog", "report-miss"],
    ] {
        let (code, out) = run(&args, &root);
        assert_eq!(code, 2, "{args:?}: {out}");
        assert!(out.contains("usage:"), "{args:?}: {out}");
    }
    // Usage failures write nothing into the registry.
    assert!(
        !root.join("registry").join(".miss-reports.jsonl").exists(),
        "usage errors must not write"
    );
}
