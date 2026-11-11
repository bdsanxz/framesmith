//! Consented model manager (Sprint 23, T001 / CAP-001).
//!
//! Decision order mirrors [`ffmpeg::bootstrap`]: disabled
//! configuration refuses first, then the installed-cache check, then
//! consent, then offline, then the explicit registry source, then the
//! remote download — every step verified by SHA256 before caching.
//! Nothing downloads without explicit consent (`--yes` or an
//! interactive yes); a non-interactive shell without `--yes` refuses
//! instead of prompting forever.

use crate::catalog::{find_model, human_size, ModelEntry, KNOWN_MODELS};
use crate::config::{MlConfig, ML_REGISTRY_BASE};

/// Install state of one known model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelState {
    /// Verified bytes sit in the tool cache.
    Ready,
    /// Nothing cached (or the cache entry is corrupt).
    Missing,
}

/// How the maker consented to the download.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Consent {
    /// `--yes` on the command line.
    Flag,
    /// An interactive `y`/`yes` answer.
    Interactive,
    /// No consent: refuse without downloading.
    Withheld,
}

/// Outcome of a consented install.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallOutcome {
    /// Bytes verified and cached (or re-verified and kept).
    Installed {
        /// The install was already present and verified; nothing
        /// downloaded.
        already: bool,
        announcements: Vec<String>,
    },
    /// Nothing downloads; the message names the cause and the fix.
    Refused { code: &'static str, message: String },
}

/// Verify cached bytes against the expected SHA256. A corrupt cache
/// entry is discarded here, never used.
fn verify_cache(path: &std::path::Path, expected: &str) -> bool {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(_) => return false,
    };
    let found = ffmpeg::sha256::sha256_hex(&bytes);
    if found.to_lowercase() != expected.to_lowercase() {
        let _ = std::fs::remove_file(path);
        return false;
    }
    true
}

/// Install state of one model: ready only when the cached bytes
/// verify against the expected checksum.
pub fn model_status(cfg: &MlConfig, model: &ModelEntry) -> ModelState {
    let path = cfg.cached_model(model.name);
    if !path.is_file() {
        return ModelState::Missing;
    }
    match cfg.expected_sha256(model) {
        Some(expected) if verify_cache(&path, &expected) => ModelState::Ready,
        // No published checksum: bytes can never count as verified.
        _ => ModelState::Missing,
    }
}

/// One line per known model for `models list`: name, kind, engine,
/// size, state, and the install command when missing.
pub fn list_lines(cfg: &MlConfig) -> Vec<String> {
    let mut out = Vec::new();
    for m in KNOWN_MODELS {
        let state = match model_status(cfg, m) {
            ModelState::Ready => "ready".to_string(),
            ModelState::Missing => {
                format!("missing (fix: framesmith models install {} --yes)", m.name)
            }
        };
        out.push(format!(
            "model {} ({}; engine {}; {}) — {state}; {}",
            m.name,
            m.kind,
            m.engine,
            human_size(m.size_bytes),
            m.blurb,
        ));
    }
    out
}

/// Read consent: `--yes` wins; otherwise an interactive yes on a
/// terminal; a non-terminal without `--yes` withholds (never hangs).
pub fn read_consent(yes_flag: bool) -> Consent {
    if yes_flag {
        return Consent::Flag;
    }
    if !std::io::IsTerminal::is_terminal(&std::io::stdin()) {
        return Consent::Withheld;
    }
    Consent::Interactive
}

/// Ask the maker on the terminal; true only for `y`/`yes`.
pub fn ask_consent(model: &ModelEntry) -> bool {
    use std::io::Write;
    print!(
        "download model {} ({}; {})? [y/N] ",
        model.name,
        model.engine,
        human_size(model.size_bytes)
    );
    let _ = std::io::stdout().flush();
    let mut line = String::new();
    if std::io::stdin().read_line(&mut line).is_err() {
        return false;
    }
    matches!(line.trim().to_lowercase().as_str(), "y" | "yes")
}

