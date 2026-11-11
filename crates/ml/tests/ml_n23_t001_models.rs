//! Sprint 23, T001: model manager with consent.
//!
//! Covers: `models list` states, no-consent refusal (non-interactive),
//! SHA256 verify-then-cache through the local registry, re-install
//! skip with a note, corrupted-cache discard, offline refusal with
//! the manual-provision fix, unknown-model refusal, disabled-config
//! refusal, and stub-only-behind-override engine gates.

use ml::config::MlConfig;
use ml::models::{
    install_model_with_consent, list_lines, model_status, InstallOutcome, ModelState,
};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);
static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-ml-n23-t001-{tag}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn cfg_for(root: &PathBuf, registry: Option<PathBuf>, offline: bool, auto: bool) -> MlConfig {
    MlConfig {
        cache_dir: root.join("cache"),
        auto_download: auto,
        offline,
        registry_dir: registry,
        download_timeout_secs: 30,
    }
}

const MODEL: &str = "whisper-tiny-en";

fn fixture_bytes() -> Vec<u8> {
    b"framesmith-test-model-bytes-whisper-tiny-en".to_vec()
}

fn fixture_sha() -> String {
    ffmpeg::sha256::sha256_hex(&fixture_bytes())
}

/// Install with a local registry and checksum override; returns the
/// root holding cache + registry.
fn install_ok() -> (PathBuf, MlConfig) {
    let root = tmp("install");
    let reg = root.join("reg");
    std::fs::create_dir_all(&reg).unwrap();
    std::fs::write(reg.join(MODEL), fixture_bytes()).unwrap();
    std::env::set_var("FRAMESMITH_ML_SHA256_WHISPER_TINY_EN", fixture_sha());
    let cfg = cfg_for(&root, Some(reg), false, true);
    match install_model_with_consent(&cfg, MODEL, true) {
        InstallOutcome::Installed { already, .. } => assert!(!already),
        InstallOutcome::Refused { message, .. } => panic!("install refused: {message}"),
    }
    (root, cfg)
}

#[test]
fn list_shows_every_model_missing_on_clean_cache() {
    let _guard = ENV_LOCK.lock().unwrap();
    let root = tmp("list");
    let cfg = cfg_for(&root, None, false, true);
    let lines = list_lines(&cfg);
    assert_eq!(lines.len(), ml::KNOWN_MODELS.len());
    for line in &lines {
        assert!(line.contains("missing"), "{line}");
        assert!(line.contains("framesmith models install"), "{line}");
    }
    assert!(lines.iter().any(|l| l.contains("whisper-tiny-en")));
    assert!(lines.iter().any(|l| l.contains("tts-en-aria")));
    assert!(lines.iter().any(|l| l.contains("rembg-base")));
}

#[test]
fn install_without_consent_refuses_and_downloads_nothing() {
    let _guard = ENV_LOCK.lock().unwrap();
    let root = tmp("noconsent");
    let cfg = cfg_for(&root, None, false, true);
    // Non-interactive stdin (cargo test pipes it) without --yes.
    match install_model_with_consent(&cfg, MODEL, false) {
        InstallOutcome::Refused { code, message } => {
            assert_eq!(code, "E_NO_CONSENT");
            assert!(message.contains("--yes"), "{message}");
        }
        InstallOutcome::Installed { .. } => panic!("installed without consent"),
    }
    assert!(!root.join("cache").exists());
}

#[test]
fn install_verifies_then_caches_and_skips_reinstall() {
    let _guard = ENV_LOCK.lock().unwrap();
    let (root, cfg) = install_ok();
    let cached = cfg.cached_model(MODEL);
    assert!(cached.is_file());
    assert_eq!(std::fs::read(&cached).unwrap(), fixture_bytes());
    assert_eq!(
        model_status(&cfg, &ml::find_model(MODEL).unwrap()),
        ModelState::Ready
    );
    let lines = list_lines(&cfg);
    assert!(
        lines
            .iter()
            .any(|l| l.contains("whisper-tiny-en") && l.contains("ready")),
        "{lines:?}"
    );
    // Re-install verifies the cache and skips the download.
    match install_model_with_consent(&cfg, MODEL, true) {
        InstallOutcome::Installed {
            already,
            announcements,
        } => {
            assert!(already);
            assert!(
                announcements.iter().any(|a| a.contains("skipping")),
                "{announcements:?}"
            );
        }
        InstallOutcome::Refused { message, .. } => panic!("re-install refused: {message}"),
    }
    let _ = root;
}

#[test]
fn checksum_mismatch_discards_and_refuses() {
    let _guard = ENV_LOCK.lock().unwrap();
    let root = tmp("mismatch");
    let reg = root.join("reg");
    std::fs::create_dir_all(&reg).unwrap();
    std::fs::write(reg.join(MODEL), b"tampered bytes").unwrap();
    std::env::set_var("FRAMESMITH_ML_SHA256_WHISPER_TINY_EN", fixture_sha());
    let cfg = cfg_for(&root, Some(reg), false, true);
    match install_model_with_consent(&cfg, MODEL, true) {
        InstallOutcome::Refused { code, message } => {
            assert_eq!(code, "E_DOWNLOAD_FAILED");
            assert!(message.contains("checksum mismatch"), "{message}");
        }
        InstallOutcome::Installed { .. } => panic!("installed tampered bytes"),
    }
    assert!(!cfg.cached_model(MODEL).is_file());
}

