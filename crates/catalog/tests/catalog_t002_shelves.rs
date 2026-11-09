//! Sprint 19, T002: bundled clean-room catalog covering all 22 shelves.
//!
//! Covers every acceptance criterion: all 22 shelves appear, each
//! holds several original items, per-shelf counts print on request,
//! no HyperFrames item content appears anywhere, and an emptied shelf
//! fails the contribution check by name — plus the refusal paths.
//! All fixture content is original clean-room work: boxes-only KDL
//! with hyphenated style attributes, no `font-family`, and no
//! external references.

use catalog::{
    check_bundled_coverage, check_listing_coverage, format_shelf_counts, kdl_hygiene,
    listing_shelf_counts, load_registry, manifest_json, materialize_bundled, parse_manifest_str,
    shelf_counts, shelf_of_manifest, ItemType, BANNED_HYPERFRAMES_NAMES, BUNDLED,
    MIN_ITEMS_PER_SHELF, SHELVES,
};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-n19-t002-{tag}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// All 22 shelves appear, exactly once each.
#[test]
fn all_22_shelves_appear() {
    assert_eq!(SHELVES.len(), 22, "the bundled registry covers 22 shelves");
    let mut seen = HashSet::new();
    for shelf in SHELVES {
        assert!(!shelf.name.is_empty(), "shelf name must not be empty");
        assert!(!shelf.title.is_empty(), "shelf title must not be empty");
        assert!(seen.insert(shelf.name), "duplicate shelf {:?}", shelf.name);
    }
}

/// Each shelf holds several original items: at least three, spanning
/// block, component, and example kinds, with unique well-formed names
/// and distinct non-empty titles and descriptions.
#[test]
fn each_shelf_holds_several_original_items() {
    for (shelf, count) in shelf_counts(BUNDLED) {
        assert!(
            count >= MIN_ITEMS_PER_SHELF,
            "shelf {shelf:?} holds only {count} items"
        );
    }
    let mut kinds: HashSet<(&str, &str)> = HashSet::new();
    let mut names = HashSet::new();
    let mut titles = HashSet::new();
    let mut descriptions = HashSet::new();
    for item in BUNDLED {
        assert!(
            catalog::lookup_shelf(item.shelf).is_some(),
            "item {:?} names an unknown shelf",
            item.name
        );
        assert!(
            names.insert(item.name),
            "duplicate item name {:?}",
            item.name
        );
        assert!(
            !item.title.trim().is_empty(),
            "item {:?} needs a title",
            item.name
        );
        assert!(
            titles.insert(item.title),
            "duplicate title {:?} — items must be original",
            item.title
        );
        assert!(
            descriptions.insert(item.description),
            "duplicate description for {:?} — items must be original",
            item.name
        );
        kinds.insert((item.shelf, item.item_type.as_str()));
    }
    assert_eq!(names.len(), BUNDLED.len());
    for shelf in SHELVES {
        for kind in [ItemType::Block, ItemType::Component, ItemType::Example] {
            assert!(
                kinds.contains(&(shelf.name, kind.as_str())),
                "shelf {:?} needs a {kind} item",
                shelf.name
            );
        }
    }
}

/// Per-shelf counts print on request: every shelf is named with its
/// exact count and kind breakdown, and the total matches the registry.
#[test]
fn per_shelf_counts_print_on_request() {
    let table = format_shelf_counts(BUNDLED);
    assert!(table.contains(&BUNDLED.len().to_string()), "{table}");
    assert!(table.contains("22 shelves"), "{table}");
    for (shelf, count) in shelf_counts(BUNDLED) {
        let line = table
            .lines()
            .find(|l| l.starts_with(&format!("{shelf}:")))
            .unwrap_or_else(|| panic!("counts must name shelf {shelf:?}:\n{table}"));
        assert!(line.contains(&count.to_string()), "wrong count in {line:?}");
        assert!(line.contains("1 block"), "breakdown missing in {line:?}");
        assert!(
            line.contains("1 component"),
            "breakdown missing in {line:?}"
        );
        assert!(line.contains("1 example"), "breakdown missing in {line:?}");
    }
    assert_eq!(
        table.lines().filter(|l| l.contains(':')).count(),
        SHELVES.len() + 1,
        "one header line plus one line per shelf:\n{table}"
    );
}

/// No HyperFrames item content appears anywhere: names, titles,
/// descriptions, tags, and KDL bodies never mention a distinctive
/// HyperFrames item, template, preset, or shader name.
#[test]
fn no_hyperframes_item_content_anywhere() {
    for item in BUNDLED {
        let mut haystacks = vec![
            item.name.to_string(),
            item.title.to_string(),
            item.description.to_string(),
        ];
        haystacks.extend(item.tags.iter().map(|t| t.to_string()));
        haystacks.extend(item.files.iter().map(|f| f.contents.to_string()));
        for hay in &haystacks {
            let lower = hay.to_lowercase();
            for banned in BANNED_HYPERFRAMES_NAMES {
                assert!(
                    !lower.contains(banned),
                    "item {:?} must not copy HyperFrames content {banned:?}",
                    item.name
                );
            }
        }
    }
}

