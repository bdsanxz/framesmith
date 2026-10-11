//! The seek measurement: how long the page takes to put itself at a frame.
//!
//! The page times each seek itself with the one real timer the loader keeps
//! (`costs()` in `docs/spec/page-protocol.md`). Painting, capture and
//! encoding happen outside that span and are not counted.

use std::path::Path;
use std::process::Command;

/// A seek must take less than this, in milliseconds.
pub const LIMIT_MS: f64 = 2.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Summary {
    pub frames: usize,
    pub largest: f64,
    /// The frame whose seek took longest.
    pub largest_frame: usize,
    /// The middle value: half the seeks took no longer than this.
    pub typical: f64,
    pub pass: bool,
}

/// `None` when nothing was measured.
pub fn summarise(costs: &[f64]) -> Option<Summary> {
    let (largest_frame, &largest) = costs.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1))?;
    let mut sorted = costs.to_vec();
    sorted.sort_by(f64::total_cmp);
    Some(Summary {
        frames: costs.len(),
        largest,
        largest_frame,
        typical: sorted[sorted.len() / 2],
        pass: largest < LIMIT_MS,
    })
}

/// The report: one line for each frame, then the summary and the result.
pub fn report(costs: &[f64]) -> String {
    let mut text = String::from("seek measurement: real time inside the page for each seek, in milliseconds\n");
    for (frame, cost) in costs.iter().enumerate() {
        text.push_str(&format!("  frame {frame:>4}  {cost:.3}\n"));
    }
    match summarise(costs) {
        Some(summary) => {
            text.push_str(&format!(
                "summary: {} frames, largest {:.3} ms (frame {}), typical {:.3} ms, limit under {LIMIT_MS} ms\n",
                summary.frames, summary.largest, summary.largest_frame, summary.typical
            ));
            text.push_str(if summary.pass {
                "result: pass\n"
            } else {
                "result: fail (a seek took 2 milliseconds or longer)\n"
            });
        }
        None => text.push_str("summary: no frame was measured\nresult: fail (nothing to judge)\n"),
    }
    text
}

/// The same as one JSON document.
pub fn json(costs: &[f64]) -> String {
    let summary = summarise(costs);
    let document = serde_json::json!({
        "measurement": "seek",
        "unit": "ms",
        "limit": LIMIT_MS,
        "frames": costs,
        "largest": summary.map(|s| s.largest),
        "largest_frame": summary.map(|s| s.largest_frame),
        "typical": summary.map(|s| s.typical),
        "result": if summary.is_some_and(|s| s.pass) { "pass" } else { "fail" },
    });
    format!("{document}\n")
}

/// Renders the project with `framesmith` and returns what each seek cost,
/// in frame order. The video and the record are written under `work`.
pub fn measure(framesmith: &Path, project: &Path, work: &Path) -> Result<Vec<f64>, String> {
    std::fs::create_dir_all(work).map_err(|e| format!("{} could not be made ({e})", work.display()))?;
    let record = work.join("seek-costs.json");
    let _ = std::fs::remove_file(&record);
    let ran = Command::new(framesmith)
        .arg("render")
        .arg(project)
        .arg("--output")
        .arg(work.join("seek.mp4"))
        .env("FRAMESMITH_DEBUG_COSTS", &record)
        .env_remove("FRAMESMITH_DEBUG_ORDER")
        .output()
        .map_err(|e| format!("{} could not be run ({e})", framesmith.display()))?;
    if !ran.status.success() {
        return Err(format!(
            "`framesmith render {}` did not complete:\n{}{}",
            project.display(),
            String::from_utf8_lossy(&ran.stdout),
            String::from_utf8_lossy(&ran.stderr)
        ));
    }
    let text = std::fs::read_to_string(&record).map_err(|e| format!("the record {} could not be read ({e})", record.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("the record {} is not a list of numbers ({e})", record.display()))
}
