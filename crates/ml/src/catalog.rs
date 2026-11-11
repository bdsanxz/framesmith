//! Known-model catalog (Sprint 23, T001 / CAP-001).
//!
//! Every entry names the download asset, the engine that runs it, and
//! the approximate on-disk size shown by `models list`. Checksums are
//! the publisher SHA256: entries whose checksum is not yet published
//! carry `None` and refuse to install until it is (CAP-001 failure:
//! a missing checksum refuses with the cause and installs nothing).
//! Tests supply fixture checksums through
//! `FRAMESMITH_ML_SHA256_<NAME>` without touching this table.

/// One downloadable model: what `models list` shows and what
/// `models install` fetches after consent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelEntry {
    /// `models install` name (exact, case-sensitive).
    pub name: &'static str,
    /// Runner family this model serves: `transcribe`, `tts`, `segment`.
    pub kind: &'static str,
    /// Default engine that runs it.
    pub engine: &'static str,
    /// Approximate download bytes, for the listing only.
    pub size_bytes: u64,
    /// Release asset path under [`crate::config::ML_REGISTRY_BASE`].
    pub asset: &'static str,
    /// Publisher SHA256, when published. `None` refuses to install.
    pub sha256: Option<&'static str>,
    /// One-line purpose shown in listings.
    pub blurb: &'static str,
}

/// Every known model, in listing order.
pub const KNOWN_MODELS: &[ModelEntry] = &[
    ModelEntry {
        name: "whisper-tiny-en",
        kind: "transcribe",
        engine: "whisper",
        size_bytes: 78_643_200,
        asset: "whisper-tiny-en/model.bin",
        sha256: None,
        blurb: "English transcription (tiny, fastest)",
    },
    ModelEntry {
        name: "whisper-base-en",
        kind: "transcribe",
        engine: "whisper",
        size_bytes: 147_456_000,
        asset: "whisper-base-en/model.bin",
        sha256: None,
        blurb: "English transcription (base, steadier)",
    },
    ModelEntry {
        name: "parakeet-tdt-0720",
        kind: "transcribe",
        engine: "parakeet",
        size_bytes: 629_145_600,
        asset: "parakeet-tdt-0720/model.bin",
        sha256: None,
        blurb: "English transcription (parakeet, precise)",
    },
    ModelEntry {
        name: "tts-en-aria",
        kind: "tts",
        engine: "piper",
        size_bytes: 63_488_000,
        asset: "tts-en-aria/model.bin",
        sha256: None,
        blurb: "English text-to-speech voice model",
    },
    ModelEntry {
        name: "rembg-fast",
        kind: "segment",
        engine: "rembg",
        size_bytes: 4_718_592,
        asset: "rembg-fast/model.bin",
        sha256: None,
        blurb: "Background segmentation (fast, light)",
    },
    ModelEntry {
        name: "rembg-base",
        kind: "segment",
        engine: "rembg",
        size_bytes: 176_160_768,
        asset: "rembg-base/model.bin",
        sha256: None,
        blurb: "Background segmentation (balanced)",
    },
    ModelEntry {
        name: "rembg-precise",
        kind: "segment",
        engine: "rembg",
        size_bytes: 176_160_768,
        asset: "rembg-precise/model.bin",
        sha256: None,
        blurb: "Background segmentation (precise edges)",
    },
];

/// One runnable engine: the binary name looked up on `PATH`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EngineEntry {
    /// `--engine` name (exact, case-sensitive). `stub` is the test
    /// override and is listed here so diagnostics can name it.
    pub name: &'static str,
    /// Runner family it serves.
    pub kind: &'static str,
    /// Binary names probed on `PATH`, in order.
    pub binaries: &'static [&'static str],
    /// One-line purpose shown in refusals.
    pub blurb: &'static str,
}

/// Every known engine, in `auto` preference order per family.
pub const KNOWN_ENGINES: &[EngineEntry] = &[
    EngineEntry {
        name: "whisper",
        kind: "transcribe",
        binaries: &["whisper-cli", "whisper", "framesmith-whisper"],
        blurb: "transcription engine",
    },
    EngineEntry {
        name: "parakeet",
        kind: "transcribe",
        binaries: &["parakeet", "framesmith-parakeet"],
        blurb: "transcription engine",
    },
    EngineEntry {
        name: "piper",
        kind: "tts",
        binaries: &["piper", "framesmith-piper"],
        blurb: "text-to-speech engine",
    },
    EngineEntry {
        name: "rembg",
        kind: "segment",
        binaries: &["rembg", "framesmith-rembg"],
        blurb: "background-segmentation engine",
    },
    EngineEntry {
        name: "stub",
        kind: "*",
        binaries: &[],
        blurb: "deterministic test stub (explicit `--engine stub` only, never `auto`)",
    },
];

/// Voices the TTS runner accepts. Unknown voices refuse naming the
/// voice (CAP-003); hosted voices stay excluded.
pub const KNOWN_VOICES: &[&str] = &["aria", "kai", "coral", "sage"];

/// Quality presets for `remove-background`: each names the segment
/// model it selects. Presets only change the model, never the path.
pub const QUALITY_MODELS: &[(&str, &str)] = &[
    ("fast", "rembg-fast"),
    ("balanced", "rembg-base"),
    ("precise", "rembg-precise"),
];

/// Look up a model by exact name.
pub fn find_model(name: &str) -> Option<ModelEntry> {
    KNOWN_MODELS.iter().find(|m| m.name == name).copied()
}

/// Look up an engine by exact name.
pub fn find_engine(name: &str) -> Option<EngineEntry> {
    KNOWN_ENGINES.iter().find(|e| e.name == name).copied()
}

/// Default engine for a model: its own `engine` field.
pub fn engine_for_model(model: &ModelEntry) -> &'static str {
    model.engine
}

/// Human size for `models list` (`75.0 MB`).
pub fn human_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} {}", bytes, UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}
