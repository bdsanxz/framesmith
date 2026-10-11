//! Progress of a render, as text: the stage names in order and a count of
//! frames. No percentage and no remaining time.

use render::{Event, Reporter};
use std::io::Write;
use std::time::{Duration, Instant};

/// How often a new capture line is printed when lines cannot be rewritten.
const CAPTURE_EVERY: Duration = Duration::from_secs(1);

pub struct Progress<W: Write> {
    out: W,
    /// The stream is a terminal: the current line is rewritten in place.
    live: bool,
    /// A line has been started and not ended.
    open: bool,
    last_capture: Option<Instant>,
    /// The output path as the user wrote it.
    output: String,
}

impl<W: Write> Progress<W> {
    pub fn new(out: W, live: bool, output: String) -> Progress<W> {
        Progress {
            out,
            live,
            open: false,
            last_capture: None,
            output,
        }
    }

    fn line(&mut self, stage: &str, rest: &str) {
        let back = if self.open { "\r" } else { "" };
        let _ = writeln!(self.out, "{back}{stage:<10}{rest}");
        self.open = false;
    }

    /// Ends a line left open, so the next thing printed starts on its own line.
    pub fn end_line(&mut self) {
        if self.open {
            let _ = writeln!(self.out);
            self.open = false;
        }
    }

    /// Prints a closing line.
    pub fn say(&mut self, text: &str) {
        self.end_line();
        let _ = writeln!(self.out, "{text}");
    }
}

impl<W: Write> Reporter for Progress<W> {
    fn event(&mut self, event: &Event) {
        match event {
            Event::Entered(stage) => {
                if self.live {
                    let _ = write!(self.out, "{:<10}", stage.name());
                    let _ = self.out.flush();
                    self.open = true;
                }
            }
            Event::CompileDone => self.line("compile", "done"),
            Event::ProbeDone {
                width,
                height,
                fps,
                length,
                frames,
            } => self.line(
                "probe",
                &format!("done      {width}x{height}, {fps} fps, {length} s, {frames} frames"),
            ),
            Event::Capture { done, total } => {
                let last = done == total;
                if self.live {
                    let _ = write!(self.out, "\r{:<10}{done}/{total}", "capture");
                    let _ = self.out.flush();
                    self.open = true;
                    if last {
                        self.end_line();
                    }
                } else if *done == 0 || last || self.last_capture.is_none_or(|at| at.elapsed() >= CAPTURE_EVERY) {
                    self.last_capture = Some(Instant::now());
                    self.line("capture", &format!("{done}/{total}"));
                }
            }
            Event::EncodeDone => self.line("encode", "done"),
            Event::Committed { .. } => {
                let output = self.output.clone();
                self.line("commit", &output);
            }
        }
    }
}
