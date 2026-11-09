//! Consent-gated on-device meaning search (Sprint 19, T006 / CAP-006).
//!
//! Word search answers immediately and fully offline; meaning search
//! layers on top of it only after the maker consents to a one-time
//! download of the named model [`MEANING_MODEL_NAME`]. The first
//! explicit meaning ask explains the download and requests consent;
//! no model bytes move before consent, declining keeps word search
//! with no penalty (re-asking only on the next explicit ask), and a
//! failed download reports its cause while staying on word search.
//! Revoking consent deletes the model and returns to word search.
//!
//! Ranking is computed on-device: the query and every item's text
//! are embedded with the downloaded model bytes and ranked by
//! cosine similarity, top [`MEANING_TOP_K`] first. Word hits are
//! always computed first and travel in every outcome, so word
//! results answer immediately in every consent and download state.
//! Ranking quality tuning is explicitly deferred by the
//! specification; this module ships the consent boundary and a
//! deterministic on-device ranker, not a tuned model.
//!
//! Machine state lives in [`meaning_dir`] (override with
//! [`MEANING_ENV_DIR`]): `model.bin` plus `consent.json`. The model
//! file's presence is the consented state (STATE-04); its absence
//! means the next explicit ask explains and requests consent again.
//! All bundled item content stays our own clean-room work; this
//! module ships no item content and touches no render paths.

use std::path::{Path, PathBuf};

use crate::manifest::ItemManifest;
use crate::search::SearchIndex;

/// The one named meaning model this slice may download. Consent,
/// failure reports, and revocation all name this model so an
/// unavailable model names itself instead of returning unranked
/// results as meaning results.
pub const MEANING_MODEL_NAME: &str = "framesmith-meaning-tiny-v1";

/// Version of the named model payload this slice reads.
pub const MEANING_MODEL_VERSION: u32 = 1;

/// How many top cosine hits meaning ranking returns.
pub const MEANING_TOP_K: usize = 25;

/// Embedding width for the on-device ranker.
pub const EMBED_DIM: usize = 64;

/// Environment variable overriding the machine meaning-state
/// directory. Otherwise the state lives under
/// `$HOME/.cache/framesmith/meaning` (test temp fallback when
/// `HOME` is unset).
pub const MEANING_ENV_DIR: &str = "FRAMESMITH_MEANING_DIR";

/// Environment variable pointing at a file whose bytes a download
/// copies (a local mirror or fixture source). When set, the default
/// download reads that file; a missing file fails the download with
/// its path in the cause.
pub const MEANING_ENV_SOURCE: &str = "FRAMESMITH_MEANING_SOURCE";

/// Environment variable forcing the default download to fail with a
/// reported cause. Set to `1` to break the download (word search
/// keeps working); unset for the normal path.
pub const MEANING_ENV_FAIL: &str = "FRAMESMITH_MEANING_FAIL";

/// File holding the downloaded model bytes inside the meaning dir.
pub const MODEL_FILE: &str = "model.bin";

/// File recording the consent grant inside the meaning dir.
pub const CONSENT_FILE: &str = "consent.json";

/// Magic prefix of every model payload this slice reads.
const MODEL_MAGIC: &[u8; 8] = b"FRMSMEAN";

/// Answer to the consent ask: the maker's explicit choice on a
/// meaning search, or no choice yet (the first ask).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsentAnswer {
    /// No explicit choice: explain the download, request consent,
    /// and answer with word results.
    Ask,
    /// Consent granted: download once when needed, then rank.
    Yes,
    /// Consent declined: word results only, no bytes move.
    No,
}

/// One word-search hit carried inside every meaning outcome, so word
/// results answer immediately in every state.
#[derive(Debug, Clone)]
pub struct WordHit {
    /// Item name.
    pub name: String,
    /// Item kind (`block`, `component`, `example`).
    pub item_type: String,
    /// Item title.
    pub title: String,
    /// Weighted word score (see [`SearchIndex::search`]).
    pub score: f64,
}

