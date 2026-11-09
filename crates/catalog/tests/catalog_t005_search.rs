//! Sprint 19 T005: offline word search over the catalog.
//!
//! Weighted name/title/description/tags ranking with stemming and
//! compound splitting answers immediately and fully offline; the
//! CLI, the human-friendly picker, and the Studio panel rank
//! through one shared [`SearchIndex`]. A search with words but no
//! hits offers search-miss reporting; an empty index reports the
//! empty registry instead of zero hits. Every fixture below is our
//! own clean-room content.

use catalog::{
    append_miss_report, empty_json, empty_registry_report, hits_json, load_search_index,
    no_hits_json, no_hits_report, render_picker, resolve_registry_dir, SearchIndex,
};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-n19-t005-{tag}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
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

/// A small original registry: names, titles, tags, and
/// descriptions use distinct words so each ranking test isolates
/// one field.
fn sample_registry() -> PathBuf {
    let dir = tmp("sample");
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
    dir
}

fn names<'a>(hits: &[catalog::RankedHit<'a>]) -> Vec<String> {
    hits.iter().map(|h| h.item.name.clone()).collect()
}

#[test]
fn weighted_fields_rank_name_above_title_above_tags_above_description() {
    let dir = sample_registry();
    let (_, index) = load_search_index(&dir);
    // "sunset" hits every item in a different field: name
    // (sunset-glow) beats title (dawn-chorus) beats tags
    // (meadow-walk) beats description (pebble-sort).
    let order = names(&index.search("sunset"));
    assert_eq!(
        order,
        vec!["sunset-glow", "dawn-chorus", "meadow-walk", "pebble-sort"],
        "name > title > tags > description"
    );
    let hits = index.search("sunset");
    assert!(
        hits[0].score > hits[1].score
            && hits[1].score > hits[2].score
            && hits[2].score > hits[3].score,
        "scores must strictly separate the fields: {:?}",
        hits.iter().map(|h| h.score).collect::<Vec<_>>()
    );
}

#[test]
fn stemming_finds_inflected_forms() {
    let dir = sample_registry();
    let (_, index) = load_search_index(&dir);
    // "evenings" is tagged on sunset-glow; the singular finds it.
    let order = names(&index.search("evening"));
    assert_eq!(order, vec!["sunset-glow"], "{order:?}");
    // Plural query reaches the singular title word.
    let order = names(&index.search("accents"));
    assert_eq!(order, vec!["dawn-chorus"], "{order:?}");
}

#[test]
fn compound_splitting_finds_joined_and_humped_names() {
    let dir = tmp("compounds");
    manifest(
        "title-card",
        "block",
        "Opening Card",
        "An original first-frame greeting.",
        &["openers"],
        &dir,
    );
    manifest(
        "lowerThird",
        "component",
        "Guest Stripe",
        "An original name stripe for guests.",
        &["stripes"],
        &dir,
    );
    let (_, index) = load_search_index(&dir);
    // Joined query splits against the index vocabulary.
    assert_eq!(names(&index.search("titlecard")), vec!["title-card"]);
    // Camel humps index as parts.
    let order = names(&index.search("lower third"));
    assert_eq!(order, vec!["lowerThird"], "{order:?}");
    assert_eq!(names(&index.search("lowerthird")), vec!["lowerThird"]);
}

