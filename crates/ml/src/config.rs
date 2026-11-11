//! ML configuration from the environment (Sprint 23, T001).
//!
//! Mirrors the FFmpeg bootstrap sources: an explicit tool-cache
//! override wins, then the per-user default; an explicit offline
//! switch refuses before any network; an explicit local registry
//! serves tests and managed installs without the network.

use std::path::PathBuf;

/// Release base for model assets: `<base>/<asset>` per catalog
/// entry. Unpublished until the registry ships; downloads against it
/// fail closed with the cause (never a guessed model).
pub const ML_REGISTRY_BASE: &str = "https://github.com/bdsanxz/framesmith-models/releases/download";

/// Marker the runner sets only when spawning the stub re-exec. The
/// hidden `__ml-stub` verb requires it, so stubs never activate
/// implicitly (CAP-006).
pub const STUB_ACTIVE_ENV: &str = "FRAMESMITH_ML_STUB_ACTIVE";

/// All inputs the model manager and runners need. Built from the
/// environment by [`MlConfig::from_env`]; tests construct it directly
/// so no test mutates process state.
#[derive(Debug, Clone)]
pub struct MlConfig {
    /// Tool-cache root for installed models.
    pub cache_dir: PathBuf,
    /// Self-download switch (`FRAMESMITH_ML_AUTO_DOWNLOAD=0` disables).
    pub auto_download: bool,
    /// Offline switch (shared `FRAMESMITH_OFFLINE=1` with FFmpeg).
    pub offline: bool,
    /// Local registry override: files named exactly `<model-name>`
    /// are copied instead of downloaded (tests and managed installs).
    pub registry_dir: Option<PathBuf>,
    /// Download time budget in seconds.
    pub download_timeout_secs: u64,
}

impl MlConfig {
    /// Read every field from the environment.
    pub fn from_env() -> Self {
        Self {
            cache_dir: tool_cache_dir(),
            auto_download: auto_download_enabled(),
            offline: offline_forced(),
            registry_dir: registry_dir_from_env(),
            download_timeout_secs: download_timeout_secs(),
        }
    }

    /// Expected checksum for a model: the test override
    /// `FRAMESMITH_ML_SHA256_<NAME>` wins (name uppercased, `-` to
    /// `_`), else the catalog checksum.
    pub fn expected_sha256(&self, model: &crate::catalog::ModelEntry) -> Option<String> {
        let key = format!(
            "FRAMESMITH_ML_SHA256_{}",
            model.name.to_uppercase().replace('-', "_")
        );
        if let Ok(v) = std::env::var(&key) {
            if !v.trim().is_empty() {
                return Some(v.trim().to_string());
            }
        }
        model.sha256.map(|s| s.to_string())
    }

    /// Registry bytes for a model under the local override, if set.
    pub fn registry_bytes(&self, model: &crate::catalog::ModelEntry) -> Option<std::path::PathBuf> {
        let dir = self.registry_dir.as_ref()?;
        let cand = dir.join(model.name);
        if cand.is_file() {
            Some(cand)
        } else {
            None
        }
    }

    /// Installed location of a verified model:
    /// `<cache>/<name>/model.bin`.
    pub fn cached_model(&self, name: &str) -> PathBuf {
        tool_cache_dir_for(&self.cache_dir)
            .join(name)
            .join("model.bin")
    }
}

/// Tool-cache root: `FRAMESMITH_ML_DIR` wins (tests and managed
/// installs), otherwise `~/.framesmith/models`.
pub fn tool_cache_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("FRAMESMITH_ML_DIR") {
        if !dir.is_empty() {
            return PathBuf::from(dir);
        }
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".framesmith").join("models")
}

fn tool_cache_dir_for(cache_dir: &std::path::Path) -> PathBuf {
    cache_dir.to_path_buf()
}

/// Self-download switch: `FRAMESMITH_ML_AUTO_DOWNLOAD=0` (or `false`,
/// `no`, `off`) disables even consented installs, mirroring
/// `ffmpeg_auto_download = false`.
pub fn auto_download_enabled() -> bool {
    match std::env::var("FRAMESMITH_ML_AUTO_DOWNLOAD") {
        Ok(v) => !matches!(
            v.trim().to_lowercase().as_str(),
            "0" | "false" | "no" | "off"
        ),
        Err(_) => true,
    }
}

/// Explicit offline switch (shared with the FFmpeg bootstrap): when
/// `FRAMESMITH_OFFLINE` is `1` no install attempts any network.
pub fn offline_forced() -> bool {
    std::env::var("FRAMESMITH_OFFLINE")
        .map(|v| v == "1")
        .unwrap_or(false)
}

/// Local registry override for tests and managed installs.
pub fn registry_dir_from_env() -> Option<PathBuf> {
    std::env::var("FRAMESMITH_ML_REGISTRY_DIR")
        .ok()
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

/// Download time budget in seconds (`FRAMESMITH_DOWNLOAD_TIMEOUT_SECS`
/// overrides, shared with the FFmpeg bootstrap); always clamped.
pub fn download_timeout_secs() -> u64 {
    std::env::var("FRAMESMITH_DOWNLOAD_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .map(|v| v.clamp(3, 900))
        .unwrap_or(240)
}