fn disabled_message(model: &ModelEntry) -> String {
    format!(
        "models install refused (E_NO_MODEL): model {} cannot download because model self-download is disabled (`FRAMESMITH_ML_AUTO_DOWNLOAD=0`).\n\
         fix: unset `FRAMESMITH_ML_AUTO_DOWNLOAD` (or set it to `1`) and re-run `framesmith models install {} --yes`. Nothing was downloaded.",
        model.name, model.name,
    )
}

fn consent_message(model: &ModelEntry) -> String {
    format!(
        "models install refused (E_NO_CONSENT): downloading model {} needs explicit consent.\n\
         fix: re-run with `--yes` (`framesmith models install {} --yes`), or answer `y` at the prompt on a terminal. Nothing was downloaded.",
        model.name, model.name,
    )
}

fn offline_message(model: &ModelEntry, cached: &std::path::Path) -> String {
    format!(
        "models install refused (E_NO_MODEL): model {} cannot download on an offline machine (FRAMESMITH_OFFLINE=1).\n\
         fix: provision it manually by placing the verified model bytes at {} (SHA256 must match the publisher checksum), then re-run. Nothing was downloaded.",
        model.name,
        cached.display(),
    )
}

fn checksum_message(model: &ModelEntry) -> String {
    format!(
        "models install refused (E_NO_MODEL): model {} publishes no SHA256 checksum yet, so its bytes cannot be verified.\n\
         fix: wait for the registry to publish the checksum (see {}), or serve the bytes locally with `FRAMESMITH_ML_REGISTRY_DIR` plus `FRAMESMITH_ML_SHA256_{}` for a managed install. Nothing was downloaded.",
        model.name,
        ML_REGISTRY_BASE,
        model.name.to_uppercase().replace('-', "_"),
    )
}

fn download_failed_message(model: &ModelEntry, cause: &str) -> String {
    format!(
        "models install refused (E_DOWNLOAD_FAILED): model {} could not be fetched: {cause}.\n\
         fix: check network access and re-run `framesmith models install {} --yes` (the cache is unchanged), or provision the bytes manually (see the offline fix). Nothing was installed.",
        model.name, model.name,
    )
}

/// Install one model after consent. Decision order: unknown model,
/// disabled config, verified-cache skip, consent, offline, missing
/// checksum, explicit registry source, remote download — verified
/// bytes land in the tool cache or nothing installs.
pub fn install_model_with_consent(cfg: &MlConfig, name: &str, yes_flag: bool) -> InstallOutcome {
    let Some(model) = find_model(name) else {
        let known = KNOWN_MODELS
            .iter()
            .map(|m| m.name.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        return InstallOutcome::Refused {
            code: "E_NO_MODEL",
            message: format!(
                "models install refused (E_NO_MODEL): unknown model {name:?}.\n\
                 fix: install one of {known}. Nothing was downloaded.",
            ),
        };
    };
    if !cfg.auto_download {
        return InstallOutcome::Refused {
            code: "E_NO_MODEL",
            message: disabled_message(&model),
        };
    }
    // Re-install verifies the cache and skips the download with a
    // note (a corrupt entry is discarded by the check and falls
    // through to a fresh download).
    let cached = cfg.cached_model(model.name);
    if cached.is_file() {
        if let Some(expected) = cfg.expected_sha256(&model) {
            if verify_cache(&cached, &expected) {
                return InstallOutcome::Installed {
                    already: true,
                    announcements: vec![format!(
                        "model {} is already installed at {} (SHA256 re-verified); skipping the download.",
                        model.name,
                        cached.display(),
                    )],
                };
            }
        }
    }
    match read_consent(yes_flag) {
        Consent::Flag => {}
        Consent::Interactive => {
            if !ask_consent(&model) {
                return InstallOutcome::Refused {
                    code: "E_NO_CONSENT",
                    message: consent_message(&model),
                };
            }
        }
        Consent::Withheld => {
            return InstallOutcome::Refused {
                code: "E_NO_CONSENT",
                message: consent_message(&model),
            };
        }
    }
    if cfg.offline {
        return InstallOutcome::Refused {
            code: "E_NO_MODEL",
            message: offline_message(&model, &cached),
        };
    }
    let Some(expected) = cfg.expected_sha256(&model) else {
        return InstallOutcome::Refused {
            code: "E_NO_MODEL",
            message: checksum_message(&model),
        };
    };
    let bytes = match fetch_bytes(cfg, &model) {
        Ok(b) => b,
        Err(cause) => {
            return InstallOutcome::Refused {
                code: "E_DOWNLOAD_FAILED",
                message: download_failed_message(&model, &cause),
            };
        }
    };
    let found = ffmpeg::sha256::sha256_hex(&bytes);
    if found.to_lowercase() != expected.to_lowercase() {
        return InstallOutcome::Refused {
            code: "E_DOWNLOAD_FAILED",
            message: download_failed_message(
                &model,
                &format!("checksum mismatch: expected {expected} but found {found}"),
            ),
        };
    }
    if let Some(parent) = cached.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            return InstallOutcome::Refused {
                code: "E_DOWNLOAD_FAILED",
                message: download_failed_message(
                    &model,
                    &format!("cannot create tool cache {}: {e}", parent.display()),
                ),
            };
        }
    }
    if let Err(e) = std::fs::write(&cached, &bytes) {
        return InstallOutcome::Refused {
            code: "E_DOWNLOAD_FAILED",
            message: download_failed_message(
                &model,
                &format!("cannot write tool cache {}: {e}", cached.display()),
            ),
        };
    }
    InstallOutcome::Installed {
        already: false,
        announcements: vec![format!(
            "downloaded model {} (SHA256 verified) to {}; runners can use it.",
            model.name,
            cached.display(),
        )],
    }
}

