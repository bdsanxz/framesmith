//! The five stages of a render, and the clean-up when one stops early.

use crate::{Debug, Event, Failure, Job, JobFormat, Order, Reporter, Site, Stage, Staging, Stopped, Summary};
use browser::Browser;
use media::Encoder;
use model::{first_difference, Manifest, ENTRY_NAME};
use protocol::{Boot, ProblemReport, ReadyReport, Unsupported, BOOT_GLOBAL, PAGE_GLOBAL, READY_TIMEOUT_SECS, VERSION};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use timebase::frame_count;

/// How long a run waits at a stage when a maintainer asks it to hold there.
const HOLD_TIME: Duration = Duration::from_secs(20);

/// Everything a run needs. The checks before the stages are already done.
pub struct Setup<'a> {
    pub run_id: &'a str,
    pub project: &'a Path,
    /// The composition as read from the files.
    pub manifest: &'a Manifest,
    /// The frame rate of the run.
    pub fps: u32,
    pub browser: &'a Path,
    pub ffmpeg: &'a Path,
    /// Set from outside to cancel the run.
    pub cancel: Arc<AtomicBool>,
    pub debug: Debug,
}

struct Run<'a> {
    setup: &'a Setup<'a>,
    stage: Stage,
    frame: Option<u64>,
    frames: u64,
    browser: Option<Browser>,
    encoder: Option<Encoder>,
    /// Where the video is to appear, for messages.
    output: std::path::PathBuf,
}

fn internal(what: impl Into<String>) -> Stopped {
    Stopped::Failed(Failure::new(
        "FS-R016",
        format!("An internal error stopped the render: {}.", what.into()),
        "Run the command again. If it happens again, report it with the log file.",
    ))
}

fn unsupported_use(found: &Unsupported) -> Failure {
    let description = match found.code.as_str() {
        "FS-U016" => format!(
            "The page animates with the browser's own animation features ({}); they are not supported in this version.",
            found.detail
        ),
        "FS-U017" => {
            "A page script uses the framesmith script interface for building motion; it is not supported in this version.".to_string()
        }
        "FS-U021" => format!(
            "An element that motion targets has its own {}; motion writes that property, so the two cannot be combined in this version.",
            found.detail
        ),
        _ => format!("The page uses something that is not supported in this version ({}).", found.detail),
    };
    let remedy = match found.code.as_str() {
        "FS-U021" => "Remove the property from the element's styles, or use transform for the fixed part.",
        _ => "Remove it and animate with a motion block instead.",
    };
    Failure::new(&found.code, description, remedy)
}