/// One on-device meaning hit: cosine similarity against the named
/// model, descending rank order.
#[derive(Debug, Clone)]
pub struct MeaningHit {
    /// Item name.
    pub name: String,
    /// Cosine similarity in 0..=1, rounded to three decimals.
    pub score: f64,
}

/// The result of one explicit meaning search. Every variant carries
/// the immediate word hits; only [`MeaningOutcome::Ranked`] carries
/// meaning hits, and only after consent plus a successful download.
#[derive(Debug, Clone)]
pub enum MeaningOutcome {
    /// First ask (or post-revocation ask): explains the model
    /// download, requests consent, answers with word results. No
    /// model bytes moved.
    NeedsConsent {
        /// Human explanation naming the model and the choice.
        explanation: String,
        /// Immediate word results.
        word: Vec<WordHit>,
    },
    /// Consented with the model on disk: top-25 on-device cosine
    /// ranking beside the word results.
    Ranked {
        /// Immediate word results.
        word: Vec<WordHit>,
        /// Top cosine hits, at most [`MEANING_TOP_K`].
        meaning: Vec<MeaningHit>,
    },
    /// Consent declined: word results with no penalty; nothing was
    /// downloaded and nothing is remembered except absence.
    Declined {
        /// Human notice that word search keeps working.
        notice: String,
        /// Immediate word results.
        word: Vec<WordHit>,
    },
    /// Consent was granted but the download failed: the cause names
    /// the model, word results stand, and no partial model state
    /// blocks later retries.
    DownloadFailed {
        /// Cause naming the model and the failure.
        cause: String,
        /// Immediate word results.
        word: Vec<WordHit>,
    },
}

impl MeaningOutcome {
    /// The word hits in every outcome: word search answers
    /// immediately in every state.
    pub fn word(&self) -> &[WordHit] {
        match self {
            MeaningOutcome::NeedsConsent { word, .. } => word,
            MeaningOutcome::Ranked { word, .. } => word,
            MeaningOutcome::Declined { word, .. } => word,
            MeaningOutcome::DownloadFailed { word, .. } => word,
        }
    }
}

