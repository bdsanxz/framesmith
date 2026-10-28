//! The `framesmith` command.
//!
//! Behaviour is specified in `docs/spec/check-command.md` and `docs/spec/render-command.md`. This
//! version has two commands, `check` and `render`.

use std::ffi::OsString;
use std::io::Write;
use std::path::PathBuf;

use draw::Renderer;
use draw_cpu::CpuRenderer;
use draw_gpu::CardRenderer;
use project::{Backend, Code, Kind, OpenError, Options, Problem, Project, Report, Settings};
use render::Scene;

pub mod runlog;

/// The exit code of a command that found nothing wrong.
pub const EXIT_OK: u8 = 0;
/// The exit code of a command line that cannot be read, an invalid project, or no project.
pub const EXIT_FAILED: u8 = 1;
/// The exit code of a project that is valid, except that it uses what this version does not support.
pub const EXIT_UNSUPPORTED: u8 = 2;

/// The switch of `render` that prints the mark of every frame drawn after the summary (`docs/spec/render-command.md`,
/// section 7.1). It sets no setting of the project file, so it is not one of `project`'s options.
const MARKS: &str = "--marks";

const CHECK_USAGE: &str = "usage: framesmith check <path>";
const RENDER_USAGE: &str = "usage: framesmith render <path> [--output <path>] [--backend auto|cpu|gpu] [--quality draft|standard|high] [--encoder hardware|software] [--codec h264|hevc] [--workers auto|1-16] [--marks]";

/// The starter of the renderer on the graphics card: `CardRenderer::new`, boxed as a renderer. It is
/// a parameter of `run_with`, so that a test can run the command on a machine with no card.
pub type StartCard = dyn Fn() -> Result<Box<dyn Renderer>, draw::Error>;

/// Runs the command with the arguments after its name. Results go to `out`, a command line that
/// cannot be read to `err`. Returns the exit code.
///
/// The run is logged under one run id (`docs/spec/run-log.md`); a log that cannot be written adds
/// one line to `err` and changes nothing else.
pub fn run(args: &[OsString], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    run_with(args, out, err, &card_renderer)
}

/// Runs the command as `run` does, with the starter of the graphics card given by the caller.
pub fn run_with(args: &[OsString], out: &mut dyn Write, err: &mut dyn Write, start_card: &StartCard) -> u8 {
    let log = runlog::Log::start();
    let code = log.scope(|| {
        tracing::info!(
            version = env!("CARGO_PKG_VERSION"),
            args = %runlog::word_list(args),
            "command started"
        );
        log.note_unreadable_level();
        command(args, out, err, start_card)
    });
    log.finish(err);
    code
}

/// Starts the renderer on the graphics card, or says why it cannot.
fn card_renderer() -> Result<Box<dyn Renderer>, draw::Error> {
    Ok(Box::new(CardRenderer::new()?))
}

/// Reads the command line, runs the command, prints its reply and logs its result.
fn command(args: &[OsString], out: &mut dyn Write, err: &mut dyn Write, start_card: &StartCard) -> u8 {
    // The codecs of the scenes this command makes, with the system's backend registered once, here where the command starts.
    let codecs = render::system_codecs();
    let reply = match args.split_first() {
        None => Reply::Unreadable(Code::CommandLineNotRead, "no command is given".to_string(), BOTH_USAGES),
        Some((command, rest)) if command == "check" => with_path("check", &[CHECK_USAGE], rest, &|path| check(path, &codecs)),
        Some((command, rest)) if command == "render" => match render_line(rest) {
            Ok((path, options, marks)) => render(path, &options, marks, start_card, &codecs),
            Err((code, why)) => Reply::Unreadable(code, why, &[RENDER_USAGE]),
        },
        Some((command, _)) => Reply::Unreadable(
            Code::CommandLineNotRead,
            format!("`{}` is not a command", command.to_string_lossy()),
            BOTH_USAGES,
        ),
    };
    let written = match &reply {
        Reply::Printed(lines, ..) => lines.iter().try_for_each(|line| writeln!(out, "{line}")),
        Reply::Unreadable(code, why, usages) => {
            writeln!(err, "framesmith: {}", code.statement(why)).and_then(|()| usages.iter().try_for_each(|usage| writeln!(err, "{usage}")))
        }
    };
    let (code, outcome) = match (written, reply) {
        (Ok(()), Reply::Printed(_, code, outcome)) => (code, outcome),
        (Err(_), Reply::Printed(_, _, outcome)) => (EXIT_FAILED, outcome),
        (_, Reply::Unreadable(..)) => (EXIT_FAILED, Outcome::NotRead),
    };
    outcome.log(code);
    code
}

