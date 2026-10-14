//! The comparison of two videos: the four structural checks, the picture check, the event check and
//! the tracking check, as one report (`docs/dev/comparison.md`, sections 1 to 7). Both videos are read with frx7; no program is
//! started.

use crate::difference;
use crate::picture::{self, Excuse, Judgement, Scene, State};
use crate::report::{Report, Verdict};
use crate::sheet::{self, SheetEntry};
use crate::structure;
use crate::tracking::{self, Marks, Seen};
use measure::{Frame, VideoFacts};
use serde_json::json;
use std::path::{Path, PathBuf};

/// The most frames shown on the sheet.
pub const SHEET_FRAMES: usize = 12;

/// What to compare by and what to write.
#[derive(Clone, Debug, Default)]
pub struct Options {
    pub scenes: Vec<Scene>,
    pub excuses: Vec<Excuse>,
    /// Lists the score of every frame in the text report (the JSON report always has them).
    pub per_frame: bool,
    /// Where to write the sheet of the low frames, if wanted.
    pub sheet: Option<PathBuf>,
    /// The elements the fixture marks for the event check and the tracking check.
    pub marks: Marks,
}

/// The lines of the report that name what was not compared here.
pub fn not_compared() -> Vec<String> {
    vec!["the sound (a check of its own)".to_string()]
}

/// One low frame kept for the sheet.
struct Kept {
    number: u64,
    score: String,
    state: &'static str,
    ours: Frame,
    theirs: Frame,
    mask: Vec<bool>,
}

fn facts_of(path: &Path) -> Result<VideoFacts, String> {
    measure::video_facts(path)
}

/// Compares the video `ours` with the reference's `theirs`.
pub fn compare(ours: &Path, theirs: &Path, options: &Options) -> Result<Report, String> {
    let a = facts_of(ours)?;
    let b = facts_of(theirs)?;
    let mut report = Report::new(format!(
        "parity: ours ({}) against the reference's render ({})",
        ours.display(),
        theirs.display()
    ));
    report.checks.extend(structure::checks(&a, &b));
    report.not_compared = not_compared();

    options.marks.check()?;
    // Pairing needs the same number of pictures and the same size.
    let unpaired = |report: &mut Report| {
        for name in ["events", "tracking"] {
            report.check(name, Verdict::NotRun, "needs frames that can be paired");
        }
    };
    if a.frames != b.frames {
        report.check(
            "picture",
            Verdict::Fail,
            format!("frames cannot be paired: ours has {} frames, reference has {}", a.frames, b.frames),
        );
        unpaired(&mut report);
        return Ok(report);
    }
    if (a.width, a.height) != (b.width, b.height) {
        report.check(
            "picture",
            Verdict::Fail,
            format!(
                "frames cannot be paired: ours is {}x{}, reference is {}x{}",
                a.width, a.height, b.width, b.height
            ),
        );
        unpaired(&mut report);
        return Ok(report);
    }
    if a.frames == 0 {
        report.check("picture", Verdict::Fail, "the videos have no frames to compare");
        unpaired(&mut report);
        return Ok(report);
    }

    let scenes = picture::scenes_for(&options.scenes, a.frames)?;
    picture::check_excuses(&options.excuses, a.frames)?;

    let mut first = measure::open_pictures(ours)?;
    let mut second = measure::open_pictures(theirs)?;
    for (name, reader) in [(ours, &first), (theirs, &second)] {
        if reader.count() != a.frames {
            return Err(format!(
                "`{}` holds {} pictures by its headers and {} by the decoder",
                name.display(),
                a.frames,
                reader.count()
            ));
        }
    }
    let sheet_wanted = options.sheet.is_some();
    let mut counts: Vec<(u64, u64)> = Vec::with_capacity(a.frames as usize);
    let mut shown: Vec<Kept> = Vec::new();
    let (mut seen_ours, mut seen_theirs): (Vec<Seen>, Vec<Seen>) = (Vec::new(), Vec::new());
    for number in 0..a.frames {
        let (Some(x), Some(y)) = (first.next_frame()?, second.next_frame()?) else {
            return Err(format!("a video ended at frame {number} of {}", a.frames));
        };
        tracking::observe(&options.marks, &x, &mut seen_ours);
        tracking::observe(&options.marks, &y, &mut seen_theirs);
        let found = difference::compare(&x, &y)?;
        counts.push((found.pixels, found.differing));
        if sheet_wanted && shown.len() < SHEET_FRAMES && picture::is_low(found.pixels, found.differing) {
            let cause = options.excuses.iter().any(|e| e.first <= number && number <= e.last);
            shown.push(Kept {
                number,
                score: picture::percent(found.pixels, found.differing),
                state: if cause { "excused" } else { "fail" },
                ours: x.clone(),
                theirs: y.clone(),
                mask: found.mask,
            });
        }
    }
    let judgement = picture::judge(&counts, &scenes, &options.excuses);
    describe(&mut report, &judgement, options);
    tracking::event_check(&mut report, &options.marks, &seen_ours, &seen_theirs);
    tracking::tracking_check(&mut report, &options.marks, &seen_ours, &seen_theirs, options.per_frame);
    if let Some(path) = &options.sheet {
        if shown.is_empty() {
            report.notes.push("no sheet was written: no frame is under 99 %".to_string());
        } else {
            let entries: Vec<SheetEntry> = shown
                .iter()
                .map(|k| SheetEntry {
                    frame: k.number,
                    score: k.score.clone(),
                    state: k.state,
                    ours: &k.ours,
                    theirs: &k.theirs,
                    mask: &k.mask,
                })
                .collect();
            sheet::write(&entries, path)?;
            report.notes.push(format!(
                "sheet: {} ({} of the {} frames under 99 %: ours, the reference's, and the pixels that differ)",
                path.display(),
                shown.len(),
                judgement.low().count()
            ));
        }
    }
    Ok(report)
}

