//! Offline word search (Sprint 19, T005 / CAP-005).
//!
//! Word search ranks matches across the `name`, `title`,
//! `description`, and `tags` manifest fields with stemming and
//! compound splitting, answering immediately and fully offline: this
//! module never touches the network, never reads the meaning model,
//! and never writes. The CLI (`framesmith catalog search`), the
//! human-friendly picker (the numbered CLI listing), and the Studio
//! catalog panel all build one [`SearchIndex`] from the same merged
//! item list and rank through the same [`SearchIndex::search`], so
//! the three surfaces always agree.
//!
//! Scoring: every query word is tokenized, stemmed, and
//! compound-split exactly like the indexed fields. An exact stem hit
//! in a field scores that field's full weight (`name` 8, `title` 5,
//! `tags` 4, `description` 1); a prefix hit (either side starts with
//! the other, both sides at least 3 characters) scores half weight,
//! which is also what carries inflected forms the stemmer does not
//! conflate (for example `type` still reaches `typing`). Hits sort by
//! descending score, ties by ascending name, so ranking is
//! deterministic.
//!
//! A search over an empty registry reports the empty registry
//! instead of zero hits; a search with words but no hits offers
//! search-miss reporting via
//! `framesmith catalog report-miss "<words>"`.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::manifest::ItemManifest;
use crate::registry::{load_registry, MergedListing, RegistryListing};

/// Full weight of an exact stem hit in the item name.
pub const NAME_WEIGHT: f64 = 8.0;
/// Full weight of an exact stem hit in the item title.
pub const TITLE_WEIGHT: f64 = 5.0;
/// Full weight of an exact stem hit in one item tag.
pub const TAG_WEIGHT: f64 = 4.0;
/// Full weight of an exact stem hit in the item description.
pub const DESCRIPTION_WEIGHT: f64 = 1.0;
/// Fraction of a field weight paid for a prefix (near) hit.
pub const PREFIX_FRACTION: f64 = 0.5;

/// Environment variable overriding the registry directory search
/// reads. An explicit `--registry` flag wins over this; otherwise
/// search reads `./registry` under the current directory.
pub const REGISTRY_ENV: &str = "FRAMESMITH_REGISTRY";

/// File (inside the registry directory) that `report-miss` appends
/// search-miss reports to, one JSON object per line.
pub const MISS_REPORTS_FILE: &str = ".miss-reports.jsonl";

/// Resolve which registry directory search reads: the explicit flag
/// value, else [`REGISTRY_ENV`], else `./registry`. The directory
/// does not need to exist: a missing directory lists as an empty
/// registry and search reports it (see [`empty_registry_report`]).
pub fn resolve_registry_dir(explicit: Option<&str>) -> PathBuf {
    if let Some(dir) = explicit {
        return PathBuf::from(dir);
    }
    if let Ok(dir) = std::env::var(REGISTRY_ENV) {
        if !dir.trim().is_empty() {
            return PathBuf::from(dir);
        }
    }
    PathBuf::from("registry")
}

/// Split text into lowercase search tokens. Splits on anything that
/// is not alphanumeric (spaces, hyphens, underscores, slashes), and
/// additionally splits camelCase humps (`TitleCard` yields `title`
/// and `card` alongside the whole `titlecard`), so compound names
/// match their parts. Empty input yields no tokens.
pub fn tokenize(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    for raw in text.split(|c: char| !c.is_alphanumeric()) {
        if raw.is_empty() {
            continue;
        }
        let lowered: String = raw.chars().flat_map(|c| c.to_lowercase()).collect();
        tokens.push(lowered.clone());
        // Camel-hump parts: cut before an uppercase letter that
        // follows a lowercase one, and between letters and digits.
        let chars: Vec<char> = raw.chars().collect();
        let mut part = String::new();
        let mut parts: Vec<String> = Vec::new();
        for (i, c) in chars.iter().enumerate() {
            let boundary = if i == 0 {
                false
            } else {
                let prev = chars[i - 1];
                (c.is_uppercase() && prev.is_lowercase())
                    || (c.is_ascii_digit() != prev.is_ascii_digit())
            };
            if boundary && !part.is_empty() {
                parts.push(part.clone());
                part.clear();
            }
            part.extend(c.to_lowercase());
        }
        if !part.is_empty() {
            parts.push(part);
        }
        if parts.len() > 1 {
            let whole = parts.concat();
            for p in parts {
                if p != whole {
                    tokens.push(p);
                }
            }
        }
    }
    tokens
}

