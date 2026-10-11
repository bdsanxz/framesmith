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
    pub detail: String,
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
        });
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
            out.push_str(&format!("  {:<width$}  {:<7}  {}\n", c.name, c.verdict.word(), c.detail));
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
            "checks": self.checks.iter().map(|c| json!({"name": c.name, "verdict": c.verdict.word(), "detail": c.detail})).collect::<Vec<_>>(),
            "information": self.info,
            "not_compared": self.not_compared,
            "notes": self.notes,
            "result": match self.overall() { Verdict::Pass => "passed", Verdict::Fail => "failed", Verdict::NotRun => "not run" },
        })
    }
}
