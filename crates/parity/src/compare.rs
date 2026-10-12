//! The whole comparison of one pair of videos in one report: container facts, pictures, the timing of
//! every clip of a chain, and the position of a moving element.
//!
//! The form of the report (the specification of this module; the agreed checks and limits are those of
//! section 5 of the plan, decision D5):
//!
//! * One line per check, with the group the check belongs to, a verdict (`pass`, `fail` or `not run`),
//!   what was measured, and the agreed limit the measure is held to, written `| threshold: ...`.
//! * The groups and their limits:
//!   - `container`: frame count, frame rate, size, codec and pixel format; threshold `identical`.
//!   - `pictures`: one line per sampled frame, then one line for the lowest of them; the similarity (SSIM)
//!     of the frame in both videos; threshold `at least 0.97`. The side-by-side sheet shows the same frames.
//!   - `timing`: for each named clip, the frame on which it appears and the frame on which it disappears,
//!     in both videos; threshold `at most 1 frame apart`.
//!   - `position`: the largest distance between the two tracked centres of the moving element over all
//!     frames; threshold `at most 1 px apart on every frame`.
//! * When a check fails, a closing section `failed checks` repeats each failed check with its measured
//!   value and its threshold, so that nothing has to be searched for.
//! * The last line is `result: passed`, `result: failed` or `result: not run`. The exit code is 0, 1 or 3.
//!   A comparison that could not measure a check never says passed.
//! * Audio is not compared: the comparison covers the video stream only.

use crate::report::Report;
use crate::tools::Tools;
use crate::{container, pictures, timing};
use std::path::Path;

/// The limits of the agreed checks, in words (the numbers themselves are those of each group's module).
pub const CONTAINER_THRESHOLD: &str = "identical";

#[derive(Clone, Debug, Default)]
pub struct Options {
    pub pictures: pictures::Options,
    pub timing: timing::Options,
}

/// Writes the group before the name of every check of a report; the lines that stand for a whole group
/// ("pictures", "position") are named for what they stand for.
fn grouped(mut report: Report, group: &str) -> Report {
    for line in &mut report.checks {
        line.name = match (group, line.name.as_str()) {
            ("pictures", "pictures") => "pictures: lowest of the sampled frames".to_string(),
            ("timing", "position") => "position: moving element".to_string(),
            _ => format!("{group}: {}", line.name),
        };
    }
    report
}

fn timing_limit() -> String {
    format!("at most {} frame apart", timing::EVENT_LIMIT_FRAMES)
}

fn position_limit() -> String {
    format!("at most {} px apart on every frame", timing::POSITION_LIMIT_PIXELS)
}

/// Compares two videos on every group of checks.
pub fn compare_files(tools: &Tools, ours: &Path, theirs: &Path, options: &Options) -> Result<Report, String> {
    let asks_timing = options.timing.clip.is_some() || !options.timing.named_clips.is_empty() || options.timing.mover.is_some();
    let mut report = Report::new("pair comparison: ours (first) against the reference (second), video stream only; frames count from 0");

    let mut facts = grouped(container::compare_files(&tools.ffprobe, ours, theirs)?, "container");
    facts.state_thresholds(|_| CONTAINER_THRESHOLD.to_string());

    let mut pics = grouped(pictures::compare(tools, ours, theirs, &options.pictures)?, "pictures");
    let ssim = format!("ssim at least {:.2}", options.pictures.threshold);
    pics.state_thresholds(|name| {
        if name == "pictures: pairing" {
            "the same frame count and size in both videos".to_string()
        } else {
            ssim.clone()
        }
    });

    let mut parts = vec![facts, pics];
    if asks_timing {
        let mut seen = grouped(timing::compare_files(tools, ours, theirs, &options.timing)?, "timing");
        seen.state_thresholds(|name| {
            if name.starts_with("position:") {
                position_limit()
            } else {
                timing_limit()
            }
        });
        parts.push(seen);
    }
    for part in parts {
        report.checks.extend(part.checks);
        report.info.extend(part.info);
        report.not_compared.extend(part.not_compared);
        report.notes.extend(part.notes);
    }
    Ok(report)
}
