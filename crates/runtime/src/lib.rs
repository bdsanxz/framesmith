//! The in-page runtime. The loader hands it what the page declares; it
//! resolves that with the same code the host uses on the files, and then
//! gives the state of any frame: which clips show, and every animated value.
//!
//! `Session` is plain Rust and is tested on the host. `exports` wraps it for
//! WebAssembly.

mod session;

#[cfg(target_arch = "wasm32")]
mod exports;

pub use session::Session;
