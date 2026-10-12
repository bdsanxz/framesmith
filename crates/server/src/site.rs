//! What each path answers. Nothing here writes: the project is only read. Specified in
//! `docs/spec/preview.md`, "What the server answers".

use crate::http::Response;
use crate::page;
use model::{classify, Manifest, Reference, ENTRY_NAME};
use protocol::{LOADER_PATH, RUNTIME_PATH};
use std::path::{Path, PathBuf};

const HTML: &str = "text/html; charset=utf-8";
const JAVASCRIPT: &str = "text/javascript; charset=utf-8";

fn content_type(path: &Path) -> &'static str {
    let extension = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase());
    match extension.as_deref() {
        Some("html" | "htm") => HTML,
        Some("css") => "text/css; charset=utf-8",
        Some("js" | "mjs") => JAVASCRIPT,
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

/// The manifest of the project as it is on disk now, when the project passes the check.
fn passing_manifest(root: &Path) -> Option<Manifest> {
    let report = model::check(root);
    match report.manifest {
        Some(manifest) if report.exit_code() == 0 => Some(manifest),
        _ => None,
    }
}

/// The entry page, prepared for the browser, or `None` when the project does not pass.
fn entry_page(root: &Path) -> Option<String> {
    let manifest = passing_manifest(root)?;
    let source = std::fs::read(root.join(ENTRY_NAME)).ok()?;
    Some(compile::entry_page(
        &String::from_utf8_lossy(&source),
        manifest.width,
        manifest.height,
    ))
}

/// A file of the project: only a file that lies inside the project folder, also after links are followed.
fn project_file(root: &Path, relative: &Path) -> Option<(Vec<u8>, &'static str)> {
    let full: PathBuf = root.join(relative);
    let inside = match (std::fs::canonicalize(&full), std::fs::canonicalize(root)) {
        (Ok(full), Ok(root)) => full.starts_with(root),
        _ => false,
    };
    if !inside || !full.is_file() {
        return None;
    }
    let bytes = std::fs::read(&full).ok()?;
    Some((bytes, content_type(relative)))
}

/// The answer to `GET target`, for the project folder `root`.
pub fn answer(root: &Path, target: &str) -> Response {
    let path = target.split(['?', '#']).next().unwrap_or("/");
    if path == "/" {
        return match passing_manifest(root) {
            Some(manifest) => Response::new(200, HTML, page::shell(&manifest).into_bytes()),
            None => Response::new(200, HTML, page::blocked().into_bytes()),
        };
    }
    if path == page::SCRIPT_PATH {
        return Response::new(200, JAVASCRIPT, page::SCRIPT.as_bytes().to_vec());
    }
    if path == LOADER_PATH {
        return Response::new(200, JAVASCRIPT, compile::LOADER_JS.as_bytes().to_vec());
    }
    if path == RUNTIME_PATH {
        return Response::new(200, "application/wasm", compile::RUNTIME_WASM.to_vec());
    }
    let Reference::Local(relative) = classify(path, Path::new("")) else {
        return Response::text(404, "Not found.");
    };
    // The entry page is always the prepared one, however its name is written (`/%69ndex.html` too).
    if relative == Path::new(ENTRY_NAME) {
        return match entry_page(root) {
            Some(page) => Response::new(200, HTML, page.into_bytes()),
            None => Response::text(503, "The project does not pass `framesmith check`."),
        };
    }
    match project_file(root, &relative) {
        Some((bytes, content_type)) => Response::new(200, content_type, bytes),
        None => Response::text(404, "Not found."),
    }
}
