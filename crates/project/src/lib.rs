//! The project folder: the project file and the composition it names, read and judged.
//!
//! Behaviour is specified in `docs/spec/check-command.md` (sections 2, 5 and 6),
//! `docs/spec/project-file.md` and `docs/spec/project-location.md`. A project is named by a folder,
//! a `framesmith.yaml` or a composition file. A folder that holds a `composition.xml` is a complete
//! project; a `framesmith.yaml` beside it may name another composition and set how the project is
//! rendered. Every path written in a project is relative to the project folder.

mod file;
mod locate;
mod nesting;
mod options;
mod settings;

use std::fmt;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

pub use nesting::{ShownFile, MAX_CHAIN_DEPTH};
pub use notation::{
    colour_value, easing, number_value, shown, Attribute, Code, Colour, CountFormat, Easing, Element, Kind, Length, Node, Number, Part,
    Problem, Root, Tree,
};
pub use options::{Options, OPTION_NAMES};
pub use settings::{Backend, Codec, Encoder, Quality, Settings, Source, Workers, MAX_WORKERS, PROJECT_FILE};

use file::CompositionChoice;

/// The place a report of an option names in place of a file: it has no line
/// (`docs/spec/render-command.md` section 3).
pub const COMMAND_LINE: &str = "command line";

/// The name of the composition file in a project folder whose project file does not name another.
pub const COMPOSITION_FILE: &str = "composition.xml";

/// A project whose project file and composition have been read and are valid.
#[derive(Clone, Debug)]
pub struct Project {
    folder: PathBuf,
    /// The composition read, as a path from the project folder written with `/`.
    composition: String,
    settings: Settings,
    root: Root,
    tree: Tree,
    /// The compositions the root shows with `<composition src>`, read and judged, in the order they are reached.
    shown: Vec<ShownFile>,
}

/// Why a folder did not open as a project.
#[derive(Debug)]
pub enum OpenError {
    /// There is no composition to read. The paths are as the caller wrote them.
    NoProject(NoProject),
    /// The project file or the composition is not valid: every problem found. The reports of the
    /// project file come first, then those of the composition, each in the order of the file's
    /// lines. There is at least one.
    Invalid(Vec<Report>),
    /// The project file has problems, and there is no composition to read: the reports of the
    /// project file (at least one) and the reason there is no composition.
    InvalidAndNoProject(Vec<Report>, NoProject),
}

/// One problem of a file of the project, with the name of the file.
///
/// It prints as one line, `file, line N: code: kind: message. remedy` (`docs/spec/check-command.md`
/// section 4), and the line is the same whether the project has one file or many.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Report {
    file: String,
    problem: Problem,
}

impl Report {
    /// A report of `problem` in `file`, a path from the project folder written with `/`.
    pub fn new(file: impl Into<String>, problem: Problem) -> Report {
        Report {
            file: file.into(),
            problem,
        }
    }

    /// The file the problem is in, as the path from the project folder written with `/`.
    pub fn file(&self) -> &str {
        &self.file
    }

    /// The problem, with its code, its line and its message.
    pub fn problem(&self) -> &Problem {
        &self.problem
    }
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.file == COMMAND_LINE {
            // An option has no line.
            write!(f, "{COMMAND_LINE}: {}", self.problem.code().statement(self.problem.message()))
        } else {
            write!(f, "{}, {}", self.file, self.problem)
        }
    }
}

/// The reasons there is no composition to read.
#[derive(Debug)]
pub enum NoProject {
    /// Nothing exists at the path, which was written with a closing `/` and so names a folder.
    FolderMissing(PathBuf),
    /// The path was written with a closing `/` and is a file, or it is neither a folder nor a file.
    NotAFolder(PathBuf),
    /// Nothing exists at the path.
    PathMissing(PathBuf),
    /// The path exists and could not be examined, or it is the composition file given and could
    /// not be read: the path as written, and the reason the system gave.
    PathUnreadable(PathBuf, String),
    /// The project folder holds no `composition.xml`, and its project file, if it has one, names
    /// no other composition.
    NoComposition(PathBuf),
    /// The folder holds a `composition.xml` that could not be read, with the reason the system gave.
    Unreadable(PathBuf, String),
    /// The folder holds a `framesmith.yaml` that could not be read, with the reason the system gave.
    ProjectFileUnreadable(PathBuf, String),
    /// The composition the project file names does not exist: its path as written, the line of the
    /// key `composition`, and the project folder.
    NamedCompositionMissing(String, u32, PathBuf),
    /// The composition the project file names exists and could not be read: its path as written,
    /// the line of the key, the project folder and the reason the system gave.
    NamedCompositionUnreadable(String, u32, PathBuf, String),
}

impl NoProject {
    /// The code of the reason (`docs/spec/error-catalogue.md`).
    pub fn code(&self) -> Code {
        match self {
            NoProject::FolderMissing(_) => Code::FolderMissing,
            NoProject::NotAFolder(_) => Code::NotAFolder,
            NoProject::PathMissing(_) => Code::PathMissing,
            NoProject::PathUnreadable(..) => Code::PathUnreadable,
            NoProject::NoComposition(_) => Code::NoComposition,
            NoProject::Unreadable(..) => Code::CompositionUnreadable,
            NoProject::ProjectFileUnreadable(..) => Code::ProjectFileUnreadable,
            NoProject::NamedCompositionMissing(..) => Code::NamedCompositionMissing,
            NoProject::NamedCompositionUnreadable(..) => Code::NamedCompositionUnreadable,
        }
    }

