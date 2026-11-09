//! Install with dependency resolution (Sprint 19, T003 / CAP-003).
//!
//! Installing by exact name or tag — several names in one call —
//! resolves `registryDependencies` in topological order fully before
//! the first write, so dependents always land after their
//! dependencies. The two phases stay separate on purpose:
//!
//! 1. [`resolve`] reads the registry and returns the ordered plan. It
//!    never writes. The CLI echoes this order (INT-001) before
//!    anything lands in the project.
//! 2. [`write_plan`] validates every target, remaps through the
//!    project configuration, stages every byte, and only then writes.
//!
//! A cycle or a missing dependency refuses the whole install naming
//! the chain, with nothing written (REC-001). Target validation is
//! fail-closed too: absolute targets, `..` escapes, and targets that
//! would clobber the project configuration refuse naming the item
//! and target, again with nothing written.
//!
//! Adopted audio carries its voice-or-sfx label: a manifest file entry
//! may declare `"audio": "voice"` or `"sfx"`, and the plan and report
//! echo that label beside the target. The label is author metadata
//! read back truthfully; files without one still install.
//!
//! All bundled content is our own clean-room work; no HyperFrames item
//! is ever copied. KDL shipped by the registry keeps hyphenated style
//! attributes (`border-width`, never `border_width`).
//!
//! Version gates and the SHA256 lock (Sprint 19, T004 / CAP-004) guard
//! the writes above: [`check_version_gates`] refuses a resolved set
//! whose `minCliVersion` outruns the running CLI before any file is
//! written, [`InstallOptions::vars`] bakes `--vars` values into
//! `variable` defaults (or reports a `--variables` mount snippet for
//! values with no bake site), and the lock at
//! [`CATALOG_LOCK_FILE`] records the hash of every installed file so
//! a plain reinstall keeps local edits while `--force` restores
//! registry bytes.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

use crate::sha256::sha256_hex;

use crate::manifest::ItemManifest;
use crate::registry::locate_items;
use crate::{AudioRole, BrokenItem};

/// The project configuration file read for target remapping. Unknown
/// fields are ignored so later slices (custom registries in T007) can
/// extend the same file without breaking installs.
pub const PROJECT_CONFIG_FILE: &str = "framesmith.json";

/// File extensions treated as adopted audio (matched case-insensitively
/// on the target suffix). A file entry carrying one of these suffixes
/// may declare its voice-or-sfx role; the installer echoes it.
pub const AUDIO_EXTENSIONS: &[&str] = &[
    "wav", "mp3", "ogg", "oga", "flac", "m4a", "aac", "opus", "wma", "aiff", "aif",
];

/// True when `target` names an audio asset by suffix.
pub fn is_audio_target(target: &str) -> bool {
    let suffix = target.rsplit('/').next().unwrap_or(target);
    match suffix.rsplit_once('.') {
        Some((_, ext)) if !ext.is_empty() => AUDIO_EXTENSIONS
            .iter()
            .any(|known| known.eq_ignore_ascii_case(ext)),
        _ => false,
    }
}

/// What to install: exact item names plus tags, resolved as one set.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Selection {
    /// Exact item names (case-sensitive).
    pub names: Vec<String>,
    /// Tags: every item carrying a tag joins the set with its deps.
    pub tags: Vec<String>,
}

/// One ordered plan entry: the manifest plus the item directory holding
/// its `src` files.
#[derive(Debug, Clone)]
pub struct PlannedItem {
    pub manifest: ItemManifest,
    pub dir: PathBuf,
}

/// The resolved install order: dependencies before dependents.
#[derive(Debug, Clone)]
pub struct ResolvedPlan {
    pub items: Vec<PlannedItem>,
}

impl ResolvedPlan {
    /// Ordered item names in write order.
    pub fn order(&self) -> Vec<&str> {
        self.items
            .iter()
            .map(|item| item.manifest.name.as_str())
            .collect()
    }
}

/// One adopted audio file in a plan: the owning item, its (remapped)
/// target, and its voice-or-sfx label. Computed from a plan alone so
/// the CLI can echo labels before writing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdoptedAudio {
    pub item: String,
    pub target: String,
    pub role: AudioRole,
}

/// Adopted audio across a plan in write order.
pub fn adopted_audio(plan: &ResolvedPlan, remaps: &[(String, String)]) -> Vec<AdoptedAudio> {
    let mut out = Vec::new();
    for item in &plan.items {
        for file in &item.manifest.files {
            if let Some(role) = file.audio {
                if is_audio_target(&file.target) {
                    out.push(AdoptedAudio {
                        item: item.manifest.name.clone(),
                        target: remap_target(&file.target, remaps),
                        role,
                    });
                }
            }
        }
    }
    out
}

/// One written file in an install report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledFile {
    pub item: String,
    pub target: String,
    pub audio: Option<AudioRole>,
    pub bytes: usize,
}

/// The completed install: ordered item names plus every written file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallReport {
    pub items: Vec<String>,
    pub files: Vec<InstalledFile>,
    /// Targets whose local edits survived a plain reinstall: the
    /// on-disk bytes differed from the lock hash, so they were kept
    /// and listed instead of overwritten (REC-002).
    pub kept: Vec<String>,
    /// Targets overwritten by a forced reinstall even though the
    /// on-disk bytes differed from the lock hash: registry bytes were
    /// restored exactly.
    pub forced: Vec<String>,
    /// `--vars` ids baked into at least one installed file's
    /// `variable` defaults, in sorted order.
    pub vars_baked: Vec<String>,
    /// Mount snippet for `--vars` values with no bake site in any
    /// installed text file: the `--variables` payload that mounts
    /// them at check/render time instead. `None` when every value
    /// baked or no values were passed.
    pub mount_snippet: Option<String>,
}

/// A refused install. Every variant except mid-write I/O failures
/// guarantees nothing was written; see [`InstallError::wrote_nothing`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallError {
    message: String,
    wrote_nothing: bool,
}

impl InstallError {
    fn refusal(message: String) -> InstallError {
        InstallError {
            message,
            wrote_nothing: true,
        }
    }

    fn io(message: String) -> InstallError {
        InstallError {
            message,
            wrote_nothing: false,
        }
    }

    /// True when the failure happened before the first write, so the
    /// project is byte-identical to before the install ran.
    pub fn wrote_nothing(&self) -> bool {
        self.wrote_nothing
    }

