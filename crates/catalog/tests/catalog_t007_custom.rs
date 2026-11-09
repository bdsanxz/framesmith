//! Sprint 19, T007: custom registries named by the project configuration.
//!
//! Covers every acceptance criterion at the library level: custom
//! items appear with their registry of origin and install like
//! bundled ones (gates and lock included), name collisions follow the
//! configured order both ways, an unreachable custom registry warns
//! by name while the bundled base keeps working, and an invalid
//! custom manifest is skipped naming its file while healthy items
//! still list. All fixture content is original clean-room work: no
//! HyperFrames item is ever copied, and KDL keeps hyphenated style
//! attributes.

use catalog::{
    check_version_gates, install_merged_with_options, load_merged, project_registries,
    registry_sources, resolve_merged, CustomRegistry, InstallOptions, RegistrySource, Selection,
    PROJECT_CONFIG_FILE,
};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-n19-t007-{tag}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_bytes(path: &Path, bytes: &[u8]) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, bytes).unwrap();
}

/// One registry item: manifest plus file contents.
fn write_item(registry: &Path, dir: &str, manifest: &str, files: &[(&str, &[u8])]) {
    let folder = registry.join(dir);
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join(catalog::MANIFEST_FILE), manifest).unwrap();
    for (src, bytes) in files {
        write_bytes(&folder.join(src), bytes);
    }
}

/// Every file under a directory: relative path to bytes.
fn tree_of(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    if !root.exists() {
        return out;
    }
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let mut entries: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().collect();
        entries.sort_by_key(|e| e.path());
        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                out.insert(
                    path.strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                    std::fs::read(&path).unwrap(),
                );
            }
        }
    }
    out
}

fn selection(names: &[&str], tags: &[&str]) -> Selection {
    Selection {
        names: names.iter().map(|s| s.to_string()).collect(),
        tags: tags.iter().map(|s| s.to_string()).collect(),
    }
}

fn base_source(dir: &Path) -> RegistrySource {
    RegistrySource {
        name: "bundled".to_string(),
        dir: dir.to_path_buf(),
        before_bundled: false,
    }
}

fn custom_source(name: &str, dir: &Path, before_bundled: bool) -> RegistrySource {
    RegistrySource {
        name: name.to_string(),
        dir: dir.to_path_buf(),
        before_bundled,
    }
}

const BUNDLED_MANIFEST: &str = r#"{
    "name": "meadow-card",
    "type": "block",
    "title": "Meadow Card",
    "description": "An original slow card over a quiet meadow at dusk.",
    "files": [{"src": "composition.kdl", "target": "composition.kdl"}],
    "width": 1280, "height": 720, "duration": 5,
    "tags": ["meadow", "shared-tag"]
}"#;

const BUNDLED_KDL: &str = "composition name=\"meadow\" width=1280 height=720 duration=5 fps=30 background=\"#1a1a2e\" version=1 {\n    box x=80 y=200 width=400 height=120 fill=\"#f4f1de\" radius=16 border-width=4 border-color=\"#e07a5f\" start=0 duration=5\n}\n";

const CUSTOM_MANIFEST: &str = r#"{
    "name": "team-banner",
    "type": "component",
    "title": "Team Banner",
    "description": "An original shared banner snippet from the team registry.",
    "files": [{"src": "snippet.kdl", "target": "snippets/team-banner.kdl"}],
    "tags": ["team", "shared-tag"]
}"#;

const CUSTOM_KDL: &str = "// An original team banner snippet.\nbox x=80 y=120 width=560 height=96 fill=\"#fefae0\" radius=12 border-width=2 border-color=\"#606c38\" start=0 duration=4\n";

/// A project configuration naming one custom registry by path.
fn write_config(project: &Path, registries_json: &str) {
    std::fs::create_dir_all(project).unwrap();
    std::fs::write(
        project.join(PROJECT_CONFIG_FILE),
        format!("{{\"registries\": {registries_json}}}"),
    )
    .unwrap();
}

