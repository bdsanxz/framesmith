//! Diagnostics: what `check` reports.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Unsupported,
    Unknown,
    Warning,
}

impl Severity {
    pub fn word(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Unsupported => "unsupported",
            Severity::Unknown => "unknown",
            Severity::Warning => "warning",
        }
    }

    /// The kind a code belongs to, from its letter: `FS-E…`, `FS-U…`, `FS-X…`, `FS-W…`.
    pub fn of_code(code: &str) -> Severity {
        match code.as_bytes().get(3) {
            Some(b'U') => Severity::Unsupported,
            Some(b'X') => Severity::Unknown,
            Some(b'W') => Severity::Warning,
            _ => Severity::Error,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub code: String,
    pub kind: Severity,
    /// Path relative to the project folder, or the project path itself.
    pub file: String,
    /// Line counted from 1; 0 when the problem has no line.
    pub line: u32,
    pub description: String,
    pub remedy: String,
}

impl Diagnostic {
    pub fn new(code: &str, file: &str, line: u32, description: impl Into<String>, remedy: impl Into<String>) -> Diagnostic {
        Diagnostic {
            code: code.to_string(),
            kind: Severity::of_code(code),
            file: file.to_string(),
            line,
            description: description.into(),
            remedy: remedy.into(),
        }
    }

    /// `file:line`, or just the file when there is no line.
    pub fn location(&self) -> String {
        if self.line == 0 {
            self.file.clone()
        } else {
            format!("{}:{}", self.file, self.line)
        }
    }
}

/// The exit code a list of diagnostics gives: 1 for any error, else 2 for
/// any unsupported or unknown use, else 0.
pub fn exit_code(diagnostics: &[Diagnostic]) -> i32 {
    if diagnostics.iter().any(|d| d.kind == Severity::Error) {
        1
    } else if diagnostics
        .iter()
        .any(|d| matches!(d.kind, Severity::Unsupported | Severity::Unknown))
    {
        2
    } else {
        0
    }
}