    /// Human message naming the item, dependency, target, or file.
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl std::fmt::Display for InstallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for InstallError {}

/// Resolve a selection to its ordered plan without writing anything.
///
/// Exact names and tags merge into one set; every dependency joins
/// transitively. Output is deterministic: roots visit in sorted order
/// and each dependency list runs sorted, so the same registry and
/// selection always yield the same order.
pub fn resolve(registry_root: &Path, selection: &Selection) -> Result<ResolvedPlan, InstallError> {
    if selection.names.is_empty() && selection.tags.is_empty() {
        return Err(InstallError::refusal(
            "nothing to install: pass at least one item name or tag".to_string(),
        ));
    }
    let (located, broken) = locate_items(registry_root);
    let mut by_name: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (index, item) in located.iter().enumerate() {
        by_name
            .entry(item.manifest.name.as_str())
            .or_default()
            .push(index);
    }

    // Seed the wanted set: exact names plus every item carrying each
    // requested tag, as one set.
    let mut wanted: BTreeSet<String> = BTreeSet::new();
    let mut wanted_order: Vec<String> = Vec::new();
    let mut want = |name: String| {
        if wanted.insert(name.clone()) {
            wanted_order.push(name);
        }
    };
    for name in &selection.names {
        want(name.clone());
    }
    let mut tags = selection.tags.clone();
    tags.sort();
    for tag in &tags {
        let mut matched: Vec<String> = located
            .iter()
            .filter(|item| item.manifest.tags.iter().any(|t| t == tag))
            .map(|item| item.manifest.name.clone())
            .collect();
        matched.sort();
        matched.dedup();
        if matched.is_empty() {
            return Err(InstallError::refusal(format!(
                "unknown tag {tag:?}: no registry item carries it"
            )));
        }
        for name in matched {
            want(name);
        }
    }

    // Every wanted name must resolve to exactly one healthy item. A
    // broken manifest never installs (naming item and field); a name
    // claimed twice refuses rather than guessing.
    let mut healthy: BTreeMap<String, usize> = BTreeMap::new();
    for name in &wanted_order {
        match by_name.get(name.as_str()) {
            None => {
                if let Some(entry) = broken.iter().find(|b| b.item == *name) {
                    return Err(InstallError::refusal(format!(
                        "cannot install {:?}: manifest is broken (field {:?}): {}",
                        name, entry.field, entry.message
                    )));
                }
                let mut available: Vec<&str> = by_name.keys().copied().collect();
                available.sort();
                return Err(InstallError::refusal(format!(
                    "unknown item {name:?} (available: {})",
                    available.join(", ")
                )));
            }
            Some(indices) if indices.len() > 1 => {
                let dirs: Vec<String> = indices
                    .iter()
                    .map(|i| located[*i].dir.display().to_string())
                    .collect();
                return Err(InstallError::refusal(format!(
                    "item {name:?} is declared in more than one place ({}); refusing rather than guessing",
                    dirs.join(", ")
                )));
            }
            Some(indices) => {
                healthy.insert(name.clone(), indices[0]);
            }
        }
    }

    // Depth-first topological order with sorted edges; the stack names
    // the current chain for cycle and missing-dependency refusals.
    let mut done: BTreeSet<String> = BTreeSet::new();
    let mut stack: Vec<String> = Vec::new();
    let mut order: Vec<String> = Vec::new();
    for root in wanted_order {
        visit(
            &root, &located, &by_name, &broken, &mut done, &mut stack, &mut order,
        )?;
    }
    let mut items: Vec<PlannedItem> = Vec::with_capacity(order.len());
    for name in order {
        // Every ordered name passed through `visit`, which refuses
        // duplicates and absences; exactly one location remains.
        let index = match by_name.get(name.as_str()) {
            Some(indices) if indices.len() == 1 => indices[0],
            _ => {
                return Err(InstallError::refusal(format!(
                    "item {name:?} cannot be located exactly once; refusing with nothing written"
                )));
            }
        };
        items.push(PlannedItem {
            manifest: located[index].manifest.clone(),
            dir: located[index].dir.clone(),
        });
        // Keep the seed map truthful for diagnostics: every resolved
        // name, root or transitive, records its location.
        healthy.insert(name, index);
    }
    Ok(ResolvedPlan { items })
}

fn visit(
    name: &str,
    located: &[crate::registry::LocatedItem],
    by_name: &BTreeMap<&str, Vec<usize>>,
    broken: &[BrokenItem],
    done: &mut BTreeSet<String>,
    stack: &mut Vec<String>,
    order: &mut Vec<String>,
) -> Result<(), InstallError> {
    if done.contains(name) {
        return Ok(());
    }
    if let Some(at) = stack.iter().position(|n| n == name) {
        let mut chain: Vec<&str> = stack[at..].iter().map(|s| s.as_str()).collect();
        chain.push(name);
        return Err(InstallError::refusal(format!(
            "dependency cycle: {}",
            chain.join(" -> ")
        )));
    }
    stack.push(name.to_string());
    let index = match by_name.get(name) {
        Some(indices) if indices.len() == 1 => indices[0],
        Some(_) => {
            stack.pop();
            return Err(InstallError::refusal(format!(
                "item {name:?} is declared in more than one place; refusing rather than guessing"
            )));
        }
        None => {
            stack.pop();
            // A dependency claimed by two manifests would have been
            // refused above; here the name is simply absent — unless a
            // broken manifest claims it, which names its field.
            if let Some(entry) = broken.iter().find(|b| b.item == name) {
                let needed_by = stack.join(" -> ");
                return Err(InstallError::refusal(format!(
                    "cannot install {}: dependency {:?} is broken (field {:?}): {}",
                    needed_by, name, entry.field, entry.message
                )));
            }
            let mut chain = stack.clone();
            chain.push(name.to_string());
            return Err(InstallError::refusal(format!(
                "missing dependency: {}",
                chain.join(" -> ")
            )));
        }
    };
    let mut deps = located[index].manifest.registry_dependencies.clone();
    deps.sort();
    deps.dedup();
    for dep in deps {
        visit(&dep, located, by_name, broken, done, stack, order)?;
    }
    stack.pop();
    done.insert(name.to_string());
    order.push(name.to_string());
    Ok(())
}

/// Read the project configuration's target remaps (`framesmith.json`
/// `paths`: target-prefix to replacement-prefix). A missing file means
/// no remapping; an unreadable or misshapen file refuses naming it.
/// Public so the CLI can echo the remapped order before writing.
pub fn project_remaps(project_dir: &Path) -> Result<Vec<(String, String)>, InstallError> {
    let path = project_dir.join(PROJECT_CONFIG_FILE);
    let text = match std::fs::read_to_string(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => {
            return Err(InstallError::refusal(format!(
                "cannot read {}: {e}",
                path.display()
            )))
        }
        Ok(text) => text,
    };
    let value: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| InstallError::refusal(format!("{} is not valid JSON: {e}", path.display())))?;
    let obj = value.as_object().ok_or_else(|| {
        InstallError::refusal(format!("{} must be a JSON object", path.display()))
    })?;
    let paths = match obj.get("paths") {
        None => return Ok(Vec::new()),
        Some(v) => v.as_object().ok_or_else(|| {
            InstallError::refusal(format!(
                "{} field \"paths\" must be an object of target-prefix to replacement-prefix",
                path.display()
            ))
        })?,
    };
    let mut remaps = Vec::with_capacity(paths.len());
    for (from, to) in paths {
        let to = to.as_str().ok_or_else(|| {
            InstallError::refusal(format!(
                "{} field \"paths\": replacement for {from:?} must be a string",
                path.display()
            ))
        })?;
        for candidate in [from.as_str(), to] {
            if candidate.is_empty() || is_abs_or_escaping(candidate) {
                return Err(InstallError::refusal(format!(
                    "{} field \"paths\": prefix {candidate:?} must be a relative path without `..`",
                    path.display()
                )));
            }
        }
        remaps.push((from.clone(), to.to_string()));
    }
    // Longest prefix first so nested remaps resolve specifically.
    remaps.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then(a.0.cmp(&b.0)));
    Ok(remaps)
}

/// Apply the longest matching prefix remap to a target. Public so the
/// CLI echoes the remapped order before writing. Matching is longest
/// first regardless of input order, on whole segments only.
pub fn remap_target(target: &str, remaps: &[(String, String)]) -> String {
    let mut best: Option<(&str, &str)> = None;
    for (from, to) in remaps {
        let matches = target == from
            || target
                .strip_prefix(from.as_str())
                .is_some_and(|rest| rest.starts_with('/'));
        if matches && best.is_none_or(|(prior, _)| from.len() > prior.len()) {
            best = Some((from, to));
        }
    }
    match best {
        None => target.to_string(),
        Some((from, to)) => {
            if target == from {
                to.to_string()
            } else {
                format!("{to}{}", &target[from.len()..])
            }
        }
    }
}

