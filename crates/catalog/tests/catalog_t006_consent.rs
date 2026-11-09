//! Sprint 19 T006: consent-gated meaning search over the catalog.
//!
//! The first meaning ask explains the model download and requests
//! consent; no bytes move before consent, consenting downloads once
//! and ranks the top 25 by cosine similarity on-device, declining
//! keeps word search with no penalty, word results answer
//! immediately in every state, a failed download reports its cause
//! on word search, and revocation returns to word search. Every
//! fixture below is our own clean-room content.

use catalog::{
    declined_json, download_failed_json, ensure_model, load_search_index, meaning_ranked_json,
    meaning_search, model_path, needs_consent_json, revoke_consent, ConsentAnswer, MeaningOutcome,
    MEANING_MODEL_NAME, MEANING_TOP_K,
};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-n19-t006-{tag}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn manifest(name: &str, title: &str, description: &str, tags: &[&str], dir: &Path) {
    let tags_json = tags
        .iter()
        .map(|t| format!("{t:?}"))
        .collect::<Vec<_>>()
        .join(",");
    let text = format!(
        r#"{{"name": {name:?}, "type": "block", "title": {title:?}, "description": {description:?},
            "files": [{{"src": "composition.kdl", "target": "composition.kdl"}}],
            "width": 1280, "height": 720, "duration": 4, "tags": [{tags_json}]}}"#
    );
    let folder = dir.join(name);
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("manifest.json"), text).unwrap();
}

/// A small original registry: `harbor-light` matches `harbor` by
/// name, `evening-tide` by title, `still-water` by tag.
fn sample_registry() -> PathBuf {
    let dir = tmp("registry");
    manifest(
        "harbor-light",
        "Lantern Room",
        "An original miniature about a keeper polishing lenses.",
        &["beacons"],
        &dir,
    );
    manifest(
        "evening-tide",
        "Harbor Study",
        "A quiet sketch of gulls over low water.",
        &["sketches"],
        &dir,
    );
    manifest(
        "still-water",
        "Pebble Sorting",
        "A slow inventory of stones by weight.",
        &["harbor"],
        &dir,
    );
    dir
}

fn items_of(dir: &Path) -> Vec<catalog::ItemManifest> {
    load_search_index(dir).0.items.clone()
}

fn failing_fetch(message: &str) -> impl FnMut() -> Result<Vec<u8>, String> + '_ {
    move || Err(message.to_string())
}

fn panicking_fetch() -> impl FnMut() -> Result<Vec<u8>, String> {
    move || panic!("fetch must not run before consent or after a completed download")
}

#[test]
fn first_ask_explains_and_moves_no_bytes() {
    let reg = sample_registry();
    let items = items_of(&reg);
    let state = tmp("state");
    let mut fetch = panicking_fetch();
    let outcome = meaning_search(&items, "harbor", &state, ConsentAnswer::Ask, &mut fetch);
    let (explanation, word) = match outcome {
        MeaningOutcome::NeedsConsent { explanation, word } => (explanation, word),
        other => panic!("expected NeedsConsent, got {other:?}"),
    };
    assert!(explanation.contains(MEANING_MODEL_NAME), "{explanation}");
    assert!(explanation.contains("--consent yes"), "{explanation}");
    assert!(!word.is_empty(), "word results answer immediately");
    assert!(
        !model_path(&state).exists(),
        "no model bytes before consent"
    );
}

#[test]
fn decline_moves_no_bytes_and_keeps_word_search() {
    let reg = sample_registry();
    let items = items_of(&reg);
    let state = tmp("state");
    let expected: Vec<String> = load_search_index(&reg)
        .1
        .search("harbor")
        .iter()
        .map(|h| h.item.name.clone())
        .collect();
    let mut fetch = panicking_fetch();
    let outcome = meaning_search(&items, "harbor", &state, ConsentAnswer::No, &mut fetch);
    let (notice, word) = match outcome {
        MeaningOutcome::Declined { notice, word } => (notice, word),
        other => panic!("expected Declined, got {other:?}"),
    };
    assert!(notice.contains("word search"), "{notice}");
    assert!(!model_path(&state).exists(), "declining downloads nothing");
    let got: Vec<String> = word.iter().map(|h| h.name.clone()).collect();
    assert_eq!(got, expected, "declining keeps word search with no penalty");
}

#[test]
fn word_results_answer_without_model_work_in_every_state() {
    let reg = sample_registry();
    let items = items_of(&reg);
    let state = tmp("state");
    // Ask and decline must answer even when the fetch path would
    // explode: word search never waits on model work.
    for consent in [ConsentAnswer::Ask, ConsentAnswer::No] {
        let mut fetch = panicking_fetch();
        let outcome = meaning_search(&items, "harbor", &state, consent, &mut fetch);
        assert!(
            !outcome.word().is_empty(),
            "word answers immediately for {consent:?}"
        );
    }
    // A failed download still carries word results.
    let mut fetch = failing_fetch("boom");
    let outcome = meaning_search(&items, "harbor", &state, ConsentAnswer::Yes, &mut fetch);
    match &outcome {
        MeaningOutcome::DownloadFailed { word, .. } => assert!(!word.is_empty()),
        other => panic!("expected DownloadFailed, got {other:?}"),
    }
    assert!(!outcome.word().is_empty());
}

