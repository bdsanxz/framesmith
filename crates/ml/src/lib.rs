//! ML runners (Sprint 23 / CAP-001..CAP-006).
//!
//! One model manager plus four runners, all deterministic: the same
//! inputs, engine, and model always produce byte-identical artifacts.
//! Models never ship inside any framesmith binary; they install into
//! the tool cache only after explicit user consent with SHA256
//! verification. Engines run as separate processes exactly like the
//! FFmpeg bootstrap: executed, never linked or bundled.
//!
//! Decision order for every gated step mirrors
//! [`ffmpeg::bootstrap`]: disabled configuration refuses first, then
//! offline, then the explicit source, then the registry, then the
//! verified install. Every refusal names the file or piece and the
//! cause, plus the exact fix; nothing is ever staged silently.
//!
//! Stub engines prove the whole orchestration deterministically
//! without any network. They activate only behind the explicit
//! `--engine stub` override (never through `auto`), and the hidden
//! `__ml-stub` re-exec entry requires the `FRAMESMITH_ML_STUB_ACTIVE=1`
//! marker the runner sets when spawning it.

pub mod beats;
pub mod catalog;
pub mod config;
pub mod doctor;
pub mod engines;
pub mod matting;
pub mod models;
pub mod stub;
pub mod transcribe;
pub mod tts;
pub mod wav;

pub use catalog::{engine_for_model, find_model, KNOWN_ENGINES, KNOWN_MODELS, KNOWN_VOICES};
pub use config::MlConfig;
pub use models::{install_model_with_consent, model_status, Consent, InstallOutcome, ModelState};