impl Run<'_> {
    fn at(&self) -> String {
        match self.frame {
            Some(frame) => format!("during {}, at frame {frame} of {}", self.stage.name(), self.frames),
            None => format!("during {}", self.stage.name()),
        }
    }

    fn enter(&mut self, stage: Stage, reporter: &mut dyn Reporter) -> Result<(), Stopped> {
        self.stage = stage;
        tracing::info!(target: "render", stage = stage.name(), "stage started");
        reporter.event(&Event::Entered(stage));
        if self.setup.debug.hold == Some(stage) {
            eprintln!("holding in {}", stage.name());
            let until = Instant::now() + HOLD_TIME;
            while Instant::now() < until && !self.cancelled() {
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        self.check_cancel()
    }

    fn cancelled(&self) -> bool {
        self.setup.cancel.load(Ordering::SeqCst)
    }

    fn check_cancel(&self) -> Result<(), Stopped> {
        if self.cancelled() {
            Err(Stopped::Cancelled(Some(self.stage)))
        } else {
            Ok(())
        }
    }

    fn browser_failed(&self, error: browser::Error) -> Stopped {
        let failure = match error {
            browser::Error::Cancelled => return Stopped::Cancelled(Some(self.stage)),
            browser::Error::Timeout => Failure::new(
                "FS-R008",
                format!("The page did not become ready within {READY_TIMEOUT_SECS} seconds."),
                "Check for a script or a file in the page that never finishes loading.",
            ),
            browser::Error::Script(why) => return internal(format!("the page did not answer as expected ({why})")),
            browser::Error::Start(why) | browser::Error::Stopped(why) | browser::Error::Refused(why) => {
                let mut failure = Failure::new(
                    "FS-R011",
                    format!("The browser stopped or reported an error {}: {why}.", self.at()),
                    "Run the command again. If it keeps happening, name another browser with FRAMESMITH_BROWSER.",
                );
                failure.program = Some("browser");
                failure
            }
        };
        Stopped::Failed(failure)
    }

    fn encoder_failed(&self, error: media::Error) -> Stopped {
        let failure = match error {
            media::Error::Cancelled => return Stopped::Cancelled(Some(self.stage)),
            media::Error::Start(why) => Failure::new(
                "FS-R012",
                format!("ffmpeg could not be started {}: {why}.", self.at()),
                "Check that ffmpeg runs on its own, for example `ffmpeg -version`.",
            ),
            media::Error::Stopped { words, .. } if words.contains("No space left on device") => {
                return Stopped::Failed(Failure::new(
                    "FS-R005",
                    format!(
                        "The output cannot be written at {}: there is no space left on the disk it is on ({}).",
                        self.output.display(),
                        self.at()
                    ),
                    "Free some space, or choose an output path on another disk with --output <path>.",
                ));
            }
            media::Error::Stopped { words, .. } if words.contains("Unknown encoder") => Failure::new(
                "FS-R012",
                format!("The installed ffmpeg cannot encode H.264; it stopped {} ({words}).", self.at()),
                "Install an ffmpeg build that includes the libx264 encoder.",
            ),
            media::Error::Stopped { status, words } => Failure::new(
                "FS-R012",
                format!("ffmpeg stopped or reported an error {} ({status}): {words}.", self.at()),
                "Check that ffmpeg runs on its own, for example `ffmpeg -version`, and run the command again.",
            ),
        };
        Stopped::Failed(Failure {
            program: Some("ffmpeg"),
            ..failure
        })
    }

    fn browser(&mut self) -> Result<&mut Browser, Stopped> {
        self.browser.as_mut().ok_or_else(|| internal("the browser is not running"))
    }

    fn evaluate(&mut self, expression: &str, wait: Option<Duration>) -> Result<serde_json::Value, Stopped> {
        let result = self.browser()?.evaluate(expression, wait);
        result.map_err(|error| self.browser_failed(error))
    }

    /// Stops the run for anything the page reports. A refused network
    /// address comes first: the failed load it causes is only its symptom.
    fn judge(&mut self, report: &ProblemReport) -> Result<(), Stopped> {
        if let Some(url) = self.browser()?.refused().first() {
            return Err(Stopped::Failed(Failure::new(
                "FS-U009",
                format!(
                    "The page asked for {url}, a network address; network assets are not supported in this version. Nothing was fetched."
                ),
                "Download the file into the project folder and refer to the local copy.",
            )));
        }
        if let Some(failure) = report.failures.first() {
            let failure = match failure.kind.as_str() {
                "script" => Failure::new(
                    "FS-R007",
                    format!("A page script failed: {}.", failure.detail),
                    "Fix the script. `framesmith check` does not run scripts; open the page in a browser to see the error.",
                ),
                kind => Failure::new(
                    "FS-R006",
                    format!("The {kind} {} did not load in the page.", failure.detail),
                    format!("Check that the file is a valid {kind} the browser can read."),
                ),
            };
            return Err(Stopped::Failed(failure));
        }
        match report.unsupported.first() {
            Some(found) => Err(Stopped::Failed(unsupported_use(found))),
            None => Ok(()),
        }
    }

    fn judge_text(&mut self, text: &serde_json::Value) -> Result<(), Stopped> {
        let text = text.as_str().unwrap_or_default();
        let report = if text.is_empty() {
            ProblemReport::default()
        } else {
            serde_json::from_str(text).map_err(|e| internal(format!("the page's report could not be read ({e})")))?
        };
        self.judge(&report)
    }

    fn go(&mut self, staging: &mut Staging, reporter: &mut dyn Reporter) -> Result<(), Stopped> {
        let setup = self.setup;
        let manifest = setup.manifest;
        let (width, height, fps) = (manifest.width, manifest.height, setup.fps);

        self.enter(Stage::Compile, reporter)?;
        let source =
            std::fs::read(setup.project.join(ENTRY_NAME)).map_err(|e| internal(format!("{ENTRY_NAME} could not be read ({e})")))?;
        let page = compile::entry_page(&String::from_utf8_lossy(&source), width, height);
        self.frames = frame_count(manifest.length, fps);
        let job = Job {
            run_id: setup.run_id.to_string(),
            project: setup.project.to_path_buf(),
            entry: ENTRY_NAME.to_string(),
            output: staging.output().to_path_buf(),
            manifest: manifest.clone(),
            fps,
            frames: self.frames,
            format: JobFormat::default(),
        };
        tracing::info!(target: "render", width, height, fps, frames = job.frames, "render job created");
        if let Some(file) = &setup.debug.job_file {
            std::fs::write(file, job.to_json())
                .map_err(|e| internal(format!("the job could not be written to {} ({e})", file.display())))?;
        }
        reporter.event(&Event::CompileDone);

        self.enter(Stage::Probe, reporter)?;
        let profile = std::env::temp_dir().join(format!("framesmith-{}", setup.run_id));
        let launched = Browser::launch(setup.browser, &profile, width, height, Arc::clone(&setup.cancel));
        self.browser = Some(launched.map_err(|error| self.browser_failed(error))?);
        let boot = Boot {
            protocol: VERSION,
            fault: setup.debug.fault.clone(),
        };
        let before = format!("window.{BOOT_GLOBAL} = {};", serde_json::to_string(&boot).unwrap_or_default());
        let site = Site::new(setup.project, page);
        let opened = self
            .browser()?
            .open(&Site::entry_url(), width, height, &before, Box::new(move |url| site.serve(url)));
        opened.map_err(|error| self.browser_failed(error))?;
        // The loader announces itself as soon as it runs; until then the
        // page is still arriving, and an answer from the page it replaces
        // means nothing.
        let deadline = Instant::now() + Duration::from_secs(READY_TIMEOUT_SECS);
        let left = |deadline: Instant| Some(deadline.saturating_duration_since(Instant::now()));
        loop {
            match self.browser()?.evaluate(&format!("typeof {PAGE_GLOBAL}"), left(deadline)) {
                Ok(kind) if kind == "object" => break,
                Ok(_) | Err(browser::Error::Script(_) | browser::Error::Refused(_)) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(5));
                }
                Ok(_) | Err(browser::Error::Script(_) | browser::Error::Refused(_)) => {
                    return Err(self.browser_failed(browser::Error::Timeout));
                }
                Err(error) => return Err(self.browser_failed(error)),
            }
        }
        let ready = self.evaluate(&format!("{PAGE_GLOBAL}.ready"), left(deadline))?;
        let report: ReadyReport = serde_json::from_str(ready.as_str().unwrap_or_default()).map_err(|error| {
            Stopped::Failed(Failure::new(
                "FS-R013",
                format!("The page's ready report could not be read ({error}); the page and Framesmith speak different protocol versions."),
                "Rebuild Framesmith so the loader and the binary come from the same version.",
            ))
        })?;
        if report.protocol != VERSION {
            return Err(Stopped::Failed(Failure::new(
                "FS-R013",
                format!(
                    "The page speaks protocol version {}; Framesmith speaks version {VERSION}.",
                    report.protocol
                ),
                "Rebuild Framesmith so the loader and the binary come from the same version.",
            )));
        }
        if !report.clocks {
            return Err(Stopped::Failed(Failure::new(
                "FS-R009",
                "The page's clocks could not be replaced, so its frames would not be repeatable.",
                "Check for a script that redefines Date, performance.now, setTimeout or requestAnimationFrame.",
            )));
        }
        self.judge(&ProblemReport {
            unsupported: report.unsupported,
            failures: report.failures,
        })?;
        let disagrees = |what: String| {
            Stopped::Failed(Failure::new(
                "FS-R010",
                format!("The page disagrees with the files: {what}."),
                "Check for a script that changes the composition while the page loads, or markup a browser reads differently from how it is written.",
            ))
        };
        let Some(page_manifest) = report.manifest else {
            let why = report
                .problems
                .first()
                .map_or_else(|| "it gave no reason".to_string(), |p| format!("{} ({})", p.description, p.code));
            return Err(disagrees(format!("the page could not resolve its composition: {why}")));
        };
        if let Some(difference) = first_difference(manifest, &page_manifest) {
            return Err(disagrees(format!(
                "{} is {} in the files and {} in the page",
                difference.what, difference.files, difference.page
            )));
        }
        let began = self.evaluate(&format!("{PAGE_GLOBAL}.begin({{ fps: {fps} }})"), None)?;
        self.judge_text(&began)?;
        reporter.event(&Event::ProbeDone {
            width,
            height,
            fps,
            length: manifest.length,
            frames: self.frames,
        });

        self.enter(Stage::Capture, reporter)?;
        let format = media::Format {
            width,
            height,
            fps,
            quality: job.format.quality,
            preset: job.format.preset.clone(),
        };
        let started = Encoder::start(setup.ffmpeg, &format, staging.path());
        self.encoder = Some(started.map_err(|error| self.encoder_failed(error))?);
        if let Some(folder) = &setup.debug.frames_dir {
            std::fs::create_dir_all(folder).map_err(|e| internal(format!("{} could not be made ({e})", folder.display())))?;
        }
        // Out of order, the pictures are held and fed to the encoder in order afterwards.
        let order = setup.debug.order.of(self.frames);
        let hold = setup.debug.order != Order::Forward;
        let mut held: Vec<Vec<u8>> = Vec::new();
        if hold {
            held.resize(self.frames as usize, Vec::new());
        }
        reporter.event(&Event::Capture {
            done: 0,
            total: self.frames,
        });
        for (step, &frame) in order.iter().enumerate() {
            self.frame = Some(frame);
            self.check_cancel()?;
            let sought = self.evaluate(&format!("{PAGE_GLOBAL}.seek({frame})"), None)?;
            self.judge_text(&sought)?;
            let taken = self.browser()?.screenshot();
            let png = taken.map_err(|error| self.browser_failed(error))?;
            if let Some(folder) = &setup.debug.frames_dir {
                let file = folder.join(format!("frame-{frame:05}.png"));
                std::fs::write(&file, &png).map_err(|e| internal(format!("{} could not be written ({e})", file.display())))?;
            }
            if hold {
                held[frame as usize] = png;
            } else {
                self.feed(&png)?;
            }
            reporter.event(&Event::Capture {
                done: step as u64 + 1,
                total: self.frames,
            });
        }
        for (frame, png) in held.iter().enumerate() {
            self.frame = Some(frame as u64);
            self.check_cancel()?;
            self.feed(png)?;
        }
        self.frame = None;
        if let Some(file) = &setup.debug.costs_file {
            let costs = self.evaluate(&format!("{PAGE_GLOBAL}.costs()"), None)?;
            std::fs::write(file, costs.as_str().unwrap_or("[]"))
                .map_err(|e| internal(format!("the seek costs could not be written to {} ({e})", file.display())))?;
        }

        self.enter(Stage::Encode, reporter)?;
        let finished = match self.encoder.as_mut() {
            Some(encoder) => encoder.finish(&setup.cancel),
            None => return Err(internal("the encoder is not running")),
        };
        finished.map_err(|error| self.encoder_failed(error))?;
        if let Some(mut browser) = self.browser.take() {
            browser.close();
        }
        reporter.event(&Event::EncodeDone);

        self.enter(Stage::Commit, reporter)?;
        staging.commit().map_err(Stopped::Failed)?;
        reporter.event(&Event::Committed {
            output: staging.output().to_path_buf(),
        });
        Ok(())
    }

    fn feed(&mut self, png: &[u8]) -> Result<(), Stopped> {
        let fed = match self.encoder.as_mut() {
            Some(encoder) => encoder.frame(png),
            None => return Err(internal("the encoder is not running")),
        };
        fed.map_err(|error| self.encoder_failed(error))
    }
}

