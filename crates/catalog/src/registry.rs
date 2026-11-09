//! Registry listing (Sprint 19, T001 / CAP-001).
//!
//! [`load_registry`] walks a registry directory offline — every
//! directory holding a [`MANIFEST_FILE`](crate::MANIFEST_FILE) is one
//! item — and splits it into healthy items plus broken entries. A
//! broken manifest is reported with its item name and every failing
//! field, and never blocks the healthy items beside it. Output order
//! is deterministic: items sort by name, broken entries by item then
//! field.
//!
//! The walk descends into plain subdirectories (so later slices may
//! group items, for example by shelf) but never into an item
//! directory itself, and skips hidden directories. A missing or
//! unreadable registry directory lists as empty: reporting an empty
//! registry is a search-time concern for a later slice.

use std::path::{Path, PathBuf};

use crate::manifest::{parse_manifest_str, peek_name, ManifestError, MANIFEST_FILE};
use crate::ItemManifest;

/// One schema-failing manifest: the item name (the manifest's own
/// `name` when it parses that far, else the directory name) with one
/// failing field and a human message. One manifest with several bad
/// fields yields one entry per field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrokenItem {
    pub item: String,
    pub field: String,
    pub message: String,
}

impl std::fmt::Display for BrokenItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "registry item {:?} is broken: field {:?}: {}",
            self.item, self.field, self.message
        )
    }
}

/// One healthy registry item with the directory holding its files, so
/// install slices (T003) can read the `src` bytes a manifest declares.
#[derive(Debug, Clone, PartialEq)]
pub struct LocatedItem {
    pub dir: PathBuf,
    pub manifest: ItemManifest,
}

/// A listed registry: every healthy item plus every broken entry. A
/// listing with broken entries still carries all healthy items.
#[derive(Debug, Clone, PartialEq)]
pub struct RegistryListing {
    pub items: Vec<ItemManifest>,
    pub broken: Vec<BrokenItem>,
}

impl RegistryListing {
    /// True when no manifest failed its schema.
    pub fn is_clean(&self) -> bool {
        self.broken.is_empty()
    }

    /// Healthy item names in listing order.
    pub fn names(&self) -> Vec<&str> {
        self.items.iter().map(|m| m.name.as_str()).collect()
    }
}

fn broken_for(dir_name: &str, errors: Vec<ManifestError>) -> Vec<BrokenItem> {
    errors
        .into_iter()
        .map(|e| BrokenItem {
            item: dir_name.to_string(),
            field: e.field,
            message: e.message,
        })
        .collect()
}

/// Origin name of the bundled base in every merged listing, search
/// hit, and install echo: custom registries use their configured
/// names, so origins stay truthful everywhere they print.
pub const BUNDLED_ORIGIN: &str = "bundled";

/// One named registry source for a merged listing: the bundled base
/// plus every custom registry the project configuration names (Sprint
/// 19, T007 / CAP-007). `before_bundled` is the configured collision
/// order: sources carrying it win name collisions over the bundled
/// base; the base wins over every other custom source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistrySource {
    /// Origin shown beside merged items (for example `"bundled"` or
    /// the custom registry's configured name).
    pub name: String,
    /// Registry directory holding item folders.
    pub dir: PathBuf,
    /// True when this source wins collisions over the bundled base.
    pub before_bundled: bool,
}

/// One merged item: the winning manifest plus the directory holding
/// its files and the origin name search and install display.
#[derive(Debug, Clone, PartialEq)]
pub struct MergedItem {
    pub manifest: ItemManifest,
    pub dir: PathBuf,
    pub origin: String,
}

/// One resolved name collision: the winning origin plus every losing
/// origin in precedence order. Recorded (never silent) so the
/// configured order stays auditable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollisionNote {
    pub name: String,
    pub winner: String,
    pub losers: Vec<String>,
}

/// One name claimed by two item directories inside a single source:
/// the merged listing skips every copy from that source (other
/// sources still compete normally) and resolving the name refuses
/// naming the item and its directories, mirroring [`super::resolve`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateClaim {
    pub source: String,
    pub name: String,
    pub dirs: Vec<String>,
}

/// A merged registry listing (Sprint 19, T007 / CAP-007): the winning
/// item per name across the base plus every reachable custom source,
/// every schema failure beside them, human warnings for unreachable
/// custom registries and skipped invalid custom manifests, and the
/// collision notes naming each winner. A listing with warnings still
/// carries every healthy item it could reach.
#[derive(Debug, Clone, PartialEq)]
pub struct MergedListing {
    pub items: Vec<MergedItem>,
    pub broken: Vec<BrokenItem>,
    pub warnings: Vec<String>,
    pub collisions: Vec<CollisionNote>,
    pub duplicates: Vec<DuplicateClaim>,
}

impl MergedListing {
    /// True when no manifest failed its schema in any reached source.
    pub fn is_clean(&self) -> bool {
        self.broken.is_empty()
    }

