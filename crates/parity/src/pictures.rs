//! Picture similarity at a fixed, repeatable set of frames, and the side-by-side sheet.

use crate::frames::each_frame;
use crate::report::{Report, Verdict};
use crate::sheet::{self, SheetEntry};
use crate::tools::Tools;
use measure::Frame;
use std::path::{Path, PathBuf};

/// The agreed threshold: SSIM of at least this on every sampled frame.
pub const THRESHOLD: f64 = 0.97;
/// How many frames are sampled when nothing else is asked.
pub const SAMPLES: usize = 8;

#[derive(Clone, Debug)]
pub struct Options {
    pub samples: usize,
    /// Frames to look at instead of the evenly spaced ones: counting from 0, each less than the frame count.
    pub frames: Option<Vec<usize>>,
    pub threshold: f64,
    /// Where to write the side-by-side sheet, if wanted.
    pub sheet: Option<PathBuf>,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            samples: SAMPLES,
            frames: None,
            threshold: THRESHOLD,
            sheet: None,
        }
    }
}

/// The frames to look at: the first, the last and evenly spaced ones between, at most `count`.
/// The same total and count always give the same list.
pub fn sample_indices(total: usize, count: usize) -> Vec<usize> {
    if total == 0 || count == 0 {
        return Vec::new();
    }
    if count == 1 || total == 1 {
        return vec![0];
    }
    let count = count.min(total);
    let mut picked: Vec<usize> = (0..count).map(|i| (i * (total - 1) + (count - 1) / 2) / (count - 1)).collect();
    picked.dedup();
    picked
}

fn read_samples(tools: &Tools, video: &Path, width: u32, height: u32, wanted: &[usize]) -> Result<Vec<Frame>, String> {
    let mut found: Vec<Frame> = Vec::new();
    each_frame(&tools.ffmpeg, video, width, height, |at, frame| {
        if wanted.contains(&at) {
            found.push(frame);
        }
    })?;
    if found.len() != wanted.len() {
        return Err(format!(
            "{} gave {} of the {} sampled frames",
            video.display(),
            found.len(),
            wanted.len()
        ));
    }
    Ok(found)
}

/// Compares the pictures of two videos at the sampled frames.
pub fn compare(tools: &Tools, ours: &Path, theirs: &Path, options: &Options) -> Result<Report, String> {
    let a = measure::facts(&tools.ffprobe, ours)?;
    let b = measure::facts(&tools.ffprobe, theirs)?;
    let mut report = Report::new("pictures: ours (first) against the reference (second), SSIM on brightness");
    if a.frames != b.frames {
        report.check(
            "pairing",
            Verdict::Fail,
            format!("frames cannot be paired: ours has {} frames, reference has {}", a.frames, b.frames),
        );
        report.check("pictures", Verdict::Fail, "no picture was compared, so this cannot pass");
        return Ok(report);
    }
    if (a.width, a.height) != (b.width, b.height) {
        report.check(
            "pairing",
            Verdict::Fail,
            format!(
                "frames cannot be paired: ours is {}x{}, reference is {}x{}",
                a.width, a.height, b.width, b.height
            ),
        );
        report.check("pictures", Verdict::Fail, "no picture was compared, so this cannot pass");
        return Ok(report);
    }
    let total = a.frames as usize;
    let indices = match &options.frames {
        Some(asked) => {
            if let Some(beyond) = asked.iter().find(|&&n| n >= total) {
                return Err(format!(
                    "frame {beyond} was asked for, and the videos have {total} frames (counting from 0)"
                ));
            }
            let mut asked = asked.clone();
            asked.sort_unstable();
            asked.dedup();
            asked
        }
        None => sample_indices(total, options.samples),
    };
    if indices.is_empty() {
        return Err("there are no frames to sample".into());
    }
    let ours_frames = read_samples(tools, ours, a.width, a.height, &indices)?;
    let their_frames = read_samples(tools, theirs, b.width, b.height, &indices)?;

    let mut lowest: Option<(usize, f64)> = None;
    let mut entries = Vec::new();
    for (k, &index) in indices.iter().enumerate() {
        let value = measure::similarity(&ours_frames[k], &their_frames[k]).ok_or("the frames are too small to compare")?;
        let verdict = if value >= options.threshold { Verdict::Pass } else { Verdict::Fail };
        report.check(
            format!("frame {index}"),
            verdict,
            format!("ssim {value:.5} (threshold {:.2})", options.threshold),
        );
        if lowest.is_none_or(|(_, v)| value < v) {
            lowest = Some((index, value));
        }
        entries.push(SheetEntry {
            frame: index,
            ssim: value,
            ours: &ours_frames[k],
            theirs: &their_frames[k],
        });
    }
    let (low_frame, low_value) = lowest.ok_or("no frame was measured")?;
    if low_value >= options.threshold {
        report.check(
            "pictures",
            Verdict::Pass,
            format!(
                "all {} sampled frames are at or above {:.2}; lowest is frame {low_frame} at {low_value:.5}",
                indices.len(),
                options.threshold
            ),
        );
    } else {
        report.check(
            "pictures",
            Verdict::Fail,
            format!(
                "lowest-scoring frame is {low_frame} at {low_value:.5}, below the threshold {:.2}",
                options.threshold
            ),
        );
    }
    report
        .notes
        .push(format!("sampled frames (counting from 0): {indices:?} of {total}"));
    if let Some(path) = &options.sheet {
        sheet::write(&tools.ffmpeg, &entries, path)?;
        report.notes.push(format!(
            "side-by-side sheet: {} (left of each pair is ours, right is the reference)",
            path.display()
        ));
    }
    Ok(report)
}