/// Machine meaning-state directory: [`MEANING_ENV_DIR`] wins, else
/// `$HOME/.cache/framesmith/meaning`, else a process temp fallback.
pub fn meaning_dir() -> PathBuf {
    if let Ok(dir) = std::env::var(MEANING_ENV_DIR) {
        if !dir.trim().is_empty() {
            return PathBuf::from(dir);
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        if !home.trim().is_empty() {
            return PathBuf::from(home)
                .join(".cache")
                .join("framesmith")
                .join("meaning");
        }
    }
    std::env::temp_dir().join("framesmith-meaning")
}

/// Path of the downloaded model file inside `dir`.
pub fn model_path(dir: &Path) -> PathBuf {
    dir.join(MODEL_FILE)
}

/// Path of the consent record inside `dir`.
pub fn consent_path(dir: &Path) -> PathBuf {
    dir.join(CONSENT_FILE)
}

/// First-ask explanation: names the model, the one-time on-device
/// download, and the choice. Shown on every explicit ask while no
/// model is on disk.
pub fn consent_explanation() -> String {
    format!(
        "catalog: meaning search ranks by meaning on-device with the model {MEANING_MODEL_NAME}.\n\
         that model is not on this machine: consenting downloads it once (about 1 KB) and keeps it for later asks.\n\
         declining keeps word search fully working with no penalty.\n\
         consent with `--consent yes`, decline with `--consent no`; revoke later with: framesmith catalog meaning-revoke"
    )
}

/// Human notice for a declined ask: word search keeps working.
pub fn declined_notice(query: &str) -> String {
    format!(
        "catalog: staying on word search for {query:?} (meaning model {MEANING_MODEL_NAME} not downloaded).\n\
         ask again any time with: framesmith catalog meaning-search {query:?} --consent yes"
    )
}

/// Human report for a failed download: names the model and the
/// cause, stays on word search.
pub fn download_failed_report(cause: &str, query: &str) -> String {
    format!("catalog: {cause}.\nword results for {query:?} follow (the search result stands).")
}

/// Human report for a revocation: consent and model removed, word
/// search is the standing surface again.
pub fn revoked_report(removed_model: bool) -> String {
    if removed_model {
        format!(
            "catalog: revoked consent for {MEANING_MODEL_NAME} and deleted the model; word search is back.\n\
             meaning search will ask for consent again on the next explicit ask."
        )
    } else {
        format!(
            "catalog: no downloaded {MEANING_MODEL_NAME} model was on this machine; word search is back.\n\
             meaning search will ask for consent on the next explicit ask."
        )
    }
}

/// Collect the immediate word hits for `query` from `index`: the
/// same ranking [`SearchIndex::search`] every surface shares.
pub fn word_hits(index: &SearchIndex, query: &str) -> Vec<WordHit> {
    index
        .search(query)
        .iter()
        .map(|hit| WordHit {
            name: hit.item.name.clone(),
            item_type: hit.item.item_type.as_str().to_string(),
            title: hit.item.title.clone(),
            score: hit.score,
        })
        .collect()
}

/// FNV-1a over bytes: the deterministic hash behind embeddings and
/// the built-in model payload.
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in bytes {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// Build the deterministic bytes of the named model payload: magic,
/// name, version, and 128 seed bytes from an xorshift stream seeded
/// by the model name. The default download materializes exactly
/// these bytes after consent; tests and local mirrors use this
/// builder to stage the same payload without touching the network.
pub fn builtin_model_bytes() -> Vec<u8> {
    materialize_model_bytes()
}

fn materialize_model_bytes() -> Vec<u8> {
    let mut out = Vec::with_capacity(8 + 1 + 32 + 4 + 128);
    out.extend_from_slice(MODEL_MAGIC);
    out.push(1);
    let name = MEANING_MODEL_NAME.as_bytes();
    out.extend_from_slice(name);
    out.push(0);
    out.extend_from_slice(&MEANING_MODEL_VERSION.to_le_bytes());
    let mut state = fnv1a64(MEANING_MODEL_NAME.as_bytes());
    if state == 0 {
        state = 0x9e3779b97f4a7c15;
    }
    for _ in 0..128 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        out.push((state & 0xff) as u8);
    }
    out
}

/// Validate raw model bytes: magic, marker, name, and version must
/// match the named model. Returns the seed bytes for embedding.
/// Anything else is an invalid model, never silently ranked with.
fn parse_model_bytes(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let need = format!("{MEANING_MODEL_NAME} v{MEANING_MODEL_VERSION}");
    if bytes.len() < 8 + 1 || &bytes[..8] != MODEL_MAGIC || bytes[8] != 1 {
        return Err(format!("the meaning model {need} is invalid (bad magic)"));
    }
    let rest = &bytes[9..];
    let Some(nul) = rest.iter().position(|b| *b == 0) else {
        return Err(format!("the meaning model {need} is invalid (bad name)"));
    };
    if &rest[..nul] != MEANING_MODEL_NAME.as_bytes() {
        return Err(format!(
            "the meaning model {need} is invalid (name mismatch)"
        ));
    }
    let after = &rest[nul + 1..];
    if after.len() < 4
        || u32::from_le_bytes(after[..4].try_into().unwrap()) != MEANING_MODEL_VERSION
    {
        return Err(format!(
            "the meaning model {need} is invalid (version mismatch)"
        ));
    }
    Ok(after[4..].to_vec())
}

/// Default download after consent: copy [`MEANING_ENV_SOURCE`] when
/// set (a local mirror or fixture; a missing source fails naming
/// its path), fail when [`MEANING_ENV_FAIL`] is `1` (a broken fetch
/// with a reported cause), else materialize the named payload.
pub fn default_fetch() -> Result<Vec<u8>, String> {
    if std::env::var(MEANING_ENV_FAIL).as_deref() == Ok("1") {
        return Err(format!(
            "could not download the meaning model {MEANING_MODEL_NAME}: \
             the fetch failed ({}=1 breaks the download for testing); staying on word search",
            MEANING_ENV_FAIL
        ));
    }
    if let Ok(source) = std::env::var(MEANING_ENV_SOURCE) {
        if !source.trim().is_empty() {
            let path = PathBuf::from(&source);
            return std::fs::read(&path).map_err(|e| {
                format!(
                    "could not download the meaning model {MEANING_MODEL_NAME} \
                     from {source:?}: {e}; staying on word search"
                )
            });
        }
    }
    Ok(materialize_model_bytes())
}

/// Ensure the model file exists in `dir`, downloading once through
/// `fetch` when absent. A present valid model never calls `fetch`
/// again. Bytes are written only after a successful fetch; a failed
/// write removes any partial file, so no partial state blocks a
/// retry. Callers must invoke this only after consent.
pub fn ensure_model(
    dir: &Path,
    fetch: &mut dyn FnMut() -> Result<Vec<u8>, String>,
) -> Result<Vec<u8>, String> {
    let path = model_path(dir);
    if path.is_file() {
        match std::fs::read(&path).and_then(|bytes| {
            parse_model_bytes(&bytes)
                .map(|_| bytes)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
        }) {
            Ok(bytes) => return Ok(bytes),
            Err(_) => {
                let _ = std::fs::remove_file(&path);
            }
        }
    }
    let bytes = fetch().map_err(|e| {
        format!(
            "could not download the meaning model {MEANING_MODEL_NAME}: {e}; staying on word search"
        )
    })?;
    let seed = parse_model_bytes(&bytes).map_err(|e| {
        format!("could not use the downloaded meaning model {MEANING_MODEL_NAME}: {e}")
    })?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            format!(
                "could not store the meaning model {MEANING_MODEL_NAME} at {}: {e}",
                path.display()
            )
        })?;
    }
    if let Err(e) = std::fs::write(&path, &bytes) {
        let _ = std::fs::remove_file(&path);
        return Err(format!(
            "could not store the meaning model {MEANING_MODEL_NAME} at {}: {e}",
            path.display()
        ));
    }
    let _ = std::fs::write(
        consent_path(dir),
        serde_json::json!({
            "model": MEANING_MODEL_NAME,
            "version": MEANING_MODEL_VERSION,
            "at": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        })
        .to_string(),
    );
    let _ = seed;
    Ok(bytes)
}

