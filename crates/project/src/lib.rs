//! The project folder: the project file and the composition it names, read and judged.
//!
//! Behaviour is specified in `docs/spec/check-command.md` (sections 2, 5 and 6) and
//! `docs/spec/project-file.md`. A folder that holds a `composition.xml` is a complete project; a
//! `framesmith.yaml` beside it may name another composition and set how the project is rendered.

mod file;
mod settings;

use std::fmt;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

pub use notation::{Code, Colour, Kind, Problem, Root};
pub use settings::{Backend, Codec, Encoder, Quality, Settings, Workers, MAX_WORKERS, PROJECT_FILE};

use file::CompositionChoice;

/// The name of the composition file in a project folder whose project file does not name another.
pub const COMPOSITION_FILE: &str = "composition.xml";

/// A project whose project file and composition have been read and are valid.
#[derive(Clone, Debug)]
pub struct Project {
    folder: PathBuf,
    settings: Settings,
    root: Root,
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
        write!(f, "{}, {}", self.file, self.problem)
    }
}

/// The reasons there is no composition to read in a folder.
#[derive(Debug)]
pub enum NoProject {
    /// Nothing exists at the path.
    FolderMissing(PathBuf),
    /// The path is not a folder.
    NotAFolder(PathBuf),
    /// The folder holds no `composition.xml`.
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
    /// Opens the project in `folder`: reads its `framesmith.yaml` when it has one, then the
    /// composition that file names (`composition.xml` when it names none), for reading only, and
    /// judges the project file's keys and the composition's root, reporting every problem they
    /// have. Nothing is created, changed or removed.
    pub fn open(folder: &Path) -> Result<Project, OpenError> {
        match fs::metadata(folder) {
            Err(error) if error.kind() == ErrorKind::NotFound => {
                return Err(OpenError::NoProject(NoProject::FolderMissing(folder.to_path_buf())))
            }
            Err(error) => return Err(OpenError::NoProject(NoProject::Unreadable(folder.to_path_buf(), reason(&error)))),
            Ok(metadata) if !metadata.is_dir() => return Err(OpenError::NoProject(NoProject::NotAFolder(folder.to_path_buf()))),
            Ok(_) => {}
        }

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
        let (path, line) = match &judged.composition {
            CompositionChoice::Default => (COMPOSITION_FILE.to_string(), None),
            CompositionChoice::Named { path, line } => (path.clone(), Some(*line)),
            CompositionChoice::Unknown => return Err(OpenError::Invalid(reports)),
        };
        let source = match fs::read(folder.join(&path)) {
            Ok(source) => source,
            Err(error) => {
                let missing = error.kind() == ErrorKind::NotFound;
                let no_project = match (line, missing) {
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
        let judged_root = notation::read_root(&source);
        tracing::debug!(problems = judged_root.as_ref().err().map_or(0, Vec::len), "root judged");
        let root = match judged_root {
            Ok(root) => Some(root),
            Err(problems) => {
                reports.extend(problems.into_iter().map(|problem| Report {
                    file: path.clone(),
                    problem,
                }));
                None
            }
        };
        match root {
            Some(root) if reports.is_empty() => Ok(Project {
                folder: folder.to_path_buf(),
                settings: judged.settings,
                root,
            }),
            _ => Err(OpenError::Invalid(reports)),
        }
    }

    /// The project folder, as it was given to [`Project::open`].
    pub fn folder(&self) -> &Path {
        &self.folder
    }

    /// The settings of the project file, with the defaults of the keys it does not write.
    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// The name of the project: `name` of the project file, or the name of the project folder.
    pub fn name(&self) -> String {
        match self.settings.name_written() {
            Some(name) => name.to_string(),
            None => settings::folder_name(&self.folder),
        }
    }

    /// The finished file, as a path from the project folder: `render.output`, or
    /// `<paths.renders>/<name>.mp4`.
    pub fn output(&self) -> String {
        self.settings.output(&settings::folder_name(&self.folder))
    }

    /// The composition's root.
    pub fn root(&self) -> &Root {
        &self.root
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
            NoProject::NoComposition(path) => write!(f, "there is no {COMPOSITION_FILE} in `{}`", path.display()),
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