    /// The kind of the reason: the kind of its code.
    pub fn kind(&self) -> Kind {
        self.code().kind()
    }

    /// What to change: the remedy of the reason's code.
    pub fn remedy(&self) -> &'static str {
        self.code().remedy()
    }
}

impl Project {
    /// Opens the project that `path` names: a folder, a `framesmith.yaml` or a composition file
    /// (`docs/spec/project-location.md`). It finds the project folder, reads the folder's
    /// `framesmith.yaml` when it has one, then the composition (the file given, else the one that
    /// project file names, else `composition.xml`), for reading only, and judges the project
    /// file's keys and the composition's root, reporting every problem they have. Nothing is
    /// created, changed or removed.
    pub fn open(path: &Path) -> Result<Project, OpenError> {
        let located = locate::locate(path).map_err(OpenError::NoProject)?;
        tracing::debug!(
            form = located.form.as_str(),
            folders_searched = located.folders_searched,
            "project located"
        );
        let folder = located.folder.as_path();

        let judged = match fs::read(folder.join(PROJECT_FILE)) {
            Ok(source) => {
                tracing::debug!(bytes = source.len(), "project file read");
                let judged = file::read(&source);
                tracing::debug!(problems = judged.problems.len(), "project file judged");
                judged
            }
            Err(error) if error.kind() == ErrorKind::NotFound => file::Judged {
                settings: Settings::default(),
                problems: Vec::new(),
                composition: CompositionChoice::Default,
            },
            Err(error) => {
                return Err(OpenError::NoProject(NoProject::ProjectFileUnreadable(
                    folder.to_path_buf(),
                    reason(&error),
                )))
            }
        };
        let mut reports: Vec<Report> = judged
            .problems
            .into_iter()
            .map(|problem| Report {
                file: PROJECT_FILE.to_string(),
                problem,
            })
            .collect();

        // The composition: the file the project file names, the default one, or none when nothing
        // says which file is meant.
        let (path, line) = match (&located.composition, &judged.composition) {
            // The composition file the command was given is the one rendered, whatever the project
            // file names (`project-location.md` section 1).
            (Some(given), _) => (given.clone(), None),
            (None, CompositionChoice::Default) => (COMPOSITION_FILE.to_string(), None),
            (None, CompositionChoice::Named { path, line }) => (path.clone(), Some(*line)),
            (None, CompositionChoice::Unknown) => return Err(OpenError::Invalid(reports)),
        };
        let source = match fs::read(written_path(folder, &path)) {
            Ok(source) => source,
            Err(error) => {
                let missing = error.kind() == ErrorKind::NotFound;
                let no_project = match (line, missing) {
                    (None, true) if located.composition.is_some() => NoProject::PathMissing(located.given.clone()),
                    (None, false) if located.composition.is_some() => NoProject::PathUnreadable(located.given.clone(), reason(&error)),
                    (None, true) => NoProject::NoComposition(folder.to_path_buf()),
                    (None, false) => NoProject::Unreadable(folder.to_path_buf(), reason(&error)),
                    (Some(line), true) => NoProject::NamedCompositionMissing(path, line, folder.to_path_buf()),
                    (Some(line), false) => NoProject::NamedCompositionUnreadable(path, line, folder.to_path_buf(), reason(&error)),
                };
                return Err(if reports.is_empty() {
                    OpenError::NoProject(no_project)
                } else {
                    OpenError::InvalidAndNoProject(reports, no_project)
                });
            }
        };
        tracing::debug!(bytes = source.len(), "composition file read");
        let judged_root = notation::read(&source);
        tracing::debug!(problems = judged_root.as_ref().err().map_or(0, Vec::len), "root judged");
        let root = match judged_root {
            Ok(composition) => Some(composition),
            Err(problems) => {
                reports.extend(problems.into_iter().map(|problem| Report {
                    file: path.clone(),
                    problem,
                }));
                None
            }
        };
        // The compositions the root shows are walked whatever the root's own problems are, so that
        // one run reports every problem of the chain (`docs/spec/notation.md` section 10).
        let walked = nesting::walk(folder, &path, &notation::shows(&source));
        reports.extend(walked.reports);
        match root {
            Some(composition) if reports.is_empty() => Ok(Project {
                folder: folder.to_path_buf(),
                composition: path,
                settings: judged.settings,
                root: composition.root,
                tree: composition.tree,
                shown: walked.files,
            }),
            _ => Err(OpenError::Invalid(reports)),
        }
    }

    /// The project folder: the folder given to [`Project::open`], the folder of the project file
    /// given, or the folder found for the composition file given (`docs/spec/project-location.md`).
    pub fn folder(&self) -> &Path {
        &self.folder
    }