/// One custom registry named by the project configuration
/// (`framesmith.json` field `"registries"`, Sprint 19, T007 /
/// CAP-007). Each entry is either a path string (the registry name
/// defaults to the path) or an object with a required `"path"`, an
/// optional `"name"`, and an optional `"beforeBundled"` flag that
/// moves the custom registry ahead of the bundled base in the
/// configured collision order. Paths resolve relative to the project
/// directory holding the configuration file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomRegistry {
    pub name: String,
    pub path: String,
    pub before_bundled: bool,
}

/// Read the custom registries a project configuration names. A
/// missing configuration file (or one without `"registries"`) means
/// no custom registries; an unreadable file or a misshapen
/// `"registries"` value refuses naming the configuration file, like
/// [`project_remaps`] does. Unknown fields (including whole later
/// slices) stay ignored.
pub fn project_registries(project_dir: &Path) -> Result<Vec<CustomRegistry>, InstallError> {
    let path = project_dir.join(PROJECT_CONFIG_FILE);
    let text = match std::fs::read_to_string(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => {
            return Err(InstallError::refusal(format!(
                "cannot read {}: {e}",
                path.display()
            )))
        }
        Ok(text) => text,
    };
    let value: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| InstallError::refusal(format!("{} is not valid JSON: {e}", path.display())))?;
    let obj = value.as_object().ok_or_else(|| {
        InstallError::refusal(format!("{} must be a JSON object", path.display()))
    })?;
    let entries = match obj.get("registries") {
        None => return Ok(Vec::new()),
        Some(v) => v.as_array().ok_or_else(|| {
            InstallError::refusal(format!(
                "{} field \"registries\" must be an array of paths or objects",
                path.display()
            ))
        })?,
    };
    let mut out = Vec::with_capacity(entries.len());
    for (index, entry) in entries.iter().enumerate() {
        let where_it = format!("{} field \"registries\" entry {index}", path.display());
        let (name, registry_path, before_bundled) = match entry {
            serde_json::Value::String(s) => (s.clone(), s.clone(), false),
            serde_json::Value::Object(map) => {
                let registry_path = map.get("path").and_then(|v| v.as_str()).ok_or_else(|| {
                    InstallError::refusal(format!("{where_it} must carry a \"path\" string"))
                })?;
                let name = map
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or(registry_path)
                    .to_string();
                let before_bundled = match map.get("beforeBundled") {
                    None => false,
                    Some(serde_json::Value::Bool(b)) => *b,
                    Some(_) => {
                        return Err(InstallError::refusal(format!(
                            "{where_it} field \"beforeBundled\" must be true or false"
                        )));
                    }
                };
                (name, registry_path.to_string(), before_bundled)
            }
            _ => {
                return Err(InstallError::refusal(format!(
                    "{where_it} must be a path string or an object with a \"path\" string"
                )));
            }
        };
        if registry_path.trim().is_empty() {
            return Err(InstallError::refusal(format!(
                "{where_it} carries an empty registry path; refusing with nothing written"
            )));
        }
        if name.trim().is_empty() {
            return Err(InstallError::refusal(format!(
                "{where_it} carries an empty registry name; refusing with nothing written"
            )));
        }
        if name == crate::BUNDLED_ORIGIN {
            return Err(InstallError::refusal(format!(
                "{where_it} names a custom registry \"bundled\", which is reserved for the bundled base; refusing with nothing written"
            )));
        }
        if out.iter().any(|seen: &CustomRegistry| seen.name == name) {
            return Err(InstallError::refusal(format!(
                "{where_it} repeats custom registry name {name:?}; refusing with nothing written"
            )));
        }
        out.push(CustomRegistry {
            name,
            path: registry_path,
            before_bundled,
        });
    }
    Ok(out)
}

/// Resolve custom registry entries against the project directory into
/// named sources for [`crate::load_merged`]: relative paths resolve
/// under the project directory, absolute paths stay as given.
pub fn registry_sources(
    project_dir: &Path,
    customs: &[CustomRegistry],
) -> Vec<crate::RegistrySource> {
    customs
        .iter()
        .map(|custom| {
            let dir = Path::new(&custom.path);
            let dir = if dir.is_absolute() {
                dir.to_path_buf()
            } else {
                project_dir.join(dir)
            };
            crate::RegistrySource {
                name: custom.name.clone(),
                dir,
                before_bundled: custom.before_bundled,
            }
        })
        .collect()
}

/// A resolved install over the merged registries: the ordered plan
/// plus the winning origin per ordered item (parallel to
/// [`ResolvedPlan::order`]), the merge warnings search and install
/// surface (unreachable custom registries, skipped invalid custom
/// manifests), and the collision notes naming each winner.
#[derive(Debug, Clone)]
pub struct MergedPlan {
    pub plan: ResolvedPlan,
    pub origins: Vec<String>,
    pub warnings: Vec<String>,
    pub collisions: Vec<crate::CollisionNote>,
}

/// Resolve a selection against the merged registries: the bundled
/// base at `base_dir` (origin `"bundled"`) plus the custom sources
/// from the project configuration. Winners resolve as one set with
/// dependencies ordered before dependents, exactly like [`resolve`];
/// collisions follow the configured order (see
/// [`crate::load_merged`]). Resolution still runs fully before any
/// write: this function never writes.
pub fn resolve_merged(
    base_dir: &Path,
    extras: &[crate::RegistrySource],
    selection: &Selection,
) -> Result<MergedPlan, InstallError> {
    if selection.names.is_empty() && selection.tags.is_empty() {
        return Err(InstallError::refusal(
            "nothing to install: pass at least one item name or tag".to_string(),
        ));
    }
    let base = crate::RegistrySource {
        name: crate::BUNDLED_ORIGIN.to_string(),
        dir: base_dir.to_path_buf(),
        before_bundled: false,
    };
    let merged = crate::load_merged(&base, extras);
    let located: Vec<crate::LocatedItem> = merged
        .items
        .iter()
        .map(|item| crate::LocatedItem {
            dir: item.dir.clone(),
            manifest: item.manifest.clone(),
        })
        .collect();
    let mut by_name: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (index, item) in located.iter().enumerate() {
        by_name
            .entry(item.manifest.name.as_str())
            .or_default()
            .push(index);
    }
    // Winners are unique by construction, so every name locates
    // exactly once; keep the refusal shape for diagnostics anyway.
    let mut wanted: BTreeSet<String> = BTreeSet::new();
    let mut wanted_order: Vec<String> = Vec::new();
    let mut want = |name: String| {
        if wanted.insert(name.clone()) {
            wanted_order.push(name);
        }
    };
    for name in &selection.names {
        want(name.clone());
    }
    let mut tags = selection.tags.clone();
    tags.sort();
    for tag in &tags {
        let mut matched: Vec<String> = located
            .iter()
            .filter(|item| item.manifest.tags.iter().any(|t| t == tag))
            .map(|item| item.manifest.name.clone())
            .collect();
        matched.sort();
        matched.dedup();
        if matched.is_empty() {
            return Err(InstallError::refusal(format!(
                "unknown tag {tag:?}: no registry item carries it"
            )));
        }
        for name in matched {
            want(name);
        }
    }
    let mut healthy: BTreeMap<String, usize> = BTreeMap::new();
    for name in &wanted_order {
        if let Some(dupe) = merged.duplicates.iter().find(|d| &d.name == name) {
            return Err(InstallError::refusal(format!(
                "item {:?} is declared in more than one place in {:?} ({}); refusing rather than guessing",
                dupe.name,
                dupe.source,
                dupe.dirs.join(", ")
            )));
        }
        match by_name.get(name.as_str()) {
            None => {
                if let Some(entry) = merged.broken.iter().find(|b| b.item == *name) {
                    return Err(InstallError::refusal(format!(
                        "cannot install {:?}: manifest is broken (field {:?}): {}",
                        name, entry.field, entry.message
                    )));
                }
                let mut available: Vec<&str> = by_name.keys().copied().collect();
                available.sort();
                return Err(InstallError::refusal(format!(
                    "unknown item {name:?} (available: {})",
                    available.join(", ")
                )));
            }
            Some(indices) if indices.len() > 1 => {
                let dirs: Vec<String> = indices
                    .iter()
                    .map(|i| located[*i].dir.display().to_string())
                    .collect();
                return Err(InstallError::refusal(format!(
                    "item {name:?} is declared in more than one place ({}); refusing rather than guessing",
                    dirs.join(", ")
                )));
            }
            Some(indices) => {
                healthy.insert(name.clone(), indices[0]);
            }
        }
    }
    let mut done: BTreeSet<String> = BTreeSet::new();
    let mut stack: Vec<String> = Vec::new();
    let mut order: Vec<String> = Vec::new();
    for root in &wanted_order {
        visit(
            root,
            &located,
            &by_name,
            &merged.broken,
            &mut done,
            &mut stack,
            &mut order,
        )?;
    }
    // A name claimed twice inside one source refuses rather than
    // guessing, mirroring `resolve`; across sources the configured
    // winner already stands. Only a requested name refuses: an
    // unrelated defective claim never blocks a healthy install.
    for name in wanted_order.iter().chain(order.iter()) {
        if let Some(dupe) = merged.duplicates.iter().find(|d| &d.name == name) {
            return Err(InstallError::refusal(format!(
                "item {:?} is declared in more than one place in {:?} ({}); refusing rather than guessing",
                dupe.name,
                dupe.source,
                dupe.dirs.join(", ")
            )));
        }
    }
    let mut items: Vec<PlannedItem> = Vec::with_capacity(order.len());
    let mut origins: Vec<String> = Vec::with_capacity(order.len());
    for name in &order {
        let index = match by_name.get(name.as_str()) {
            Some(indices) if indices.len() == 1 => indices[0],
            _ => {
                return Err(InstallError::refusal(format!(
                    "item {name:?} cannot be located exactly once; refusing with nothing written"
                )));
            }
        };
        let winner = merged
            .find(name)
            .expect("ordered names always win the merge");
        items.push(PlannedItem {
            manifest: located[index].manifest.clone(),
            dir: located[index].dir.clone(),
        });
        origins.push(winner.origin.clone());
        healthy.insert(name.clone(), index);
    }
    Ok(MergedPlan {
        plan: ResolvedPlan { items },
        origins,
        warnings: merged.warnings,
        collisions: merged.collisions,
    })
}

