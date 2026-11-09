//! Studio catalog panel (Sprint 19, T005 / CAP-005, STATE-01).
//!
//! The panel ranks the same merged offline word index the CLI and
//! the picker use: [`catalog::SearchIndex`] built from the same
//! registry listing, so all three surfaces agree on order and
//! scores. Served as:
//!
//! * `GET /studio/catalog` — the human panel page (a search box
//!   over `fetch`, no network beyond loopback, no model runtime).
//! * `GET /api/studio/catalog/search?q=WORDS` — the ranked hits as
//!   the same JSON envelope `catalog search --json` prints.
//!
//! A search over an empty registry reports the empty registry; a
//! search with words but no hits carries the search-miss offer.

use std::path::PathBuf;

/// Registry directory the panel searches: the shared resolution
/// (explicit flag wins in the CLI; here the environment override or
/// `./registry`), so the panel and the CLI read the same index.
pub fn panel_registry_dir() -> PathBuf {
    catalog::resolve_registry_dir(None)
}

/// Answer `GET /api/studio/catalog/search`: the status plus the
/// JSON body. A missing query refuses with 400; every answered
/// search (hits, no hits, or an empty registry) is 200 carrying the
/// same envelope the CLI prints, merge warnings included.
pub fn catalog_search_response(query: Option<String>) -> (&'static str, String) {
    let dir = panel_registry_dir();
    catalog_search_response_in(query, &dir)
}

/// Custom registry sources for the panel: the project configuration
/// in the current directory names extra registries merged with the
/// bundled base, exactly like the CLI. A misshapen configuration
/// warns inside the envelope and keeps the bundled base working.
fn panel_custom_sources() -> (Vec<catalog::RegistrySource>, Vec<String>) {
    let cwd = std::path::PathBuf::from(".");
    match catalog::project_registries(&cwd) {
        Ok(customs) => (catalog::registry_sources(&cwd, &customs), Vec::new()),
        Err(error) => (
            Vec::new(),
            vec![format!(
                "project configuration names no usable custom registries: {error}"
            )],
        ),
    }
}

/// Answer the catalog search over an explicit registry directory:
/// the env-free core the server delegates to, so tests exercise
/// hits, misses, and empty registries without touching the process
/// environment. Custom registries from the current directory's
/// project configuration merge in, like the CLI.
pub fn catalog_search_response_in(
    query: Option<String>,
    dir: &std::path::Path,
) -> (&'static str, String) {
    let Some(q) = query
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
    else {
        return (
            "400 Bad Request",
            "preview refused: /api/studio/catalog/search needs `?q=WORDS` (search words); pass the words to rank".to_string(),
        );
    };
    let base = catalog::RegistrySource {
        name: catalog::BUNDLED_ORIGIN.to_string(),
        dir: dir.to_path_buf(),
        before_bundled: false,
    };
    let (extras, mut warnings) = panel_custom_sources();
    let (listing, index) = catalog::load_merged_search_index(&base, &extras);
    warnings.extend(listing.warnings.iter().cloned());
    if index.is_empty() {
        return ("200 OK", catalog::empty_json_with_warnings(dir, &warnings));
    }
    let hits = index.search(&q);
    if hits.is_empty() {
        (
            "200 OK",
            catalog::no_hits_json_with_warnings(&q, dir, &warnings),
        )
    } else {
        (
            "200 OK",
            catalog::hits_json_with_warnings(&q, dir, &hits, &warnings),
        )
    }
}