    /// The composition that is read, as a path from the project folder written with `/`.
    pub fn composition(&self) -> &str {
        &self.composition
    }

    /// The path of the composition file that is read.
    pub fn composition_path(&self) -> PathBuf {
        self.resolve(&self.composition)
    }

    /// The path of a file written in the project, as it is written: `src` of a composition,
    /// `composition`, `paths.renders` and `render.output` of the project file. It is relative to
    /// the project folder, never to the folder the command is run from or to the folder of the
    /// composition that holds it. A written path that begins with `/` is a path of the machine and
    /// is taken as it is (`docs/spec/project-location.md` section 4).
    pub fn resolve(&self, written: &str) -> PathBuf {
        written_path(&self.folder, written)
    }

    /// The settings in use: the options given to [`Project::apply`], then the project file, then
    /// the defaults of the keys it does not write.
    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// Takes the options of the command line: each replaces the one setting it names for this run
    /// (`docs/spec/render-command.md` section 1.1). Nothing is written to the project file.
    pub fn apply(&mut self, options: &Options) {
        self.settings.apply(options);
    }

    /// The name of the project: `name` of the project file, or the name of the project folder.
    pub fn name(&self) -> String {
        match self.settings.name_written() {
            Some(name) => name.to_string(),
            None => settings::folder_name(&self.folder),
        }
    }

    /// The finished file, as a path from the project folder: `--output`, `render.output`, or
    /// `<paths.renders>/<name>.mp4`.
    pub fn output(&self) -> String {
        self.settings.output(&settings::folder_name(&self.folder))
    }

    /// The composition's root.
    pub fn root(&self) -> &Root {
        &self.root
    }

    /// The elements of the composition as they are written: the root first, then the others in
    /// document order.
    pub fn tree(&self) -> &Tree {
        &self.tree
    }

    /// The compositions the root shows with `<composition src>`, read and judged, in the order they
    /// are reached (`docs/spec/notation.md` section 10).
    pub fn shown_files(&self) -> &[ShownFile] {
        &self.shown
    }

    /// The composition that the element `<composition src>` names with `src` as written shows: the
    /// file found from the project folder. `None` when no element of the root names that file.
    pub fn shown_file(&self, written: &str) -> Option<&ShownFile> {
        let path = self.resolve(written);
        self.shown.iter().find(|file| file.path() == path.as_path())
    }
}

/// The path of a file written in a project folder (`docs/spec/project-location.md` section 4). A
/// written path that begins with `/` is taken as it is, on every system. Any other is joined to
/// the folder. The rule is not left to the join: on Windows a join keeps the drive of a folder
/// that has one, so a joined `/` path would not be the path that was written.
fn written_path(folder: &Path, written: &str) -> PathBuf {
    if written.starts_with('/') {
        PathBuf::from(written)
    } else {
        folder.join(written)
    }
}

/// The reason the system gives, without the number of the error.
fn reason(error: &std::io::Error) -> String {
    let text = error.to_string();
    match text.rfind(" (os error ") {
        Some(at) => text[..at].to_string(),
        None => text,
    }
}

impl fmt::Display for NoProject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NoProject::FolderMissing(path) => write!(f, "the folder `{}` does not exist", path.display()),
            NoProject::NotAFolder(path) => write!(f, "`{}` is not a folder", path.display()),
            NoProject::PathMissing(path) => write!(f, "`{}` does not exist", path.display()),
            NoProject::PathUnreadable(path, why) => write!(f, "`{}` could not be read: {why}", path.display()),
            NoProject::NoComposition(path) => write!(
                f,
                "there is no {COMPOSITION_FILE} in `{}`, and no {PROJECT_FILE} there names a composition",
                path.display()
            ),
            NoProject::Unreadable(path, why) => {
                write!(f, "{COMPOSITION_FILE} in `{}` could not be read: {why}", path.display())
            }
            NoProject::ProjectFileUnreadable(path, why) => {
                write!(f, "{PROJECT_FILE} in `{}` could not be read: {why}", path.display())
            }
            NoProject::NamedCompositionMissing(composition, line, path) => write!(
                f,
                "the composition `{}`, named by `composition` on line {line} of {PROJECT_FILE}, does not exist in `{}`",
                notation::shown(composition),
                path.display()
            ),
            NoProject::NamedCompositionUnreadable(composition, line, path, why) => write!(
                f,
                "the composition `{}`, named by `composition` on line {line} of {PROJECT_FILE}, in `{}` could not be read: {why}",
                notation::shown(composition),
                path.display()
            ),
        }
    }
}

impl fmt::Display for OpenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OpenError::NoProject(no_project) => {
                write!(f, "No project: {}", no_project.code().statement(&no_project.to_string()))
            }
            OpenError::Invalid(reports) => {
                for (index, report) in reports.iter().enumerate() {
                    if index > 0 {
                        writeln!(f)?;
                    }
                    write!(f, "{report}")?;
                }
                Ok(())
            }
            OpenError::InvalidAndNoProject(reports, no_project) => {
                for report in reports {
                    writeln!(f, "{report}")?;
                }
                write!(f, "No project: {}", no_project.code().statement(&no_project.to_string()))
            }
        }
    }
}
