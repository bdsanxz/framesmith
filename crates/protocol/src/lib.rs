//! The page protocol, version 1: the messages the host and the page
//! exchange. Specified in `docs/spec/page-protocol.md`; change that first.

use model::Manifest;
use serde::{Deserialize, Serialize};

/// The protocol version both sides must speak.
pub const VERSION: u32 = 1;
/// The origin the host serves the project from. Nothing listens there:
/// the host answers every request itself.
pub const ORIGIN: &str = "http://framesmith.localhost";
/// The paths the two runtime files are served from.
pub const LOADER_PATH: &str = "/__framesmith__/loader.js";
pub const RUNTIME_PATH: &str = "/__framesmith__/runtime.wasm";
/// The attribute the loader puts on a clip while it shows.
pub const SHOWN_ATTR: &str = "fs-shown";
/// How long the host waits for the page to report ready.
pub const READY_TIMEOUT_SECS: u64 = 30;
/// The name of the global the host sets before any document script runs.
pub const BOOT_GLOBAL: &str = "__framesmithBoot";
/// The name of the global the loader defines.
pub const PAGE_GLOBAL: &str = "__framesmith";

/// What the host tells the loader before the page starts.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Boot {
    pub protocol: u32,
    /// Test only: a fault the loader is asked to commit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fault: Option<String>,
}

/// A problem the page found in its own composition.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Problem {
    pub code: String,
    pub description: String,
}

/// An unsupported use seen while the page ran.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Unsupported {
    pub code: String,
    pub detail: String,
}

/// Something that stopped the page from being usable: `image`, `font` or `script`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Failure {
    pub kind: String,
    pub detail: String,
}

/// What `ready` resolves with.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadyReport {
    pub protocol: u32,
    pub clocks: bool,
    pub manifest: Option<Manifest>,
    pub problems: Vec<Problem>,
    pub unsupported: Vec<Unsupported>,
    pub failures: Vec<Failure>,
}

/// What `begin` and `seek` return when there is something new to report.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProblemReport {
    #[serde(default)]
    pub unsupported: Vec<Unsupported>,
    #[serde(default)]
    pub failures: Vec<Failure>,
}

/// What the runtime hands the loader once the page's selectors are matched.
/// Internal to the two files that are built together.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Resolution {
    pub manifest: Option<Manifest>,
    pub problems: Vec<Problem>,
    /// For each clip of the manifest, the index of its element in the snapshot.
    pub clips: Vec<usize>,
}

/// How many numbers the frame block holds for each animated element: a
/// set of flags, then the five property values.
pub const ELEMENT_NUMBERS: usize = 6;