/// The human panel page: a search box that ranks through the JSON
/// endpoint above. Fully offline (no CDN, no model); install picks
/// hand to the installer slice per INT-001.
pub fn catalog_panel_html() -> String {
    let html = r#"<!doctype html>
<html lang="en">
<head><meta charset="utf-8"><title>Catalog — Framesmith Studio</title></head>
<body>
<main>
<h1>Catalog search</h1>
<p>Offline word search over the merged registry index: the same ranking the CLI picker prints.</p>
<form id="catalog-search">
<label for="q">Words</label>
<input id="q" name="q" type="text" autocomplete="off">
<button type="submit">Search</button>
</form>
<p id="catalog-status" aria-live="polite"></p>
<ol id="catalog-hits"></ol>
<p id="catalog-offer"></p>
</main>
<script>
const form = document.getElementById('catalog-search');
const status = document.getElementById('catalog-status');
const hits = document.getElementById('catalog-hits');
const offer = document.getElementById('catalog-offer');
form.addEventListener('submit', async (event) => {
  event.preventDefault();
  const q = document.getElementById('q').value;
  hits.replaceChildren();
  offer.textContent = '';
  status.textContent = 'Searching ' + JSON.stringify(q) + '…';
  const res = await fetch('/api/studio/catalog/search?q=' + encodeURIComponent(q));
  const body = await res.json();
  if (!res.ok) { status.textContent = 'Search refused: ' + JSON.stringify(body); return; }
  if (body.empty) { status.textContent = body.message; return; }
  status.textContent = body.hits.length + ' hit(s) for ' + JSON.stringify(body.query) + '.';
  for (const [i, hit] of body.hits.entries()) {
    const li = document.createElement('li');
    li.textContent = hit.name + ' (' + hit.type + ') — ' + hit.title + ' [score ' + hit.score + ']';
    hits.appendChild(li);
  }
  if (body.hits.length === 0) { offer.textContent = 'No hits. Report this miss with: ' + body.report_offer; }
});
</script>
</body>
</html>"#;
    html.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_query_refuses_naming_the_flag() {
        let (status, body) = catalog_search_response(None);
        assert_eq!(status, "400 Bad Request");
        assert!(body.contains("?q=WORDS"), "{body}");
        let (status, body) = catalog_search_response(Some("   ".to_string()));
        assert_eq!(status, "400 Bad Request");
        assert!(body.contains("?q=WORDS"), "{body}");
    }

    fn write_manifest(dir: &std::path::Path, name: &str, title: &str, description: &str) {
        let folder = dir.join(name);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(
            folder.join("manifest.json"),
            format!(
                r#"{{"name": {name:?}, "type": "block", "title": {title:?}, "description": {description:?},
                    "files": [{{"src": "composition.kdl", "target": "composition.kdl"}}],
                    "width": 1280, "height": 720, "duration": 4}}"#
            ),
        )
        .unwrap();
    }

    #[test]
    fn panel_page_is_offline_with_a_search_box() {
        let page = catalog_panel_html();
        assert!(page.contains("catalog-search"), "{page}");
        assert!(page.contains("/api/studio/catalog/search"), "{page}");
        assert!(!page.contains("http://") && !page.contains("https://"));
    }

    #[test]
    fn panel_search_ranks_hits_like_the_cli_index() {
        let dir = std::env::temp_dir().join(format!(
            "framesmith-studio-catalog-unit-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        write_manifest(
            &dir,
            "sunset-glow",
            "Harbor Evening",
            "An original amber composition over still water.",
        );
        write_manifest(
            &dir,
            "dawn-chorus",
            "Sunset Accents",
            "A short snippet with soft edges.",
        );
        let (status, body) = catalog_search_response_in(Some("sunset".to_string()), &dir);
        assert_eq!(status, "200 OK", "{body}");
        assert!(
            body.find("\"sunset-glow\"").unwrap() < body.find("\"dawn-chorus\"").unwrap(),
            "name beats title: {body}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn panel_search_reports_empty_and_misses() {
        let dir = std::env::temp_dir().join(format!(
            "framesmith-studio-catalog-empty-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let (status, body) = catalog_search_response_in(Some("sunset".to_string()), &dir);
        assert_eq!(status, "200 OK", "{body}");
        assert!(body.contains("\"empty\":true"), "{body}");
        write_manifest(&dir, "pebble-sort", "Pebble Sort", "A quiet pond scene.");
        let (status, body) = catalog_search_response_in(Some("xylophone".to_string()), &dir);
        assert_eq!(status, "200 OK", "{body}");
        assert!(body.contains("report-miss"), "{body}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
