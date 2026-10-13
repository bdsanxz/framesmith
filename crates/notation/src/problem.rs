use std::fmt;

use crate::code::{Code, Kind};

/// One thing wrong in a composition file, with its code and the line it is on.
///
/// It prints as `line N: ` and the code's statement: the code, its kind, the message as one
/// sentence and the code's remedy (`docs/spec/error-catalogue.md` section 3). It does not know the
/// file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    code: Code,
    line: u32,
    message: String,
}

impl Problem {
    pub(crate) fn new(code: Code, line: u32, message: String) -> Problem {
        Problem { code, line, message }
    }

    /// The code of the kind of problem.
    pub fn code(&self) -> Code {
        self.code
    }

    /// The line of the file the problem is on, counted from 1.
    pub fn line(&self) -> u32 {
        self.line
    }

    /// The kind of the problem: the kind of its code.
    pub fn kind(&self) -> Kind {
        self.code.kind()
    }

    /// What to change: the remedy of the problem's code.
    pub fn remedy(&self) -> &'static str {
        self.code.remedy()
    }

    /// What is wrong, without the line, the code and the remedy.
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.code.statement(&self.message))
    }
}