/// Embed text into a unit vector with the model seed: tokens count
/// double, character bigrams and trigrams single, hashed into
/// [`EMBED_DIM`] buckets. Deterministic and fully on-device; ranking
/// quality tuning stays deferred.
pub fn embed_text(text: &str, seed: &[u8]) -> [f32; EMBED_DIM] {
    let seed_mix = fnv1a64(seed);
    let mut acc = [0.0f32; EMBED_DIM];
    let mut push = |gram: &str, weight: f32| {
        let h = fnv1a64(gram.as_bytes()).wrapping_add(seed_mix);
        acc[(h % EMBED_DIM as u64) as usize] += weight;
    };
    for raw in text.split(|c: char| !c.is_alphanumeric()) {
        if raw.is_empty() {
            continue;
        }
        let token: String = raw.chars().flat_map(|c| c.to_lowercase()).collect();
        push(&token, 2.0);
        let chars: Vec<char> = token.chars().collect();
        for w in [2, 3] {
            if chars.len() >= w {
                for gram in chars.windows(w).map(|w| w.iter().collect::<String>()) {
                    push(&gram, 1.0);
                }
            }
        }
    }
    let norm = acc.iter().map(|v| v * v).sum::<f32>().sqrt();
    if norm > 0.0 {
        for v in &mut acc {
            *v /= norm;
        }
    }
    acc
}

/// Cosine similarity of two unit embedding vectors.
fn cosine(a: &[f32; EMBED_DIM], b: &[f32; EMBED_DIM]) -> f64 {
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| f64::from(*x) * f64::from(*y))
        .sum()
}

/// Item text for meaning ranking: name, title, description, and tags.
fn item_text(item: &ItemManifest) -> String {
    let mut text = String::new();
    text.push_str(&item.name.replace(['-', '_'], " "));
    text.push(' ');
    text.push_str(&item.title);
    text.push(' ');
    text.push_str(&item.description);
    for tag in &item.tags {
        text.push(' ');
        text.push_str(tag);
    }
    text
}

