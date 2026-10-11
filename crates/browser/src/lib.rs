//! Finds a Chromium-based browser, starts it with a private profile, and
//! drives one page: serving its files, running expressions, taking pictures.
//!
//! The browser is spoken to over a pipe, never a network port, and it is
//! never given a network to use.

pub mod base64;
mod find;
mod process;

pub use find::{find, FindError, Places};
pub use process::{Browser, Error, Served};
