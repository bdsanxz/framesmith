//! Sprint 19, T001: item manifest schema and broken-item listing.
//!
//! Covers every acceptance criterion: required fields read back,
//! blocks and examples carry dimensions and duration while components
//! never do, optional fields read truthfully, schema failures list
//! broken naming item and field, and broken items never block healthy
//! listings — plus the refusal paths. All fixture content is original
//! clean-room work. This slice ships no KDL, so the hyphenated style
//! attribute gate holds vacuously here.

use catalog::{load_registry, MANIFEST_FILE};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-n19-t001-{tag}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_item(registry: &Path, dir: &str, manifest: &str) {
    write_nested_item(registry, &PathBuf::from(dir), manifest);
}

fn write_nested_item(registry: &Path, rel: &Path, manifest: &str) {
    let folder = registry.join(rel);
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join(MANIFEST_FILE), manifest).unwrap();
}

const BLOCK: &str = r#"{
    "name": "harbor-morning",
    "type": "block",
    "title": "Harbor Morning",
    "description": "An original slow pan over a quiet harbor at dawn.",
    "files": [
        {"src": "composition.kdl", "target": "composition.kdl"},
        {"src": "notes.txt", "target": "docs/notes.txt"}
    ],
    "width": 1280,
    "height": 720,
    "duration": 5
}"#;

const COMPONENT: &str = r#"{
    "name": "soft-caption",
    "type": "component",
    "title": "Soft Caption",
    "description": "An original lower caption snippet.",
    "files": [{"src": "snippet.kdl", "target": "snippet.kdl"}]
}"#;

/// Every required field reads back from a listed item.
#[test]
fn required_fields_read_back() {
    let registry = tmp("required");
    write_item(&registry, "harbor", BLOCK);
    let listing = load_registry(&registry);
    assert!(
        listing.broken.is_empty(),
        "unexpected broken: {:?}",
        listing.broken
    );
    assert_eq!(listing.items.len(), 1);
    let m = &listing.items[0];
    assert_eq!(m.name, "harbor-morning");
    assert_eq!(m.item_type, catalog::ItemType::Block);
    assert_eq!(m.title, "Harbor Morning");
    assert_eq!(
        m.description,
        "An original slow pan over a quiet harbor at dawn."
    );
    assert_eq!(listing.items[0].files.len(), 2);
    assert_eq!(listing.items[0].files[0].src, "composition.kdl");
    assert_eq!(listing.items[0].files[0].target, "composition.kdl");
    assert_eq!(listing.items[0].files[1].target, "docs/notes.txt");
}

/// Blocks carry dimensions and duration.
#[test]
fn block_dimensions_read_back() {
    let registry = tmp("block-dims");
    write_item(&registry, "harbor", BLOCK);
    let listing = load_registry(&registry);
    let m = &listing.items[0];
    assert_eq!(m.width, Some(1280));
    assert_eq!(m.height, Some(720));
    assert_eq!(m.duration, Some(5.0));
}

/// Examples carry dimensions and duration too.
#[test]
fn example_dimensions_read_back() {
    let registry = tmp("example-dims");
    write_item(
        &registry,
        "lantern",
        r#"{
            "name": "lantern-night", "type": "example",
            "title": "Lantern Night",
            "description": "An original starter under paper lanterns.",
            "files": [{"src": "composition.kdl", "target": "composition.kdl"}],
            "width": 1920, "height": 1080, "duration": 4.5
        }"#,
    );
    let listing = load_registry(&registry);
    assert!(
        listing.broken.is_empty(),
        "unexpected broken: {:?}",
        listing.broken
    );
    let m = &listing.items[0];
    assert_eq!(
        (m.width, m.height, m.duration),
        (Some(1920), Some(1080), Some(4.5))
    );
}

/// A block missing dimensions fails naming the field.
#[test]
fn block_missing_width_lists_broken() {
    let registry = tmp("block-no-width");
    write_item(&registry, "harbor", &BLOCK.replace("\"width\": 1280,", ""));
    let listing = load_registry(&registry);
    assert!(listing.items.is_empty());
    assert_eq!(listing.broken.len(), 1);
    assert_eq!(listing.broken[0].item, "harbor-morning");
    assert_eq!(listing.broken[0].field, "width");
}

/// An example missing duration fails naming the field.
#[test]
fn example_missing_duration_lists_broken() {
    let registry = tmp("example-no-duration");
    write_item(
        &registry,
        "lantern",
        r#"{
            "name": "lantern-night", "type": "example",
            "title": "Lantern Night",
            "description": "An original starter under paper lanterns.",
            "files": [{"src": "composition.kdl", "target": "composition.kdl"}],
            "width": 1920, "height": 1080
        }"#,
    );
    let listing = load_registry(&registry);
    assert!(listing.items.is_empty());
    assert_eq!(listing.broken.len(), 1);
    assert_eq!(listing.broken[0].item, "lantern-night");
    assert_eq!(listing.broken[0].field, "duration");
}