#[test]
fn consent_downloads_once_then_ranks_top_25_on_device() {
    let reg = sample_registry();
    let items = items_of(&reg);
    let state = tmp("state");
    let mut calls = 0usize;
    let mut fetch = || -> Result<Vec<u8>, String> {
        calls += 1;
        Ok(catalog::builtin_model_bytes())
    };
    let outcome = meaning_search(&items, "harbor", &state, ConsentAnswer::Yes, &mut fetch);
    let (word, meaning) = match outcome {
        MeaningOutcome::Ranked { word, meaning } => (word, meaning),
        other => panic!("expected Ranked, got {other:?}"),
    };
    assert_eq!(calls, 1, "consent downloads exactly once");
    assert!(!word.is_empty(), "ranked answers carry word hits");
    assert!(!meaning.is_empty(), "consented search ranks by meaning");
    assert!(meaning.len() <= MEANING_TOP_K);
    let mut sorted = meaning.clone();
    sorted.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap()
            .then_with(|| a.name.cmp(&b.name))
    });
    let names: Vec<&str> = meaning.iter().map(|h| h.name.as_str()).collect();
    let want: Vec<&str> = sorted.iter().map(|h| h.name.as_str()).collect();
    assert_eq!(names, want, "meaning ranks by descending cosine");
    assert!(model_path(&state).is_file(), "model stored after consent");
    assert!(
        catalog::consent_path(&state).is_file(),
        "consent recorded after download"
    );
    // A second consented search reuses the stored model: the fetch
    // path must not run again.
    let mut fetch = panicking_fetch();
    let outcome = meaning_search(&items, "harbor", &state, ConsentAnswer::Yes, &mut fetch);
    assert!(
        matches!(outcome, MeaningOutcome::Ranked { .. }),
        "stored model ranks without redownloading"
    );
}

#[test]
fn meaning_ranking_caps_at_top_25() {
    let dir = tmp("many");
    for i in 0..30 {
        manifest(
            &format!("lantern-study-{i:02}"),
            &format!("Lantern Study Number {i}"),
            &format!("An original miniature of lantern light, variant {i}, glowing softly over still water."),
            &["lanterns"],
            &dir,
        );
    }
    let items = items_of(&dir);
    let state = tmp("state");
    let mut calls = 0usize;
    let mut fetch = || -> Result<Vec<u8>, String> {
        calls += 1;
        Ok(catalog::builtin_model_bytes())
    };
    let outcome = meaning_search(&items, "lantern", &state, ConsentAnswer::Yes, &mut fetch);
    let meaning = match outcome {
        MeaningOutcome::Ranked { meaning, .. } => meaning,
        other => panic!("expected Ranked, got {other:?}"),
    };
    assert_eq!(calls, 1);
    assert_eq!(meaning.len(), MEANING_TOP_K, "ranking caps at top 25");
    for pair in meaning.windows(2) {
        assert!(pair[0].score >= pair[1].score, "descending cosine order");
    }
}

#[test]
fn failed_download_reports_cause_on_word_search() {
    let reg = sample_registry();
    let items = items_of(&reg);
    let state = tmp("state");
    let mut fetch = failing_fetch("network unreachable");
    let outcome = meaning_search(&items, "harbor", &state, ConsentAnswer::Yes, &mut fetch);
    let (cause, word) = match outcome {
        MeaningOutcome::DownloadFailed { cause, word } => (cause, word),
        other => panic!("expected DownloadFailed, got {other:?}"),
    };
    assert!(cause.contains(MEANING_MODEL_NAME), "{cause}");
    assert!(cause.contains("network unreachable"), "{cause}");
    assert!(!word.is_empty(), "failed download stays on word search");
    assert!(
        !model_path(&state).exists(),
        "no partial model state blocks a retry"
    );
}

#[test]
fn corrupt_download_is_refused_by_name() {
    let reg = sample_registry();
    let items = items_of(&reg);
    let state = tmp("state");
    let mut fetch = || -> Result<Vec<u8>, String> { Ok(b"not-a-model".to_vec()) };
    let outcome = meaning_search(&items, "harbor", &state, ConsentAnswer::Yes, &mut fetch);
    match &outcome {
        MeaningOutcome::DownloadFailed { cause, word } => {
            assert!(cause.contains(MEANING_MODEL_NAME), "{cause}");
            assert!(!word.is_empty());
        }
        other => panic!("expected DownloadFailed, got {other:?}"),
    }
    assert!(!model_path(&state).exists());
}