    /// Winning item names in listing order.
    pub fn names(&self) -> Vec<&str> {
        self.items
            .iter()
            .map(|m| m.manifest.name.as_str())
            .collect()
    }

    /// Find the winning item by exact name.
    pub fn find(&self, name: &str) -> Option<&MergedItem> {
        self.items.iter().find(|m| m.manifest.name == name)
    }
}

/// Merge one base source with custom sources into a single listing.
/// Every source validates like bundled content before listing: a
/// schema failure in a custom source is skipped with a warning naming
/// the manifest file, never blocking healthy items. An unreachable
/// custom source warns by name while the rest keeps working. Name
/// collisions follow the configured order: `before_bundled` custom
/// sources (in source order) beat the base, which beats every other
/// custom source (in source order). The base itself never warns: a
/// missing base still lists as empty, as [`load_registry`] always has.
pub fn load_merged(base: &RegistrySource, extras: &[RegistrySource]) -> MergedListing {
    // Precedence rank: lower wins. Before-bundled customs (in order),
    // then the base, then the remaining customs (in order).
    let before: Vec<&RegistrySource> = extras.iter().filter(|s| s.before_bundled).collect();
    let after: Vec<&RegistrySource> = extras.iter().filter(|s| !s.before_bundled).collect();
    let mut ordered: Vec<&RegistrySource> = Vec::with_capacity(extras.len() + 1);
    ordered.extend(before.iter().copied());
    let before_len = before.len();
    ordered.push(base);
    ordered.extend(after);

    let mut warnings: Vec<String> = Vec::new();
    let mut broken: Vec<BrokenItem> = Vec::new();
    // Winners by name in first-claim (precedence) order.
    let mut winners: Vec<MergedItem> = Vec::new();
    let mut winner_index: std::collections::BTreeMap<String, usize> =
        std::collections::BTreeMap::new();
    let mut collisions: std::collections::BTreeMap<String, CollisionNote> =
        std::collections::BTreeMap::new();
    let mut all_duplicates: Vec<DuplicateClaim> = Vec::new();

    for (rank, source) in ordered.iter().enumerate() {
        let is_base = rank == before_len;
        let (located, detailed) = locate_items_detailed(&source.dir);
        if detailed.is_empty() && located.is_empty() && std::fs::read_dir(&source.dir).is_err() {
            // Missing or unreadable: the base stays silent (empty
            // listing, as always); a custom source warns by name and
            // the merged index keeps working without it.
            if !is_base {
                let reason = std::fs::read_dir(&source.dir)
                    .err()
                    .map(|e| e.to_string())
                    .unwrap_or_else(|| "unreadable".to_string());
                warnings.push(format!(
                    "custom registry {:?} at {} is unreachable: {reason}; continuing with the bundled registry",
                    source.name,
                    source.dir.display()
                ));
            }
            continue;
        }
        if !is_base {
            for (file, entry) in &detailed {
                warnings.push(format!(
                    "custom registry {:?}: skipping invalid manifest at {} (item {:?}, field {:?}): {}",
                    source.name,
                    file.display(),
                    entry.item,
                    entry.field,
                    entry.message
                ));
            }
        }
        broken.extend(detailed.into_iter().map(|(_, entry)| entry));
        let mut local = located;
        local.sort_by(|a, b| a.manifest.name.cmp(&b.manifest.name));
        // One source claiming a name twice is a defect, not a
        // collision: skip every copy from that source (other sources
        // still compete) and record the claim for resolution to
        // refuse. Across sources the first precedence claim wins and
        // later ones record collision losers.
        let mut local_claims: std::collections::BTreeMap<String, Vec<LocatedItem>> =
            std::collections::BTreeMap::new();
        for item in local {
            local_claims
                .entry(item.manifest.name.clone())
                .or_default()
                .push(item);
        }
        let mut duplicates: Vec<DuplicateClaim> = Vec::new();
        for (name, claims) in local_claims {
            if claims.len() > 1 {
                let mut dirs: Vec<String> = claims
                    .iter()
                    .map(|item| item.dir.display().to_string())
                    .collect();
                dirs.sort();
                if !is_base {
                    warnings.push(format!(
                        "custom registry {:?} declares item {:?} in more than one place ({}); skipping those copies",
                        source.name,
                        name,
                        dirs.join(", ")
                    ));
                }
                duplicates.push(DuplicateClaim {
                    source: source.name.clone(),
                    name,
                    dirs,
                });
                continue;
            }
            let item = claims.into_iter().next().expect("one claim");
            let name = item.manifest.name.clone();
            if let Some(at) = winner_index.get(&name) {
                let winner_origin = winners[*at].origin.clone();
                collisions
                    .entry(name.clone())
                    .or_insert_with(|| CollisionNote {
                        name: name.clone(),
                        winner: winner_origin,
                        losers: Vec::new(),
                    })
                    .losers
                    .push(source.name.clone());
            } else {
                winner_index.insert(name, winners.len());
                winners.push(MergedItem {
                    manifest: item.manifest,
                    dir: item.dir,
                    origin: source.name.clone(),
                });
            }
        }
        all_duplicates.extend(duplicates);
    }
    // Deterministic listing order: sort winners by name (as
    // `load_registry` does); collisions and duplicates by name.
    winners.sort_by(|a, b| a.manifest.name.cmp(&b.manifest.name));
    broken.sort_by(|a, b| (&a.item, &a.field).cmp(&(&b.item, &b.field)));
    let collisions: Vec<CollisionNote> = collisions.into_values().collect();
    all_duplicates.sort_by(|a, b| (&a.source, &a.name).cmp(&(&b.source, &b.name)));
    MergedListing {
        items: winners,
        broken,
        warnings,
        collisions,
        duplicates: all_duplicates,
    }
}

