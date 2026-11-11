//! Doctor health reporting for engines and models (Sprint 23,
//! T001 / INT-003): the same resolution the runners use, so health
//! output and runner behavior agree.

use crate::catalog::KNOWN_MODELS;
use crate::config::MlConfig;
use crate::engines::engine_health;
use crate::models::model_status;

/// Human `doctor:` lines for ML engines and models.
pub fn doctor_lines(cfg: &MlConfig) -> Vec<String> {
    let mut out = Vec::new();
    for (name, kind, found) in engine_health() {
        match found {
            Some(where_) => out.push(format!("doctor: ml engine {name} ({kind}) present at {where_}.")),
            None => out.push(format!(
                "doctor: ml engine {name} ({kind}) missing (install the {name} binary on PATH, or use `--engine stub`)."
            )),
        }
    }
    for m in KNOWN_MODELS {
        match model_status(cfg, m) {
            crate::models::ModelState::Ready => out.push(format!(
                "doctor: ml model {} ({}) ready at {}.",
                m.name,
                m.engine,
                cfg.cached_model(m.name).display()
            )),
            crate::models::ModelState::Missing => out.push(format!(
                "doctor: ml model {} ({}) missing (fix: framesmith models install {} --yes).",
                m.name, m.engine, m.name
            )),
        }
    }
    out
}
