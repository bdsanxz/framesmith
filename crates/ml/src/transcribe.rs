//! Transcription runner (Sprint 23, T002 / CAP-002).
//!
//! `transcribe <input>` orchestrates audio decode, engine inference,
//! and parsing into the Sprint 22 transcript JSON shape with
//! per-word times; the JSON imports into the captions track with zero
//! drift (same formatter both sides). `auto` picks the first
//! installed engine; missing engines or models refuse with the
//! install fix before any decode.

use crate::catalog::find_model;
use crate::config::MlConfig;
use crate::engines::{resolve_engine, run_engine_process, ResolvedEngine};
use crate::models::require_model;

/// Audio extensions the transcribe gate accepts (the Sprint 3 mix
/// set, decoded through the engine). Anything else refuses naming
/// the input.
fn audio_supported(path: &str) -> bool {
    const KNOWN: &[&str] = &[
        "wav", "mp3", "flac", "ogg", "oga", "opus", "m4a", "aac", "aiff", "aif", "wma", "mka",
        "ac3", "mp2", "amr",
    ];
    let lower = path.to_lowercase();
    KNOWN.iter().any(|ext| lower.ends_with(&format!(".{ext}")))
}

/// Transcribe options, parsed by the CLI layer.
#[derive(Debug, Clone)]
pub struct TranscribeOptions {
    pub engine: String,
    pub engine_bin: Option<String>,
    pub model: String,
    pub language: String,
    pub to: Option<String>,
    pub out: Option<String>,
}

impl Default for TranscribeOptions {
    fn default() -> Self {
        Self {
            engine: "auto".to_string(),
            engine_bin: None,
            model: "whisper-tiny-en".to_string(),
            language: "en".to_string(),
            to: None,
            out: None,
        }
    }
}