/// The usage lines of a command line that names no command of the program.
const BOTH_USAGES: &[&str] = &[CHECK_USAGE, RENDER_USAGE];

/// A command that takes the path of one project and no option: reads the words after its name and
/// runs it.
fn with_path(name: &str, usages: &'static [&'static str], rest: &[OsString], run: &dyn Fn(PathBuf) -> Reply) -> Reply {
    if let Some(option) = rest.iter().find(|word| word.to_string_lossy().starts_with('-')) {
        return Reply::Unreadable(
            Code::CommandLineNotRead,
            format!("`{name}` takes no option, and {} is one", option.to_string_lossy()),
            usages,
        );
    }
    match rest {
        [] => Reply::Unreadable(Code::CommandLineNotRead, format!("`{name}` needs the path of a project"), usages),
        [path] => run(PathBuf::from(path)),
        [_, extra, ..] => Reply::Unreadable(
            Code::CommandLineNotRead,
            format!(
                "`{name}` takes the path of one project, and {} is one word too many",
                extra.to_string_lossy()
            ),
            usages,
        ),
    }
}

/// Reads the words after `render`: the path of one project and the options, in any order
/// (`docs/spec/render-command.md` section 1), and whether `--marks` asks for the frame marks (section 7.1). Gives the
/// sentence that says what is wrong otherwise.
fn render_line(rest: &[OsString]) -> Result<(PathBuf, Options, bool), (Code, String)> {
    let not_read = |why: String| (Code::CommandLineNotRead, why);
    let not_taken = |why: String| (Code::OptionNotTaken, why);
    let mut options = Options::default();
    let mut path: Option<&OsString> = None;
    let mut marks = false;
    let mut words = rest.iter();
    while let Some(word) = words.next() {
        let text = word.to_string_lossy();
        if !text.starts_with('-') {
            if path.is_some() {
                return Err(not_read(format!(
                    "`render` takes the path of one project, and {text} is one word too many"
                )));
            }
            path = Some(word);
            continue;
        }
        let (name, written) = match text.split_once('=') {
            Some((name, value)) => (name.to_string(), Some(value.to_string())),
            None => (text.to_string(), None),
        };
        if name == MARKS {
            if written.is_some() {
                return Err(not_taken(format!("`{MARKS}` takes no value, and is written alone")));
            }
            if marks {
                return Err(not_taken(format!("`{MARKS}` is given twice; give it once")));
            }
            marks = true;
            continue;
        }
        if !Options::is_option(&name) {
            return Err(not_taken(format!("{}, {MARKS}", Options::unknown(&name))));
        }
        let value = match written {
            Some(value) => value,
            None => match words.next() {
                Some(value) => value.to_string_lossy().into_owned(),
                None => return Err(not_taken(Options::no_value(&name))),
            },
        };
        options.set(&name, &value).map_err(not_taken)?;
    }
    match path {
        Some(path) => Ok((PathBuf::from(path), options, marks)),
        None => Err(not_read("`render` needs the path of a project".to_string())),
    }
}

