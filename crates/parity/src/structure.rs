//! The four structural checks of two videos: frame count, frame rate, size and length
//! (`docs/dev/comparison.md`, section 3). Each is a line of the report with both values.

use crate::report::{Line, Verdict};
use measure::VideoFacts;

/// Whether two fractions are equal (30/1 and 60000/2000 are).
pub fn same_rate(ours: (u32, u32), theirs: (u32, u32)) -> bool {
    u64::from(ours.0) * u64::from(theirs.1) == u64::from(theirs.0) * u64::from(ours.1)
}

/// Whether two lengths, each a count of ticks of a clock, are the same length: no margin.
pub fn same_length(ours: &VideoFacts, theirs: &VideoFacts) -> bool {
    u128::from(ours.ticks) * u128::from(theirs.timescale) == u128::from(theirs.ticks) * u128::from(ours.timescale)
}

fn length(facts: &VideoFacts) -> String {
    format!("{:.6} s ({}/{})", facts.seconds(), facts.ticks, facts.timescale)
}

/// The four checks, in the order frame count, frame rate, size, length.
pub fn checks(ours: &VideoFacts, theirs: &VideoFacts) -> Vec<Line> {
    let line = |name: &str, same: bool, detail: String| Line {
        name: name.to_string(),
        verdict: if same { Verdict::Pass } else { Verdict::Fail },
        detail,
    };
    vec![
        line(
            "frame count",
            ours.frames == theirs.frames,
            format!("ours {}, reference {}", ours.frames, theirs.frames),
        ),
        line(
            "frame rate",
            same_rate(ours.rate, theirs.rate),
            format!(
                "ours {}/{}, reference {}/{}",
                ours.rate.0, ours.rate.1, theirs.rate.0, theirs.rate.1
            ),
        ),
        line(
            "size",
            (ours.width, ours.height) == (theirs.width, theirs.height),
            format!("ours {}x{}, reference {}x{}", ours.width, ours.height, theirs.width, theirs.height),
        ),
        line(
            "length",
            same_length(ours, theirs),
            format!("ours {}, reference {}", length(ours), length(theirs)),
        ),
    ]
}
