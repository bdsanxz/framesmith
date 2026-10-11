//! Finding a browser to use. The order is fixed and documented in
//! `docs/spec/render-stages.md`.

use std::ffi::OsString;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// The variable that names the browser to use.
pub const BROWSER_VAR: &str = "FRAMESMITH_BROWSER";

const APP_NAMES: [&str; 8] = [
    "Google Chrome",
    "Chromium",
    "Google Chrome for Testing",
    "Microsoft Edge",
    "Brave Browser",
    "Vivaldi",
    "Google Chrome Beta",
    "Google Chrome Canary",
];
const PATH_NAMES: [&str; 6] = [
    "chromium",
    "chromium-browser",
    "google-chrome",
    "google-chrome-stable",
    "chrome",
    "chrome-headless-shell",
];
const CACHE_NAMES: [&str; 5] = [
    "Google Chrome for Testing",
    "Chromium",
    "chrome-headless-shell",
    "chrome",
    "headless_shell",
];
/// How deep a cache folder is searched.
const CACHE_DEPTH: usize = 8;

/// Where to look. Taken from the process by `Places::from_process`; tests
/// build their own.
#[derive(Clone, Debug, Default)]
pub struct Places {
    /// The value of `FRAMESMITH_BROWSER`, when set.
    pub named: Option<OsString>,
    /// Folders that hold application bundles (macOS).
    pub applications: Vec<PathBuf>,
    /// The folders of the search path.
    pub path: Vec<PathBuf>,
    /// Folders in which browser automation tools keep downloaded browsers.
    pub caches: Vec<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FindError {
    /// No browser anywhere.
    NotFound,
    /// The named browser does not exist or cannot be run.
    Unusable { path: PathBuf, reason: String },
}

impl Places {
    pub fn from_process() -> Places {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let mut applications = Vec::new();
        let mut caches = Vec::new();
        if cfg!(target_os = "macos") {
            applications.push(PathBuf::from("/Applications"));
        }
        if let Some(home) = &home {
            if cfg!(target_os = "macos") {
                applications.push(home.join("Applications"));
                caches.push(home.join("Library/Caches/ms-playwright"));
            }
            caches.push(home.join(".cache/puppeteer"));
            caches.push(home.join(".cache/ms-playwright"));
        }
        Places {
            named: std::env::var_os(BROWSER_VAR).filter(|value| !value.is_empty()),
            applications,
            path: std::env::var_os("PATH")
                .map(|path| std::env::split_paths(&path).collect())
                .unwrap_or_default(),
            caches,
        }
    }
}

fn runnable(path: &Path) -> Result<(), String> {
    let metadata = std::fs::metadata(path).map_err(|_| "it does not exist".to_string())?;
    if !metadata.is_file() {
        return Err("it is not a file".to_string());
    }
    if metadata.permissions().mode() & 0o111 == 0 {
        return Err("it is not marked as a program that can be run".to_string());
    }
    Ok(())
}

fn search_cache(folder: &Path, depth: usize, found: &mut Vec<(SystemTime, PathBuf)>) {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        let Ok(metadata) = std::fs::metadata(&path) else {
            continue;
        };
        if metadata.is_dir() {
            if depth > 0 {
                search_cache(&path, depth - 1, found);
            }
        } else if CACHE_NAMES.iter().any(|name| entry.file_name() == *name) && runnable(&path).is_ok() {
            found.push((metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH), path));
        }
    }
}

/// Finds the browser to use.
pub fn find(places: &Places) -> Result<PathBuf, FindError> {
    if let Some(named) = &places.named {
        let path = PathBuf::from(named);
        return match runnable(&path) {
            Ok(()) => Ok(path),
            Err(reason) => Err(FindError::Unusable { path, reason }),
        };
    }
    for folder in &places.applications {
        for name in APP_NAMES {
            let path = folder.join(format!("{name}.app")).join("Contents/MacOS").join(name);
            if runnable(&path).is_ok() {
                return Ok(path);
            }
        }
    }
    for name in PATH_NAMES {
        for folder in &places.path {
            let path = folder.join(name);
            if runnable(&path).is_ok() {
                return Ok(path);
            }
        }
    }
    let mut cached: Vec<(SystemTime, PathBuf)> = Vec::new();
    for cache in &places.caches {
        search_cache(cache, CACHE_DEPTH, &mut cached);
    }
    // The newest build; the path settles a tie so the choice is repeatable.
    cached.sort();
    cached.pop().map(|(_, path)| path).ok_or(FindError::NotFound)
}