/// How a command ended, for the log (`docs/spec/run-log.md` section 4).
enum Outcome {
    Ready,
    Invalid {
        problems: usize,
    },
    Unsupported {
        problems: usize,
    },
    /// No composition to read; `problems` counts the reports of the project file before it.
    NoProject {
        code: Code,
        problems: usize,
    },
    NotRead,
    /// `render` wrote the film.
    Rendered {
        frames: u64,
    },
    /// `render` could not make the film.
    VideoNotMade {
        code: Code,
    },
    /// `render` refused the film before its first frame, because the renderer it was asked for is
    /// not there.
    Refused {
        code: Code,
    },
}

impl Outcome {
    /// Writes the last line of the run: the level follows the outcome.
    fn log(&self, exit: u8) {
        match self {
            Outcome::Ready => tracing::info!(outcome = "ready", exit, "command finished"),
            Outcome::Invalid { problems } => {
                tracing::warn!(outcome = "invalid", exit, problems = *problems, "command finished")
            }
            Outcome::Unsupported { problems } => {
                tracing::warn!(outcome = "unsupported", exit, problems = *problems, "command finished")
            }
            Outcome::NoProject { code, problems: 0 } if code.kind() == Kind::Failure => {
                tracing::error!(outcome = "no project", exit, code = code.as_str(), "command finished")
            }
            Outcome::NoProject { code, problems: 0 } => {
                tracing::warn!(outcome = "no project", exit, code = code.as_str(), "command finished")
            }
            Outcome::NoProject { code, problems } if code.kind() == Kind::Failure => {
                tracing::error!(
                    outcome = "no project",
                    exit,
                    code = code.as_str(),
                    problems = *problems,
                    "command finished"
                )
            }
            Outcome::NoProject { code, problems } => {
                tracing::warn!(
                    outcome = "no project",
                    exit,
                    code = code.as_str(),
                    problems = *problems,
                    "command finished"
                )
            }
            Outcome::NotRead => tracing::warn!(outcome = "command line not read", exit, "command finished"),
            Outcome::Rendered { frames } => tracing::info!(outcome = "rendered", exit, frames = *frames, "command finished"),
            Outcome::VideoNotMade { code } => {
                tracing::error!(outcome = "video not made", exit, code = code.as_str(), "command finished")
            }
            Outcome::Refused { code } => {
                tracing::warn!(outcome = "refused", exit, code = code.as_str(), "command finished")
            }
        }
    }
}

/// What a command has to say, and how it ends.
enum Reply {
    /// Lines for standard output, the exit code, and how it ended.
    Printed(Vec<String>, u8, Outcome),
    /// A command line that cannot be read, with what is wrong with it and the usage lines to print.
    Unreadable(Code, String, &'static [&'static str]),
}

/// `framesmith check <path>`: opens the project that the path names (a folder, a project file or a
/// composition file) and says whether its project file and its composition are valid, or every
/// problem they have. The forms of a place, a size, a margin and a position are judged by the code
/// `render` uses, so that `check` names each one `render` refuses (`docs/spec/check-command.md`).
fn check(path: PathBuf, codecs: &codec::Registry) -> Reply {
    match Project::open(&path) {
        Ok(project) => {
            let (mut reports, notes) = render::place_reports_and_notes(&project, codecs);
            // The sound files are opened as `render` opens them, once the composition has no other problem, so that
            // `check` names each one `render` refuses (`docs/spec/sound.md`, section 5).
            if reports.is_empty() {
                if let Err(found) = render::open_sounds(project.composition(), &render::sounds(&project)) {
                    reports = found;
                }
            }
            if !notes.is_empty() {
                tracing::debug!(notes = notes.len(), "notes");
            }
            if !reports.is_empty() {
                // The notes are printed before the closing line (`docs/spec/check-command.md`, section 1).
                return match reports_reply(&reports) {
                    Reply::Printed(mut lines, code, outcome) => {
                        let closing = lines.pop();
                        lines.extend(notes);
                        lines.extend(closing);
                        Reply::Printed(lines, code, outcome)
                    }
                    other => other,
                };
            }
            let root = project.root();
            let frames = if root.frames == 1 {
                "1 frame".to_string()
            } else {
                format!("{} frames", root.frames)
            };
            let mut lines = notes;
            lines.push(format!(
                "Ready: the project is valid, {frames}, {}x{}, {} fps",
                root.width, root.height, root.fps
            ));
            Reply::Printed(lines, EXIT_OK, Outcome::Ready)
        }
        Err(error) => not_opened(error),
    }
}

