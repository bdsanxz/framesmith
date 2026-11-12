//! Persistent render journal (Sprint 26, T003 / CAP-007):
//! a JSON file under the project's hidden state that records the
//! chunk plan, every started and finished chunk, and every
//! interruption, so an interrupted run can resume exactly the
//! unfinished chunks. Writes are atomic (temp + rename) so
//! concurrent readers see complete files.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Status of one journaled chunk.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ChunkStatus {
    Planned,
    Started,
    Finished,
}

/// One chunk row inside the journal.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JournalChunk {
    pub index: usize,
    pub start_frame: u64,
    pub end_frame: u64,
    pub status: ChunkStatus,
    pub artifact: Option<String>,
    pub frames: Option<u64>,
    pub bytes: Option<u64>,
}

/// One interruption or resume marker with run identity.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Interruption {
    pub run: String,
    pub ts: String,
    pub kind: String,
    pub detail: String,
}

/// Overall journal status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum JournalStatus {
    InProgress,
    Finished,
}

/// Persistent render journal for one chunked output.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Journal {
    pub version: u32,
    pub output: String,
    pub total_frames: usize,
    pub status: JournalStatus,
    pub run: String,
    pub created_at: String,
    pub finished_at: Option<String>,
    pub chunks: Vec<JournalChunk>,
    pub interruptions: Vec<Interruption>,
}

pub fn now_iso() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = (secs / 86_400) as i64;
    let tod = secs % 86_400;
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    y += if m <= 2 { 1 } else { 0 };
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        y,
        m,
        d,
        tod / 3_600,
        (tod % 3_600) / 60,
        tod % 60
    )
}

fn atomic_write(path: &Path, data: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| {
                format!(
                    "cannot create journal directory {}: {e}",
                    parent.display()
                )
            })?;
        }
    }
    let mut tmp_name = path.as_os_str().to_owned();
    tmp_name.push(".tmp");
    // Include pid to avoid concurrent writers clobbering each other's temp.
    let mut tmp = PathBuf::from(tmp_name);
    // Append pid suffix via file name extension trick: a distinct tmp file.
    let pid = std::process::id();
    let s = tmp.display().to_string();
    tmp = PathBuf::from(format!("{s}-{pid}"));
    std::fs::write(&tmp, data).map_err(|e| {
        format!("cannot write journal temp {}: {e}", tmp.display())
    })?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("cannot rename journal temp {} to {}: {e}", tmp.display(), path.display())
    })
}

impl Journal {
    /// Create a fresh journal for `output` with `chunks` planned.
    pub fn create(
        output: &str,
        total_frames: usize,
        chunks: &[crate::chunk::ChunkRange],
        run: &str,
    ) -> Journal {
        let ts = now_iso();
        Journal {
            version: 1,
            output: output.to_string(),
            total_frames,
            status: JournalStatus::InProgress,
            run: run.to_string(),
            created_at: ts,
            finished_at: None,
            chunks: chunks
                .iter()
                .map(|c| JournalChunk {
                    index: c.index,
                    start_frame: c.start_frame,
                    end_frame: c.end_frame,
                    status: ChunkStatus::Planned,
                    artifact: None,
                    frames: None,
                    bytes: None,
                })
                .collect(),
            interruptions: Vec::new(),
        }
    }

    /// Load the journal at `path`. A missing or corrupt file reports
    /// a string naming the path and the cause; nothing is guessed.
    pub fn load(path: &Path) -> Result<Journal, String> {
        let body = std::fs::read_to_string(path).map_err(|e| {
            format!("cannot read render journal {}: {e}", path.display())
        })?;
        let j: Journal = serde_json::from_str(&body).map_err(|e| {
            format!("cannot parse render journal {}: {e}", path.display())
        })?;
        if j.version != 1 {
            return Err(format!(
                "render journal {} has unsupported version {}: expected 1",
                path.display(),
                j.version
            ));
        }
        Ok(j)
    }

    /// Persist the journal atomically (temp + rename) so concurrent
    /// readers see complete files.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let data = serde_json::to_string_pretty(self).map_err(|e| {
            format!("cannot encode render journal {}: {e}", path.display())
        })?;
        atomic_write(path, &data).map_err(|e| {
            format!("cannot write render journal {}: {e}", path.display())
        })
    }

    /// Mark chunk `index` as started, if present.
    pub fn mark_started(&mut self, index: usize) {
        if let Some(entry) = self.chunks.iter_mut().find(|c| c.index == index) {
            if entry.status != ChunkStatus::Finished {
                entry.status = ChunkStatus::Started;
            }
        }
    }

    /// Mark chunk `index` as finished with its artifact identity.
    pub fn mark_finished(
        &mut self,
        index: usize,
        artifact: &Path,
        bytes: u64,
        frames: u64,
    ) {
        if let Some(entry) = self.chunks.iter_mut().find(|c| c.index == index) {
            entry.status = ChunkStatus::Finished;
            entry.artifact = Some(artifact.display().to_string());
            entry.frames = Some(frames);
            entry.bytes = Some(bytes);
        }
    }

    /// Record an interruption (or resume marker) with run identity.
    pub fn record_interruption(&mut self, run: &str, kind: &str, detail: &str) {
        self.interruptions.push(Interruption {
            run: run.to_string(),
            ts: now_iso(),
            kind: kind.to_string(),
            detail: detail.to_string(),
        });
    }

    /// True when the journal reports the run finished.
    pub fn is_finished(&self) -> bool {
        self.status == JournalStatus::Finished
    }

    /// Mark the journal finished (called after successful reassembly).
    pub fn mark_finished_global(&mut self, run: &str) {
        self.status = JournalStatus::Finished;
        self.finished_at = Some(now_iso());
        self.run = run.to_string();
    }

    /// Indices that still need rendering (status != finished). Gaps
    /// never resurrect finished chunks: a finished entry stays
    /// finished even if neighbouring entries are missing.
    pub fn unfinished(&self) -> Vec<usize> {
        self.chunks
            .iter()
            .filter(|c| c.status != ChunkStatus::Finished)
            .map(|c| c.index)
            .collect()
    }

    /// Find one entry by index.
    pub fn entry(&self, index: usize) -> Option<&JournalChunk> {
        self.chunks.iter().find(|c| c.index == index)
    }
}

