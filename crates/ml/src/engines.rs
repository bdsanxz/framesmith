//! Engine resolution and separate-process execution (Sprint 23,
//! T001 / CAP-006).
//!
//! Engines run as separate processes exactly like the FFmpeg
//! bootstrap: executed, never linked or bundled. Resolution order per
//! runner: the explicit `--engine-bin` path wins (tests and proofs),
//! then the explicit `--engine stub` deterministic stub (never through
//! `auto`), then the named engine binary on `PATH`, then `auto` takes
//! the first installed engine of the family. Anything missing refuses
//! naming the engine and the install fix before any decode.

use crate::catalog::{find_engine, KNOWN_ENGINES};

/// A resolved engine: how to invoke it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedEngine {
    /// Explicit binary path from `--engine-bin`.
    Explicit {
        /// Engine family name (for the contract).
        name: String,
        bin: std::path::PathBuf,
    },
    /// Deterministic stub via the hidden re-exec verb.
    Stub {
        /// Engine family name the stub proves.
        name: String,
    },
    /// Named engine found on `PATH`.
    Host {
        name: String,
        bin: std::path::PathBuf,
    },
}

impl ResolvedEngine {
    /// Engine family name.
    pub fn name(&self) -> &str {
        match self {
            ResolvedEngine::Explicit { name, .. } => name,
            ResolvedEngine::Stub { name } => name,
            ResolvedEngine::Host { name, .. } => name,
        }
    }

    /// True only for the explicit stub override.
    pub fn is_stub(&self) -> bool {
        matches!(self, ResolvedEngine::Stub { .. })
    }
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

/// Engines of one runner family in `auto` preference order (the stub
/// never participates: it activates only when named explicitly).
pub fn family_engines(kind: &str) -> Vec<&'static str> {
    KNOWN_ENGINES
        .iter()
        .filter(|e| e.kind == kind)
        .map(|e| e.name)
        .collect()
}

/// Resolve the engine for one runner: `engine` is `auto`, a known
/// name, or `stub`; `engine_bin` is the explicit path override. The
/// refusal names the missing engine and the fix (never silent).
pub fn resolve_engine(
    kind: &str,
    engine: &str,
    engine_bin: Option<&str>,
) -> Result<ResolvedEngine, (String, String)> {
    if let Some(bin) = engine_bin {
        let path = std::path::PathBuf::from(bin);
        if !path.is_file() {
            return Err((
                "E_NO_ENGINE".to_string(),
                format!(
                    "runner refused (E_NO_ENGINE): engine override {bin:?} is not a file.\n\
                     fix: pass an executable engine binary to `--engine-bin`, or drop the flag to resolve `{kind}` engines from PATH. No work was done.",
                ),
            ));
        }
        let name = if engine == "auto" || engine == "stub" {
            stub_or_first(kind).to_string()
        } else {
            engine.to_string()
        };
        return Ok(ResolvedEngine::Explicit { name, bin: path });
    }
    if engine == "stub" {
        return Ok(ResolvedEngine::Stub {
            name: stub_or_first(kind).to_string(),
        });
    }
    if engine == "auto" {
        for name in family_engines(kind) {
            if let Some(entry) = find_engine(name) {
                for bin in entry.binaries {
                    if let Some(path) = find_on_path(bin) {
                        return Ok(ResolvedEngine::Host {
                            name: name.to_string(),
                            bin: path,
                        });
                    }
                }
            }
        }
        return Err((
            "E_NO_ENGINE".to_string(),
            format!(
                "runner refused (E_NO_ENGINE): no `{kind}` engine is installed.\n\
                 fix: install one of {} on PATH, or re-run with `--engine stub` for the deterministic test stub. No work was done and no partial artifact was staged.",
                family_engines(kind).join(", "),
            ),
        ));
    }
    let Some(entry) = find_engine(engine) else {
        let mut known: Vec<&str> = family_engines(kind);
        known.push("stub");
        return Err((
            "E_NO_ENGINE".to_string(),
            format!(
                "runner refused (E_NO_ENGINE): unknown engine {engine:?} for `{kind}`.\n\
                 fix: use one of {} (or `auto`). No work was done.",
                known.join(", "),
            ),
        ));
    };
    if entry.kind != kind {
        return Err((
            "E_NO_ENGINE".to_string(),
            format!(
                "runner refused (E_NO_ENGINE): engine {engine:?} serves `{}` runs, not `{kind}`.\n\
                 fix: use one of {} (or `auto`). No work was done.",
                entry.kind,
                family_engines(kind).join(", "),
            ),
        ));
    }
    for bin in entry.binaries {
        if let Some(path) = find_on_path(bin) {
            return Ok(ResolvedEngine::Host {
                name: engine.to_string(),
                bin: path,
            });
        }
    }
    Err((
        "E_NO_ENGINE".to_string(),
        format!(
            "runner refused (E_NO_ENGINE): engine {engine:?} is not installed (looked for {} on PATH).\n\
             fix: install the {engine} binary on PATH, or re-run with `--engine stub` for the deterministic test stub. No work was done and no partial artifact was staged.",
            entry.binaries.join(", "),
        ),
    ))
}

fn stub_or_first(kind: &str) -> &'static str {
    family_engines(kind).into_iter().next().unwrap_or("stub")
}

/// Presence of every known engine for `doctor`: `(name, kind, path
/// when installed)`. The stub reports as available only — it needs
/// no binary — so health matches exactly what runners resolve.
pub fn engine_health() -> Vec<(String, String, Option<String>)> {
    let mut out = Vec::new();
    for entry in KNOWN_ENGINES {
        if entry.name == "stub" {
            out.push((
                "stub".to_string(),
                "*".to_string(),
                Some("explicit `--engine stub` only".to_string()),
            ));
            continue;
        }
        let mut found = None;
        for bin in entry.binaries {
            if let Some(path) = find_on_path(bin) {
                found = Some(path.display().to_string());
                break;
            }
        }
        out.push((entry.name.to_string(), entry.kind.to_string(), found));
    }
    out
}

/// Run an engine binary as a separate process with a bounded wait.
/// Returns stdout on success; any failure names the engine and the
/// cause (timeouts refuse, never hang forever).
pub fn run_engine_process(
    bin: &std::path::Path,
    args: &[String],
    timeout_secs: u64,
) -> Result<Vec<u8>, String> {
    let mut child = std::process::Command::new(bin)
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("could not launch engine `{}`: {e}", bin.display()))?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs.max(5));
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let mut stdout = Vec::new();
                let mut stderr = Vec::new();
                if let Some(mut pipe) = child.stdout.take() {
                    use std::io::Read;
                    let _ = pipe.read_to_end(&mut stdout);
                }
                if let Some(mut pipe) = child.stderr.take() {
                    use std::io::Read;
                    let _ = pipe.read_to_end(&mut stderr);
                }
                if !status.success() {
                    let mut detail = String::from_utf8_lossy(&stderr).trim().to_string();
                    if detail.is_empty() {
                        detail = format!("engine exited with status {status}");
                    }
                    if detail.len() > 300 {
                        detail.truncate(300);
                    }
                    return Err(format!("engine `{}` failed: {detail}", bin.display()));
                }
                return Ok(stdout);
            }
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!(
                        "engine `{}` timed out after {timeout_secs}s",
                        bin.display()
                    ));
                }
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
            Err(e) => {
                let _ = child.kill();
                return Err(format!("could not poll engine `{}`: {e}", bin.display()));
            }
        }
    }
}
