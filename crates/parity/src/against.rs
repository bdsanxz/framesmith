//! The comparison against a render the reference makes: it runs the reference on a counterpart
//! composition, then compares its video with ours. With the reference absent nothing is run and the
//! report says so, check by check (`docs/dev/comparison.md`, sections 8 and 9).

use crate::blackbox::{self, Job, Outcome};
use crate::compare::{compare, Options};
use crate::report::{Report, Verdict};
use std::path::Path;

/// The checks that need the reference's render, in the order of the report.
const NEEDING_THE_RENDER: [&str; 7] = ["frame count", "frame rate", "size", "length", "picture", "events", "tracking"];

fn without_a_render(title: String, render: Verdict, detail: String) -> Report {
    let mut report = Report::new(title);
    report.check("reference render", render, detail);
    for name in NEEDING_THE_RENDER {
        report.check(name, Verdict::NotRun, "needs the reference's render");
    }
    report
}

/// The report for a reference that is not installed or cannot start: every check is `not run`.
pub fn not_run(title: String, why: &str) -> Report {
    without_a_render(title, Verdict::NotRun, format!("{why} (nothing was run)"))
}

/// Runs the reference as `job` says and compares its video with `ours`.
pub fn against(job: &Job, ours: &Path, options: &Options) -> Result<Report, String> {
    let title = format!(
        "parity: ours ({}) against the reference's render of {}",
        ours.display(),
        job.counterpart.display()
    );
    if let Err(why) = blackbox::absence(&job.blackbox, &std::env::var_os("PATH").unwrap_or_default()) {
        return Ok(not_run(title, &why));
    }
    match blackbox::run(job) {
        Outcome::NotRun(why) => Ok(not_run(title, &why)),
        Outcome::Failed(why) => Ok(without_a_render(title, Verdict::Fail, why)),
        Outcome::Rendered { version } => {
            let mut report = compare(ours, &job.output, options)?;
            report.title = title;
            report.checks.insert(
                0,
                crate::report::Line {
                    name: "reference render".to_string(),
                    verdict: Verdict::Pass,
                    detail: format!("reference version {version} wrote {}", job.output.display()),
                },
            );
            Ok(report)
        }
    }
}