/// Journal path for `final_path` under the project's hidden state:
/// `<project>/.framesmith/render-journal/<stem>.journal.json` where
/// `<stem>` is the final file's file name stem (so Windows separators
/// and `.exe` tool paths stay correct via path joins).
pub fn journal_path_for(project_folder: &Path, final_path: &Path) -> PathBuf {
    let stem = final_path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "render".to_string());
    // Keep the original file name (including extension) as the
    // journal stem so two outputs with the same stem but different
    // extensions get distinct journals, e.g. `clip.mp4.journal.json`.
    let leaf = if stem == "render" {
        "render.journal.json".to_string()
    } else if stem.contains('.') {
        // stem is actually the full file name (e.g. "clip.mp4")
        format!("{stem}.journal.json")
    } else {
        format!("{stem}.journal.json")
    };
    let mut dir = project_folder.join(".framesmith").join("render-journal");
    // Ensure the leaf does not escape the hidden-state dir via a
    // hostile output name (e.g. "../evil"). Fall back to a hashed name.
    let leaf_path = Path::new(&leaf);
    let file_name = leaf_path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "render.journal.json".to_string());
    dir.push(file_name);
    dir
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_plan_records_all_chunks() {
        let ranges = vec![
            crate::chunk::ChunkRange { index: 0, start_frame: 0, end_frame: 15 },
            crate::chunk::ChunkRange { index: 1, start_frame: 15, end_frame: 30 },
        ];
        let j = Journal::create("out.mp4", 30, &ranges, "run-1-1");
        assert_eq!(j.chunks.len(), 2);
        assert_eq!(j.chunks[0].status, ChunkStatus::Planned);
        assert!(!j.is_finished());
        assert_eq!(j.unfinished(), vec![0, 1]);
    }

    #[test]
    fn gaps_never_resurrect_finished_chunks() {
        let ranges = vec![
            crate::chunk::ChunkRange { index: 0, start_frame: 0, end_frame: 10 },
            crate::chunk::ChunkRange { index: 1, start_frame: 10, end_frame: 20 },
            crate::chunk::ChunkRange { index: 2, start_frame: 20, end_frame: 30 },
        ];
        let mut j = Journal::create("out.mp4", 30, &ranges, "run-1-1");
        j.mark_finished(0, Path::new("a.mp4"), 100, 10);
        j.mark_finished(2, Path::new("c.mp4"), 300, 10);
        // Gap at 1 (still planned) must not change finished set.
        assert_eq!(j.unfinished(), vec![1]);
        assert_eq!(j.entry(0).unwrap().status, ChunkStatus::Finished);
        assert_eq!(j.entry(2).unwrap().status, ChunkStatus::Finished);
    }

    #[test]
    fn atomic_roundtrip_via_temp_rename() {
        let dir = std::env::temp_dir().join(format!(
            "framesmith-journal-test-{}-{}",
            std::process::id(),
            now_iso().replace(':', "-").replace('T', "-")
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("journal.json");
        let ranges = vec![crate::chunk::ChunkRange { index: 0, start_frame: 0, end_frame: 10 }];
        let mut j = Journal::create("out.mp4", 10, &ranges, "run-1-1");
        j.mark_started(0);
        j.save(&path).unwrap();
        // Temp file must not remain beside the journal.
        let leftovers: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert!(!leftovers.iter().any(|n| n.contains(".tmp")), "{leftovers:?}");
        let loaded = Journal::load(&path).unwrap();
        assert_eq!(loaded.chunks[0].status, ChunkStatus::Started);
        assert!(path.is_file(), "complete journal file lands");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupt_journal_reports_the_file() {
        let dir = std::env::temp_dir().join(format!("framesmith-journal-corrupt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("journal.json");
        std::fs::write(&path, "{ not json").unwrap();
        let e = Journal::load(&path).unwrap_err();
        assert!(e.contains(&path.display().to_string()), "{e}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn journal_path_lives_under_hidden_state() {
        let p = journal_path_for(Path::new("/tmp/proj"), Path::new("/tmp/out/clip.mp4"));
        let s = p.display().to_string().replace('\\', "/");
        assert!(s.contains(".framesmith/render-journal"), "{s}");
        assert!(s.contains("clip.mp4.journal.json"), "{s}");
    }
}
