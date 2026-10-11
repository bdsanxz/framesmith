//! The output location. A run writes to a staging file beside the output
//! path and moves it into place only at the end, so the output path never
//! holds a partial file.

use crate::Failure;
use std::path::{Path, PathBuf};

pub struct Staging {
    output: PathBuf,
    staging: PathBuf,
    /// Folders this run made, outermost first.
    made: Vec<PathBuf>,
    settled: bool,
}

fn cannot_write(path: &Path, reason: impl std::fmt::Display) -> Failure {
    Failure::new(
        "FS-R005",
        format!("The output cannot be written at {}: {reason}.", path.display()),
        "Choose an output path in a folder you can write to, with --output <path>.",
    )
}

impl Staging {
    /// Makes missing parent folders and the staging file. `output` must be
    /// an absolute path. On failure nothing is left behind.
    pub fn create(output: &Path, run_id: &str) -> Result<Staging, Failure> {
        if output.is_dir() {
            return Err(cannot_write(output, "it is a folder, not a file"));
        }
        let (Some(parent), Some(name)) = (output.parent(), output.file_name()) else {
            return Err(cannot_write(output, "it does not name a file"));
        };
        let mut made: Vec<PathBuf> = Vec::new();
        let mut missing = Some(parent);
        while let Some(folder) = missing.filter(|folder| !folder.as_os_str().is_empty() && !folder.exists()) {
            made.insert(0, folder.to_path_buf());
            missing = folder.parent();
        }
        let mut staging = Staging {
            output: output.to_path_buf(),
            staging: parent.join(format!(".{}.{run_id}.part", name.to_string_lossy())),
            made,
            settled: false,
        };
        if let Err(error) = std::fs::create_dir_all(parent) {
            staging.discard();
            let reason = format!("the folder {} could not be made ({error})", parent.display());
            return Err(cannot_write(output, reason));
        }
        if let Err(error) = std::fs::File::create_new(&staging.staging) {
            staging.discard();
            let reason = format!("a file cannot be made in the folder {} ({error})", parent.display());
            return Err(cannot_write(output, reason));
        }
        Ok(staging)
    }

    /// The file the encoder writes.
    pub fn path(&self) -> &Path {
        &self.staging
    }

    pub fn output(&self) -> &Path {
        &self.output
    }

    /// Moves the staging file onto the output path in one step, replacing
    /// a file already there.
    pub fn commit(&mut self) -> Result<(), Failure> {
        match std::fs::rename(&self.staging, &self.output) {
            Ok(()) => {
                self.settled = true;
                Ok(())
            }
            Err(error) => {
                let failure = cannot_write(&self.output, error);
                self.discard();
                Err(failure)
            }
        }
    }

    /// Removes the staging file, and the folders this run made when they
    /// are empty. A file already at the output path is not touched.
    pub fn discard(&mut self) {
        if self.settled {
            return;
        }
        self.settled = true;
        let _ = std::fs::remove_file(&self.staging);
        for folder in self.made.iter().rev() {
            if std::fs::remove_dir(folder).is_err() {
                break;
            }
        }
    }
}

impl Drop for Staging {
    fn drop(&mut self) {
        self.discard();
    }
}