/// Stem one lowercase token with a small deterministic English
/// suffix stripper. Handles plurals (`titles` to `title`,
/// `stories` to `story`, `brushes` to `brush`) and verb forms
/// (`fading`/`faded` to `fad`, `running` to `run`); anything the
/// stemmer does not conflate still matches through prefix scoring
/// at search time. Non-ASCII tokens pass through unchanged.
pub fn stem(token: &str) -> String {
    if !token.is_ascii() {
        return token.to_string();
    }
    let mut w = token.to_string();
    // Plurals first: -ies -> -y, sibilant -es, plain -s.
    if w.len() > 4 && w.ends_with("ies") {
        w.truncate(w.len() - 3);
        w.push('y');
        return w;
    }
    if w.len() > 4 && w.ends_with("es") {
        let base = &w[..w.len() - 2];
        if base.ends_with('s')
            || base.ends_with('x')
            || base.ends_with('z')
            || base.ends_with("ch")
            || base.ends_with("sh")
        {
            return base.to_string();
        }
    }
    if w.len() > 3 && w.ends_with('s') && !w.ends_with("ss") {
        w.truncate(w.len() - 1);
    }
    // Verb forms: -ing and -ed with double-consonant collapse.
    for suffix in ["ing", "ed"] {
        if w.len() > suffix.len() + 2 && w.ends_with(suffix) {
            w.truncate(w.len() - suffix.len());
            let bytes = w.as_bytes();
            let n = bytes.len();
            if n >= 2 && bytes[n - 1] == bytes[n - 2] && !"aeiou".contains(bytes[n - 1] as char) {
                w.truncate(n - 1);
            }
            break;
        }
    }
    w
}

/// Tokenize plus stem one text: the indexed/query unit.
pub fn stems_of(text: &str) -> Vec<String> {
    tokenize(text).into_iter().map(|t| stem(&t)).collect()
}

/// One indexed item: the manifest plus its per-field stem multisets.
#[derive(Debug, Clone)]
struct IndexedItem {
    manifest: ItemManifest,
    origin: String,
    name: Vec<String>,
    title: Vec<String>,
    tags: Vec<String>,
    description: Vec<String>,
}

impl IndexedItem {
    fn index(manifest: ItemManifest) -> Self {
        Self::index_with_origin(manifest, crate::BUNDLED_ORIGIN)
    }

    fn index_with_origin(manifest: ItemManifest, origin: &str) -> Self {
        let name = stems_of(&manifest.name.replace(['-', '_'], " "));
        let title = stems_of(&manifest.title);
        let tags = manifest.tags.iter().flat_map(|t| stems_of(t)).collect();
        let description = stems_of(&manifest.description);
        Self {
            manifest,
            origin: origin.to_string(),
            name,
            title,
            tags,
            description,
        }
    }
}

/// One merged word-search index over a registry listing. Build once
/// with [`SearchIndex::build`] and rank every surface (CLI, picker,
/// Studio panel) through [`SearchIndex::search`].
#[derive(Debug, Clone, Default)]
pub struct SearchIndex {
    items: Vec<IndexedItem>,
    /// Every stem the index holds: the compound-splitting dictionary.
    vocab: HashSet<String>,
}

impl SearchIndex {
    /// Build the one merged index over these items (usually
    /// `load_registry(&dir).items`). Every item reads as the bundled
    /// origin; merged listings build with [`SearchIndex::build_merged`]
    /// so each hit carries its registry of origin.
    pub fn build(items: &[ItemManifest]) -> Self {
        let indexed: Vec<IndexedItem> = items.iter().cloned().map(IndexedItem::index).collect();
        let mut vocab = HashSet::new();
        for item in &indexed {
            for s in item
                .name
                .iter()
                .chain(item.title.iter())
                .chain(item.tags.iter())
                .chain(item.description.iter())
            {
                vocab.insert(s.clone());
            }
        }
        Self {
            items: indexed,
            vocab,
        }
    }