/// Custom items appear with their registry of origin in the merged
/// listing and the shared search index.
#[test]
fn custom_items_appear_with_origin_shown() {
    let root = tmp("origin");
    let base = root.join("registry");
    write_item(
        &base,
        "meadow-card",
        BUNDLED_MANIFEST,
        &[("composition.kdl", BUNDLED_KDL.as_bytes())],
    );
    let team = root.join("team-registry");
    write_item(
        &team,
        "team-banner",
        CUSTOM_MANIFEST,
        &[("snippet.kdl", CUSTOM_KDL.as_bytes())],
    );
    let project = root.join("project");
    write_config(
        &project,
        r#"[{"name": "team", "path": "../team-registry"}]"#,
    );
    let customs = project_registries(&project).expect("config parses");
    assert_eq!(customs.len(), 1);
    assert_eq!(customs[0].name, "team");
    let extras = registry_sources(&project, &customs);
    let listing = load_merged(&base_source(&base), &extras);
    assert!(listing.warnings.is_empty(), "{:?}", listing.warnings);
    assert!(listing.collisions.is_empty());
    assert_eq!(listing.names(), vec!["meadow-card", "team-banner"]);
    assert_eq!(listing.find("team-banner").unwrap().origin, "team");
    assert_eq!(listing.find("meadow-card").unwrap().origin, "bundled");
    // The shared index ranks merged items with origins attached.
    let index = catalog::SearchIndex::build_merged(&listing.items);
    assert_eq!(index.len(), 2);
    let hits = index.search("banner");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].item.name, "team-banner");
    assert_eq!(hits[0].origin, "team");
    let hits = index.search("meadow");
    assert_eq!(hits[0].origin, "bundled");
}