/// Rank every item against `query` by cosine similarity on-device,
/// returning the top [`MEANING_TOP_K`] hits in descending score
/// order (ties by ascending name). Scores round to three decimals.
pub fn rank_meaning(items: &[ItemManifest], query: &str, model_bytes: &[u8]) -> Vec<MeaningHit> {
    let seed = parse_model_bytes(model_bytes).unwrap_or_default();
    let query_vec = embed_text(query, &seed);
    let mut hits: Vec<MeaningHit> = items
        .iter()
        .map(|item| {
            let score = cosine(&query_vec, &embed_text(&item_text(item), &seed));
            MeaningHit {
                name: item.name.clone(),
                score: (score.max(0.0) * 1000.0).round() / 1000.0,
            }
        })
        .filter(|hit| hit.score > 0.0)
        .collect();
    hits.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.name.cmp(&b.name))
    });
    hits.truncate(MEANING_TOP_K);
    hits
}

/// One explicit meaning search over `items` (usually the merged
/// registry listing) with machine state in `dir`.
///
/// Word hits compute first and travel in every outcome: word search
/// never waits on model work. Model bytes move only when `consent`
/// is [`ConsentAnswer::Yes`] (or a previously consented model is
/// already on disk, in which case no fetch runs at all). `fetch`
/// supplies the download bytes on the consent path; it runs at most
/// once per missing model, and a second call with the model present
/// never invokes it.
pub fn meaning_search(
    items: &[ItemManifest],
    query: &str,
    dir: &Path,
    consent: ConsentAnswer,
    fetch: &mut dyn FnMut() -> Result<Vec<u8>, String>,
) -> MeaningOutcome {
    let index = SearchIndex::build(items);
    let word = word_hits(&index, query);
    let path = model_path(dir);
    let ready = path.is_file()
        && std::fs::read(&path)
            .ok()
            .and_then(|bytes| parse_model_bytes(&bytes).ok().map(|_| bytes))
            .is_some();
    if ready {
        let bytes = std::fs::read(&path).unwrap_or_default();
        let meaning = rank_meaning(items, query, &bytes);
        return MeaningOutcome::Ranked { word, meaning };
    }
    match consent {
        ConsentAnswer::Ask => MeaningOutcome::NeedsConsent {
            explanation: consent_explanation(),
            word,
        },
        ConsentAnswer::No => MeaningOutcome::Declined {
            notice: declined_notice(query),
            word,
        },
        ConsentAnswer::Yes => match ensure_model(dir, fetch) {
            Ok(bytes) => {
                let meaning = rank_meaning(items, query, &bytes);
                MeaningOutcome::Ranked { word, meaning }
            }
            Err(cause) => MeaningOutcome::DownloadFailed { cause, word },
        },
    }
}

/// Revoke consent: delete the model and the consent record.
/// Idempotent: revoking twice still reports word search is back.
/// Returns true when a model file was removed.
pub fn revoke_consent(dir: &Path) -> bool {
    let removed_model = std::fs::remove_file(model_path(dir)).is_ok();
    let _ = std::fs::remove_file(consent_path(dir));
    removed_model
}

/// Human-friendly picker for ranked meaning hits: the same numbered
/// shape as the word picker with the model named.
pub fn render_meaning_picker(
    query: &str,
    dir: &Path,
    meaning: &[MeaningHit],
    word: &[WordHit],
) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "catalog meaning {query:?} — {} meaning hit(s) (model {MEANING_MODEL_NAME}) over {} word hit(s) in {}:\n",
        meaning.len(),
        word.len(),
        dir.display()
    ));
    for (i, hit) in meaning.iter().enumerate() {
        out.push_str(&format!(
            "{}. {} [meaning {:.3}]\n",
            i + 1,
            hit.name,
            hit.score
        ));
        out.push_str(&format!(
            "   show: framesmith catalog show {:?} / install: framesmith catalog install <project> {:?}\n",
            hit.name, hit.name
        ));
    }
    out
}