/// Resolve a merged selection against the registries and write it
/// into a project: resolution first, gates second, validation third,
/// writes last — the same order [`install_with_options`] keeps, with
/// version gates, `--vars` baking, and the SHA256 lock applied to the
/// merged plan. Every refusal below happens before the first write.
/// Returns the install report with the merged plan (origins,
/// warnings, collisions) beside it.
pub fn install_merged(
    base_dir: &Path,
    project_dir: &Path,
    extras: &[crate::RegistrySource],
    selection: &Selection,
) -> Result<(InstallReport, MergedPlan), InstallError> {
    install_merged_with_options(
        base_dir,
        project_dir,
        extras,
        selection,
        &InstallOptions::default(),
    )
}

/// [`install_merged`] with version gates, `--vars` baking, and the
/// SHA256 lock: the merged equivalent of [`install_with_options`].
/// Custom items install exactly like bundled ones, gates and lock
/// included.
pub fn install_merged_with_options(
    base_dir: &Path,
    project_dir: &Path,
    extras: &[crate::RegistrySource],
    selection: &Selection,
    options: &InstallOptions,
) -> Result<(InstallReport, MergedPlan), InstallError> {
    let merged = resolve_merged(base_dir, extras, selection)?;
    check_version_gates(&merged.plan, &options.current_version)?;
    let report = write_plan_with_options(&merged.plan, project_dir, options)?;
    Ok((report, merged))
}

/// True for absolute paths and paths escaping their parent.
fn is_abs_or_escaping(target: &str) -> bool {
    let path = Path::new(target);
    if path.is_absolute() {
        return true;
    }
    path.components().any(|c| {
        matches!(
            c,
            Component::Prefix(_) | Component::RootDir | Component::ParentDir
        )
    })
}

/// Validate one install target after remapping: relative, contained,
/// non-empty, never the project configuration, and never inside the
/// tool-owned hidden directory that holds the catalog lock.
fn check_target(item: &str, target: &str) -> Result<(), InstallError> {
    if target.is_empty() || target == "." {
        return Err(InstallError::refusal(format!(
            "item {item:?} has an empty install target; refusing with nothing written"
        )));
    }
    if is_abs_or_escaping(target) {
        return Err(InstallError::refusal(format!(
            "item {item:?} targets {target:?}, which escapes the project; refusing with nothing written"
        )));
    }
    if target == PROJECT_CONFIG_FILE {
        return Err(InstallError::refusal(format!(
            "item {item:?} targets {target:?}, the project configuration; refusing with nothing written"
        )));
    }
    if target == CATALOG_LOCK_FILE
        || target
            .strip_prefix(HIDDEN_TOOL_DIR)
            .is_some_and(|rest| rest.starts_with('/'))
    {
        return Err(InstallError::refusal(format!(
            "item {item:?} targets {target:?}, inside the tool-owned {HIDDEN_TOOL_DIR}/ directory; refusing with nothing written"
        )));
    }
    Ok(())
}

/// Validate one registry source path and read its bytes. Sources stay
/// inside their item directory: absolute or escaping `src` values
/// refuse naming the item and file.
fn read_source(item: &ItemManifest, dir: &Path, src: &str) -> Result<Vec<u8>, InstallError> {
    if src.is_empty() || is_abs_or_escaping(src) {
        return Err(InstallError::refusal(format!(
            "item {:?} ships source {src:?}, which escapes its item directory; refusing with nothing written",
            item.name
        )));
    }
    let path = dir.join(src);
    match std::fs::read(&path) {
        Ok(bytes) => {
            if !path.is_file() {
                return Err(InstallError::refusal(format!(
                    "item {:?} ships source {src:?}, which is not a file; refusing with nothing written",
                    item.name
                )));
            }
            Ok(bytes)
        }
        Err(e) => Err(InstallError::refusal(format!(
            "item {:?} ships source {src:?}, which cannot be read ({e}); refusing with nothing written",
            item.name
        ))),
    }
}

/// Write an already-resolved plan: validate and remap every target,
/// stage every byte, then write. Anything failing before the first
/// write refuses with nothing written.
pub fn write_plan(plan: &ResolvedPlan, project_dir: &Path) -> Result<InstallReport, InstallError> {
    write_plan_with_options(plan, project_dir, &InstallOptions::default())
}

/// Resolve a selection against a registry and write it into a
/// project: resolution first, validation second, writes last.
pub fn install(
    registry_root: &Path,
    project_dir: &Path,
    selection: &Selection,
) -> Result<InstallReport, InstallError> {
    install_with_options(
        registry_root,
        project_dir,
        selection,
        &InstallOptions::default(),
    )
}

/// Resolve a selection against a registry and write it into a
/// project with version gates, `--vars` baking, and the SHA256 lock:
/// resolution first, gates second, validation third, writes last.
/// Every refusal below happens before the first write.
pub fn install_with_options(
    registry_root: &Path,
    project_dir: &Path,
    selection: &Selection,
    options: &InstallOptions,
) -> Result<InstallReport, InstallError> {
    let plan = resolve(registry_root, selection)?;
    check_version_gates(&plan, &options.current_version)?;
    write_plan_with_options(&plan, project_dir, options)
}

