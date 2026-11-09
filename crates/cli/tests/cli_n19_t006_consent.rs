//! Sprint 19 T006: `framesmith catalog meaning-search|meaning-revoke`
//! end to end over a fixture registry.
//!
//! The first meaning ask explains the model download and requests
//! consent; `--consent no` keeps word search with no download,
//! `--consent yes` downloads the named model once and ranks
//! on-device, a broken download reports its cause on word search,
//! and `meaning-revoke` returns to word search. Every fixture is our
//! own clean-room content.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-n19-t006-cli-{tag}-{}-{}",
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

fn run(args: &[&str], cwd: &Path, meaning_dir: &Path, extra_env: &[(&str, &str)]) -> (i32, String) {
    let mut cmd = Command::new(bin());
    cmd.args(args)
        .current_dir(cwd)
        .env("FRAMESMITH_MEANING_DIR", meaning_dir);
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("run framesmith binary");
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    (out.status.code().unwrap_or(-1), text)
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

/// Fixture registry with distinct words per field, plus a root dir
/// to run the binary from.
fn sample_project() -> (PathBuf, PathBuf) {
    let root = tmp("proj");
    let dir = root.join("registry");
    std::fs::create_dir_all(&dir).unwrap();
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
    (root, dir)
}

const MODEL: &str = "framesmith-meaning-tiny-v1";

#[test]
fn first_ask_explains_requests_consent_and_downloads_nothing() {
    let (root, reg) = sample_project();
    let reg = reg.display().to_string();
    let state = tmp("state");
    let (code, text) = run(
        &["catalog", "meaning-search", "harbor", "--registry", &reg],
        &root,
        &state,
        &[],
    );
    assert_eq!(code, 0, "{text}");
    assert!(text.contains(MODEL), "{text}");
    assert!(text.contains("--consent yes"), "{text}");
    assert!(text.contains("harbor-light"), "{text}");
    assert!(
        !state.join("model.bin").exists(),
        "no bytes move before consent"
    );
}

#[test]
fn decline_keeps_word_search_with_no_download() {
    let (root, reg) = sample_project();
    let reg = reg.display().to_string();
    let state = tmp("state");
    let (code, declined) = run(
        &[
            "catalog",
            "meaning-search",
            "harbor",
            "--consent",
            "no",
            "--registry",
            &reg,
        ],
        &root,
        &state,
        &[],
    );
    assert_eq!(code, 0, "{declined}");
    assert!(declined.contains("word search"), "{declined}");
    assert!(declined.contains("harbor-light"), "{declined}");
    assert!(
        !state.join("model.bin").exists(),
        "declining downloads nothing"
    );
    // Word order matches plain word search: no penalty for declining.
    let (_, words) = run(
        &["catalog", "search", "harbor", "--registry", &reg],
        &root,
        &state,
        &[],
    );
    for name in ["harbor-light", "evening-tide", "still-water"] {
        assert_eq!(
            declined.contains(name),
            words.contains(name),
            "declined word set matches word search for {name}"
        );
    }
}

#[test]
fn consent_downloads_once_and_ranks_on_device() {
    let (root, reg) = sample_project();
    let reg = reg.display().to_string();
    let state = tmp("state");
    let (code, text) = run(
        &[
            "catalog",
            "meaning-search",
            "harbor",
            "--consent",
            "yes",
            "--registry",
            &reg,
        ],
        &root,
        &state,
        &[],
    );
    assert_eq!(code, 0, "{text}");
    assert!(text.contains("meaning"), "{text}");
    assert!(text.contains(MODEL), "{text}");
    assert!(text.contains("harbor-light"), "{text}");
    assert!(state.join("model.bin").is_file(), "model stored once");
    assert!(state.join("consent.json").is_file(), "consent recorded");
    // A later ask without flags reuses the stored model: it ranks
    // instead of asking again, proving the one-time download.
    let (code, again) = run(
        &["catalog", "meaning-search", "harbor", "--registry", &reg],
        &root,
        &state,
        &[],
    );
    assert_eq!(code, 0, "{again}");
    assert!(again.contains("meaning"), "{again}");
    assert!(!again.contains("--consent yes"), "{again}");
}

#[test]
fn broken_download_reports_cause_and_stays_on_word_search() {
    let (root, reg) = sample_project();
    let reg = reg.display().to_string();
    let state = tmp("state");
    let (code, text) = run(
        &[
            "catalog",
            "meaning-search",
            "harbor",
            "--consent",
            "yes",
            "--registry",
            &reg,
        ],
        &root,
        &state,
        &[("FRAMESMITH_MEANING_FAIL", "1")],
    );
    assert_eq!(code, 0, "{text}");
    assert!(text.contains(MODEL), "the failure names the model: {text}");
    assert!(text.contains("harbor-light"), "word results stand: {text}");
    assert!(
        !state.join("model.bin").exists(),
        "no partial model blocks a retry"
    );
}

#[test]
fn revoke_returns_to_word_search() {
    let (root, reg) = sample_project();
    let reg = reg.display().to_string();
    let state = tmp("state");
    let (code, _) = run(
        &[
            "catalog",
            "meaning-search",
            "harbor",
            "--consent",
            "yes",
            "--registry",
            &reg,
        ],
        &root,
        &state,
        &[],
    );
    assert_eq!(code, 0);
    let (code, text) = run(&["catalog", "meaning-revoke"], &root, &state, &[]);
    assert_eq!(code, 0, "{text}");
    assert!(text.contains("word search"), "{text}");
    assert!(!state.join("model.bin").exists(), "model deleted");
    // The next explicit ask explains and requests consent again.
    let (code, text) = run(
        &["catalog", "meaning-search", "harbor", "--registry", &reg],
        &root,
        &state,
        &[],
    );
    assert_eq!(code, 0, "{text}");
    assert!(text.contains("--consent yes"), "{text}");
    // Revoking twice stays quiet and successful.
    let (code, text) = run(&["catalog", "meaning-revoke"], &root, &state, &[]);
    assert_eq!(code, 0, "{text}");
    assert!(text.contains("word search"), "{text}");
}

#[test]
fn json_first_ask_names_model_with_word_hits_only() {
    let (root, reg) = sample_project();
    let reg = reg.display().to_string();
    let state = tmp("state");
    let (code, text) = run(
        &[
            "catalog",
            "meaning-search",
            "harbor",
            "--json",
            "--registry",
            &reg,
        ],
        &root,
        &state,
        &[],
    );
    assert_eq!(code, 0, "{text}");
    let value: serde_json::Value = serde_json::from_str(&text).expect("machine envelope parses");
    assert_eq!(value["consent"], serde_json::json!("required"));
    assert_eq!(value["mode"], serde_json::json!("word"));
    assert_eq!(value["model"], serde_json::json!(MODEL));
    assert!(value.get("meaning").is_none());
}

#[test]
fn json_ranked_envelope_carries_meaning_after_consent() {
    let (root, reg) = sample_project();
    let reg = reg.display().to_string();
    let state = tmp("state");
    let (code, text) = run(
        &[
            "catalog",
            "meaning-search",
            "harbor",
            "--consent",
            "yes",
            "--json",
            "--registry",
            &reg,
        ],
        &root,
        &state,
        &[],
    );
    assert_eq!(code, 0, "{text}");
    let value: serde_json::Value = serde_json::from_str(&text).expect("machine envelope parses");
    assert_eq!(value["mode"], serde_json::json!("meaning"));
    assert_eq!(value["consent"], serde_json::json!("granted"));
    assert_eq!(value["model"], serde_json::json!(MODEL));
    assert!(value["hits"].as_array().unwrap().len() <= 25);
}

#[test]
fn empty_registry_and_usage_refuse() {
    let root = tmp("empty");
    let missing = root.join("no-registry").display().to_string();
    let state = tmp("state");
    let (code, text) = run(
        &[
            "catalog",
            "meaning-search",
            "harbor",
            "--registry",
            &missing,
        ],
        &root,
        &state,
        &[],
    );
    assert_eq!(code, 1, "{text}");
    let (code, _) = run(
        &["catalog", "meaning-search", "--registry", &missing],
        &root,
        &state,
        &[],
    );
    assert_eq!(code, 2);
    let (code, _) = run(
        &[
            "catalog",
            "meaning-search",
            "harbor",
            "--consent",
            "maybe",
            "--registry",
            &missing,
        ],
        &root,
        &state,
        &[],
    );
    assert_eq!(code, 2);
    let (code, _) = run(&["catalog", "meaning-revoke", "extra"], &root, &state, &[]);
    assert_eq!(code, 2);
}