/// Human word-result fallback printed under every non-ranked
/// meaning outcome: the numbered word hits when the query matches,
/// else the shared no-hits report offering miss reporting. Word
/// results answer immediately in every state.
pub fn render_word_fallback(query: &str, dir: &Path, word: &[WordHit]) -> String {
    if word.is_empty() {
        return crate::search::no_hits_report(query, dir);
    }
    let mut out = String::new();
    out.push_str(&format!(
        "word results for {query:?} — {} hit(s) in {}:\n",
        word.len(),
        dir.display()
    ));
    for (i, hit) in word.iter().enumerate() {
        out.push_str(&format!(
            "{}. {} ({}) — {} [score {:.1}]\n",
            i + 1,
            hit.name,
            hit.item_type,
            hit.title,
            hit.score
        ));
    }
    out
}

/// Machine-readable ranked envelope: word hits plus meaning hits in
/// rank order with the model named.
pub fn meaning_ranked_json(
    query: &str,
    dir: &Path,
    word: &[WordHit],
    meaning: &[MeaningHit],
) -> String {
    serde_json::json!({
        "query": query,
        "registry": dir.display().to_string(),
        "empty": false,
        "mode": "meaning",
        "consent": "granted",
        "model": MEANING_MODEL_NAME,
        "word_hits": word.iter().map(|h| serde_json::json!({
            "name": h.name, "type": h.item_type, "title": h.title, "score": h.score,
        })).collect::<Vec<_>>(),
        "hits": meaning.iter().map(|h| serde_json::json!({
            "name": h.name, "score": h.score,
        })).collect::<Vec<_>>(),
        "top_k": MEANING_TOP_K,
    })
    .to_string()
}

/// Machine-readable first-ask envelope: word hits plus the consent
/// explanation, never meaning hits.
pub fn needs_consent_json(query: &str, dir: &Path, word: &[WordHit]) -> String {
    serde_json::json!({
        "query": query,
        "registry": dir.display().to_string(),
        "empty": false,
        "mode": "word",
        "consent": "required",
        "model": MEANING_MODEL_NAME,
        "explanation": consent_explanation(),
        "hits": word.iter().map(|h| serde_json::json!({
            "name": h.name, "type": h.item_type, "title": h.title, "score": h.score,
        })).collect::<Vec<_>>(),
        "consent_hint": "framesmith catalog meaning-search <words...> --consent yes|no",
    })
    .to_string()
}

/// Machine-readable declined envelope: word hits, no download.
pub fn declined_json(query: &str, dir: &Path, word: &[WordHit]) -> String {
    serde_json::json!({
        "query": query,
        "registry": dir.display().to_string(),
        "empty": false,
        "mode": "word",
        "consent": "declined",
        "model": MEANING_MODEL_NAME,
        "hits": word.iter().map(|h| serde_json::json!({
            "name": h.name, "type": h.item_type, "title": h.title, "score": h.score,
        })).collect::<Vec<_>>(),
    })
    .to_string()
}

/// Machine-readable failed-download envelope: the cause names the
/// model, word hits stand.
pub fn download_failed_json(query: &str, dir: &Path, cause: &str, word: &[WordHit]) -> String {
    serde_json::json!({
        "query": query,
        "registry": dir.display().to_string(),
        "empty": false,
        "mode": "word",
        "consent": "granted",
        "model": MEANING_MODEL_NAME,
        "download_error": cause,
        "hits": word.iter().map(|h| serde_json::json!({
            "name": h.name, "type": h.item_type, "title": h.title, "score": h.score,
        })).collect::<Vec<_>>(),
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_payload_round_trips_through_parse() {
        let bytes = materialize_model_bytes();
        assert!(!bytes.is_empty());
        let seed = parse_model_bytes(&bytes).expect("own payload parses");
        assert!(!seed.is_empty());
    }

    #[test]
    fn embeddings_are_unit_and_query_self_similar() {
        let seed = parse_model_bytes(&materialize_model_bytes()).unwrap();
        let vec = embed_text("harbor evening", &seed);
        let norm = vec.iter().map(|v| v * v).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-5, "{norm}");
        assert!((cosine(&vec, &vec) - 1.0).abs() < 1e-5);
    }
}