/// Fetch raw model bytes: the local registry override wins (tests and
/// managed installs, no network); otherwise `curl` the release asset
/// with a bounded time budget, exactly like the FFmpeg bootstrap.
fn fetch_bytes(cfg: &MlConfig, model: &ModelEntry) -> Result<Vec<u8>, String> {
    if let Some(path) = cfg.registry_bytes(model) {
        return std::fs::read(&path)
            .map_err(|e| format!("cannot read registry file {}: {e}", path.display()));
    }
    let curl = find_on_path("curl")
        .ok_or_else(|| "no `curl` found on PATH to fetch the model".to_string())?;
    let url = format!("{ML_REGISTRY_BASE}/{}", model.asset);
    let timeout = cfg.download_timeout_secs.to_string();
    let out = std::process::Command::new(&curl)
        .args(["-fsSL", "--connect-timeout", "20", "--max-time", &timeout])
        .arg(&url)
        .output()
        .map_err(|e| format!("could not launch `{}`: {e}", curl.display()))?;
    if !out.status.success() {
        let mut detail = String::from_utf8_lossy(&out.stderr).trim().to_string();
        if detail.is_empty() {
            detail = format!("curl exited with status {}", out.status);
        }
        if detail.len() > 300 {
            detail.truncate(300);
        }
        return Err(format!("download failed: {detail}"));
    }
    Ok(out.stdout)
}

fn find_on_path(name: &str) -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let cand = dir.join(name);
        if cand.is_file() {
            return Some(cand);
        }
    }
    None
}

/// Require an installed, verified model before any runner work: the
/// refusal names the model and the exact install fix. A corrupt cache
/// entry is discarded here and refused (never used).
pub fn require_model(cfg: &MlConfig, name: &str) -> Result<std::path::PathBuf, (String, String)> {
    let Some(model) = find_model(name) else {
        let known = KNOWN_MODELS
            .iter()
            .map(|m| m.name.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        return Err((
            "E_NO_MODEL".to_string(),
            format!(
                "runner refused (E_NO_MODEL): unknown model {name:?}.\n\
                 fix: use one of {known}. No work was done and no partial artifact was staged.",
            ),
        ));
    };
    let path = cfg.cached_model(model.name);
    match cfg.expected_sha256(&model) {
        Some(expected) if path.is_file() && verify_cache(&path, &expected) => Ok(path),
        _ => {
            if path.is_file() {
                let _ = std::fs::remove_file(&path);
            }
            Err((
                "E_NO_MODEL".to_string(),
                format!(
                    "runner refused (E_NO_MODEL): model {} is not installed.\n\
                     fix: `framesmith models install {} --yes`, then re-run. No work was done and no partial artifact was staged.",
                    model.name, model.name,
                ),
            ))
        }
    }
}
