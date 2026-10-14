//! From the path a command is given to the project folder: `docs/spec/project-location.md`.

use std::fs;
use std::io::ErrorKind;
use std::path::{self, Path, PathBuf};

use crate::{reason, NoProject, PROJECT_FILE};

/// Which of the three forms the path given was.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Form {
    Folder,
    ProjectFile,
    CompositionFile,
}

impl Form {
    /// The word the log gives the form.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Form::Folder => "folder",
            Form::ProjectFile => "project file",
            Form::CompositionFile => "composition file",
        }
    }
}

/// Where the path given leads.
#[derive(Debug)]
pub(crate) struct Located {
    pub form: Form,
    /// The project folder.
    pub folder: PathBuf,
    /// The composition file the path gave, as a path from the project folder written with `/`:
    /// only for the form composition file.
    pub composition: Option<String>,
    /// The path as it was written, for the reasons that name it.
    pub given: PathBuf,
    /// How many folders were looked in for a project file.
    pub folders_searched: usize,
}

/// Examines `given` and finds the project folder.
pub(crate) fn locate(given: &Path) -> Result<Located, NoProject> {
    let text = given.to_string_lossy();
    let closing = text.len() > 1 && text.ends_with(path::is_separator);
    let examined = if closing {
        let trimmed = text.trim_end_matches(path::is_separator);
        if trimmed.is_empty() {
            given.to_path_buf()
        } else {
            PathBuf::from(trimmed)
        }
    } else {
        given.to_path_buf()
    };

    let metadata = match fs::metadata(&examined) {
        Ok(metadata) => metadata,
        Err(error) if closing && is_not_there(&error) => return Err(NoProject::FolderMissing(given.to_path_buf())),
        Err(error) if is_not_there(&error) => return Err(NoProject::PathMissing(given.to_path_buf())),
        Err(error) => return Err(NoProject::PathUnreadable(given.to_path_buf(), reason(&error))),
    };

    if metadata.is_dir() {
        return Ok(Located {
            form: Form::Folder,
            folder: given.to_path_buf(),
            composition: None,
            given: given.to_path_buf(),
            folders_searched: 0,
        });
    }
    // A closing `/` can only name a folder, and a pipe or a device is read by nobody.
    if closing || !metadata.is_file() {
        return Err(NoProject::NotAFolder(given.to_path_buf()));
    }
    let Some(name) = examined.file_name() else {
        return Err(NoProject::NotAFolder(given.to_path_buf()));
    };
    let parent = match examined.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => PathBuf::from("."),
    };

    if name == PROJECT_FILE {
        return Ok(Located {
            form: Form::ProjectFile,
            folder: parent,
            composition: None,
            given: given.to_path_buf(),
            folders_searched: 0,
        });
    }

    // A composition file belongs to the nearest folder at or above it that holds a project file.
    let real = fs::canonicalize(&parent).map_err(|error| NoProject::PathUnreadable(given.to_path_buf(), reason(&error)))?;
    let name = name.to_string_lossy().into_owned();
    let mut searched = 0;
    let mut found = None;
    for dir in real.ancestors() {
        searched += 1;
        if fs::metadata(dir.join(PROJECT_FILE)).is_ok() {
            found = Some(dir);
            break;
        }
    }
    match found {
        Some(dir) if dir != real => {
            let below = real.strip_prefix(dir).unwrap_or(Path::new(""));
            let mut composition: Vec<String> = below
                .components()
                .map(|part| part.as_os_str().to_string_lossy().into_owned())
                .collect();
            composition.push(name);
            Ok(Located {
                form: Form::CompositionFile,
                folder: dir.to_path_buf(),
                composition: Some(composition.join("/")),
                given: given.to_path_buf(),
                folders_searched: searched,
            })
        }
        // The file's own folder holds the project file, or no folder above does: the project
        // folder is the file's own.
        _ => Ok(Located {
            form: Form::CompositionFile,
            folder: parent,
            composition: Some(name),
            given: given.to_path_buf(),
            folders_searched: searched,
        }),
    }
}

/// Whether the error says that nothing is at the path (a name on the way that is a file counts).
fn is_not_there(error: &std::io::Error) -> bool {
    matches!(error.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory)
}
