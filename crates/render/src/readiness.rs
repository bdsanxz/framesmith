//! Prepare-time adapter source gate (Sprint 21 adapters, T007 /
//! CAP-007, STATE-03, REC-001).
//!
//! Validation decodes every adapter source, but a file can vanish
//! between `check` and `render`, and library callers can prepare an
//! unchecked folder. This gate re-resolves every file-backed adapter
//! source before the first frame and refuses naming the file and
//! cause — never a partial MP4, never per-frame work. The per-arm
//! decodes below keep their detailed refusals; this gate only adds
//! the uniform vanished-source refusal every file-backed kind
//! shares. Chart data and scene geometry ride in the validated clip,
//! so the gate checks their external files (chart `data`) and skips
//! authored-only sources (scene3d); a still-loading registry source
//! blocks by name through
//! [`crate::custom_source::CustomSourceRegistry::render_at`], and
//! [`crate::seek::wait_for_ready`] waits it out at seek time.
//!
//! Pure standard library plus the notation model: no browser, no
//! JavaScript, no network at render time.

use notation::model::{Clip, Composition};
use std::path::Path;

/// One vanished file-backed adapter source: the kind, the clip name,
/// and the file that is no longer readable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VanishedSource {
    /// Adapter kind (`lottie`, `chart`, `map`).
    pub kind: &'static str,
    /// Clip name for the refusal line.
    pub name: String,
    /// Source path as written.
    pub source: String,
}

impl VanishedSource {
    /// Refusal naming the file and cause, before the first frame, so
    /// a source lost after validation reads exactly like any other
    /// undecodable source: file first, cause second.
    pub fn refusal(&self, cause: &str) -> String {
        format!(
            "render refused: {} source {:?} is no longer readable ({cause}); no MP4 was written",
            self.kind, self.source
        )
    }
}

/// Reviewable clip name (Sprint 21 adapters, T007): the written `id`
/// when present, else the `<kind clip N>` label validation prints.
fn clip_name(clips: &[Clip], index: usize, kind: &'static str) -> String {
    match clips[index].id() {
        Some(id) => format!("{kind} {id:?}"),
        None => format!("<{kind} clip {}>", index + 1),
    }
}

/// One scan over every file-backed adapter source: the vanished
/// source plus the operating-system cause, in document order with
/// repeat references to one file deduplicated. Both public queries
/// below read this scan, so the inspector listing and the render
/// gate always agree on the same sources.
fn scan_vanished(folder: &Path, comps: &[&Composition]) -> Vec<(VanishedSource, String)> {
    let mut out = Vec::new();
    for comp in comps {
        for (index, clip) in comp.clips.iter().enumerate() {
            // One entry per file-backed source of the clip: caption
            // tracks resolve both their source file and their subject
            // matte (Sprint 22 captions, T001/T003); every other
            // file-backed kind resolves its single source.
            let mut sources: Vec<(&'static str, String)> = Vec::new();
            match clip {
                Clip::Lottie(lo) => sources.push(("lottie", lo.path.clone())),
                Clip::Chart(ch) => match ch.data_file.clone() {
                    Some(data) => sources.push(("chart", data)),
                    None => continue,
                },
                Clip::Map(m) => sources.push(("map", m.pack.clone())),
                Clip::Caption(cc) => {
                    if let Some(src) = &cc.src {
                        sources.push(("captions", src.clone()));
                    }
                    if let Some(matte) = &cc.matte {
                        sources.push(("captions", matte.clone()));
                    }
                    if sources.is_empty() {
                        continue;
                    }
                }
                _ => continue,
            };
            for (kind, source) in sources {
                push_vanished(folder, &comp.clips, index, kind, source, &mut out);
            }
        }
    }
    out
}

/// Probe one file-backed adapter source for the vanished-source scan
/// (Sprint 22 captions, T001/T003): a source lost after validation is
/// recorded with the operating-system cause, deduplicated per source.
/// Both public queries below read the same scan, so the inspector
/// listing and the render gate always agree on the same sources.
fn push_vanished(
    folder: &Path,
    clips: &[Clip],
    index: usize,
    kind: &'static str,
    source: String,
    out: &mut Vec<(VanishedSource, String)>,
) {
    if let Err(e) = std::fs::metadata(folder.join(&source)) {
        // Deduplicate repeat references to one file: one entry per
        // source, never per clip or per frame.
        if out
            .iter()
            .any(|(v, _): &(VanishedSource, String)| v.source == source)
        {
            return;
        }
        out.push((
            VanishedSource {
                kind,
                name: clip_name(clips, index, kind),
                source,
            },
            e.to_string(),
        ));
    }
}

/// File-backed adapter sources of these compositions that are no
/// longer readable from `folder`: at most one entry per source, in
/// document order, so the report lists per source, never per frame.
/// Empty for projects whose sources all resolve — the common case —
/// so default renders pay one metadata probe per file-backed source
/// and change nothing else.
pub fn vanished_sources(folder: &Path, comps: &[&Composition]) -> Vec<VanishedSource> {
    scan_vanished(folder, comps)
        .into_iter()
        .map(|(vanished, _)| vanished)
        .collect()
}

/// Refuse vanished adapter sources before the first frame (Sprint 21
/// adapters, T007): the first vanished source refuses naming its
/// file and cause, so a project whose bytes changed after validation
/// reads exactly like an undecodable source. `Ok` when every
/// file-backed source still resolves — valid renders behave exactly
/// as before.
pub fn gate_adapter_sources(folder: &Path, comps: &[&Composition]) -> Result<(), String> {
    match scan_vanished(folder, comps).into_iter().next() {
        Some((vanished, cause)) => Err(vanished.refusal(&cause)),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_and_source_free_projects_pass() {
        let dir = std::env::temp_dir();
        assert!(gate_adapter_sources(&dir, &[]).is_ok());
        assert!(vanished_sources(&dir, &[]).is_empty());
    }
}
