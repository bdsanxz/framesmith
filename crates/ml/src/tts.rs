//! Text-to-speech runner (Sprint 23, T003 / CAP-003).
//!
//! `tts <text>` orchestrates engine inference into voice audio
//! (WAV) that lands on an audio track and renders deterministically:
//! the same text, voice, speed, and model always yield
//! byte-identical bytes. Unknown voices refuse naming the voice;
//! missing models refuse with the install fix before any work.

use crate::catalog::{find_model, KNOWN_VOICES};
use crate::config::MlConfig;
use crate::engines::{resolve_engine, run_engine_process, ResolvedEngine};
use crate::models::require_model;

/// TTS options, parsed by the CLI layer.
#[derive(Debug, Clone)]
pub struct TtsOptions {
    pub engine: String,
    pub engine_bin: Option<String>,
    pub model: String,
    pub voice: String,
    pub speed: f64,
    pub language: String,
    pub out: Option<String>,
}

impl Default for TtsOptions {
    fn default() -> Self {
        Self {
            engine: "auto".to_string(),
            engine_bin: None,
            model: "tts-en-aria".to_string(),
            voice: "aria".to_string(),
            speed: 1.0,
            language: "en".to_string(),
            out: None,
        }
    }
}

/// Run TTS. On success returns the WAV path. Every failure names the
/// piece and the cause; no partial artifact is ever staged.
pub fn run_tts(cfg: &MlConfig, text: &str, opts: &TtsOptions) -> Result<String, (String, String)> {
    if text.trim().is_empty() {
        return Err((
            "E_BAD_TTS".to_string(),
            "tts refused (E_BAD_TTS): the text is empty.\n\
             fix: pass non-empty text to synthesize. No work was done."
                .to_string(),
        ));
    }
    if !KNOWN_VOICES.contains(&opts.voice.as_str()) {
        return Err((
            "E_BAD_TTS".to_string(),
            format!(
                "tts refused (E_BAD_TTS): unknown voice {:?}.\n\
                 fix: use one of {}. No work was done and no partial artifact was staged.",
                opts.voice,
                KNOWN_VOICES.join(", ")
            ),
        ));
    }
    if !opts.speed.is_finite() || opts.speed < 0.25 || opts.speed > 4.0 {
        return Err((
            "E_BAD_TTS".to_string(),
            format!(
                "tts refused (E_BAD_TTS): `--speed {}` is outside 0.25..4.0.\n\
                 fix: pass a speed in 0.25..4.0 (1.0 is natural pace). No work was done.",
                opts.speed
            ),
        ));
    }
    let Some(model) = find_model(&opts.model) else {
        return Err((
            "E_NO_MODEL".to_string(),
            format!(
                "tts refused (E_NO_MODEL): unknown model {:?}.\n\
                 fix: `framesmith models install tts-en-aria --yes`, then re-run. No work was done.",
                opts.model
            ),
        ));
    };
    if model.kind != "tts" {
        return Err((
            "E_NO_MODEL".to_string(),
            format!(
                "tts refused (E_NO_MODEL): model {} serves `{}` runs, not speech synthesis.\n\
                 fix: use tts-en-aria. No work was done.",
                model.name, model.kind
            ),
        ));
    }
    let engine = resolve_engine("tts", &opts.engine, opts.engine_bin.as_deref())?;
    let model_path = require_model(cfg, model.name)?;
    let out_path = match &opts.out {
        Some(o) => o.clone(),
        None => "voice.wav".to_string(),
    };
    if let Some(parent) = std::path::Path::new(&out_path).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| {
                (
                    "E_BAD_TTS".to_string(),
                    format!(
                        "tts refused (E_BAD_TTS): cannot create output dir {}: {e}. No artifact was staged.",
                        parent.display()
                    ),
                )
            })?;
        }
    }
    let map_err = |cause: String| {
        (
            "E_ENGINE_FAILED".to_string(),
            format!("tts refused (E_ENGINE_FAILED): {cause}. No artifact was staged."),
        )
    };
    // Engine contract: `<bin> --model --voice --speed --language
    // --text --out` writes the WAV itself. The stub proves the same
    // contract through the hidden re-exec verb.
    let args = vec![
        "--model".to_string(),
        model_path.display().to_string(),
        "--voice".to_string(),
        opts.voice.clone(),
        "--speed".to_string(),
        format!("{:.6}", opts.speed),
        "--language".to_string(),
        opts.language.clone(),
        "--text".to_string(),
        text.to_string(),
        "--out".to_string(),
        out_path.clone(),
    ];
    match &engine {
        ResolvedEngine::Stub { .. } => {
            crate::stub::spawn_stub("tts", &args).map_err(map_err)?;
        }
        ResolvedEngine::Explicit { bin, .. } | ResolvedEngine::Host { bin, .. } => {
            run_engine_process(bin, &args, cfg.download_timeout_secs).map_err(map_err)?;
        }
    }
    // The WAV on disk is authoritative: it must decode, or the engine
    // output is corrupt and the file is discarded, never kept.
    if let Err(cause) = crate::wav::read_wav(std::path::Path::new(&out_path)) {
        let _ = std::fs::remove_file(&out_path);
        return Err((
            "E_ENGINE_FAILED".to_string(),
            format!(
                "tts refused (E_ENGINE_FAILED): engine `{}` wrote an unreadable WAV for the text ({cause}). No artifact was staged.",
                engine.name()
            ),
        ));
    }
    Ok(out_path)
}