/// How the renderer of `render` came to be chosen (`docs/spec/render-command.md` section 1.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Chosen {
    /// The author named the renderer, with `cpu` or `gpu`.
    Setting,
    /// `auto` found a card that starts, and takes it.
    AutoCard,
    /// `auto` found no card that starts, and takes the processor, and the summary says so.
    AutoProcessor,
}

impl Chosen {
    /// The word of the `renderer chosen` line of the log (`docs/spec/run-log.md`).
    pub fn as_str(self) -> &'static str {
        match self {
            Chosen::Setting => "setting",
            Chosen::AutoCard => "auto card",
            Chosen::AutoProcessor => "auto no card",
        }
    }
}

/// The renderer that `render` draws with, and how it was chosen.
pub struct Choice {
    renderer: Box<dyn Renderer>,
    chosen: Chosen,
}

impl Choice {
    /// The renderer's name: `processor` or `card`.
    pub fn name(&self) -> &'static str {
        self.renderer.name()
    }

    /// The renderer, to draw the frames of the film with.
    pub fn renderer_mut(&mut self) -> &mut dyn Renderer {
        self.renderer.as_mut()
    }

    /// How the renderer was chosen.
    pub fn chosen(&self) -> Chosen {
        self.chosen
    }

    /// The line of the summary that names the renderer and how it was chosen.
    pub fn summary_line(&self) -> String {
        match self.chosen {
            Chosen::Setting => format!("renderer: {}", self.name()),
            Chosen::AutoCard => format!("renderer: {} (chosen by auto)", self.name()),
            Chosen::AutoProcessor => format!("renderer: {} (chosen by auto: no suitable card)", self.name()),
        }
    }
}

/// Chooses the renderer for the settings. This is the one place where the choice is made.
///
/// `cpu` takes the processor and never starts the card. `gpu` starts the card, and a card that does
/// not start is the error: nothing falls back to the processor. `auto` starts the card, and takes the
/// processor when the card does not start, so it never refuses for want of a card. The card is
/// started here, before the film is opened.
pub fn renderer_for(settings: &Settings, start_card: &StartCard) -> Result<Choice, draw::Error> {
    match settings.backend() {
        Backend::Cpu => Ok(Choice {
            renderer: Box::new(CpuRenderer),
            chosen: Chosen::Setting,
        }),
        Backend::Gpu => Ok(Choice {
            renderer: start_card()?,
            chosen: Chosen::Setting,
        }),
        Backend::Auto => Ok(match start_card() {
            Ok(renderer) => Choice {
                renderer,
                chosen: Chosen::AutoCard,
            },
            Err(_) => Choice {
                renderer: Box::new(CpuRenderer),
                chosen: Chosen::AutoProcessor,
            },
        }),
    }
}