/// Every bundled manifest parses under the T001 schema with the right
/// shape: blocks and examples carry dimensions and duration while
/// components never do, and every manifest names its shelf first.
#[test]
fn bundled_manifests_all_parse_with_dims_rules() {
    for item in BUNDLED {
        let doc = manifest_json(item);
        let parsed = parse_manifest_str(&doc).unwrap_or_else(|errors| {
            panic!(
                "item {:?} manifest must parse, got {errors:?}\n{doc}",
                item.name
            )
        });
        assert_eq!(parsed.name, item.name);
        assert_eq!(parsed.item_type, item.item_type);
        assert_eq!(parsed.title, item.title);
        assert_eq!(parsed.description, item.description);
        assert!(!parsed.files.is_empty());
        match item.item_type {
            ItemType::Block | ItemType::Example => {
                assert_eq!(parsed.width, item.width, "item {:?}", item.name);
                assert_eq!(parsed.height, item.height, "item {:?}", item.name);
                assert_eq!(parsed.duration, item.duration, "item {:?}", item.name);
            }
            ItemType::Component => {
                assert_eq!(parsed.width, None, "item {:?}", item.name);
                assert_eq!(parsed.height, None, "item {:?}", item.name);
                assert_eq!(parsed.duration, None, "item {:?}", item.name);
            }
        }
        assert_eq!(
            shelf_of_manifest(&parsed),
            Some(item.shelf),
            "item {:?} must group back into its shelf",
            item.name
        );
    }
}

/// Catalog KDL hygiene: every body parses as KDL, uses hyphenated
/// attributes only, names no fonts, and carries no external
/// references; every painted clip names a fill.
#[test]
fn catalog_kdl_is_hygienic_and_parses() {
    for item in BUNDLED {
        for f in item.files {
            let body = f.contents.replace("{name}", item.name);
            assert!(
                kdl_hygiene(f.contents).is_empty(),
                "item {:?} KDL hygiene: {:?}",
                item.name,
                kdl_hygiene(f.contents)
            );
            notation::parse::parse_document(&body)
                .unwrap_or_else(|e| panic!("item {:?} KDL must parse: {e:?}\n{body}", item.name));
            for line in body.lines() {
                let line = line.trim_start();
                if line.starts_with("box ") {
                    assert!(
                        line.contains("fill="),
                        "item {:?} box must carry paint:\n{line}",
                        item.name
                    );
                }
            }
        }
    }
}

/// The materialized registry lists healthy: every item lands on disk
/// grouped by shelf, loads with zero broken entries, and the coverage
/// check over the healthy listing is clean.
#[test]
fn materialized_registry_lists_healthy() {
    let root = tmp("materialize");
    let written = materialize_bundled(&root).unwrap();
    assert_eq!(written, BUNDLED.len());
    assert_eq!(written, SHELVES.len() * MIN_ITEMS_PER_SHELF);
    for shelf in SHELVES {
        assert!(
            root.join(shelf.name).is_dir(),
            "shelf {:?} must materialize as a directory",
            shelf.name
        );
    }
    let listing = load_registry(&root);
    assert!(
        listing.broken.is_empty(),
        "unexpected broken: {:?}",
        listing.broken
    );
    assert_eq!(listing.items.len(), BUNDLED.len());
    for (shelf, count) in listing_shelf_counts(&listing) {
        assert_eq!(count, MIN_ITEMS_PER_SHELF, "shelf {shelf:?} count");
    }
    assert!(
        check_listing_coverage(&listing).is_empty(),
        "healthy listing must pass coverage: {:?}",
        check_listing_coverage(&listing)
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// Refusal path: an emptied shelf fails the contribution check by
/// name, while every other shelf keeps passing.
#[test]
fn emptied_shelf_fails_coverage_by_name() {
    let root = tmp("emptied");
    materialize_bundled(&root).unwrap();
    std::fs::remove_dir_all(root.join("maps")).unwrap();
    let listing = load_registry(&root);
    assert!(
        listing.broken.is_empty(),
        "unexpected broken: {:?}",
        listing.broken
    );
    let faults = check_listing_coverage(&listing);
    assert_eq!(faults.len(), 1, "exactly one fault expected: {faults:?}");
    assert_eq!(faults[0].shelf, "maps");
    assert!(
        faults[0].message.contains("maps"),
        "fault must name the shelf: {}",
        faults[0].message
    );
    assert!(faults[0].to_string().contains("maps"), "{faults:?}");
    let _ = std::fs::remove_dir_all(&root);
}

/// Refusal path: a thinned shelf (below the per-shelf minimum but not
/// empty) also fails, naming the shelf and the short count.
#[test]
fn thinned_shelf_fails_coverage_by_name() {
    let root = tmp("thinned");
    materialize_bundled(&root).unwrap();
    std::fs::remove_file(root.join("maps").join("route-bar").join("manifest.json")).unwrap();
    std::fs::remove_dir_all(root.join("maps").join("route-bar")).unwrap();
    let listing = load_registry(&root);
    let faults = check_listing_coverage(&listing);
    assert_eq!(faults.len(), 1, "exactly one fault expected: {faults:?}");
    assert_eq!(faults[0].shelf, "maps");
    assert!(
        faults[0].message.contains('2'),
        "fault names the count: {}",
        faults[0].message
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// The bundled contribution check is clean over the shipped statics.
#[test]
fn bundled_contribution_check_is_clean() {
    let faults = check_bundled_coverage();
    assert!(
        faults.is_empty(),
        "bundled registry must be clean: {faults:?}"
    );
}