/// Components list healthy with no dimensions.
#[test]
fn component_lists_without_dimensions() {
    let registry = tmp("component-clean");
    write_item(&registry, "caption", COMPONENT);
    let listing = load_registry(&registry);
    assert!(
        listing.broken.is_empty(),
        "unexpected broken: {:?}",
        listing.broken
    );
    let m = &listing.items[0];
    assert_eq!(m.item_type, catalog::ItemType::Component);
    assert_eq!((m.width, m.height, m.duration), (None, None, None));
}

/// A component carrying dimensions fails naming every carried field.
#[test]
fn component_with_dimensions_lists_broken() {
    let registry = tmp("component-dims");
    write_item(
        &registry,
        "caption",
        r#"{
            "name": "soft-caption", "type": "component",
            "title": "Soft Caption",
            "description": "An original lower caption snippet.",
            "files": [{"src": "snippet.kdl", "target": "snippet.kdl"}],
            "width": 640, "height": 120, "duration": 2
        }"#,
    );
    let listing = load_registry(&registry);
    assert!(listing.items.is_empty());
    let fields: Vec<&str> = listing.broken.iter().map(|b| b.field.as_str()).collect();
    assert!(fields.contains(&"width"), "names width: {fields:?}");
    assert!(fields.contains(&"height"), "names height: {fields:?}");
    assert!(fields.contains(&"duration"), "names duration: {fields:?}");
    assert!(listing.broken.iter().all(|b| b.item == "soft-caption"));
}

/// Optional fields read truthfully when present.
#[test]
fn optional_fields_read_truthfully() {
    let registry = tmp("optionals");
    write_item(
        &registry,
        "harbor",
        r#"{
            "name": "harbor-morning", "type": "block",
            "title": "Harbor Morning",
            "description": "An original slow pan over a quiet harbor at dawn.",
            "files": [{"src": "composition.kdl", "target": "composition.kdl"}],
            "width": 1280, "height": 720, "duration": 5,
            "tags": ["dawn", "waterfront"],
            "author": "Framesmith Studio",
            "license": "MIT",
            "minCliVersion": "0.1.0",
            "deprecated": true,
            "registryDependencies": ["soft-caption"],
            "preview": "preview.png",
            "relatedSkill": "video",
            "params": [{"id": "headline", "type": "string"}],
            "variables": [{"id": "accent", "type": "color"}]
        }"#,
    );
    let listing = load_registry(&registry);
    assert!(
        listing.broken.is_empty(),
        "unexpected broken: {:?}",
        listing.broken
    );
    let m = &listing.items[0];
    assert_eq!(m.tags, vec!["dawn".to_string(), "waterfront".to_string()]);
    assert_eq!(m.author.as_deref(), Some("Framesmith Studio"));
    assert_eq!(m.license.as_deref(), Some("MIT"));
    assert_eq!(m.min_cli_version.as_deref(), Some("0.1.0"));
    assert!(m.deprecated);
    assert_eq!(m.registry_dependencies, vec!["soft-caption".to_string()]);
    assert_eq!(m.preview.as_deref(), Some("preview.png"));
    assert_eq!(m.related_skill.as_deref(), Some("video"));
    assert_eq!(m.params.len(), 1);
    assert_eq!(m.params[0]["id"], serde_json::json!("headline"));
    assert_eq!(m.variables.len(), 1);
    assert_eq!(m.variables[0]["type"], serde_json::json!("color"));
}

/// Absent optionals fall back to honest defaults.
#[test]
fn absent_optionals_default_honestly() {
    let registry = tmp("optional-defaults");
    write_item(&registry, "harbor", BLOCK);
    let listing = load_registry(&registry);
    let m = &listing.items[0];
    assert!(m.tags.is_empty());
    assert_eq!(m.author, None);
    assert_eq!(m.license, None);
    assert_eq!(m.min_cli_version, None);
    assert!(!m.deprecated);
    assert!(m.registry_dependencies.is_empty());
    assert_eq!(m.preview, None);
    assert_eq!(m.related_skill, None);
    assert!(m.params.is_empty());
    assert!(m.variables.is_empty());
}

