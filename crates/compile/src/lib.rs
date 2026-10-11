//! The compile stage: prepares the entry page for the browser and carries
//! the two runtime files that are embedded in the binary.

use model::html::after_doctype;
use protocol::{LOADER_PATH, SHOWN_ATTR};

/// The in-page runtime, built from `crates/runtime` by `build.rs`.
pub const RUNTIME_WASM: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/runtime.wasm"));
/// The runtime loader.
pub const LOADER_JS: &str = include_str!("../web/loader.js");

/// The first, lowest-priority style sheet of the page: a black page, text
/// drawn from its outlines as designed, no body margin, the composition as
/// a clipping box of the declared size, and every clip hidden until the
/// runtime shows it. Anything the author writes wins, except over the
/// hiding of clips.
pub fn base_style(width: u32, height: u32) -> String {
    format!(
        ":where(html){{background:#000;text-rendering:geometricPrecision}}\
         :where(body){{margin:0}}\
         :where([data-fs-composition]){{position:relative;width:{width}px;height:{height}px;overflow:hidden}}\
         [data-fs-clip]:not([{SHOWN_ATTR}]),[data-fs-clip]:not([{SHOWN_ATTR}]) *{{visibility:hidden!important}}"
    )
}

/// The entry page as it is served: the loader script and the base style
/// sheet are put first, straight after the document type so the page keeps
/// its layout mode. No line is added, so line numbers stay as written.
pub fn entry_page(source: &str, width: u32, height: u32) -> String {
    let at = after_doctype(source);
    let added = format!(
        "<script src=\"{LOADER_PATH}\"></script><style>{}</style>",
        base_style(width, height)
    );
    let mut page = String::with_capacity(source.len() + added.len());
    page.push_str(&source[..at]);
    page.push_str(&added);
    page.push_str(&source[at..]);
    page
}