/// Run transcription. On success returns `(json_path, sidecar_path)`:
/// the transcript JSON plus the optional SRT/VTT conversion. Every
/// failure names the file or piece and the cause; no partial
/// artifact is ever staged.
pub fn run_transcribe(
    cfg: &MlConfig,
    input: &str,
    opts: &TranscribeOptions,
) -> Result<(String, Option<String>), (String, String)> {
    if let Some(to) = &opts.to {
        if to != "srt" && to != "vtt" {
            return Err((
                "E_BAD_TRANSCRIBE".to_string(),
                format!(
                    "transcribe refused (E_BAD_TRANSCRIBE): `--to {to:?}` names no caption format.\n\
                     fix: pass `--to srt` or `--to vtt`, or drop the flag for transcript JSON only. No work was done."
                ),
            ));
        }
    }
    let Some(model) = find_model(&opts.model) else {
        return Err((
            "E_NO_MODEL".to_string(),
            format!(
                "transcribe refused (E_NO_MODEL): unknown model {:?}.\n\
                 fix: install one of whisper-tiny-en, whisper-base-en, parakeet-tdt-0720 (`framesmith models install <name> --yes`). No work was done.",
                opts.model
            ),
        ));
    };
    if model.kind != "transcribe" {
        return Err((
            "E_NO_MODEL".to_string(),
            format!(
                "transcribe refused (E_NO_MODEL): model {} serves `{}` runs, not transcription.\n\
                 fix: use whisper-tiny-en, whisper-base-en, or parakeet-tdt-0720. No work was done.",
                model.name, model.kind
            ),
        ));
    }
    let in_path = std::path::Path::new(input);
    if !in_path.is_file() {
        return Err((
            "E_BAD_TRANSCRIBE".to_string(),
            format!(
                "transcribe refused (E_BAD_TRANSCRIBE): input {input:?} cannot be read.\n\
                 fix: pass an existing audio file. No work was done and no partial artifact was staged."
            ),
        ));
    }
    if !audio_supported(input) {
        return Err((
            "E_BAD_TRANSCRIBE".to_string(),
            format!(
                "transcribe refused (E_BAD_TRANSCRIBE): input {input:?} names no supported audio format (wav, mp3, flac, ogg, opus, m4a, aac, aiff, wma, mka, ac3, mp2, amr).\n\
                 fix: pass a supported audio file. No work was done and no partial artifact was staged."
            ),
        ));
    }
    let engine = resolve_engine("transcribe", &opts.engine, opts.engine_bin.as_deref())?;
    let model_path = require_model(cfg, model.name)?;
    let stdout = run_engine(
        &engine,
        &model_path,
        in_path,
        &opts.language,
        cfg.download_timeout_secs,
    )?;
    let text = String::from_utf8(stdout).map_err(|_| {
        (
            "E_BAD_TRANSCRIBE".to_string(),
            format!(
                "transcribe refused (E_BAD_TRANSCRIBE): engine `{}` returned non-UTF-8 output for {input:?}.\n\
                 fix: retry the run; if it persists the engine output is corrupt. No artifact was staged.",
                engine.name(),
            ),
        )
    })?;
    // The artifact shape is authoritative: the engine output must
    // parse as transcript JSON, and what parses is re-emitted through
    // the canonical formatter so the bytes are deterministic.
    let decoded = notation::captions::parse_transcript_json(&text).map_err(|e| {
        (
            "E_BAD_TRANSCRIBE".to_string(),
            format!(
                "transcribe refused (E_BAD_TRANSCRIBE): engine `{}` returned unparsable transcript JSON for {input:?} ({e}).\n\
                 fix: retry the run; if it persists the engine output is corrupt. No artifact was staged.",
                engine.name(),
            ),
        )
    })?;
    let json = notation::captions::format_transcript_json(&decoded.cues);
    let json_path = match &opts.out {
        Some(o) => o.clone(),
        None => format!("{input}.transcript.json"),
    };
    write_file(&json_path, json.as_bytes())?;
    let mut sidecar = None;
    if let Some(to) = &opts.to {
        let stem = json_path
            .strip_suffix(".json")
            .unwrap_or(&json_path)
            .to_string();
        let side_path = format!("{stem}.{to}");
        let body = if to == "srt" {
            notation::captions::format_srt(&decoded.cues)
        } else {
            notation::captions::format_vtt(&decoded.cues)
        };
        write_file(&side_path, body.as_bytes())?;
        sidecar = Some(side_path);
    }
    return Ok((json_path, sidecar));

    #[allow(clippy::unnecessary_wraps)]
    fn write_file(path: &str, bytes: &[u8]) -> Result<(), (String, String)> {
        if let Some(parent) = std::path::Path::new(path).parent() {
            if !parent.as_os_str().is_empty() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    return Err((
                        "E_BAD_TRANSCRIBE".to_string(),
                        format!(
                            "transcribe refused (E_BAD_TRANSCRIBE): cannot create output dir {}: {e}. No artifact was staged.",
                            parent.display()
                        ),
                    ));
                }
            }
        }
        std::fs::write(path, bytes).map_err(|e| {
            (
                "E_BAD_TRANSCRIBE".to_string(),
                format!("transcribe refused (E_BAD_TRANSCRIBE): cannot write {path:?}: {e}. No artifact was staged."),
            )
        })
    }
}

/// Invoke the engine as a separate process under the engine contract:
/// `<bin> --model <path> --input <file> --language <lang>` prints
/// transcript JSON on stdout. The stub proves the same contract
/// through the hidden re-exec verb.
fn run_engine(
    engine: &ResolvedEngine,
    model_path: &std::path::Path,
    input: &std::path::Path,
    language: &str,
    timeout_secs: u64,
) -> Result<Vec<u8>, (String, String)> {
    let args = vec![
        "--model".to_string(),
        model_path.display().to_string(),
        "--input".to_string(),
        input.display().to_string(),
        "--language".to_string(),
        language.to_string(),
    ];
    let map_err = |cause: String| {
        (
            "E_ENGINE_FAILED".to_string(),
            format!("transcribe refused (E_ENGINE_FAILED): {cause}. No artifact was staged."),
        )
    };
    match engine {
        ResolvedEngine::Stub { .. } => {
            crate::stub::spawn_stub("transcribe", &args).map_err(map_err)
        }
        ResolvedEngine::Explicit { bin, .. } | ResolvedEngine::Host { bin, .. } => {
            run_engine_process(bin, &args, timeout_secs).map_err(map_err)
        }
    }
}