/// `framesmith render <path>`: opens the project as `check` does, makes the scene this version
/// draws, draws every frame on the renderer that the settings choose and writes the film to the
/// project's output path. A project that does not open, or that uses what this version does not
/// draw, is told as `check` tells it and nothing is written (`docs/spec/render-command.md`).
fn render(path: PathBuf, options: &Options, marks: bool, start_card: &StartCard, codecs: &codec::Registry) -> Reply {
    let mut project = match Project::open(&path) {
        Ok(project) => project,
        Err(error) => return not_opened(error),
    };
    project.apply(options);
    tracing::debug!(options = options.count(), "settings chosen");
    let scene = match Scene::from_project(&project, codecs) {
        Ok(scene) => scene,
        Err(reports) => return reports_reply(&reports),
    };
    // The sound files are opened before the renderer and the film, so that a sound that cannot be placed writes no
    // file (`docs/spec/sound.md`, section 5).
    let sounds = match render::open_sounds(project.composition(), scene.sounds()) {
        Ok(sounds) => sounds,
        Err(reports) => return reports_reply(&reports),
    };
    let written_output = project.output();
    let output = project.resolve(&written_output);
    let settings = project.settings();
    // The renderer is chosen and started before the film is opened, so that a card that cannot be
    // started writes no file (`docs/spec/render-command.md` section 1.2).
    let mut choice = match renderer_for(settings, start_card) {
        Ok(choice) => choice,
        Err(error) => return renderer_refused(&error),
    };
    tracing::debug!(renderer = choice.name(), chosen_by = choice.chosen().as_str(), "renderer chosen");
    // A renderer that draws no text refuses the film at its first `<text>`, before any frame and before the
    // film is opened (`docs/spec/text.md`, section 9).
    if !choice.renderer_mut().draws_text() {
        if let Some((file, line)) = scene.first_text() {
            return reports_reply(&[text_not_drawn(file, line, choice.name())]);
        }
    }
    // An output place that cannot be made or written is refused before the first frame, with no file written
    // (`docs/spec/render-command.md`, section 6).
    if let Err(refusal) = render::check_place(&output) {
        return output_refused(&refusal);
    }
    let mut drawn_marks = Vec::new();
    let asked = marks.then_some(&mut drawn_marks);
    match render::write_film_marked(&scene, sounds, choice.renderer_mut(), &output, settings.audio_bitrate(), asked) {
        Ok(written) => {
            let mut lines = vec![
                format!("Rendered {}", output.display()),
                format!("frames: {}", written.frames),
                format!("size: {}x{}", scene.width(), scene.height()),
                format!("frame rate: {} fps", scene.fps()),
                format!("length: {:.3} s", written.frames as f64 / f64::from(scene.fps())),
                choice.summary_line(),
            ];
            lines.extend(settings_used(settings, &written_output));
            lines.push(format!("elapsed: {:.3} s", written.seconds));
            lines.extend(
                drawn_marks
                    .iter()
                    .enumerate()
                    .map(|(number, mark)| format!("mark {number:04}: {mark:016x}")),
            );
            Reply::Printed(lines, EXIT_OK, Outcome::Rendered { frames: written.frames })
        }
        Err(failure) => video_not_made(&format!("the video could not be made: {failure}")),
    }
}

/// The report of the first `<text>` of a film that the chosen renderer does not draw: `FS-0035` on the
/// `<text>`'s line, which names the renderer and the processor as the one that draws text in this version.
fn text_not_drawn(file: &str, line: u32, renderer: &str) -> Report {
    Report::new(
        file,
        Problem::new(
            Code::FormNotSupported,
            line,
            format!(
                "<text> is not drawn by the {renderer} renderer in this version, which draws no text; \
                 the processor draws it: use `--backend cpu`"
            ),
        ),
    )
}

/// The reply of a `render` whose output path cannot be made or written: the report of `FS-0072` with the path and the
/// system's reason, then the closing line, and exit code 2, as decision A19 gives (`docs/spec/render-command.md`,
/// sections 6 and 7). Nothing is written.
fn output_refused(refusal: &render::Refusal) -> Reply {
    let code = Code::OutputNotWritten;
    Reply::Printed(
        vec![
            code.statement(&format!(
                "the output path `{}` cannot be made or written: {}",
                refusal.path().display(),
                refusal.reason()
            )),
            "Refused: the video was not made, because its folder cannot be written".to_string(),
        ],
        EXIT_UNSUPPORTED,
        Outcome::Refused { code },
    )
}

/// The reply of a `render` whose video could not be made: the report of `FS-0036` with its reason,
/// then the closing line, and exit code 1 (`docs/spec/render-command.md` sections 5.1 and 7).
fn video_not_made(reason: &str) -> Reply {
    let code = Code::VideoNotMade;
    Reply::Printed(
        vec![code.statement(reason), "Failed: the video could not be made".to_string()],
        EXIT_FAILED,
        Outcome::VideoNotMade { code },
    )
}