/// A schema-failing manifest lists broken naming item and field
/// beside healthy items, which still list.
#[test]
fn broken_beside_healthy_names_item_and_field() {
    let registry = tmp("mixed");
    write_item(&registry, "harbor", BLOCK);
    write_item(&registry, "caption", COMPONENT);
    write_item(
        &registry,
        "broken",
        r#"{
            "name": "torn-ticket", "type": "ticket",
            "title": "Torn Ticket",
            "description": "An unknown kind can never list.",
            "files": [{"src": "composition.kdl", "target": "composition.kdl"}],
            "width": 640, "height": 360, "duration": 2
        }"#,
    );
    let listing = load_registry(&registry);
    assert_eq!(listing.names(), vec!["harbor-morning", "soft-caption"]);
    assert_eq!(listing.broken.len(), 1);
    assert_eq!(listing.broken[0].item, "torn-ticket");
    assert_eq!(listing.broken[0].field, "type");
}

/// Invalid JSON lists broken under the manifest field without
/// blocking healthy items.
#[test]
fn invalid_json_lists_manifest_field() {
    let registry = tmp("invalid-json");
    write_item(&registry, "harbor", BLOCK);
    write_item(&registry, "garbage", "{ not json");
    let listing = load_registry(&registry);
    assert_eq!(listing.names(), vec!["harbor-morning"]);
    assert_eq!(listing.broken.len(), 1);
    // The manifest never parsed, so the directory names the item.
    assert_eq!(listing.broken[0].item, "garbage");
    assert_eq!(listing.broken[0].field, "manifest");
}

/// A file entry without a target fails; a missing title fails too.
#[test]
fn file_entry_and_title_refusals() {
    let registry = tmp("file-title");
    write_item(
        &registry,
        "notarget",
        r#"{
            "name": "loose-page", "type": "block",
            "title": "Loose Page",
            "description": "A file entry with no target.",
            "files": [{"src": "composition.kdl"}],
            "width": 640, "height": 360, "duration": 2
        }"#,
    );
    write_item(
        &registry,
        "notitle",
        r#"{
            "name": "quiet-reel", "type": "block",
            "description": "No title can never list.",
            "files": [{"src": "composition.kdl", "target": "composition.kdl"}],
            "width": 640, "height": 360, "duration": 2
        }"#,
    );
    let listing = load_registry(&registry);
    assert!(listing.items.is_empty());
    assert_eq!(listing.broken.len(), 2);
    let notarget = listing
        .broken
        .iter()
        .find(|b| b.item == "loose-page")
        .unwrap();
    assert_eq!(notarget.field, "files");
    assert!(
        notarget.message.contains("files[0].target"),
        "names entry: {}",
        notarget.message
    );
    let notitle = listing
        .broken
        .iter()
        .find(|b| b.item == "quiet-reel")
        .unwrap();
    assert_eq!(notitle.field, "title");
}

/// Empty files, bad names, and zero durations refuse by field.
#[test]
fn shape_refusals_name_their_fields() {
    let registry = tmp("shapes");
    write_item(
        &registry,
        "empty",
        r#"{"name": "empty-shelf", "type": "block", "title": "T",
            "description": "D", "files": [],
            "width": 64, "height": 64, "duration": 1}"#,
    );
    write_item(
        &registry,
        "badname",
        &BLOCK.replace("\"harbor-morning\"", "\"has space\""),
    );
    write_item(
        &registry,
        "zero",
        &BLOCK
            .replace("\"harbor-morning\"", "\"still-frame\"")
            .replace("\"duration\": 5", "\"duration\": 0"),
    );
    let listing = load_registry(&registry);
    assert!(listing.items.is_empty());
    for (item, field) in [
        ("empty-shelf", "files"),
        ("has space", "name"),
        ("still-frame", "duration"),
    ] {
        let hit = listing
            .broken
            .iter()
            .find(|b| b.item == item)
            .unwrap_or_else(|| panic!("{item} must list broken: {:?}", listing.broken));
        assert_eq!(hit.field, field, "wrong field for {item}");
    }
}

/// Unknown fields never break a healthy listing.
#[test]
fn unknown_fields_are_ignored() {
    let registry = tmp("unknown");
    write_item(
        &registry,
        "harbor",
        &BLOCK.replace("}", ", \"future\": {\"v\": 2}}"),
    );
    let listing = load_registry(&registry);
    assert!(
        listing.broken.is_empty(),
        "unexpected broken: {:?}",
        listing.broken
    );
    assert_eq!(listing.names(), vec!["harbor-morning"]);
}

/// Grouped (for example shelf) subdirectories still list.
#[test]
fn grouped_subdirectories_list() {
    let registry = tmp("grouped");
    write_nested_item(&registry, &PathBuf::from("titles/harbor"), BLOCK);
    write_nested_item(&registry, &PathBuf::from("titles/caption"), COMPONENT);
    let listing = load_registry(&registry);
    assert!(
        listing.broken.is_empty(),
        "unexpected broken: {:?}",
        listing.broken
    );
    assert_eq!(listing.names(), vec!["harbor-morning", "soft-caption"]);
}
