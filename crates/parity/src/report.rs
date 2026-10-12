//! What a comparison says: each check in words, then one result.

use serde_json::{json, Value};

/// The words a check may end in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Pass,
    Fail,
    NotRun,
}

impl Verdict {
    pub fn word(self) -> &'static str {
        match self {
            Verdict::Pass => "pass",
            Verdict::Fail => "fail",
            Verdict::NotRun => "not run",
        }
    }
}

/// One check and what was measured for it.
#[derive(Clone, Debug)]
pub struct Line {
    pub name: String,
    pub verdict: Verdict,
    /// What was measured.
    pub detail: String,
    /// The agreed limit the measured value is held to, in words; empty when the check does not state one.
    pub threshold: String,
}

/// A comparison's findings.
#[derive(Clone, Debug, Default)]
pub struct Report {
    pub title: String,
    pub checks: Vec<Line>,
    /// Facts that differ but are not on the agreed list; they never fail a comparison.
    pub info: Vec<String>,
    /// Things that were deliberately not compared.
    pub not_compared: Vec<String>,
    /// Anything else the maintainer should read.
    pub notes: Vec<String>,
}

/// Exit code when every asked check passed.
pub const EXIT_PASS: i32 = 0;
/// Exit code when a check failed.
pub const EXIT_FAIL: i32 = 1;
/// Exit code for a mistake in how the tool was called or a file that could not be read.
pub const EXIT_USAGE: i32 = 2;
/// Exit code when something could not be run at all (for example the reference is not installed).
pub const EXIT_NOT_RUN: i32 = 3;

impl Report {
    pub fn new(title: impl Into<String>) -> Report {
        Report {
            title: title.into(),
            ..Report::default()
        }
    }

    pub fn check(&mut self, name: impl Into<String>, verdict: Verdict, detail: impl Into<String>) {
        self.checks.push(Line {
            name: name.into(),
            verdict,
            detail: detail.into(),
            threshold: String::new(),
        });
    }

    /// Writes the agreed limit on every check that does not state one yet, using `limit` to choose it
    /// from the check's name.
    pub fn state_thresholds(&mut self, limit: impl Fn(&str) -> String) {
        for line in self.checks.iter_mut().filter(|l| l.threshold.is_empty()) {
            line.threshold = limit(&line.name);
        }
    }

    /// The verdict of the whole report: pass only when there is at least one check and all of them pass.
    pub fn overall(&self) -> Verdict {
        if self.checks.iter().any(|c| c.verdict == Verdict::Fail) {
            Verdict::Fail
        } else if self.checks.is_empty() || self.checks.iter().any(|c| c.verdict == Verdict::NotRun) {
            Verdict::NotRun
        } else {
            Verdict::Pass
        }
    }

    pub fn exit_code(&self) -> i32 {
        match self.overall() {
            Verdict::Pass => EXIT_PASS,
            Verdict::Fail => EXIT_FAIL,
            Verdict::NotRun => EXIT_NOT_RUN,
        }
    }

    /// The report as plain text, one check per line.
    pub fn text(&self) -> String {
        let mut out = format!("{}\n", self.title);
        let width = self.checks.iter().map(|c| c.name.len()).max().unwrap_or(0);
        for c in &self.checks {
            out.push_str(&format!("  {:<width$}  {:<7}  {}", c.name, c.verdict.word(), c.detail));
            if !c.threshold.is_empty() {
                out.push_str(&format!(" | threshold: {}", c.threshold));
            }
            out.push('\n');
        }
        // A report whose checks state their limits repeats each failed check with its value and limit.
        let failed: Vec<&Line> = self
            .checks
            .iter()
            .filter(|c| c.verdict == Verdict::Fail && !c.threshold.is_empty())
            .collect();
        if !failed.is_empty() {
            out.push_str("failed checks (the check, the measured value, the threshold):\n");
            for c in failed {
                let measured = c.detail.strip_prefix("measured: ").unwrap_or(&c.detail);
                out.push_str(&format!(
                    "  {}\n    measured:  {}\n    threshold: {}\n",
                    c.name, measured, c.threshold
                ));
            }
        }
        for (heading, lines) in [
            ("information only (never fails the comparison)", &self.info),
            ("not compared in this sprint", &self.not_compared),
            ("notes", &self.notes),
        ] {
            if !lines.is_empty() {
                out.push_str(&format!("{heading}:\n"));
                for line in lines {
                    out.push_str(&format!("  {line}\n"));
                }
            }
        }
        let result = match self.overall() {
            Verdict::Pass => "passed",
            Verdict::Fail => "failed",
            Verdict::NotRun => "not run",
        };
        out.push_str(&format!("result: {result}\n"));
        out
    }

    pub fn json(&self) -> Value {
        json!({
            "title": self.title,
            "checks": self.checks.iter().map(|c| json!({"name": c.name, "verdict": c.verdict.word(), "detail": c.detail, "threshold": c.threshold})).collect::<Vec<_>>(),
            "information": self.info,
            "not_compared": self.not_compared,
            "notes": self.notes,
            "result": match self.overall() { Verdict::Pass => "passed", Verdict::Fail => "failed", Verdict::NotRun => "not run" },
        })
    }
}
