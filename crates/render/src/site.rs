//! What the page is served: the prepared entry page, the two runtime
//! files, and the files of the project folder. Nothing else.

use browser::Served;
use model::{classify, Reference, ENTRY_NAME};
use protocol::{LOADER_PATH, ORIGIN, RUNTIME_PATH};
use std::path::{Path, PathBuf};

pub struct Site {
    root: PathBuf,
    entry_page: Vec<u8>,
}

fn content_type(path: &Path) -> &'static str {
    let extension = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase());
    match extension.as_deref() {
        Some("html" | "htm") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("json") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("avif") => "image/avif",
        Some("woff2") => "font/woff2",
        Some("woff") => "font/woff",
        Some("ttf") => "font/ttf",
        Some("otf") => "font/otf",
        Some("wasm") => "application/wasm",
        Some("txt") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

impl Site {
    pub fn new(root: &Path, entry_page: String) -> Site {
        Site {
            root: root.to_path_buf(),
            entry_page: entry_page.into_bytes(),
        }
    }

    /// The address of the entry page.
    pub fn entry_url() -> String {
        format!("{ORIGIN}/{ENTRY_NAME}")
    }

    /// Answers one request. An address outside the site's origin is refused.
    pub fn serve(&self, url: &str) -> Served {
        let Some(path) = url.strip_prefix(ORIGIN).filter(|path| path.starts_with('/')) else {
            return Served::Refused;
        };
        let path = path.split(['?', '#']).next().unwrap_or("/");
        if path == LOADER_PATH {
            return Served::File {
                bytes: compile::LOADER_JS.as_bytes().to_vec(),
                content_type: "text/javascript; charset=utf-8",
            };
        }
        if path == RUNTIME_PATH {
            return Served::File {
                bytes: compile::RUNTIME_WASM.to_vec(),
                content_type: "application/wasm",
            };
        }
        if path == "/" || path.trim_start_matches('/') == ENTRY_NAME {
            return Served::File {
                bytes: self.entry_page.clone(),
                content_type: "text/html; charset=utf-8",
            };
        }
        let Reference::Local(relative) = classify(path, Path::new("")) else {
            return Served::NotFound;
        };
        let full = self.root.join(&relative);
        // A link inside the project must not lead out of it.
        let inside = match (std::fs::canonicalize(&full), std::fs::canonicalize(&self.root)) {
            (Ok(full), Ok(root)) => full.starts_with(root),
            _ => false,
        };
        match std::fs::read(&full) {
            Ok(bytes) if inside && full.is_file() => Served::File {
                bytes,
                content_type: content_type(&relative),
            },
            _ => Served::NotFound,
        }
    }
}