#[test]
fn corrupted_cache_is_discarded_never_used() {
    let _guard = ENV_LOCK.lock().unwrap();
    let (_root, cfg) = install_ok();
    let cached = cfg.cached_model(MODEL);
    std::fs::write(&cached, b"corrupted").unwrap();
    // The corrupt entry refuses and is discarded.
    let err = ml::models::require_model(&cfg, MODEL).expect_err("corrupt cache must refuse");
    assert_eq!(err.0, "E_NO_MODEL");
    assert!(err.1.contains("framesmith models install"), "{}", err.1);
    assert!(!cached.is_file(), "corrupt cache must be discarded");
}

#[test]
fn offline_install_refuses_with_manual_provision_fix() {
    let _guard = ENV_LOCK.lock().unwrap();
    let root = tmp("offline");
    let reg = root.join("reg");
    std::fs::create_dir_all(&reg).unwrap();
    std::fs::write(reg.join(MODEL), fixture_bytes()).unwrap();
    std::env::set_var("FRAMESMITH_ML_SHA256_WHISPER_TINY_EN", fixture_sha());
    let cfg = cfg_for(&root, Some(reg), true, true);
    match install_model_with_consent(&cfg, MODEL, true) {
        InstallOutcome::Refused { code, message } => {
            assert_eq!(code, "E_NO_MODEL");
            assert!(message.contains("FRAMESMITH_OFFLINE"), "{message}");
            assert!(message.contains("provision it manually"), "{message}");
        }
        InstallOutcome::Installed { .. } => panic!("installed while offline"),
    }
    // Cached models keep working offline: the gate is the cache,
    // not the network.
    let (root2, _) = install_ok();
    let offline_cfg = MlConfig {
        offline: true,
        ..cfg_for(&root2, None, false, true)
    };
    ml::models::require_model(&offline_cfg, MODEL).expect("cached model works offline");
}

#[test]
fn unknown_model_refuses_naming_known_models() {
    let _guard = ENV_LOCK.lock().unwrap();
    let root = tmp("unknown");
    let cfg = cfg_for(&root, None, false, true);
    match install_model_with_consent(&cfg, "no-such-model", true) {
        InstallOutcome::Refused { code, message } => {
            assert_eq!(code, "E_NO_MODEL");
            assert!(message.contains("unknown model"), "{message}");
        }
        InstallOutcome::Installed { .. } => panic!("installed an unknown model"),
    }
}

#[test]
fn disabled_config_refuses_before_any_download() {
    let _guard = ENV_LOCK.lock().unwrap();
    let root = tmp("disabled");
    let cfg = cfg_for(&root, None, false, false);
    match install_model_with_consent(&cfg, MODEL, true) {
        InstallOutcome::Refused { code, message } => {
            assert_eq!(code, "E_NO_MODEL");
            assert!(message.contains("FRAMESMITH_ML_AUTO_DOWNLOAD"), "{message}");
        }
        InstallOutcome::Installed { .. } => panic!("installed while disabled"),
    }
}

#[test]
fn stub_activates_only_behind_the_explicit_override() {
    let _guard = ENV_LOCK.lock().unwrap();
    // Explicit `--engine stub` resolves.
    let stub = ml::engines::resolve_engine("transcribe", "stub", None).expect("stub resolves");
    assert!(stub.is_stub());
    // `auto` never resolves the stub: with an empty PATH it refuses.
    let saved = std::env::var_os("PATH");
    std::env::set_var("PATH", "/nonexistent-framesmith-noengines");
    let err =
        ml::engines::resolve_engine("transcribe", "auto", None).expect_err("auto must refuse");
    assert_eq!(err.0, "E_NO_ENGINE");
    assert!(err.1.contains("--engine stub"), "{}", err.1);
    if let Some(p) = saved {
        std::env::set_var("PATH", p);
    } else {
        std::env::remove_var("PATH");
    }
    // Unknown engines and wrong-kind engines refuse naming the fix.
    let err =
        ml::engines::resolve_engine("transcribe", "no-engine", None).expect_err("unknown refuses");
    assert!(err.1.contains("auto"), "{}", err.1);
    let err = ml::engines::resolve_engine("tts", "whisper", None).expect_err("wrong kind refuses");
    assert!(err.1.contains("piper"), "{}", err.1);
    // Missing explicit binary refuses.
    let err = ml::engines::resolve_engine("transcribe", "auto", Some("/nonexistent/bin"))
        .expect_err("missing bin refuses");
    assert_eq!(err.0, "E_NO_ENGINE");
}