/// Custom items install exactly like bundled ones: bytes land at
/// manifest targets behind the same gates and lock.
#[test]
fn custom_items_install_like_bundled_ones() {
    let root = tmp("install");
    let base = root.join("registry");
    write_item(
        &base,
        "meadow-card",
        BUNDLED_MANIFEST,
        &[("composition.kdl", BUNDLED_KDL.as_bytes())],
    );
    let team = root.join("team-registry");
    // The custom item depends on the bundled one: one merged set,
    // dependencies ordered before dependents.
    let manifest = CUSTOM_MANIFEST.replace(
        "\"tags\"",
        "\"registryDependencies\": [\"meadow-card\"],\n    \"tags\"",
    );
    write_item(
        &team,
        "team-banner",
        &manifest,
        &[("snippet.kdl", CUSTOM_KDL.as_bytes())],
    );
    let project = root.join("project");
    write_config(&project, r#"["../team-registry"]"#);
    let customs = project_registries(&project).expect("config parses");
    assert_eq!(customs[0].name, "../team-registry");
    let extras = registry_sources(&project, &customs);
    let merged = resolve_merged(&base, &extras, &selection(&["team-banner"], &[])).unwrap();
    assert_eq!(merged.plan.order(), vec!["meadow-card", "team-banner"]);
    assert_eq!(merged.origins, vec!["bundled", "../team-registry"]);
    check_version_gates(&merged.plan, catalog::CURRENT_CLI_VERSION).unwrap();
    let options = InstallOptions::with_vars(catalog::CURRENT_CLI_VERSION, &[], false).unwrap();
    let (report, merged) = install_merged_with_options(
        &base,
        &project,
        &extras,
        &selection(&["team-banner"], &[]),
        &options,
    )
    .unwrap();
    assert_eq!(report.items, vec!["meadow-card", "team-banner"]);
    assert_eq!(
        std::fs::read(project.join("composition.kdl")).unwrap(),
        BUNDLED_KDL.as_bytes()
    );
    assert_eq!(
        std::fs::read(project.join("snippets/team-banner.kdl")).unwrap(),
        CUSTOM_KDL.as_bytes()
    );
    // The lock records every installed file, custom items included.
    let lock =
        std::fs::read_to_string(project.join(catalog::CATALOG_LOCK_FILE)).expect("lock is written");
    assert!(lock.contains("snippets/team-banner.kdl"), "{lock}");
    assert!(lock.contains("composition.kdl"), "{lock}");
    assert!(merged.origins.contains(&"../team-registry".to_string()));
}

/// A gated custom item refuses before any write, naming the item and
/// both versions; the project stays byte-identical.
#[test]
fn gated_custom_items_refuse_before_any_write() {
    let root = tmp("gated");
    let base = root.join("registry");
    write_item(
        &base,
        "meadow-card",
        BUNDLED_MANIFEST,
        &[("composition.kdl", BUNDLED_KDL.as_bytes())],
    );
    let team = root.join("team-registry");
    let manifest =
        CUSTOM_MANIFEST.replace("\"tags\"", "\"minCliVersion\": \"99.0.0\",\n    \"tags\"");
    write_item(
        &team,
        "team-banner",
        &manifest,
        &[("snippet.kdl", CUSTOM_KDL.as_bytes())],
    );
    let project = root.join("project");
    write_config(&project, r#"["../team-registry"]"#);
    let extras = registry_sources(&project, &project_registries(&project).unwrap());
    let merged = resolve_merged(&base, &extras, &selection(&["team-banner"], &[])).unwrap();
    let error = check_version_gates(&merged.plan, "0.0.0").expect_err("gate must refuse");
    assert!(error.wrote_nothing());
    assert!(
        error.message().contains("team-banner"),
        "{}",
        error.message()
    );
    assert!(error.message().contains("99.0.0"), "{}", error.message());
    assert!(error.message().contains("0.0.0"), "{}", error.message());
    let before = tree_of(&project);
    let options = InstallOptions::with_vars("0.0.0", &[], false).unwrap();
    let error = install_merged_with_options(
        &base,
        &project,
        &extras,
        &selection(&["team-banner"], &[]),
        &options,
    )
    .unwrap_err();
    assert!(error.wrote_nothing(), "{}", error.message());
    assert_eq!(tree_of(&project), before, "gated installs write nothing");
}

/// Collisions follow the configured order: bundled wins by default,
/// an explicit `beforeBundled` custom registry wins instead.
#[test]
fn collisions_follow_the_configured_order_both_ways() {
    for (tag, entries, winner_origin, winner_kdl) in [
        (
            "bundled-wins",
            r#"[{"name": "team", "path": "../team-registry"}]"#,
            "bundled",
            BUNDLED_KDL,
        ),
        (
            "custom-wins",
            r#"[{"name": "team", "path": "../team-registry", "beforeBundled": true}]"#,
            "team",
            CUSTOM_KDL,
        ),
    ] {
        let root = tmp(tag);
        let base = root.join("registry");
        write_item(
            &base,
            "meadow-card",
            BUNDLED_MANIFEST,
            &[("composition.kdl", BUNDLED_KDL.as_bytes())],
        );
        // Same item name, different bytes and target layout.
        let team = root.join("team-registry");
        let manifest = BUNDLED_MANIFEST.replace(
            "An original slow card over a quiet meadow at dusk.",
            "A team variant of the meadow card.",
        );
        write_item(
            &team,
            "meadow-card",
            &manifest,
            &[("composition.kdl", CUSTOM_KDL.as_bytes())],
        );
        let project = root.join("project");
        write_config(&project, entries);
        let extras = registry_sources(&project, &project_registries(&project).unwrap());
        let listing = load_merged(&base_source(&base), &extras);
        assert_eq!(listing.names(), vec!["meadow-card"]);
        assert_eq!(listing.find("meadow-card").unwrap().origin, winner_origin);
        assert_eq!(listing.collisions.len(), 1, "{:?}", listing.collisions);
        let collision = &listing.collisions[0];
        assert_eq!(collision.name, "meadow-card");
        assert_eq!(collision.winner, winner_origin);
        let merged = resolve_merged(&base, &extras, &selection(&["meadow-card"], &[])).unwrap();
        assert_eq!(merged.origins, vec![winner_origin]);
        let (report, _) =
            catalog::install_merged(&base, &project, &extras, &selection(&["meadow-card"], &[]))
                .unwrap();
        assert_eq!(report.items, vec!["meadow-card"]);
        assert_eq!(
            std::fs::read(project.join("composition.kdl")).unwrap(),
            winner_kdl.as_bytes(),
            "{tag}: the configured winner's bytes must land"
        );
    }
}

/// An unreachable custom registry warns by name while the bundled
/// base keeps working for search and install.
#[test]
fn unreachable_custom_registries_warn_by_name_bundled_keeps_working() {
    let root = tmp("unreachable");
    let base = root.join("registry");
    write_item(
        &base,
        "meadow-card",
        BUNDLED_MANIFEST,
        &[("composition.kdl", BUNDLED_KDL.as_bytes())],
    );
    let project = root.join("project");
    write_config(
        &project,
        r#"[{"name": "ghost", "path": "../no-such-registry"}, {"name": "team", "path": "../team-registry"}]"#,
    );
    let team = root.join("team-registry");
    write_item(
        &team,
        "team-banner",
        CUSTOM_MANIFEST,
        &[("snippet.kdl", CUSTOM_KDL.as_bytes())],
    );
    let extras = registry_sources(&project, &project_registries(&project).unwrap());
    let listing = load_merged(&base_source(&base), &extras);
    assert_eq!(listing.names(), vec!["meadow-card", "team-banner"]);
    assert_eq!(listing.warnings.len(), 1, "{:?}", listing.warnings);
    assert!(
        listing.warnings[0].contains("\"ghost\""),
        "warn by name: {:?}",
        listing.warnings
    );
    assert!(
        listing.warnings[0].contains("unreachable"),
        "{:?}",
        listing.warnings
    );
    // Bundled search and install keep working beside the warning.
    let index = catalog::SearchIndex::build_merged(&listing.items);
    assert_eq!(index.search("meadow")[0].origin, "bundled");
    let (report, merged) =
        catalog::install_merged(&base, &project, &extras, &selection(&["meadow-card"], &[]))
            .unwrap();
    assert_eq!(report.items, vec!["meadow-card"]);
    assert_eq!(merged.warnings.len(), 1);
    assert_eq!(
        std::fs::read(project.join("composition.kdl")).unwrap(),
        BUNDLED_KDL.as_bytes()
    );
}

/// An invalid custom manifest is skipped naming its file while
/// healthy custom items beside it still list and install.
#[test]
fn invalid_custom_manifests_skip_naming_the_file() {
    let root = tmp("invalid");
    let base = root.join("registry");
    write_item(
        &base,
        "meadow-card",
        BUNDLED_MANIFEST,
        &[("composition.kdl", BUNDLED_KDL.as_bytes())],
    );
    let team = root.join("team-registry");
    write_item(
        &team,
        "team-banner",
        CUSTOM_MANIFEST,
        &[("snippet.kdl", CUSTOM_KDL.as_bytes())],
    );
    // Schema failure: a component must not carry dimensions is one
    // inverted here into a block missing them.
    std::fs::create_dir_all(team.join("broken-piece")).unwrap();
    std::fs::write(
        team.join("broken-piece").join("manifest.json"),
        r#"{"name": "broken-piece", "type": "block", "title": "Broken",
            "description": "A block with no canvas.", "files": []}"#,
    )
    .unwrap();
    let project = root.join("project");
    write_config(
        &project,
        r#"[{"name": "team", "path": "../team-registry"}]"#,
    );
    let extras = registry_sources(&project, &project_registries(&project).unwrap());
    let listing = load_merged(&base_source(&base), &extras);
    // The healthy custom item still lists; the broken one never does.
    assert_eq!(listing.names(), vec!["meadow-card", "team-banner"]);
    assert!(
        !listing.broken.is_empty(),
        "the invalid manifest must report its fields"
    );
    assert!(
        listing.broken.iter().all(|b| b.item == "broken-piece"),
        "{:?}",
        listing.broken
    );
    assert!(
        listing.broken.iter().any(|b| b.field == "files"),
        "{:?}",
        listing.broken
    );
    // The skip warning names the manifest file.
    let warning = listing
        .warnings
        .iter()
        .find(|w| w.contains("broken-piece"))
        .expect("skip warning names the item");
    assert!(warning.contains("manifest.json"), "{warning}");
    assert!(warning.contains("\"team\""), "{warning}");
    let error = resolve_merged(&base, &extras, &selection(&["broken-piece"], &[])).unwrap_err();
    assert!(error.wrote_nothing());
    assert!(
        error.message().contains("broken-piece"),
        "{}",
        error.message()
    );
}

/// A name claimed twice inside one source refuses naming the item
/// and its directories, like single-registry resolution.
#[test]
fn duplicate_names_inside_one_source_refuse() {
    let root = tmp("dupes");
    let base = root.join("registry");
    write_item(
        &base,
        "first",
        BUNDLED_MANIFEST,
        &[("composition.kdl", BUNDLED_KDL.as_bytes())],
    );
    write_item(
        &base,
        "second",
        BUNDLED_MANIFEST,
        &[("composition.kdl", BUNDLED_KDL.as_bytes())],
    );
    let team = root.join("team-registry");
    write_item(
        &team,
        "team-banner",
        CUSTOM_MANIFEST,
        &[("snippet.kdl", CUSTOM_KDL.as_bytes())],
    );
    let project = root.join("project");
    write_config(&project, r#"["../team-registry"]"#);
    let extras = registry_sources(&project, &project_registries(&project).unwrap());
    // The bundled duplicate refuses when requested ...
    let error = resolve_merged(&base, &extras, &selection(&["meadow-card"], &[])).unwrap_err();
    assert!(error.wrote_nothing());
    assert!(
        error.message().contains("meadow-card"),
        "{}",
        error.message()
    );
    // ... while an unrelated healthy item still resolves and installs.
    let merged = resolve_merged(&base, &extras, &selection(&["team-banner"], &[])).unwrap();
    assert_eq!(merged.plan.order(), vec!["team-banner"]);
}

/// Misshapen `registries` values refuse naming the configuration file.
#[test]
fn misshapen_registries_config_refuses_naming_the_file() {
    for (tag, contents) in [
        ("not-array", r#"{"registries": "team"}"#),
        ("bad-entry", r#"{"registries": [42]}"#),
        ("missing-path", r#"{"registries": [{"name": "team"}]}"#),
        ("empty-path", r#"{"registries": ["  "]}"#),
        (
            "bad-flag",
            r#"{"registries": [{"path": "x", "beforeBundled": "yes"}]}"#,
        ),
        (
            "dup-name",
            r#"{"registries": ["x", {"path": "y", "name": "x"}]}"#,
        ),
        (
            "reserved-name",
            r#"{"registries": [{"path": "x", "name": "bundled"}]}"#,
        ),
    ] {
        let root = tmp(&format!("bad-config-{tag}"));
        let project = root.join("project");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(project.join(PROJECT_CONFIG_FILE), contents).unwrap();
        let error = project_registries(&project).expect_err("bad config must refuse");
        assert!(
            error.message().contains(PROJECT_CONFIG_FILE),
            "{tag}: {}",
            error.message()
        );
    }
    // Unknown registry entry fields stay ignored for later slices.
    let root = tmp("extra-fields");
    let project = root.join("project");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(
        project.join(PROJECT_CONFIG_FILE),
        r#"{"registries": [{"path": "x", "name": "team", "future": true}]}"#,
    )
    .unwrap();
    let customs = project_registries(&project).unwrap();
    assert_eq!(
        customs,
        vec![CustomRegistry {
            name: "team".to_string(),
            path: "x".to_string(),
            before_bundled: false,
        }]
    );
}

/// Tags resolve across the merged registries as one set.
#[test]
fn tags_resolve_across_registries_as_one_set() {
    let root = tmp("tags");
    let base = root.join("registry");
    write_item(
        &base,
        "meadow-card",
        BUNDLED_MANIFEST,
        &[("composition.kdl", BUNDLED_KDL.as_bytes())],
    );
    let team = root.join("team-registry");
    write_item(
        &team,
        "team-banner",
        CUSTOM_MANIFEST,
        &[("snippet.kdl", CUSTOM_KDL.as_bytes())],
    );
    let project = root.join("project");
    write_config(&project, r#"["../team-registry"]"#);
    let extras = registry_sources(&project, &project_registries(&project).unwrap());
    let merged = resolve_merged(&base, &extras, &selection(&[], &["shared-tag"])).unwrap();
    assert_eq!(merged.plan.order(), vec!["meadow-card", "team-banner"]);
    assert_eq!(merged.origins, vec!["bundled", "../team-registry"]);
    let error = resolve_merged(&base, &extras, &selection(&[], &["no-such-tag"])).unwrap_err();
    assert!(
        error.message().contains("no-such-tag"),
        "{}",
        error.message()
    );
}

/// Custom KDL keeps hyphenated style attributes, parses, and carries
/// no forbidden content.
#[test]
fn custom_kdl_stays_hygienic_and_parses() {
    for kdl in [BUNDLED_KDL, CUSTOM_KDL] {
        assert!(
            catalog::kdl_hygiene(kdl).is_empty(),
            "{:?}",
            catalog::kdl_hygiene(kdl)
        );
        assert!(kdl.contains("border-width"), "hyphenated style attrs");
        assert!(!kdl.contains("border_width"), "{kdl}");
        notation::parse::parse_document(kdl).expect("custom KDL parses");
    }
}
