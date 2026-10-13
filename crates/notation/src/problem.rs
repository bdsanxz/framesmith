use std::fmt;

use crate::code::Code;

/// One thing wrong in a composition file, with its code and the line it is on.
///
/// It prints as `line N: ` the code, `: ` and the message, the form of `docs/spec/notation.md`
/// section 13 with the code of `docs/spec/check-command.md` section 4. It does not know the file.
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

    /// What is wrong, without the line and the code.
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}: {}", self.line, self.code, self.message)
    }
}