    /// Build the one merged index over a merged listing (Sprint 19,
    /// T007 / CAP-007): the winning item per name across the bundled
    /// base plus every reachable custom source, each hit carrying its
    /// registry of origin. Shares the ranking with [`SearchIndex::build`]
    /// so single-registry and merged surfaces always agree.
    pub fn build_merged(items: &[crate::MergedItem]) -> Self {
        let indexed: Vec<IndexedItem> = items
            .iter()
            .map(|item| IndexedItem::index_with_origin(item.manifest.clone(), &item.origin))
            .collect();
        let mut vocab = HashSet::new();
        for item in &indexed {
            for s in item
                .name
                .iter()
                .chain(item.title.iter())
                .chain(item.tags.iter())
                .chain(item.description.iter())
            {
                vocab.insert(s.clone());
            }
        }
        Self {
            items: indexed,
            vocab,
        }
    }

    /// True when the index holds no items: search reports the empty
    /// registry instead of ranking.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Number of indexed items.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Rank these words against the index. Returns hits sorted by
    /// descending score (ties by ascending name); an empty query or
    /// no match yields no hits. Never touches the network.
    pub fn search(&self, query: &str) -> Vec<RankedHit<'_>> {
        let mut hits = Vec::new();
        let query_stems = self.expand_query(query);
        if query_stems.is_empty() {
            return hits;
        }
        for item in &self.items {
            let score = score_item(item, &query_stems);
            if score > 0.0 {
                hits.push(RankedHit {
                    item: &item.manifest,
                    origin: item.origin.as_str(),
                    score,
                });
            }
        }
        hits.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.item.name.cmp(&b.item.name))
        });
        hits
    }

    /// Tokenize, stem, and compound-split one query against this
    /// index's vocabulary: a query stem the index never holds is
    /// retried as a two-part compound (`titlecard` to `title` plus
    /// `card`) when both halves are known stems.
    fn expand_query(&self, query: &str) -> Vec<String> {
        let mut out = Vec::new();
        for token in tokenize(query) {
            let stemmed = stem(&token);
            if self.vocab.contains(&stemmed) {
                out.push(stemmed);
                continue;
            }
            out.extend(self.split_compound(&stemmed));
        }
        out
    }

    /// Split one unknown stem into two known halves, or keep it
    /// whole when no clean split exists (prefix scoring may still
    /// reach it).
    fn split_compound(&self, token: &str) -> Vec<String> {
        let chars: Vec<char> = token.chars().collect();
        for i in 2..chars.len().saturating_sub(1) {
            let a: String = chars[..i].iter().collect();
            let b: String = chars[i..].iter().collect();
            if a.chars().count() >= 2
                && b.chars().count() >= 2
                && self.vocab.contains(&a)
                && self.vocab.contains(&b)
            {
                return vec![a, b];
            }
        }
        vec![token.to_string()]
    }
}

/// One ranked result: the manifest, its registry of origin
/// (`"bundled"` or the custom registry's configured name), and its
/// computed score.
#[derive(Debug, Clone, Copy)]
pub struct RankedHit<'a> {
    /// The matched item manifest.
    pub item: &'a ItemManifest,
    /// The registry of origin that won the merge for this item.
    pub origin: &'a str,
    /// Weighted field score (higher ranks first).
    pub score: f64,
}

/// Score one item against expanded query stems: exact stem hits pay
/// the full field weight, prefix hits half.
fn score_item(item: &IndexedItem, query_stems: &[String]) -> f64 {
    let fields: [(&[String], f64); 4] = [
        (&item.name, NAME_WEIGHT),
        (&item.title, TITLE_WEIGHT),
        (&item.tags, TAG_WEIGHT),
        (&item.description, DESCRIPTION_WEIGHT),
    ];
    let mut total = 0.0;
    for q in query_stems {
        for (stems, weight) in &fields {
            if stems.iter().any(|s| s == q) {
                total += weight;
            } else if stems.iter().any(|s| near_match(s, q)) {
                total += weight * PREFIX_FRACTION;
            }
        }
    }
    // Round to one decimal so scores print and compare stably.
    (total * 10.0).round() / 10.0
}

/// Prefix (near) hit: either side starts with the other, both sides
/// at least 3 characters, and not an exact hit (checked first by
/// the caller).
fn near_match(field: &str, query: &str) -> bool {
    if field.len() < 3 || query.len() < 3 {
        return false;
    }
    field.starts_with(query) || query.starts_with(field)
}

