//! What a comparison says: each check in words, then one result.

use serde_json::{json, Value};

/// The words a check may end in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Pass,
    Fail,
    NotRun,
    /// The check has nothing to judge in this fixture (for example no tracked element is marked).
    /// It is neither a pass nor a fail and does not change the result.
    NotApplicable,
}

impl Verdict {
    pub fn word(self) -> &'static str {
        match self {
            Verdict::Pass => "pass",
            Verdict::Fail => "fail",
            Verdict::NotRun => "not run",
            Verdict::NotApplicable => "not applicable",
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
    /// Lists the maintainer reads after the checks (for example the frames under 99 %): a heading and its lines.
    pub sections: Vec<(String, Vec<String>)>,
    /// More members of the JSON report, for what the text shows as lists.
    pub data: Vec<(String, Value)>,
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
    /// A check that is not applicable is left out: it is neither a pass nor a fail.
    pub fn overall(&self) -> Verdict {
        let judged: Vec<&Line> = self.checks.iter().filter(|c| c.verdict != Verdict::NotApplicable).collect();
        if judged.iter().any(|c| c.verdict == Verdict::Fail) {
            Verdict::Fail
        } else if judged.is_empty() || judged.iter().any(|c| c.verdict == Verdict::NotRun) {
            Verdict::NotRun
        } else {
            Verdict::Pass
        }
    }

    pub fn exit_code(&self) -> i32 {
        match self.overall() {
            Verdict::Pass => EXIT_PASS,
            Verdict::Fail => EXIT_FAIL,
            Verdict::NotRun | Verdict::NotApplicable => EXIT_NOT_RUN,
        }
    }

    pub fn section(&mut self, heading: impl Into<String>, lines: Vec<String>) {
        self.sections.push((heading.into(), lines));
    }

    /// The one word of the whole result.
    pub fn result(&self) -> &'static str {
        match self.overall() {
            Verdict::Pass => "passed",
            Verdict::Fail => "failed",
            Verdict::NotRun | Verdict::NotApplicable => "not run",
        }
    }

    /// The report as plain text, one check per line.
    pub fn text(&self) -> String {
        let mut out = format!("{}\n", self.title);
        let width = self.checks.iter().map(|c| c.name.len()).max().unwrap_or(0);
        for c in &self.checks {
            out.push_str(&format!("  {:<width$}  {:<14}  {}\n", c.name, c.verdict.word(), c.detail));
        }
        let mut lists: Vec<(&str, &Vec<String>)> = self.sections.iter().map(|(h, l)| (h.as_str(), l)).collect();
        lists.push(("information only (never fails the comparison)", &self.info));
        lists.push(("not compared in this sprint", &self.not_compared));
        lists.push(("notes", &self.notes));
        for (heading, lines) in lists {
            if !lines.is_empty() {
                out.push_str(&format!("{heading}:\n"));
                for line in lines {
                    out.push_str(&format!("  {line}\n"));
                }
            }
        }
        out.push_str(&format!("result: {}\n", self.result()));
        out
    }

    pub fn json(&self) -> Value {
        let mut report = json!({
            "title": self.title,
            "checks": self.checks.iter().map(|c| json!({"name": c.name, "verdict": c.verdict.word(), "detail": c.detail})).collect::<Vec<_>>(),
            "information": self.info,
            "not_compared": self.not_compared,
            "notes": self.notes,
            "result": self.result(),
        });
        if let Some(members) = report.as_object_mut() {
            for (name, value) in &self.data {
                members.insert(name.clone(), value.clone());
            }
        }
        report
    }
}