/// Write an already-resolved plan with version gates, `--vars`
/// baking, and the SHA256 lock. The gate runs again here so every
/// caller of this function — not only [`install_with_options`] —
/// keeps the "no gated write" invariant.
pub fn write_plan_with_options(
    plan: &ResolvedPlan,
    project_dir: &Path,
    options: &InstallOptions,
) -> Result<InstallReport, InstallError> {
    let remaps = project_remaps(project_dir)?;
    check_version_gates(plan, &options.current_version)?;
    check_vars_known(plan, &options.vars)?;
    let mut staged: Vec<StagedFile> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for entry in &plan.items {
        let manifest = &entry.manifest;
        for file in &manifest.files {
            let target = remap_target(&file.target, &remaps);
            check_target(&manifest.name, &target)?;
            if !seen.insert(target.clone()) {
                return Err(InstallError::refusal(format!(
                    "two items write {target:?}; refusing with nothing written"
                )));
            }
            let bytes = read_source(manifest, &entry.dir, &file.src)?;
            staged.push(StagedFile {
                item: manifest.name.clone(),
                target,
                audio: file.audio,
                bytes,
            });
        }
    }
    // Bake `--vars` values into `variable` defaults while everything
    // is still staged: a bake refusal still writes nothing.
    let declared = declared_variables(plan);
    let mut baked_ids: BTreeSet<String> = BTreeSet::new();
    for write in &mut staged {
        let (bytes, baked) = bake_vars(&write.bytes, &options.vars, &declared, &write.target)?;
        write.bytes = bytes;
        baked_ids.extend(baked);
    }
    let unbaked: Vec<VarAssign> = options
        .vars
        .iter()
        .filter(|v| !baked_ids.contains(&v.id))
        .cloned()
        .collect();
    // Compare against the lock and the disk before the first write:
    // local edits are kept (plain) or restored (force), and an
    // untracked file already sitting at a target refuses rather than
    // being silently overwritten or silently skipped.
    let old_lock = read_lock(project_dir)?;
    let mut keep: Vec<String> = Vec::new();
    let mut forced: Vec<String> = Vec::new();
    let mut write_flags: Vec<bool> = Vec::with_capacity(staged.len());
    for write in &staged {
        let dest = project_dir.join(&write.target);
        let on_disk: Option<Vec<u8>> = match std::fs::read(&dest) {
            Ok(bytes) => Some(bytes),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => {
                return Err(InstallError::io(format!(
                    "cannot read {}: {e}",
                    dest.display()
                )))
            }
        };
        let Some(disk) = on_disk else {
            write_flags.push(true);
            continue;
        };
        if disk == write.bytes {
            // Byte-identical reinstall: rewrite harmlessly.
            write_flags.push(true);
            continue;
        }
        let locked = old_lock
            .as_ref()
            .and_then(|lock| lock.hash_of(&write.target));
        match locked {
            Some(hash) if sha256_hex(&disk) == hash => write_flags.push(true),
            Some(_) if options.force => {
                forced.push(write.target.clone());
                write_flags.push(true);
            }
            Some(_) => {
                keep.push(write.target.clone());
                write_flags.push(false);
            }
            None if options.force => {
                forced.push(write.target.clone());
                write_flags.push(true);
            }
            None => {
                return Err(InstallError::refusal(format!(
                    "item {:?} targets {:?}, which already exists with untracked content; refusing with nothing written (rerun with --force to overwrite it)",
                    write.item, write.target
                )));
            }
        }
    }
    if let Err(e) = std::fs::create_dir_all(project_dir) {
        return Err(InstallError::io(format!(
            "cannot create project directory {}: {e}",
            project_dir.display()
        )));
    }
    let mut files = Vec::with_capacity(staged.len());
    for (write, flag) in staged.iter().zip(write_flags) {
        if !flag {
            continue;
        }
        let dest = project_dir.join(&write.target);
        if let Some(parent) = dest.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                return Err(InstallError::io(format!(
                    "cannot create directory {}: {e}",
                    parent.display()
                )));
            }
        }
        if let Err(e) = std::fs::write(&dest, &write.bytes) {
            return Err(InstallError::io(format!(
                "cannot write {}: {e}",
                dest.display()
            )));
        }
        files.push(InstalledFile {
            item: write.item.clone(),
            target: write.target.clone(),
            audio: write.audio,
            bytes: write.bytes.len(),
        });
    }
    // The lock records every installed file by the hash of the bytes
    // as written — baked `--vars` values included — merged over
    // entries from earlier installs that this run did not touch.
    let mut lock = old_lock.unwrap_or_else(LockFile::empty);
    lock.record(plan, &staged);
    write_lock(project_dir, &lock)?;
    let mut vars_baked: Vec<String> = baked_ids.into_iter().collect();
    vars_baked.sort();
    keep.sort();
    forced.sort();
    let mount_snippet = if unbaked.is_empty() {
        None
    } else {
        Some(mount_snippet_for(&unbaked))
    };
    Ok(InstallReport {
        items: plan.order().into_iter().map(|s| s.to_string()).collect(),
        files,
        kept: keep,
        forced,
        vars_baked,
        mount_snippet,
    })
}

// ---------------------------------------------------------------------------
// Version gates, `--vars` baking, and the SHA256 lock (T004 / CAP-004).
// ---------------------------------------------------------------------------

/// Tool-owned hidden directory inside a project: SDK deltas and the
/// catalog lock live here, and install targets may never point into
/// it (see [`check_target`]).
pub const HIDDEN_TOOL_DIR: &str = ".framesmith";

/// Project-relative path of the catalog lock file. The lock travels
/// with the project: it records every installed file's SHA256 so
/// reinstalls can tell local edits from registry bytes.
pub const CATALOG_LOCK_FILE: &str = ".framesmith/catalog-lock.json";

/// Version of the lock document written by [`write_lock`]. A lock
/// carrying another version refuses the install naming the file,
/// never guessed.
pub const CATALOG_LOCK_VERSION: u64 = 1;

/// The running CLI version that `minCliVersion` gates compare
/// against. The catalog and CLI crates release in lockstep, so the
/// catalog crate version is the CLI version; the CLI passes its own
/// identical version explicitly through [`InstallOptions`].
pub const CURRENT_CLI_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Options for [`install_with_options`] and
/// [`write_plan_with_options`]: the running CLI version for gates,
/// `--vars` assignments to bake, and whether a reinstall forces
/// registry bytes over local edits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallOptions {
    /// Running CLI version, compared against every resolved item's
    /// `minCliVersion` before any write.
    pub current_version: String,
    /// `--vars` assignments to bake into `variable` defaults.
    pub vars: Vec<VarAssign>,
    /// Force mode: reinstalls overwrite locally edited files with
    /// registry bytes instead of keeping them.
    pub force: bool,
}

impl Default for InstallOptions {
    fn default() -> InstallOptions {
        InstallOptions {
            current_version: CURRENT_CLI_VERSION.to_string(),
            vars: Vec::new(),
            force: false,
        }
    }
}

impl InstallOptions {
    /// Parse `--vars` assignment strings (`id=value`) into options,
    /// refusing duplicates and malformed assignments naming them.
    /// Kept separate from [`parse_var_assign`] so the CLI can build
    /// options in one fallible call.
    pub fn with_vars(
        current_version: &str,
        raw_vars: &[String],
        force: bool,
    ) -> Result<InstallOptions, InstallError> {
        let mut vars = Vec::with_capacity(raw_vars.len());
        let mut seen: BTreeSet<String> = BTreeSet::new();
        for raw in raw_vars {
            let assign = parse_var_assign(raw)?;
            if !seen.insert(assign.id.clone()) {
                return Err(InstallError::refusal(format!(
                    "--vars {:?} is passed more than once; refusing rather than guessing which value wins",
                    assign.id
                )));
            }
            vars.push(assign);
        }
        Ok(InstallOptions {
            current_version: current_version.to_string(),
            vars,
            force,
        })
    }
}