/// Load one registry directory and build its merged search index in
/// one step: the shared entry point for the CLI, the picker, and
/// the Studio panel.
pub fn load_search_index(dir: &Path) -> (RegistryListing, SearchIndex) {
    let listing = load_registry(dir);
    let index = SearchIndex::build(&listing.items);
    (listing, index)
}

/// Human report for an empty index: names the registry directory so
/// the maker knows what to fill. Shared by the CLI and the Studio
/// panel.
pub fn empty_registry_report(dir: &Path) -> String {
    format!(
        "catalog: the registry at {} holds no items (an empty index reports the empty registry, never zero hits).\n\
         fix: add item folders each holding a manifest.json, then search again.",
        dir.display()
    )
}

/// Human report for a search with words but no hits: keeps the
/// search result and offers search-miss reporting. Shared by the
/// CLI and the Studio panel.
pub fn no_hits_report(query: &str, dir: &Path) -> String {
    format!(
        "catalog: no items match {query:?} in the registry at {}.\n\
         report this miss with: framesmith catalog report-miss {query:?} --registry {}",
        dir.display(),
        dir.display()
    )
}

/// The search-miss offer string embedded in machine output and the
/// Studio panel.
pub fn miss_report_offer(query: &str, dir: &Path) -> String {
    format!(
        "framesmith catalog report-miss {query:?} --registry {}",
        dir.display()
    )
}

/// Human-friendly picker: the ranked hits as a numbered list with
/// each item's kind, title, score, and copyable handoff commands.
/// The picker prints the same ranking [`SearchIndex::search`]
/// returns, so it never disagrees with the CLI or Studio JSON.
pub fn render_picker(query: &str, dir: &Path, hits: &[RankedHit<'_>]) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "catalog search {query:?} — {} hit(s) in the registry at {}:\n",
        hits.len(),
        dir.display()
    ));
    for (i, hit) in hits.iter().enumerate() {
        out.push_str(&format!(
            "{}. {} ({}) [{}] — {} [score {}]\n",
            i + 1,
            hit.item.name,
            hit.item.item_type,
            hit.origin,
            hit.item.title,
            format_score(hit.score)
        ));
        out.push_str(&format!(
            "   show: framesmith catalog show {:?} / install: framesmith catalog install <project> {:?}\n",
            hit.item.name, hit.item.name
        ));
    }
    out
}

/// Format one score with exactly one decimal.
pub fn format_score(score: f64) -> String {
    format!("{score:.1}")
}

/// Load one base registry directory plus the custom sources a
/// project configuration names (Sprint 19, T007 / CAP-007) and build
/// the merged search index in one step: the shared entry point for
/// the CLI, the picker, and the Studio panel once custom registries
/// enter the picture. Merge warnings (unreachable custom registries,
/// skipped invalid custom manifests) travel in the listing for the
/// caller to surface; ranking itself is unchanged.
pub fn load_merged_search_index(
    base: &crate::RegistrySource,
    extras: &[crate::RegistrySource],
) -> (MergedListing, SearchIndex) {
    let listing = crate::load_merged(base, extras);
    let index = SearchIndex::build_merged(&listing.items);
    (listing, index)
}

