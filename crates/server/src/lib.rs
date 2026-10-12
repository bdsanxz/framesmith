//! The preview server: a small web server on the loopback address that serves a project, the runtime
//! files and the preview page. Specified in `docs/spec/preview.md`.

mod http;
mod page;
mod preview;
mod site;

pub use preview::{Preview, StartError, ADDRESS_HOST};