#[test]
fn one_merged_index_serves_picker_and_machine_output() {
    let dir = sample_registry();
    let (_, index) = load_search_index(&dir);
    let hits = index.search("sunset");
    // The picker prints the same ranking the index returns.
    let picker = render_picker("sunset", &dir, &hits);
    let picker_order: Vec<String> = picker
        .lines()
        .filter_map(|l| l.split_once(". "))
        .map(|(_, rest)| rest.split_whitespace().next().unwrap().to_string())
        .collect();
    assert_eq!(picker_order, names(&hits), "picker order:\n{picker}");
    assert!(
        picker.contains("framesmith catalog install"),
        "picker hands picks to the installer:\n{picker}"
    );
    // The machine envelope carries the same ranked order.
    let value: serde_json::Value = serde_json::from_str(&hits_json("sunset", &dir, &hits)).unwrap();
    let json_order: Vec<String> = value["hits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h["name"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(json_order, names(&hits));
    assert_eq!(value["empty"], false);
}

#[test]
fn no_hit_search_offers_miss_reporting() {
    let dir = sample_registry();
    let (_, index) = load_search_index(&dir);
    assert!(index.search("xylophone").is_empty());
    let report = no_hits_report("xylophone", &dir);
    assert!(report.contains("no items match"), "{report}");
    assert!(report.contains("report-miss"), "{report}");
    let value: serde_json::Value = serde_json::from_str(&no_hits_json("xylophone", &dir)).unwrap();
    assert_eq!(value["hits"].as_array().unwrap().len(), 0);
    assert!(
        value["report_offer"]
            .as_str()
            .unwrap()
            .contains("report-miss"),
        "{value}"
    );
}

#[test]
fn empty_index_reports_the_empty_registry() {
    // A missing directory lists as empty and reports the registry.
    let missing = tmp("missing").join("no-such-registry");
    let (listing, index) = load_search_index(&missing);
    assert!(listing.items.is_empty());
    assert!(index.is_empty());
    let report = empty_registry_report(&missing);
    assert!(report.contains("holds no items"), "{report}");
    assert!(report.contains(&missing.display().to_string()), "{report}");
    let value: serde_json::Value = serde_json::from_str(&empty_json(&missing)).unwrap();
    assert_eq!(value["empty"], true);
    // An existing but item-free directory reports the same way.
    let bare = tmp("bare");
    let (_, index) = load_search_index(&bare);
    assert!(index.is_empty());
    assert!(empty_registry_report(&bare).contains("holds no items"));
}

#[test]
fn broken_manifests_never_hide_healthy_items_from_search() {
    let dir = tmp("broken");
    manifest(
        "healthy-glow",
        "block",
        "Healthy Glow",
        "An original sunny composition.",
        &["sunny"],
        &dir,
    );
    let bad = dir.join("bad-item");
    std::fs::create_dir_all(&bad).unwrap();
    std::fs::write(
        bad.join("manifest.json"),
        r#"{"name": "bad-item", "type": "component", "title": "Bad",
            "description": "Missing files.", "files": []}"#,
    )
    .unwrap();
    let (listing, index) = load_search_index(&dir);
    assert_eq!(listing.broken.len(), 1);
    assert_eq!(names(&index.search("sunny")), vec!["healthy-glow"]);
}

#[test]
fn empty_query_matches_nothing() {
    let dir = sample_registry();
    let (_, index) = load_search_index(&dir);
    assert!(index.search("").is_empty());
    assert!(index.search("   ").is_empty());
}

#[test]
fn search_is_deterministic_across_rebuilds() {
    let dir = sample_registry();
    let (_, first) = load_search_index(&dir);
    let (_, second) = load_search_index(&dir);
    assert_eq!(
        names(&first.search("sunset")),
        names(&second.search("sunset"))
    );
}

#[test]
fn miss_reports_append_and_unwritable_logs_say_so() {
    let dir = sample_registry();
    let path = append_miss_report(&dir, "xylophone").expect("writable log stores");
    assert_eq!(path, dir.join(catalog::MISS_REPORTS_FILE));
    let log = std::fs::read_to_string(&path).unwrap();
    assert!(log.contains("\"xylophone\""), "{log}");
    // A missing registry directory cannot store the report: the
    // error says so and the search result stands.
    let missing = tmp("missing-log").join("no-such-registry");
    let err = append_miss_report(&missing, "xylophone").expect_err("missing dir refuses");
    assert!(err.contains("cannot store"), "{err}");
}

#[test]
fn registry_resolution_prefers_flag_then_env_then_cwd() {
    assert_eq!(
        resolve_registry_dir(Some("/tmp/x")),
        PathBuf::from("/tmp/x")
    );
    std::env::set_var("FRAMESMITH_REGISTRY", "/tmp/from-env");
    assert_eq!(resolve_registry_dir(None), PathBuf::from("/tmp/from-env"));
    std::env::remove_var("FRAMESMITH_REGISTRY");
    assert_eq!(resolve_registry_dir(None), PathBuf::from("registry"));
}

#[test]
fn index_search_never_needs_the_network_or_the_model() {
    // Registry files are plain local JSON; search ranks them with
    // no configuration beyond the directory.
    let dir = sample_registry();
    let (listing, index) = load_search_index(&dir);
    assert_eq!(listing.items.len(), 4);
    assert_eq!(index.len(), 4);
    assert!(SearchIndex::build(&[]).is_empty());
}