/// Machine-readable hits envelope shared by `catalog search --json`
/// and the Studio panel: the same builder, so both surfaces carry
/// the same ranked order and scores. Every hit carries its registry
/// of origin; merge warnings ride in `"warnings"` (empty for
/// single-registry listings).
pub fn hits_json(query: &str, dir: &Path, hits: &[RankedHit<'_>]) -> String {
    hits_json_with_warnings(query, dir, hits, &[])
}

/// [`hits_json`] plus the merge warnings a merged listing reports
/// (unreachable custom registries, skipped invalid custom manifests).
pub fn hits_json_with_warnings(
    query: &str,
    dir: &Path,
    hits: &[RankedHit<'_>],
    warnings: &[String],
) -> String {
    let mut parts = Vec::with_capacity(hits.len());
    for hit in hits {
        parts.push(serde_json::json!({
            "name": hit.item.name,
            "type": hit.item.item_type.as_str(),
            "title": hit.item.title,
            "origin": hit.origin,
            "score": hit.score,
        }));
    }
    serde_json::json!({
        "query": query,
        "registry": dir.display().to_string(),
        "empty": false,
        "hits": parts,
        "warnings": warnings,
    })
    .to_string()
}

/// Machine-readable no-hits envelope: zero hits plus the
/// search-miss offer (see [`miss_report_offer`]).
pub fn no_hits_json(query: &str, dir: &Path) -> String {
    no_hits_json_with_warnings(query, dir, &[])
}

/// [`no_hits_json`] plus the merge warnings a merged listing reports.
pub fn no_hits_json_with_warnings(query: &str, dir: &Path, warnings: &[String]) -> String {
    serde_json::json!({
        "query": query,
        "registry": dir.display().to_string(),
        "empty": false,
        "hits": [],
        "report_offer": miss_report_offer(query, dir),
        "warnings": warnings,
    })
    .to_string()
}

/// Machine-readable empty-registry envelope: `empty` is true and no
/// zero-hits list is presented as a result.
pub fn empty_json(dir: &Path) -> String {
    empty_json_with_warnings(dir, &[])
}

/// [`empty_json`] plus the merge warnings a merged listing reports.
pub fn empty_json_with_warnings(dir: &Path, warnings: &[String]) -> String {
    serde_json::json!({
        "query": "",
        "registry": dir.display().to_string(),
        "empty": true,
        "hits": [],
        "message": empty_registry_report(dir),
        "warnings": warnings,
    })
    .to_string()
}

/// Append one search-miss report (`{"query","at"}`) to the
/// registry's miss-report log. Returns the log path. A log that
/// cannot be stored is a caller-reported refusal, never a panic:
/// the search result stands either way.
pub fn append_miss_report(dir: &Path, query: &str) -> Result<PathBuf, String> {
    let path = dir.join(MISS_REPORTS_FILE);
    let at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let line = serde_json::json!({"query": query, "at": at}).to_string() + "\n";
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .and_then(|mut f| {
            use std::io::Write;
            f.write_all(line.as_bytes())
        })
        .map_err(|e| format!("cannot store the miss report at {}: {e}", path.display()))?;
    Ok(path)
}

/// Score contributions per field, for tests and diagnostics: maps
/// each query stem to `(field, weight-paid)` pairs in query order.
#[allow(dead_code)]
pub fn explain_score(
    index: &SearchIndex,
    query: &str,
    item_name: &str,
) -> Vec<(String, String, f64)> {
    let query_stems = index.expand_query(query);
    let mut out = Vec::new();
    let Some(item) = index.items.iter().find(|i| i.manifest.name == item_name) else {
        return out;
    };
    let fields: [(&str, &[String], f64); 4] = [
        ("name", &item.name, NAME_WEIGHT),
        ("title", &item.title, TITLE_WEIGHT),
        ("tags", &item.tags, TAG_WEIGHT),
        ("description", &item.description, DESCRIPTION_WEIGHT),
    ];
    for q in &query_stems {
        for (field, stems, weight) in &fields {
            if stems.iter().any(|s| s == q) {
                out.push((q.clone(), field.to_string(), *weight));
            } else if stems.iter().any(|s| near_match(s, q)) {
                out.push((q.clone(), field.to_string(), weight * PREFIX_FRACTION));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(
        name: &str,
        kind: &str,
        title: &str,
        description: &str,
        tags: &[&str],
    ) -> ItemManifest {
        let tags_json = tags
            .iter()
            .map(|t| format!("{t:?}"))
            .collect::<Vec<_>>()
            .join(",");
        let text = format!(
            r#"{{"name": {name:?}, "type": {kind:?}, "title": {title:?}, "description": {description:?},
                "files": [{{"src": "composition.kdl", "target": "composition.kdl"}}],
                "width": 1280, "height": 720, "duration": 4, "tags": [{tags_json}]}}"#
        );
        crate::parse_manifest_str(&text).expect("fixture parses")
    }

    #[test]
    fn tokenizer_splits_hyphens_and_camel() {
        let tokens = tokenize("TitleCard title-card");
        assert!(tokens.contains(&"titlecard".to_string()), "{tokens:?}");
        assert!(tokens.contains(&"title".to_string()), "{tokens:?}");
        assert!(tokens.contains(&"card".to_string()), "{tokens:?}");
    }

    #[test]
    fn stemmer_conflates_plurals() {
        assert_eq!(stem("titles"), "title");
        assert_eq!(stem("stories"), "story");
        assert_eq!(stem("brushes"), "brush");
    }
}