/// One `--vars` assignment: the variable `id` plus its new default.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VarAssign {
    pub id: String,
    pub value: String,
}

/// Parse one `--vars` assignment of the shape `id=value`, splitting
/// on the first `=`. A missing `=` or an empty id refuses naming the
/// assignment and the expected shape.
pub fn parse_var_assign(raw: &str) -> Result<VarAssign, InstallError> {
    match raw.split_once('=') {
        Some((id, value)) if !id.trim().is_empty() => Ok(VarAssign {
            id: id.trim().to_string(),
            value: value.to_string(),
        }),
        _ => Err(InstallError::refusal(format!(
            "--vars {raw:?} must have the shape `id=value`; refusing with nothing written"
        ))),
    }
}

/// A parsed `major.minor.patch` version plus its pre-release tag.
/// Missing minor/patch parts read as zero; a leading `v` is
/// accepted. Anything else fails to parse and refuses fail-closed at
/// the gate.
#[derive(Debug, Clone, PartialEq, Eq)]
struct CliVersion {
    parts: [u64; 3],
    pre: Option<String>,
}

/// Parse a CLI version string. Returns `None` for anything that is
/// not one to three dot-separated non-negative integers with an
/// optional `-tag` or `+tag` suffix.
fn parse_version(raw: &str) -> Option<CliVersion> {
    let text = raw.trim().strip_prefix('v').unwrap_or(raw.trim());
    if text.is_empty() {
        return None;
    }
    let core = text
        .split_once(['-', '+'])
        .map(|(core, _)| core)
        .unwrap_or(text);
    let pre = text
        .split_once(['-', '+'])
        .map(|(_, tag)| tag.to_string())
        .filter(|tag| !tag.is_empty());
    // A trailing or doubled separator leaves an empty tag, which is
    // malformed rather than a release.
    if text.ends_with('-') || text.ends_with('+') || text.contains("..") {
        return None;
    }
    let mut parts = [0u64; 3];
    let mut count = 0;
    for piece in core.split('.') {
        if count >= 3 || piece.is_empty() || !piece.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        parts[count] = piece.parse().ok()?;
        count += 1;
    }
    if count == 0 {
        return None;
    }
    Some(CliVersion { parts, pre })
}

/// Compare two parsed versions: numbers first, then release versus
/// pre-release (a release outranks its own pre-release; two
/// pre-releases compare by tag).
fn cmp_versions(current: &CliVersion, required: &CliVersion) -> std::cmp::Ordering {
    match current.parts.cmp(&required.parts) {
        std::cmp::Ordering::Equal => match (&current.pre, &required.pre) {
            (None, None) => std::cmp::Ordering::Equal,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (Some(_), None) => std::cmp::Ordering::Less,
            (Some(a), Some(b)) => a.cmp(b),
        },
        other => other,
    }
}

/// Check every resolved item's `minCliVersion` against the running
/// CLI version before any file is written. The first gated item
/// refuses naming the item, the required version, and the current
/// version; an unparsable requirement refuses fail-closed naming the
/// item and the bad value. Items without `minCliVersion` always pass.
pub fn check_version_gates(plan: &ResolvedPlan, current: &str) -> Result<(), InstallError> {
    let current_parsed = parse_version(current).ok_or_else(|| {
        InstallError::refusal(format!(
            "cannot install: the running CLI version {current:?} does not parse; refusing with nothing written"
        ))
    })?;
    for entry in &plan.items {
        let required = match &entry.manifest.min_cli_version {
            None => continue,
            Some(v) => v,
        };
        let required_parsed = parse_version(required).ok_or_else(|| {
            InstallError::refusal(format!(
                "cannot install {:?}: its minCliVersion {required:?} does not parse; refusing with nothing written",
                entry.manifest.name
            ))
        })?;
        if cmp_versions(&current_parsed, &required_parsed) == std::cmp::Ordering::Less {
            return Err(InstallError::refusal(format!(
                "cannot install {:?}: it needs CLI version {required:?} but this is {current:?}; upgrade framesmith and rerun with nothing written",
                entry.manifest.name
            )));
        }
    }
    Ok(())
}

/// One variable declared by a resolved manifest: the `id` from the
/// manifest `variables` entry plus its declared type and options when
/// present, so `--vars` values bake with the right KDL spelling.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct DeclaredVar {
    var_type: Option<String>,
    options: Vec<String>,
}

/// The union of `variables` ids declared across a resolved plan.
/// Entries without a string `id` cannot be targeted by `--vars` and
/// stay out of the map; they never block an install on their own.
fn declared_variables(plan: &ResolvedPlan) -> BTreeMap<String, DeclaredVar> {
    let mut out: BTreeMap<String, DeclaredVar> = BTreeMap::new();
    for entry in &plan.items {
        for var in &entry.manifest.variables {
            let Some(id) = var.get("id").and_then(|v| v.as_str()) else {
                continue;
            };
            if id.trim().is_empty() {
                continue;
            }
            let slot = out.entry(id.to_string()).or_default();
            if slot.var_type.is_none() {
                slot.var_type = var
                    .get("type")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
            }
            if slot.options.is_empty() {
                if let Some(options) = var.get("options").and_then(|v| v.as_str()) {
                    slot.options = options
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                }
            }
        }
    }
    out
}

/// Refuse `--vars` ids that no resolved manifest declares, naming the
/// variable and the known ids: values are never silently created or
/// silently dropped at this layer (unbaked-but-declared values print
/// a mount snippet instead; see [`mount_snippet_for`]).
fn check_vars_known(plan: &ResolvedPlan, vars: &[VarAssign]) -> Result<(), InstallError> {
    if vars.is_empty() {
        return Ok(());
    }
    let declared = declared_variables(plan);
    for var in vars {
        if !declared.contains_key(&var.id) {
            let mut known: Vec<String> = declared.keys().cloned().collect();
            known.sort();
            let hint = if known.is_empty() {
                "this set declares no variables".to_string()
            } else {
                format!("declared here: {}", known.join(", "))
            };
            return Err(InstallError::refusal(format!(
                "--vars {:?} names no variable in this set ({hint}); refusing with nothing written",
                var.id
            )));
        }
    }
    Ok(())
}

