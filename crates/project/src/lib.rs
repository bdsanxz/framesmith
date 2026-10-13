//! The project folder: where a composition is found and read.
//!
//! Behaviour is specified in `docs/spec/check-command.md` (sections 2, 5 and 6). A folder that
//! holds a `composition.xml` is a complete project; the project file is read by a later part.

use std::fmt;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

pub use notation::{Colour, Problem, Root};

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
    /// The composition's root is not valid.
    Invalid(Problem),
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

impl Project {
    /// Opens the project in `folder`: reads its `composition.xml`, for reading only, and judges the
    /// root. Nothing is created, changed or removed.
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
        let root = notation::read_root(&source).map_err(OpenError::Invalid)?;
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
            OpenError::NoProject(no_project) => write!(f, "No project: {no_project}"),
            OpenError::Invalid(problem) => write!(f, "{problem}"),
        }
    }
}
