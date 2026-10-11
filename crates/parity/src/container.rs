//! Container facts: the video stream of each file, read by type, compared fact by fact.

use crate::report::{Report, Verdict};
use measure::Facts;
use std::path::Path;

fn rate_parts(text: &str) -> Option<(u64, u64)> {
    let (n, d) = text.split_once('/').unwrap_or((text, "1"));
    Some((n.trim().parse().ok()?, d.trim().parse().ok()?))
}

/// Whether two frame rates written as `n/d` are the same rate.
pub fn same_rate(a: &str, b: &str) -> bool {
    match (rate_parts(a), rate_parts(b)) {
        (Some((an, ad)), Some((bn, bd))) if ad != 0 && bd != 0 => an * bd == bn * ad,
        _ => a == b,
    }
}

/// Compares the facts of two videos. `ours` and `theirs` are only used in the wording.
pub fn compare(ours: &Facts, theirs: &Facts) -> Report {
    let mut report = Report::new("container facts: ours (first) against the reference (second)");
    let verdict = |same: bool| if same { Verdict::Pass } else { Verdict::Fail };
    let both = |a: String, b: String| format!("ours {a}, reference {b}");
    report.check(
        "frame count",
        verdict(ours.frames == theirs.frames),
        both(ours.frames.to_string(), theirs.frames.to_string()),
    );
    report.check(
        "frame rate",
        verdict(same_rate(&ours.frame_rate, &theirs.frame_rate)),
        both(ours.frame_rate.clone(), theirs.frame_rate.clone()),
    );
    report.check(
        "size",
        verdict((ours.width, ours.height) == (theirs.width, theirs.height)),
        both(
            format!("{}x{}", ours.width, ours.height),
            format!("{}x{}", theirs.width, theirs.height),
        ),
    );
    report.check(
        "codec",
        verdict(ours.codec == theirs.codec),
        both(ours.codec.clone(), theirs.codec.clone()),
    );
    report.check(
        "pixel format",
        verdict(ours.pixel_format == theirs.pixel_format),
        both(ours.pixel_format.clone(), theirs.pixel_format.clone()),
    );

    // Everything else is information. Audio is not compared in this sprint.
    report.not_compared.push(format!(
        "audio: ours has {} audio stream(s), reference has {}; audio is not compared in this sprint",
        ours.audio_streams, theirs.audio_streams
    ));
    if ours.video_streams != theirs.video_streams {
        report.info.push(format!(
            "video streams: ours {}, reference {}",
            ours.video_streams, theirs.video_streams
        ));
    }
    let mut keys: Vec<&String> = ours.other.keys().chain(theirs.other.keys()).collect();
    keys.sort();
    keys.dedup();
    for key in keys {
        let (a, b) = (ours.other.get(key), theirs.other.get(key));
        if a != b {
            report.info.push(format!(
                "{key}: ours {}, reference {}",
                a.map_or("(none)", String::as_str),
                b.map_or("(none)", String::as_str)
            ));
        }
    }
    if (ours.duration - theirs.duration).abs() > 1e-6 {
        report.info.push(format!(
            "video length: ours {:.6} s, reference {:.6} s",
            ours.duration, theirs.duration
        ));
    }
    if ours.index_first != theirs.index_first {
        report.info.push(format!(
            "index before picture data: ours {}, reference {}",
            ours.index_first, theirs.index_first
        ));
    }
    report
}

/// Reads both files and compares them.
pub fn compare_files(ffprobe: &Path, ours: &Path, theirs: &Path) -> Result<Report, String> {
    let a = measure::facts(ffprobe, ours)?;
    let b = measure::facts(ffprobe, theirs)?;
    Ok(compare(&a, &b))
}