#[test]
fn revocation_returns_to_word_search() {
    let reg = sample_registry();
    let items = items_of(&reg);
    let state = tmp("state");
    let mut fetch = || -> Result<Vec<u8>, String> { Ok(catalog::builtin_model_bytes()) };
    let outcome = meaning_search(&items, "harbor", &state, ConsentAnswer::Yes, &mut fetch);
    assert!(matches!(outcome, MeaningOutcome::Ranked { .. }));
    assert!(revoke_consent(&state), "revocation removes the model");
    assert!(!model_path(&state).exists());
    let mut fetch = panicking_fetch();
    let outcome = meaning_search(&items, "harbor", &state, ConsentAnswer::Ask, &mut fetch);
    match outcome {
        MeaningOutcome::NeedsConsent { explanation, word } => {
            assert!(explanation.contains(MEANING_MODEL_NAME));
            assert!(!word.is_empty());
        }
        other => panic!("expected NeedsConsent after revoke, got {other:?}"),
    }
    assert!(!revoke_consent(&state), "revoking twice stays quiet");
}

#[test]
fn default_fetch_breaks_loudly_through_env() {
    std::env::set_var("FRAMESMITH_MEANING_FAIL", "1");
    let failed = catalog::default_fetch();
    std::env::remove_var("FRAMESMITH_MEANING_FAIL");
    let Err(cause) = failed else {
        panic!("expected the forced failure");
    };
    assert!(cause.contains(MEANING_MODEL_NAME), "{cause}");

    std::env::set_var(
        "FRAMESMITH_MEANING_SOURCE",
        "/nonexistent-framesmith-meaning-source.bin",
    );
    let missing = catalog::default_fetch();
    std::env::remove_var("FRAMESMITH_MEANING_SOURCE");
    let Err(cause) = missing else {
        panic!("expected the missing source to fail");
    };
    assert!(cause.contains(MEANING_MODEL_NAME), "{cause}");
    assert!(
        cause.contains("nonexistent-framesmith-meaning-source"),
        "{cause}"
    );
}

#[test]
fn ensure_model_writes_nothing_on_failure() {
    let state = tmp("state");
    let mut fetch = failing_fetch("offline");
    let Err(cause) = ensure_model(&state, &mut fetch) else {
        panic!("expected failure");
    };
    assert!(cause.contains(MEANING_MODEL_NAME), "{cause}");
    assert!(!model_path(&state).exists());
}

#[test]
fn json_envelopes_name_the_model_and_never_fake_meaning() {
    let reg = sample_registry();
    let search = load_search_index(&reg);
    let word = catalog::word_hits(&search.1, "harbor");
    let ask: serde_json::Value =
        serde_json::from_str(&needs_consent_json("harbor", &reg, &word)).unwrap();
    assert_eq!(ask["consent"], serde_json::json!("required"));
    assert_eq!(ask["mode"], serde_json::json!("word"));
    assert_eq!(ask["model"], serde_json::json!(MEANING_MODEL_NAME));
    assert!(ask.get("hits").is_some());
    assert!(ask.get("meaning").is_none(), "no unranked meaning hits");

    let state = tmp("state");
    let mut fetch = || -> Result<Vec<u8>, String> { Ok(catalog::builtin_model_bytes()) };
    let outcome = meaning_search(
        &search.0.items,
        "harbor",
        &state,
        ConsentAnswer::Yes,
        &mut fetch,
    );
    let (word, meaning) = match outcome {
        MeaningOutcome::Ranked { word, meaning } => (word, meaning),
        other => panic!("expected Ranked, got {other:?}"),
    };
    let ranked: serde_json::Value =
        serde_json::from_str(&meaning_ranked_json("harbor", &reg, &word, &meaning)).unwrap();
    assert_eq!(ranked["mode"], serde_json::json!("meaning"));
    assert_eq!(ranked["consent"], serde_json::json!("granted"));
    assert_eq!(ranked["model"], serde_json::json!(MEANING_MODEL_NAME));
    assert_eq!(ranked["top_k"], serde_json::json!(MEANING_TOP_K));

    let failed: serde_json::Value = serde_json::from_str(&download_failed_json(
        "harbor",
        &reg,
        &format!("could not download the meaning model {MEANING_MODEL_NAME}: boom"),
        &word,
    ))
    .unwrap();
    assert_eq!(failed["mode"], serde_json::json!("word"));
    assert!(failed["download_error"]
        .as_str()
        .unwrap()
        .contains(MEANING_MODEL_NAME));

    let declined: serde_json::Value =
        serde_json::from_str(&declined_json("harbor", &reg, &word)).unwrap();
    assert_eq!(declined["consent"], serde_json::json!("declined"));
    assert_eq!(declined["mode"], serde_json::json!("word"));
}