fn verdict(passes: bool) -> Verdict {
    if passes {
        Verdict::Pass
    } else {
        Verdict::Fail
    }
}

fn frames_word(count: u64) -> String {
    format!("{count} frame{}", if count == 1 { "" } else { "s" })
}

/// Puts the picture check, the scenes and the lists into the report.
fn describe(report: &mut Report, judgement: &Judgement, options: &Options) {
    let low: Vec<_> = judgement.low().collect();
    let excused = low.iter().filter(|f| matches!(f.state, State::Excused(_))).count();
    let lowest = judgement.lowest();
    let lowest_text = lowest.map_or(String::new(), |f| {
        format!("; lowest frame {} at {} %", f.number, picture::percent(f.pixels, f.differing))
    });
    let under = if low.is_empty() {
        "no frame under 99 %".to_string()
    } else {
        format!("{} under 99 % ({} excused)", frames_word(low.len() as u64), excused)
    };
    report.check(
        "picture",
        verdict(judgement.passes()),
        format!(
            "average {} % over {}{lowest_text}; {under}",
            picture::percent(judgement.pixels, judgement.differing),
            frames_word(judgement.frames.len() as u64)
        ),
    );
    for result in &judgement.scenes {
        let scene = &result.scene;
        let (number, pixels, differing) = result.lowest;
        report.check(
            format!("scene {} (frames {}-{})", scene.name, scene.first, scene.last),
            verdict(result.passes()),
            format!(
                "average {} %; lowest frame {number} at {} %; {} under 99 % ({} excused, {} without a cause)",
                picture::percent(result.pixels, result.differing),
                picture::percent(pixels, differing),
                result.low,
                result.excused,
                result.failed
            ),
        );
    }
    report.section(
        "frames under 99 %",
        if low.is_empty() {
            vec!["none".to_string()]
        } else {
            low.iter()
                .map(|f| match &f.state {
                    State::Excused(cause) => format!(
                        "frame {}: {} %  excused (not passed): {cause}",
                        f.number,
                        picture::percent(f.pixels, f.differing)
                    ),
                    _ => format!("frame {}: {} %  fail", f.number, picture::percent(f.pixels, f.differing)),
                })
                .collect()
        },
    );
    if options.per_frame {
        report.section(
            "score of every frame",
            judgement
                .frames
                .iter()
                .map(|f| format!("frame {}: {} %", f.number, picture::percent(f.pixels, f.differing)))
                .collect(),
        );
    }
    for excuse in &options.excuses {
        let used = judgement
            .frames
            .iter()
            .any(|f| excuse.first <= f.number && f.number <= excuse.last && matches!(f.state, State::Excused(_)));
        if !used {
            report.notes.push(format!(
                "the cause '{}' for frames {}-{} was not used: none of them is under 99 %",
                excuse.cause, excuse.first, excuse.last
            ));
        }
    }
    report.data.push((
        "picture".to_string(),
        json!({
            "average": picture::score(judgement.pixels, judgement.differing),
            "frames": judgement.frames.len(),
            "passes": judgement.passes(),
            "scenes": judgement.scenes.iter().map(|r| json!({
                "name": r.scene.name,
                "first": r.scene.first,
                "last": r.scene.last,
                "average": picture::score(r.pixels, r.differing),
                "passes": r.passes(),
                "under_99": r.low,
                "excused": r.excused,
                "without_cause": r.failed,
            })).collect::<Vec<_>>(),
            "frame_scores": judgement.frames.iter().map(|f| json!({
                "frame": f.number,
                "score": picture::score(f.pixels, f.differing),
                "differing": f.differing,
                "pixels": f.pixels,
                "state": f.state.word(),
                "cause": match &f.state { State::Excused(cause) => json!(cause), _ => json!(null) },
            })).collect::<Vec<_>>(),
        }),
    ));
}