/// Render one baked default in KDL spelling for its declared type:
/// numbers and booleans ride bare (validated fail-closed), `enum`
/// values must pick a declared option when options are known, and
/// every other type — including `font` try-order lists, which are
/// preserved exactly as authored and never split or resolved here —
/// rides as a quoted string with `\` and `"` escaped.
fn render_baked_default(
    var: &VarAssign,
    declared: &BTreeMap<String, DeclaredVar>,
    target: &str,
) -> Result<String, InstallError> {
    let kind = declared
        .get(&var.id)
        .and_then(|d| d.var_type.as_deref())
        .unwrap_or("string");
    match kind {
        "number" => match var.value.parse::<f64>() {
            Ok(v) if v.is_finite() => Ok(var.value.clone()),
            _ => Err(InstallError::refusal(format!(
                "--vars {:?} wants {value:?} for a number variable in {target:?}; refusing with nothing written",
                var.id,
                value = var.value
            ))),
        },
        "boolean" => match var.value.as_str() {
            "true" | "false" => Ok(var.value.clone()),
            _ => Err(InstallError::refusal(format!(
                "--vars {:?} wants {:?} for a boolean variable in {target:?}; want true or false, refusing with nothing written",
                var.id, var.value
            ))),
        },
        "enum" => {
            let options = declared
                .get(&var.id)
                .map(|d| d.options.as_slice())
                .unwrap_or(&[]);
            if !options.is_empty()
                && !options.iter().any(|o| o == &var.value)
            {
                return Err(InstallError::refusal(format!(
                    "--vars {:?} wants {:?} for an enum variable in {target:?}; want one of {}, refusing with nothing written",
                    var.id,
                    var.value,
                    options
                        .iter()
                        .map(|o| format!("{o:?}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                )));
            }
            Ok(quote_kdl(&var.value))
        }
        _ => Ok(quote_kdl(&var.value)),
    }
}

/// Quote one KDL string value, escaping `\` and `"`.
fn quote_kdl(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Byte span of one prop value on a declaration line:
/// `(value_start, value_end)` in line bytes, tolerating spaces around
/// `=`. Quoted values span the inner text (without the quotes);
/// bare values run to the next whitespace. The prop name must stand
/// alone, so `mydefault=` is never a hit for `default`. Returns
/// `None` when the line carries no such prop.
fn prop_value_span(line: &str, prop: &str) -> Option<(usize, usize)> {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i + prop.len() <= bytes.len() {
        if &line[i..i + prop.len()] == prop {
            let before_ok = i == 0
                || !(bytes[i - 1].is_ascii_alphanumeric()
                    || bytes[i - 1] == b'-'
                    || bytes[i - 1] == b'_');
            let mut j = i + prop.len();
            while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
                j += 1;
            }
            if before_ok && bytes.get(j) == Some(&b'=') {
                j += 1;
                while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
                    j += 1;
                }
                if bytes.get(j) == Some(&b'"') {
                    j += 1;
                    let start = j;
                    while j < bytes.len() {
                        if bytes[j] == b'\\' {
                            j += 2;
                            continue;
                        }
                        if bytes[j] == b'"' {
                            break;
                        }
                        j += 1;
                    }
                    return Some((start, j.min(bytes.len())));
                }
                let start = j;
                while j < bytes.len() && !bytes[j].is_ascii_whitespace() {
                    j += 1;
                }
                return Some((start, j));
            }
        }
        i += 1;
    }
    None
}

/// Read the `id="..."` prop of one `variable` declaration line.
/// Returns `None` when the line is no `variable` node or carries no
/// double-quoted id.
fn variable_line_id(line: &str) -> Option<&str> {
    let rest = line.strip_prefix("variable")?;
    if !rest.is_empty() && !rest.starts_with([' ', '\t']) {
        return None;
    }
    let (start, end) = prop_value_span(line, "id")?;
    // The id must have been double-quoted: the byte before the span
    // is the opening quote.
    if line.as_bytes().get(start.wrapping_sub(1)) != Some(&b'"') {
        return None;
    }
    Some(&line[start..end])
}

/// Byte span of the `default` value on one `variable` declaration
/// line: `(value_start, value_end)` in line bytes. Returns `None`
/// when the line carries no `default` prop.
fn default_value_span(line: &str) -> Option<(usize, usize)> {
    prop_value_span(line, "default")
}

/// Bake `--vars` values into the `variable` defaults of one staged
/// file. Binary (non-UTF-8) files pass through untouched: values
/// with no text bake site anywhere surface as a mount snippet
/// instead of failing. Returns the baked bytes plus the ids baked in
/// this file. A value that breaks its declared type refuses
/// fail-closed, still before any write.
fn bake_vars(
    bytes: &[u8],
    vars: &[VarAssign],
    declared: &BTreeMap<String, DeclaredVar>,
    target: &str,
) -> Result<(Vec<u8>, Vec<String>), InstallError> {
    if vars.is_empty() {
        return Ok((bytes.to_vec(), Vec::new()));
    }
    let by_id: BTreeMap<&str, &VarAssign> = vars.iter().map(|v| (v.id.as_str(), v)).collect();
    let Ok(text) = std::str::from_utf8(bytes) else {
        return Ok((bytes.to_vec(), Vec::new()));
    };
    let mut baked: Vec<String> = Vec::new();
    let mut out = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_start();
        let Some(id) = variable_line_id(trimmed) else {
            out.push_str(line);
            continue;
        };
        let Some(var) = by_id.get(id) else {
            out.push_str(line);
            continue;
        };
        let rendered = render_baked_default(var, declared, target)?;
        // Work on the line without its trailing newline so an
        // appended default lands before it.
        let newline = if line.ends_with('\n') { "\n" } else { "" };
        let body = line.strip_suffix('\n').unwrap_or(line);
        let rebaked = match default_value_span(body) {
            Some((start, end)) => {
                // The span covers the inner text of a quoted value;
                // widen it over both quotes so the rendered default
                // (quoted or bare per its type) replaces the whole.
                let bytes = body.as_bytes();
                let (start, end) = if bytes.get(start.wrapping_sub(1)) == Some(&b'"')
                    && bytes.get(end) == Some(&b'"')
                {
                    (start - 1, end + 1)
                } else {
                    (start, end)
                };
                format!("{}{}{}", &body[..start], rendered, &body[end..])
            }
            None => format!("{} default={}", body.trim_end(), rendered),
        };
        out.push_str(&rebaked);
        out.push_str(newline);
        if !baked.iter().any(|b| b == &var.id) {
            baked.push(var.id.clone());
        }
    }
    Ok((out.into_bytes(), baked))
}

/// The mount snippet for `--vars` values with no bake site in any
/// installed text file: the exact `--variables` JSON payload that
/// mounts them at `check`/`render` time, so values are never silently
/// dropped.
pub fn mount_snippet_for(unbaked: &[VarAssign]) -> String {
    let mut pairs = String::new();
    for (i, var) in unbaked.iter().enumerate() {
        if i > 0 {
            pairs.push_str(", ");
        }
        pairs.push_str(&format!("{:?}: {:?}", var.id, var.value));
    }
    format!("mount these values at check/render time: --variables '{{{pairs}}}'")
}

/// One installed file in the lock: the owning item plus the SHA256 of
/// the bytes as written (baked `--vars` values included).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockEntry {
    pub target: String,
    pub item: String,
    pub sha256: String,
}

/// Staged bytes shared by the writer and the lock recorder.
struct StagedFile {
    item: String,
    target: String,
    audio: Option<AudioRole>,
    bytes: Vec<u8>,
}

/// The catalog lock: every installed file's hash plus the installing
/// items. It lives at [`CATALOG_LOCK_FILE`] inside the project and
/// merges across installs, so files from earlier installs keep their
/// entries when later installs land beside them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockFile {
    pub items: Vec<String>,
    pub files: Vec<LockEntry>,
}

impl LockFile {
    fn empty() -> LockFile {
        LockFile {
            items: Vec::new(),
            files: Vec::new(),
        }
    }

    /// The recorded hash for one target, if any.
    pub fn hash_of(&self, target: &str) -> Option<&str> {
        self.files
            .iter()
            .find(|e| e.target == target)
            .map(|e| e.sha256.as_str())
    }

    /// Merge one install into the lock: item names join the set and
    /// every staged target records the hash of its bytes as written.
    /// Entries for targets this install did not touch are kept.
    fn record(&mut self, plan: &ResolvedPlan, staged: &[StagedFile]) {
        for name in plan.order() {
            if !self.items.iter().any(|i| i == name) {
                self.items.push(name.to_string());
            }
        }
        self.items.sort();
        for write in staged {
            let hash = sha256_hex(&write.bytes);
            match self.files.iter_mut().find(|e| e.target == write.target) {
                Some(entry) => {
                    entry.item = write.item.clone();
                    entry.sha256 = hash;
                }
                None => self.files.push(LockEntry {
                    target: write.target.clone(),
                    item: write.item.clone(),
                    sha256: hash,
                }),
            }
        }
        self.files.sort_by(|a, b| a.target.cmp(&b.target));
    }

    /// Render the lock document.
    fn to_json(&self) -> String {
        let mut out = String::from("{\n");
        out.push_str(&format!("  \"version\": {CATALOG_LOCK_VERSION},\n"));
        out.push_str("  \"items\": [");
        for (i, name) in self.items.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            out.push_str(&format!("{name:?}"));
        }
        out.push_str("],\n  \"files\": [\n");
        for (i, entry) in self.files.iter().enumerate() {
            if i > 0 {
                out.push_str(",\n");
            }
            out.push_str(&format!(
                "    {{\"target\": {:?}, \"item\": {:?}, \"sha256\": {:?}}}",
                entry.target, entry.item, entry.sha256
            ));
        }
        out.push_str("\n  ]\n}\n");
        out
    }

    /// Parse a lock document, refusing shape violations naming the
    /// lock file.
    fn from_json(text: &str) -> Result<LockFile, InstallError> {
        let refused = |message: String| InstallError::refusal(message);
        let value: serde_json::Value = serde_json::from_str(text).map_err(|e| {
            refused(format!(
                "{CATALOG_LOCK_FILE} is not valid JSON ({e}); refusing with nothing written"
            ))
        })?;
        let obj = value.as_object().ok_or_else(|| {
            refused(format!(
                "{CATALOG_LOCK_FILE} must be a JSON object; refusing with nothing written"
            ))
        })?;
        let version = obj.get("version").and_then(|v| v.as_u64()).ok_or_else(|| {
            refused(format!(
                "{CATALOG_LOCK_FILE} carries no numeric \"version\"; refusing with nothing written"
            ))
        })?;
        if version != CATALOG_LOCK_VERSION {
            return Err(refused(format!(
                "{CATALOG_LOCK_FILE} carries version {version}, want {CATALOG_LOCK_VERSION}; refusing with nothing written"
            )));
        }
        let mut items = Vec::new();
        for v in obj.get("items").and_then(|v| v.as_array()).ok_or_else(|| {
            refused(format!(
                "{CATALOG_LOCK_FILE} carries no \"items\" array; refusing with nothing written"
            ))
        })? {
            items.push(
                v.as_str()
                    .ok_or_else(|| {
                        refused(format!(
                            "{CATALOG_LOCK_FILE} \"items\" must list strings; refusing with nothing written"
                        ))
                    })?
                    .to_string(),
            );
        }
        let mut files = Vec::new();
        for v in obj.get("files").and_then(|v| v.as_array()).ok_or_else(|| {
            refused(format!(
                "{CATALOG_LOCK_FILE} carries no \"files\" array; refusing with nothing written"
            ))
        })? {
            let entry = v.as_object().ok_or_else(|| {
                refused(format!(
                    "{CATALOG_LOCK_FILE} \"files\" entries must be objects; refusing with nothing written"
                ))
            })?;
            let get = |key: &str| {
                entry
                    .get(key)
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .ok_or_else(|| {
                        refused(format!(
                            "{CATALOG_LOCK_FILE} file entries need string {key:?}; refusing with nothing written"
                        ))
                    })
            };
            files.push(LockEntry {
                target: get("target")?,
                item: get("item")?,
                sha256: get("sha256")?,
            });
        }
        Ok(LockFile { items, files })
    }
}

