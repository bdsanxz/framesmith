//! The stages of a render running side by side (`docs/spec/render-command.md`, section 5.5): `footage`
//! (the pictures the film shows are decoded), `frames` (the frames are drawn), `encoding` (the drawn frames are
//! encoded), `sound` (the mix is handed to the sound encoder, frame by frame). Each stage counts the frames it is
//! done with; the counts are told on standard error, one line per stage, at most a few times a second, with names
//! only and no colour.

use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// How often the counts are told at most: a few times a second (the cadence of frx7's progress rule).
const TELL_AT_MOST_EVERY: Duration = Duration::from_millis(250);

/// Writes the lines of one telling.
fn write_lines(out: &mut Box<dyn Write + Send>, lines: &[ProgressLine]) {
    for line in lines {
        match *line {
            ProgressLine::Count { stage, done, total } => {
                let _ = writeln!(out, "{}: {done}/{total} frames", stage.name());
            }
            ProgressLine::Idle { stage } => {
                let _ = writeln!(out, "{}: nothing to do", stage.name());
            }
        }
    }
}

/// One line of a telling: a stage that counts, or one with nothing to do.
enum ProgressLine {
    Count { stage: Stage, done: u64, total: u64 },
    Idle { stage: Stage },
}

/// A stage of the run, in the order the spec lists them.
#[derive(Clone, Copy)]
pub enum Stage {
    Footage,
    Frames,
    Encoding,
    Sound,
}

impl Stage {
    /// The name the spec lists, the same everywhere a stage is named.
    pub fn name(self) -> &'static str {
        match self {
            Stage::Footage => "footage",
            Stage::Frames => "frames",
            Stage::Encoding => "encoding",
            Stage::Sound => "sound",
        }
    }
}

/// The counts of the stages, shared by the threads of the run.
pub struct Progress {
    done: [AtomicU64; 4],
    total: [u64; 4],
    idle: [bool; 4],
    state: Mutex<State>,
    out: Mutex<Box<dyn Write + Send>>,
}

/// What was last told, and when.
struct State {
    told: [u64; 4],
    idle_told: [bool; 4],
    at: Instant,
}

impl Progress {
    /// The counts of a run of `frames` frames. A stage with `idle` true has nothing to do and says so once
    /// instead of a count. Lines go to `out` (standard error in the command).
    pub fn new(frames: u64, idle: [bool; 4], out: Box<dyn Write + Send>) -> Progress {
        Progress {
            done: [AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0)],
            total: [frames, frames, frames, frames],
            idle,
            state: Mutex::new(State {
                told: [0, 0, 0, 0],
                idle_told: [false, false, false, false],
                // Long ago: the first counts are told at once.
                at: Instant::now() - TELL_AT_MOST_EVERY,
            }),
            out: Mutex::new(out),
        }
    }

    /// Marks `stage` done with `done` frames.
    pub fn set(&self, stage: Stage, done: u64) {
        self.done[stage as usize].store(done, Ordering::Relaxed);
        self.tell(false);
    }

    /// Tells every count, and what is left at the end: each telling shows all four stages together, so one
    /// telling is one true moment of the run.
    pub fn finish(&self) {
        self.tell(true);
    }

    fn tell(&self, last: bool) {
        // The telling is built under the state lock and written after it is released: the stages tell
        // side by side, and a telling thread parked in a write must never hold the lock the other
        // stages tell under. A periodic telling that finds another telling writing is skipped; the end
        // of the run always tells.
        let stages = [Stage::Footage, Stage::Frames, Stage::Encoding, Stage::Sound];
        let lines = {
            let mut state = self.state.lock().unwrap();
            let changed = stages.iter().enumerate().any(|(index, _)| {
                (!self.idle[index] && self.done[index].load(Ordering::Relaxed) != state.told[index])
                    || (self.idle[index] && !state.idle_told[index])
            });
            if !last && (!changed || state.at.elapsed() < TELL_AT_MOST_EVERY) {
                return;
            }
            state.at = Instant::now();
            let mut lines = Vec::new();
            for (index, stage) in stages.iter().enumerate() {
                if self.idle[index] {
                    if !state.idle_told[index] {
                        lines.push(ProgressLine::Idle { stage: *stage });
                        state.idle_told[index] = true;
                    }
                    continue;
                }
                let done = self.done[index].load(Ordering::Relaxed);
                lines.push(ProgressLine::Count {
                    stage: *stage,
                    done,
                    total: self.total[index],
                });
                state.told[index] = done;
            }
            lines
        };
        if last {
            let mut out = self.out.lock().unwrap();
            write_lines(&mut out, &lines);
        } else if let Ok(mut out) = self.out.try_lock() {
            write_lines(&mut out, &lines);
        }
    }
}