/// The reply of a `render` that refuses the film before its first frame, because the author asked for
/// the card (`gpu`) and no card can be started: the report of `FS-0038` with the card's reason, then
/// the closing line, and exit code 2. No file is written (`docs/spec/render-command.md` sections 1.2
/// and 7).
fn renderer_refused(error: &draw::Error) -> Reply {
    let code = Code::RendererNotAvailable;
    Reply::Printed(
        vec![
            code.statement(&error.to_string()),
            "Refused: the video was not made, because the graphics card that `gpu` needs is not available".to_string(),
        ],
        EXIT_UNSUPPORTED,
        Outcome::Refused { code },
    )
}

/// The lines of the summary that name each setting used, with where its value came from
/// (`docs/spec/render-command.md` section 7).
fn settings_used(settings: &Settings, output: &str) -> Vec<String> {
    let line = |label: &str, value: String, key: &str| format!("{label}: {value} ({})", settings.source(key));
    vec![
        line("output", output.to_string(), "render.output"),
        line("backend", settings.backend().to_string(), "render.backend"),
        line("quality", settings.quality().to_string(), "render.quality"),
        line("encoder", settings.encoder().to_string(), "render.encoder"),
        line("codec", settings.codec().to_string(), "render.codec"),
        line("workers", settings.workers().to_string(), "render.workers"),
        line(
            "audio bitrate",
            format!("{} kbit/s", settings.audio_bitrate()),
            "render.audio-bitrate",
        ),
    ]
}

/// What a command prints when the project did not open: the same for `check` and `render`.
fn not_opened(error: OpenError) -> Reply {
    match error {
        OpenError::NoProject(no_project) => {
            let code = no_project.code();
            Reply::Printed(
                vec![OpenError::NoProject(no_project).to_string()],
                EXIT_FAILED,
                Outcome::NoProject { code, problems: 0 },
            )
        }
        OpenError::InvalidAndNoProject(reports, no_project) => {
            log_reports(&reports);
            let code = no_project.code();
            let problems = reports.len();
            let mut lines: Vec<String> = reports.iter().map(ToString::to_string).collect();
            lines.push(OpenError::NoProject(no_project).to_string());
            Reply::Printed(lines, EXIT_FAILED, Outcome::NoProject { code, problems })
        }
        OpenError::Invalid(reports) => reports_reply(&reports),
    }
}

/// The reports of a project that has some, and the closing line: `Unsupported` and the exit code 2
/// when every report is of the kind unsupported, `Invalid` and 1 otherwise.
fn reports_reply(reports: &[Report]) -> Reply {
    log_reports(reports);
    let files = files_of(reports);
    let mut lines: Vec<String> = reports.iter().map(ToString::to_string).collect();
    // Only what is not supported yet is wrong: the rest of the project is valid.
    if reports.iter().all(|report| report.problem().kind() == Kind::Unsupported) {
        lines.push(format!(
            "Unsupported: the project uses what this version does not support ({files})"
        ));
        Reply::Printed(lines, EXIT_UNSUPPORTED, Outcome::Unsupported { problems: reports.len() })
    } else {
        lines.push(format!("Invalid: the project is not valid ({files})"));
        Reply::Printed(lines, EXIT_FAILED, Outcome::Invalid { problems: reports.len() })
    }
}

/// Logs each report of a run at the most detailed level: where it is and its code, never its words.
fn log_reports(reports: &[Report]) {
    for report in reports {
        tracing::trace!(
            file = report.file(),
            code = report.problem().code().as_str(),
            line = report.problem().line(),
            "report"
        );
    }
}

/// The files that have a report, each once, in the order the reports are printed.
fn files_of(reports: &[Report]) -> String {
    let mut files: Vec<&str> = Vec::new();
    for report in reports {
        if !files.contains(&report.file()) {
            files.push(report.file());
        }
    }
    files.join(", ")
}
