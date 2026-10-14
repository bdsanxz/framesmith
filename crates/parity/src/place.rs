//! Where the tool may write: never a place that would add a file of the reference to a git
//! repository (`docs/dev/comparison.md`, section 9).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// The path made absolute, with the part that exists made canonical (so that a link such as `/var`
/// is followed) and the rest, which does not exist yet, kept as written.
fn resolved(path: &Path) -> PathBuf {
    let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let mut rest: Vec<std::ffi::OsString> = Vec::new();
    let mut here = absolute.as_path();
    loop {
        if let Ok(real) = here.canonicalize() {
            let mut out = real;
            for part in rest.iter().rev() {
                out.push(part);
            }
            return out;
        }
        match (here.file_name(), here.parent()) {
            (Some(name), Some(parent)) => {
                rest.push(name.to_os_string());
                here = parent;
            }
            _ => return absolute,
        }
    }
}

/// Refuses a place inside a work tree of git that git would not ignore. `what` says in the message
/// what is to be written there. A place outside every repository, and a place a `.gitignore` covers,
/// are allowed. Nothing is written or made.
pub fn keep_out_of_repositories(path: &Path, what: &str) -> Result<(), String> {
    let place = resolved(path);
    let Some(root) = place.ancestors().skip(1).find(|folder| folder.join(".git").exists()) else {
        return Ok(());
    };
    let existing = place.ancestors().skip(1).find(|folder| folder.is_dir()).unwrap_or(root);
    let refused = || {
        format!(
            "{what} {} would be inside the git repository {}, which does not ignore it; \
             give a place outside a repository, or one its .gitignore covers",
            path.display(),
            root.display()
        )
    };
    let status = Command::new("git")
        .arg("-C")
        .arg(existing)
        .args(["check-ignore", "-q", "--"])
        .arg(&place)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|why| {
            format!(
                "{what} {} would be inside the git repository {}, and git could not be asked whether it is ignored ({why})",
                path.display(),
                root.display()
            )
        })?;
    match status.code() {
        Some(0) => Ok(()),
        Some(1) => Err(refused()),
        other => Err(format!(
            "{what} {} would be inside the git repository {}, and git could not say whether it is ignored (it ended with {other:?})",
            path.display(),
            root.display()
        )),
    }
}
