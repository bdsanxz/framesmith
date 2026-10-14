//! The project folder: where a composition is found and read.
//!
//! Behaviour is specified in `docs/spec/check-command.md` (sections 2, 5 and 6). A folder that
//! holds a `composition.xml` is a complete project; the project file is read by a later part.

use std::fmt;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

pub use notation::{Code, Colour, Kind, Problem, Root};

/// The name of the composition file in a project folder.
pub const COMPOSITION_FILE: &str = "composition.xml";

/// A project whose composition has been read and whose root is valid.
#[derive(Clone, Debug)]
pub struct Project {
    folder: PathBuf,
    root: Root,
}

/// Why a folder did not open as a project.
#[derive(Debug)]
pub enum OpenError {
    /// There is no composition to read. The paths are as the caller wrote them.
    NoProject(NoProject),
    /// The composition's root is not valid: every problem found, in the order of the file's lines.
    /// There is at least one.
    Invalid(Vec<Report>),
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
}

impl NoProject {
    /// The code of the reason (`docs/spec/error-catalogue.md`).
    pub fn code(&self) -> Code {
        match self {
            NoProject::FolderMissing(_) => Code::FolderMissing,
            NoProject::NotAFolder(_) => Code::NotAFolder,
            NoProject::NoComposition(_) => Code::NoComposition,
            NoProject::Unreadable(..) => Code::CompositionUnreadable,
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
    /// Opens the project in `folder`: reads its `composition.xml`, for reading only, and judges the
    /// root, reporting every problem it has. Nothing is created, changed or removed.
    pub fn open(folder: &Path) -> Result<Project, OpenError> {
        match fs::metadata(folder) {
            Err(error) if error.kind() == ErrorKind::NotFound => {
                return Err(OpenError::NoProject(NoProject::FolderMissing(folder.to_path_buf())))
            }
            Err(error) => return Err(OpenError::NoProject(NoProject::Unreadable(folder.to_path_buf(), reason(&error)))),
            Ok(metadata) if !metadata.is_dir() => return Err(OpenError::NoProject(NoProject::NotAFolder(folder.to_path_buf()))),
            Ok(_) => {}
        }
        let source = match fs::read(folder.join(COMPOSITION_FILE)) {
            Ok(source) => source,
            Err(error) if error.kind() == ErrorKind::NotFound => {
                return Err(OpenError::NoProject(NoProject::NoComposition(folder.to_path_buf())))
            }
            Err(error) => return Err(OpenError::NoProject(NoProject::Unreadable(folder.to_path_buf(), reason(&error)))),
        };
        tracing::debug!(bytes = source.len(), "composition file read");
        let judged = notation::read_root(&source);
        tracing::debug!(problems = judged.as_ref().err().map_or(0, Vec::len), "root judged");
        let root = judged.map_err(|problems| {
            OpenError::Invalid(
                problems
                    .into_iter()
                    .map(|problem| Report {
                        file: COMPOSITION_FILE.to_string(),
                        problem,
                    })
                    .collect(),
            )
        })?;
        Ok(Project {
            folder: folder.to_path_buf(),
            root,
        })
    }

    /// The project folder, as it was given to [`Project::open`].
    pub fn folder(&self) -> &Path {
        &self.folder
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
        }
    }
}
