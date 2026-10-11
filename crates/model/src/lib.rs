//! Reads a project without a browser and resolves what it declares.
//!
//! `resolve` is shared by the host and the in-page runtime, so both sides
//! derive the manifest with the same code. The other modules are host-only:
//! they read files.
//!
//! Behaviour is specified in `docs/spec/composition.md` and
//! `docs/spec/support-matrix.md`.

pub mod css;
mod diag;
pub mod html;
pub mod image;
mod manifest;
mod project;
mod raw;
mod resolve;
pub mod script;
pub mod selector;

pub use diag::{exit_code, Diagnostic, Severity};
pub use manifest::{first_difference, Clip, Difference, Manifest, TweenFact};
pub use project::{check, classify, default_output, raw_doc, CheckReport, Reference, ENTRY_NAME, RESERVED_PREFIX};
pub use raw::{RawAttr, RawDoc, RawElement, RawMotion};
pub use resolve::{Draft, Resolved, SelectorProblem, MOTION_TYPE};