/// Read the project lock, if any. A missing lock is no lock (fresh
/// installs); a corrupt one refuses fail-closed naming the file.
fn read_lock(project_dir: &Path) -> Result<Option<LockFile>, InstallError> {
    let path = project_dir.join(CATALOG_LOCK_FILE);
    let text = match std::fs::read_to_string(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(InstallError::io(format!(
                "cannot read {}: {e}",
                path.display()
            )))
        }
        Ok(text) => text,
    };
    LockFile::from_json(&text).map(Some)
}

/// Write the project lock after a successful install.
fn write_lock(project_dir: &Path, lock: &LockFile) -> Result<(), InstallError> {
    let path = project_dir.join(CATALOG_LOCK_FILE);
    if let Some(parent) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            return Err(InstallError::io(format!(
                "cannot create directory {}: {e}",
                parent.display()
            )));
        }
    }
    if let Err(e) = std::fs::write(&path, lock.to_json()) {
        return Err(InstallError::io(format!(
            "cannot write {}: {e}",
            path.display()
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_suffix_matching_is_case_insensitive() {
        assert!(is_audio_target("audio/narration.WAV"));
        assert!(is_audio_target("sfx/hit.mp3"));
        assert!(!is_audio_target("composition.kdl"));
        assert!(!is_audio_target("docs/notes.txt"));
        assert!(!is_audio_target("no-extension"));
    }

    #[test]
    fn longest_prefix_remap_wins() {
        let remaps = vec![
            ("audio".to_string(), "assets/audio".to_string()),
            ("audio/voice".to_string(), "assets/voice".to_string()),
        ];
        assert_eq!(
            remap_target("audio/voice/take.wav", &remaps),
            "assets/voice/take.wav"
        );
        assert_eq!(
            remap_target("audio/sfx/hit.wav", &remaps),
            "assets/audio/sfx/hit.wav"
        );
        assert_eq!(remap_target("snippets/a.kdl", &remaps), "snippets/a.kdl");
    }

    #[test]
    fn escaping_targets_refuse() {
        assert!(check_target("x", "../out.kdl").is_err());
        assert!(check_target("x", "/abs.kdl").is_err());
        assert!(check_target("x", "").is_err());
        assert!(check_target("x", "framesmith.json").is_err());
        assert!(check_target("x", "snippets/a.kdl").is_ok());
    }

    #[test]
    fn tool_owned_targets_refuse() {
        assert!(check_target("x", ".framesmith/catalog-lock.json").is_err());
        assert!(check_target("x", ".framesmith/notes.txt").is_err());
    }

    #[test]
    fn version_strings_order_numerically() {
        use std::cmp::Ordering;
        let v = |s: &str| parse_version(s).unwrap_or_else(|| panic!("{s:?} must parse"));
        assert_eq!(cmp_versions(&v("0.1.0"), &v("0.1.0")), Ordering::Equal);
        assert_eq!(cmp_versions(&v("0.1.0"), &v("0.0.9")), Ordering::Greater);
        assert_eq!(cmp_versions(&v("0.1.0"), &v("0.2.0")), Ordering::Less);
        assert_eq!(cmp_versions(&v("0.1"), &v("0.1.0")), Ordering::Equal);
        assert_eq!(cmp_versions(&v("v0.1.0"), &v("0.1.0")), Ordering::Equal);
        assert_eq!(
            cmp_versions(&v("0.1.0"), &v("0.1.0-rc.1")),
            Ordering::Greater
        );
        for bad in ["", "soon", "1.2.3.4", "1..2", "1.2-", "v", "1.x"] {
            assert!(parse_version(bad).is_none(), "{bad:?} must not parse");
        }
    }

    #[test]
    fn var_assignments_split_on_first_equals() {
        let assign = parse_var_assign("headline=Sale Night").unwrap();
        assert_eq!(assign.id, "headline");
        assert_eq!(assign.value, "Sale Night");
        let assign = parse_var_assign("accent=#111=222").unwrap();
        assert_eq!(assign.value, "#111=222");
        assert!(parse_var_assign("noequals").is_err());
        assert!(parse_var_assign("=empty").is_err());
    }
}
