//! How a render ends, and what it says on the way.

use std::path::PathBuf;
use std::time::Duration;
use timebase::Time;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Stage {
    Compile,
    Probe,
    Capture,
    Encode,
    Commit,
}

impl Stage {
    pub const ALL: [Stage; 5] = [Stage::Compile, Stage::Probe, Stage::Capture, Stage::Encode, Stage::Commit];

    pub fn name(self) -> &'static str {
        match self {
            Stage::Compile => "compile",
            Stage::Probe => "probe",
            Stage::Capture => "capture",
            Stage::Encode => "encode",
            Stage::Commit => "commit",
        }
    }

    pub fn from_name(name: &str) -> Option<Stage> {
        Stage::ALL.into_iter().find(|stage| stage.name() == name)
    }
}

/// Why a command stopped: a code, one sentence, and what to do about it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    pub code: String,
    pub description: String,
    pub remedy: String,
    /// `browser` or `ffmpeg`, when one of them is the cause.
    pub program: Option<&'static str>,
    pub stage: Option<Stage>,
    /// The frame reached, when frames were being captured.
    pub frame: Option<u64>,
}

impl Failure {
    pub fn new(code: &str, description: impl Into<String>, remedy: impl Into<String>) -> Failure {
        Failure {
            code: code.to_string(),
            description: description.into(),
            remedy: remedy.into(),
            program: None,
            stage: None,
            frame: None,
        }
    }

    /// 2 for an unsupported use, 1 for anything else.
    pub fn exit_code(&self) -> i32 {
        if self.code.starts_with("FS-U") {
            2
        } else {
            1
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Stopped {
    Failed(Failure),
    Cancelled(Option<Stage>),
}

/// A finished render.
#[derive(Clone, Debug, PartialEq)]
pub struct Summary {
    /// Absolute.
    pub output: PathBuf,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub frames: u64,
    pub length: Time,
    pub elapsed: Duration,
}

/// What a render reports as it goes.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// A stage has begun.
    Entered(Stage),
    CompileDone,
    ProbeDone {
        width: u32,
        height: u32,
        fps: u32,
        length: Time,
        frames: u64,
    },
    Capture {
        done: u64,
        total: u64,
    },
    EncodeDone,
    Committed {
        output: PathBuf,
    },
}

pub trait Reporter {
    fn event(&mut self, event: &Event);
}
