use std::fmt;

/// One thing wrong in a composition file, with the line it is on.
///
/// It prints as `line N: ` and the message, the form of `docs/spec/notation.md` section 13.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    line: u32,
    message: String,
}

impl Problem {
    pub(crate) fn new(line: u32, message: String) -> Problem {
        Problem { line, message }
    }

    /// The line of the file the problem is on, counted from 1.
    pub fn line(&self) -> u32 {
        self.line
    }

    /// What is wrong, without the line.
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}