/// Walk one registry directory offline into located healthy items
/// plus broken entries: every directory holding a [`MANIFEST_FILE`]
/// is one item. Never touches the network and never writes. Output is
/// walk order; callers sort for deterministic listings.
pub fn locate_items(dir: &Path) -> (Vec<LocatedItem>, Vec<BrokenItem>) {
    let (located, detailed) = locate_items_detailed(dir);
    (
        located,
        detailed.into_iter().map(|(_, entry)| entry).collect(),
    )
}

/// [`locate_items`] plus the manifest file behind every broken entry,
/// so merged listings (T007) can name the skipped file.
pub fn locate_items_detailed(dir: &Path) -> (Vec<LocatedItem>, Vec<(PathBuf, BrokenItem)>) {
    let mut items: Vec<LocatedItem> = Vec::new();
    let mut broken: Vec<(PathBuf, BrokenItem)> = Vec::new();
    // Stack walk: an item directory (one holding `manifest.json`)
    // is a leaf; anything else descends. Hidden directories stay
    // out so editor and VCS state never reads as items.
    let mut stack: Vec<PathBuf> = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let manifest_path = current.join(MANIFEST_FILE);
        if manifest_path.is_file() {
            let fallback = current
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| current.display().to_string());
            match std::fs::read_to_string(&manifest_path) {
                Err(e) => broken.push((
                    manifest_path.clone(),
                    BrokenItem {
                        item: fallback,
                        field: "manifest".to_string(),
                        message: format!("cannot read {}: {e}", manifest_path.display()),
                    },
                )),
                Ok(text) => {
                    let name = peek_name(&text).unwrap_or_else(|| fallback.clone());
                    match parse_manifest_str(&text) {
                        Ok(manifest) => items.push(LocatedItem {
                            dir: current.clone(),
                            manifest,
                        }),
                        Err(errors) => broken.extend(
                            broken_for(&name, errors)
                                .into_iter()
                                .map(|entry| (manifest_path.clone(), entry)),
                        ),
                    }
                }
            }
            continue;
        }
        let read = match std::fs::read_dir(&current) {
            Ok(r) => r,
            Err(_) => continue,
        };
        for entry in read.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let hidden = path
                .file_name()
                .map(|s| s.to_string_lossy().starts_with('.'))
                .unwrap_or(false);
            if !hidden {
                stack.push(path);
            }
        }
    }
    (items, broken)
}

/// List one registry directory offline into healthy items plus broken
/// entries. Never touches the network and never writes.
pub fn load_registry(dir: &Path) -> RegistryListing {
    let (mut located, mut broken) = locate_items(dir);
    located.sort_by(|a, b| a.manifest.name.cmp(&b.manifest.name));
    broken.sort_by(|a, b| (&a.item, &a.field).cmp(&(&b.item, &b.field)));
    RegistryListing {
        items: located.into_iter().map(|item| item.manifest).collect(),
        broken,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_item(root: &Path, dir: &str, manifest: &str) {
        let folder = root.join(dir);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join(MANIFEST_FILE), manifest).unwrap();
    }

    const HEALTHY: &str = r#"{
        "name": "lantern-night",
        "type": "example",
        "title": "Lantern Night",
        "description": "An original starter under paper lanterns.",
        "files": [{"src": "composition.kdl", "target": "composition.kdl"}],
        "width": 1280, "height": 720, "duration": 4
    }"#;

    #[test]
    fn broken_manifest_leaves_healthy_items_listed() {
        let root = std::env::temp_dir().join(format!(
            "framesmith-catalog-unit-{}-{}",
            std::process::id(),
            "broken-beside-healthy"
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        write_item(&root, "good", &HEALTHY.replace("lantern-night", "good"));
        write_item(
            &root,
            "bad",
            r#"{"name": "bad", "type": "component", "title": "Bad",
                "description": "Missing files.", "files": []}"#,
        );
        let listing = load_registry(&root);
        assert_eq!(listing.names(), vec!["good"]);
        assert_eq!(listing.broken.len(), 1);
        assert_eq!(listing.broken[0].item, "bad");
        assert_eq!(listing.broken[0].field, "files");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn missing_registry_lists_empty() {
        let listing = load_registry(Path::new("/no/such/framesmith-registry"));
        assert!(listing.items.is_empty());
        assert!(listing.broken.is_empty());
    }
}