/// Runs the five stages. On any early stop the staging file is removed,
/// folders made for this run are removed when empty, and the browser and
/// `ffmpeg` are stopped; a file already at the output path is unchanged.
pub fn run(setup: &Setup<'_>, staging: &mut Staging, reporter: &mut dyn Reporter) -> Result<Summary, Stopped> {
    let started = Instant::now();
    let mut run = Run {
        setup,
        stage: Stage::Compile,
        frame: None,
        frames: 0,
        browser: None,
        encoder: None,
        output: staging.output().to_path_buf(),
    };
    let result = run.go(staging, reporter);
    let (stage, frame) = (run.stage, run.frame);
    if let Some(mut encoder) = run.encoder.take() {
        encoder.stop();
    }
    if let Some(mut browser) = run.browser.take() {
        browser.close();
    }
    match result {
        Ok(()) => {
            tracing::info!(target: "render", frames = run.frames, "render completed");
            Ok(Summary {
                output: staging.output().to_path_buf(),
                width: setup.manifest.width,
                height: setup.manifest.height,
                fps: setup.fps,
                frames: run.frames,
                length: setup.manifest.length,
                elapsed: started.elapsed(),
            })
        }
        Err(stopped) => {
            staging.discard();
            Err(match stopped {
                Stopped::Cancelled(at) => {
                    tracing::warn!(target: "render", stage = stage.name(), "render cancelled");
                    Stopped::Cancelled(at.or(Some(stage)))
                }
                Stopped::Failed(mut failure) => {
                    failure.stage.get_or_insert(stage);
                    if failure.program.is_some() {
                        failure.frame = failure.frame.or(frame);
                    }
                    // What a page says may repeat its own text, which the log never holds.
                    let from_page = failure.code.starts_with("FS-U") || matches!(failure.code.as_str(), "FS-R006" | "FS-R007" | "FS-R010");
                    let cause = if from_page {
                        "reported by the page"
                    } else {
                        failure.description.as_str()
                    };
                    tracing::error!(
                        target: "render",
                        code = %failure.code,
                        stage = stage.name(),
                        frame = ?failure.frame,
                        project = %setup.project.display(),
                        browser = %setup.browser.display(),
                        fps = setup.fps,
                        "render stopped: {cause}"
                    );
                    Stopped::Failed(failure)
                }
            })
        }
    }
}
