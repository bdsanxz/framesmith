//! Bundled clean-room catalog covering all 22 shelves (Sprint 19, T002 / CAP-002).
//!
//! Every item below is our own original content: boxes-only KDL with
//! hyphenated style attributes, no `font-family`, and no external
//! `path`/`src` references, so the whole registry works fully offline.
//! Shelf names alone are derived from the workspace features reference
//! (shelf-level similarity only); no HyperFrames item, template, preset,
//! or shader name appears anywhere in this module.
//!
//! Shelf membership travels in each manifest's tags (the first tag is
//! always the shelf name), so [`check_listing_coverage`] can group a
//! loaded [`RegistryListing`](crate::RegistryListing) back into shelves:
//! a shelf with no items is a registry defect that names the shelf,
//! never a silent empty room.

use std::io;
use std::path::Path;

use crate::manifest::{parse_manifest_str, ItemType};
use crate::registry::RegistryListing;
use crate::ItemManifest;

/// Minimum items per shelf: "several" means at least three.
pub const MIN_ITEMS_PER_SHELF: usize = 3;

/// One catalog shelf: a category name plus its grounding in the
/// workspace features reference (section only, never item content).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shelf {
    /// Shelf key; also the item tag that assigns membership.
    pub name: &'static str,
    /// Human title shown in listings.
    pub title: &'static str,
    /// One-line blurb for the shelf.
    pub blurb: &'static str,
}

/// All 22 shelves in listing order.
pub const SHELVES: &[Shelf] = &[
    Shelf {
        name: "titles",
        title: "Titles",
        blurb: "Original title cards and plates.",
    },
    Shelf {
        name: "captions",
        title: "Captions",
        blurb: "Original caption bars and plates.",
    },
    Shelf {
        name: "transitions",
        title: "Transitions",
        blurb: "Original wipe and fade blocks.",
    },
    Shelf {
        name: "effects",
        title: "Effects",
        blurb: "Original look and treatment blocks.",
    },
    Shelf {
        name: "color-grades",
        title: "Color grades",
        blurb: "Original grade washes and tints.",
    },
    Shelf {
        name: "audio",
        title: "Audio",
        blurb: "Original rhythm guides and beat plates.",
    },
    Shelf {
        name: "backgrounds",
        title: "Backgrounds",
        blurb: "Original washes, vignettes, and fields.",
    },
    Shelf {
        name: "motion",
        title: "Motion",
        blurb: "Original drift, slide, and rise studies.",
    },
    Shelf {
        name: "slideshows",
        title: "Slideshows",
        blurb: "Original deck frames and sequences.",
    },
    Shelf {
        name: "overlays",
        title: "Overlays",
        blurb: "Original frames, keylines, and letterbox bars.",
    },
    Shelf {
        name: "text",
        title: "Text",
        blurb: "Original backing plates and bands for type.",
    },
    Shelf {
        name: "layouts",
        title: "Layouts",
        blurb: "Original splits, grids, and safe frames.",
    },
    Shelf {
        name: "storyboards",
        title: "Storyboards",
        blurb: "Original boards and panel strips.",
    },
    Shelf {
        name: "variables",
        title: "Variables",
        blurb: "Original palette sets for variable-driven work.",
    },
    Shelf {
        name: "themes",
        title: "Themes",
        blurb: "Original light, dark, warm, and cool themes.",
    },
    Shelf {
        name: "media",
        title: "Media",
        blurb: "Original media frames, mattes, and fit guides.",
    },
    Shelf {
        name: "compositions",
        title: "Compositions",
        blurb: "Original assemblies and nested-block studies.",
    },
    Shelf {
        name: "beats",
        title: "Beats",
        blurb: "Original beat bars and section markers.",
    },
    Shelf {
        name: "lyrics",
        title: "Lyrics",
        blurb: "Original lyric bars and line plates.",
    },
    Shelf {
        name: "grids",
        title: "Grids",
        blurb: "Original thirds guides and alignment frames.",
    },
    Shelf {
        name: "logos",
        title: "Logos",
        blurb: "Original logo plates, bugs, and lockups.",
    },
    Shelf {
        name: "maps",
        title: "Maps",
        blurb: "Original map frames, route bars, and pin plates.",
    },
];

/// Look up a shelf by name.
pub fn lookup_shelf(name: &str) -> Option<&'static Shelf> {
    SHELVES.iter().find(|s| s.name == name)
}

// ---------------------------------------------------------------------------
// Original KDL bodies. Boxes only, hyphenated attributes throughout, no
// `font-family`, no external references. Compositions carry the project
// name via `{name}` exactly like the CLI starters do.
// ---------------------------------------------------------------------------

// -- titles ---------------------------------------------------------------
const KDL_EMBER_TITLE: &str = "composition name=\"{name}\" width=1280 height=720 duration=4 fps=30 background=\"#14131a\" version=1 {\n    box x=240 y=220 width=800 height=220 fill=\"#f4f1de\" radius=18 border-width=6 border-color=\"#e07a5f\" start=0 duration=4\n    box x=240 y=470 width=320 height=14 fill=\"#e07a5f\" radius=7 start=0 duration=4\n    box x=240 y=500 width=180 height=14 fill=\"#3d405b\" radius=7 start=0 duration=4\n}\n";
const KDL_HEARTH_PLATE: &str = "box x=80 y=540 width=560 height=110 fill=\"#f4f1de\" radius=14 opacity=0.96 start=0 duration=4\nbox x=80 y=540 width=14 height=110 fill=\"#e07a5f\" radius=7 start=0 duration=4\n";
const KDL_KINDLING_STARTER: &str = "composition name=\"{name}\" width=1280 height=720 duration=3 fps=30 background=\"#1d1a16\" version=1 {\n    box x=390 y=240 width=500 height=180 fill=\"#f2cc8f\" radius=20 start=0 duration=3\n    box x=390 y=240 width=500 height=26 fill=\"#81b29a\" radius=13 start=0 duration=3\n    box x=430 y=446 width=220 height=12 fill=\"#e07a5f\" radius=6 start=0 duration=3\n}\n";

// -- captions -------------------------------------------------------------
const KDL_CINDER_CAPTION: &str = "composition name=\"{name}\" width=1280 height=720 duration=5 fps=30 background=\"#101014\" version=1 {\n    box x=180 y=540 width=920 height=110 fill=\"#ffffff\" radius=12 opacity=0.94 start=0 duration=5\n    box x=180 y=540 width=920 height=10 fill=\"#e63946\" radius=5 start=0 duration=5\n}\n";
const KDL_PILOT_CAPTION: &str = "box x=140 y=560 width=1000 height=90 fill=\"#16161d\" radius=12 opacity=0.92 start=0 duration=5\nbox x=140 y=560 width=10 height=90 fill=\"#f1fa8c\" radius=5 start=0 duration=5\n";
const KDL_FERRY_CAPTION: &str = "composition name=\"{name}\" width=1280 height=720 duration=4 fps=30 background=\"#0e1420\" version=1 {\n    box x=240 y=560 width=800 height=96 fill=\"#edf2f4\" radius=14 start=0 duration=4\n    box x=240 y=560 width=22 height=96 fill=\"#2a9d8f\" radius=8 start=0 duration=4\n    box x=240 y=668 width=260 height=10 fill=\"#2a9d8f\" radius=5 start=0 duration=4\n}\n";

// -- transitions ----------------------------------------------------------
const KDL_DUSK_WIPE: &str = "composition name=\"{name}\" width=1280 height=720 duration=3 fps=30 background=\"#0b0b10\" version=1 {\n    box x=0 y=0 width=640 height=720 fill=\"#e63946\" start=0 duration=3\n    box x=640 y=0 width=640 height=720 fill=\"#1d3557\" start=0 duration=3\n    box x=628 y=0 width=24 height=720 fill=\"#f1fa8c\" start=0 duration=3\n}\n";
const KDL_SOFT_CROSSFADE: &str =
    "box x=0 y=0 width=1280 height=720 fill=\"#f1fa8c\" opacity=0.35 start=0 duration=2\n";
const KDL_TIDE_WIPE: &str = "composition name=\"{name}\" width=1280 height=720 duration=3 fps=30 background=\"#12212b\" version=1 {\n    box x=0 y=0 width=1280 height=360 fill=\"#457b9d\" start=0 duration=3\n    box x=0 y=360 width=1280 height=360 fill=\"#a8dadc\" start=0 duration=3\n    box x=0 y=348 width=1280 height=24 fill=\"#f1fa8c\" start=0 duration=3\n}\n";

// -- effects --------------------------------------------------------------
const KDL_GRAIN_WASH: &str = "composition name=\"{name}\" width=1280 height=720 duration=4 fps=30 background=\"#191923\" version=1 {\n    box x=0 y=0 width=1280 height=720 fill=\"#2b2d42\" opacity=0.55 start=0 duration=4\n    box x=60 y=60 width=1160 height=600 fill=\"#8d99ae\" opacity=0.25 radius=24 start=0 duration=4\n}\n";
const KDL_SCANLINE_BAND: &str = "box x=0 y=100 width=1280 height=8 fill=\"#0b0b10\" opacity=0.6 start=0 duration=4\nbox x=0 y=220 width=1280 height=8 fill=\"#0b0b10\" opacity=0.6 start=0 duration=4\nbox x=0 y=340 width=1280 height=8 fill=\"#0b0b10\" opacity=0.6 start=0 duration=4\n";
const KDL_HALFTONE_STARTER: &str = "composition name=\"{name}\" width=1280 height=720 duration=3 fps=30 background=\"#f4f1de\" version=1 {\n    box x=180 y=140 width=120 height=120 fill=\"#3d405b\" radius=60 start=0 duration=3\n    box x=360 y=140 width=120 height=120 fill=\"#3d405b\" radius=60 start=0 duration=3\n    box x=540 y=140 width=120 height=120 fill=\"#3d405b\" radius=60 start=0 duration=3\n    box x=180 y=320 width=480 height=240 fill=\"#e07a5f\" radius=18 start=0 duration=3\n}\n";

// -- color-grades ---------------------------------------------------------
const KDL_AMBER_GRADE: &str = "composition name=\"{name}\" width=1280 height=720 duration=4 fps=30 background=\"#201812\" version=1 {\n    box x=0 y=0 width=1280 height=720 fill=\"#e9c46a\" opacity=0.28 start=0 duration=4\n    box x=0 y=560 width=1280 height=160 fill=\"#9c6644\" opacity=0.45 start=0 duration=4\n}\n";
const KDL_TEAL_TINT: &str =
    "box x=0 y=0 width=1280 height=720 fill=\"#2a9d8f\" opacity=0.22 start=0 duration=4\n";
const KDL_SLATE_GRADE: &str = "composition name=\"{name}\" width=1280 height=720 duration=4 fps=30 background=\"#1a1d24\" version=1 {\n    box x=0 y=0 width=1280 height=720 fill=\"#90a8c3\" opacity=0.2 start=0 duration=4\n    box x=120 y=120 width=1040 height=480 fill=\"#0f1115\" opacity=0.35 radius=24 start=0 duration=4\n}\n";

// -- audio ----------------------------------------------------------------
const KDL_PULSE_GUIDE: &str = "composition name=\"{name}\" width=1280 height=720 duration=4 fps=30 background=\"#0d0d12\" version=1 {\n    box x=140 y=320 width=120 height=80 fill=\"#e63946\" radius=10 start=0 duration=1\n    box x=300 y=280 width=120 height=160 fill=\"#e63946\" radius=10 start=1 duration=1\n    box x=460 y=320 width=120 height=80 fill=\"#e63946\" radius=10 start=2 duration=1\n    box x=620 y=240 width=120 height=240 fill=\"#e63946\" radius=10 start=3 duration=1\n}\n";
const KDL_BEAT_TICK: &str =
    "box x=620 y=60 width=40 height=40 fill=\"#f1fa8c\" radius=20 start=0 duration=4\n";
const KDL_METRONOME_STARTER: &str = "composition name=\"{name}\" width=1280 height=720 duration=4 fps=30 background=\"#101018\" version=1 {\n    box x=600 y=180 width=80 height=360 fill=\"#f4f1de\" radius=16 start=0 duration=4\n    box x=600 y=180 width=80 height=40 fill=\"#e07a5f\" radius=12 start=0 duration=4\n    box x=200 y=600 width=880 height=12 fill=\"#3d405b\" radius=6 start=0 duration=4\n}\n";

// -- backgrounds ----------------------------------------------------------
const KDL_MOSS_FIELD: &str = "composition name=\"{name}\" width=1280 height=720 duration=5 fps=30 background=\"#283618\" version=1 {\n    box x=0 y=480 width=1280 height=240 fill=\"#606c38\" start=0 duration=5\n    box x=0 y=0 width=1280 height=120 fill=\"#dda15e\" opacity=0.5 start=0 duration=5\n}\n";
const KDL_VIGNETTE_SHADE: &str = "box x=0 y=0 width=1280 height=90 fill=\"#0b0b10\" opacity=0.55 start=0 duration=5\nbox x=0 y=630 width=1280 height=90 fill=\"#0b0b10\" opacity=0.55 start=0 duration=5\n";
const KDL_PEBBLE_STARTER: &str = "composition name=\"{name}\" width=1280 height=720 duration=3 fps=30 background=\"#edede9\" version=1 {\n    box x=120 y=140 width=420 height=440 fill=\"#d6ccc2\" radius=28 start=0 duration=3\n    box x=600 y=140 width=560 height=200 fill=\"#e3d5ca\" radius=28 start=0 duration=3\n    box x=600 y=380 width=560 height=200 fill=\"#f5ebe0\" radius=28 start=0 duration=3\n}\n";

// -- motion ---------------------------------------------------------------
const KDL_DRIFT_STUDY: &str = "composition name=\"{name}\" width=1280 height=720 duration=4 fps=30 background=\"#0f1622\" version=1 {\n    box x=120 y=300 width=200 height=120 fill=\"#457b9d\" radius=16 start=0 duration=2\n    box x=540 y=300 width=200 height=120 fill=\"#a8dadc\" radius=16 start=1 duration=2\n    box x=960 y=300 width=200 height=120 fill=\"#f1fa8c\" radius=16 start=2 duration=2\n}\n";
const KDL_RISE_CHIP: &str = "box x=540 y=520 width=200 height=80 fill=\"#e9c46a\" radius=14 start=0 duration=3\nbox x=540 y=400 width=200 height=80 fill=\"#e9c46a\" radius=14 opacity=0.5 start=0 duration=3\n";
const KDL_SLIDE_STARTER: &str = "composition name=\"{name}\" width=1280 height=720 duration=3 fps=30 background=\"#14141c\" version=1 {\n    box x=80 y=260 width=480 height=200 fill=\"#e63946\" radius=18 start=0 duration=3\n    box x=720 y=260 width=480 height=200 fill=\"#f4f1de\" radius=18 start=0 duration=3\n    box x=80 y=490 width=1120 height=14 fill=\"#3d405b\" radius=7 start=0 duration=3\n}\n";

// -- slideshows -----------------------------------------------------------
const KDL_DECK_FRAME: &str = "composition name=\"{name}\" width=1280 height=720 duration=6 fps=30 background=\"#f8f9fa\" version=1 {\n    box x=90 y=90 width=1100 height=80 fill=\"#212529\" radius=14 start=0 duration=6\n    box x=90 y=210 width=680 height=420 fill=\"#dee2e6\" radius=18 start=0 duration=6\n    box x=810 y=210 width=380 height=420 fill=\"#adb5bd\" radius=18 start=0 duration=6\n}\n";
const KDL_SLIDE_NUMBER: &str = "box x=1150 y=650 width=70 height=40 fill=\"#212529\" radius=10 opacity=0.8 start=0 duration=6\n";
const KDL_TALK_STARTER: &str = "composition name=\"{name}\" width=1280 height=720 duration=4 fps=30 background=\"#1b263b\" version=1 {\n    box x=140 y=120 width=1000 height=140 fill=\"#e0e1dd\" radius=16 start=0 duration=4\n    box x=140 y=300 width=1000 height=60 fill=\"#778da9\" radius=12 start=0 duration=4\n    box x=140 y=390 width=640 height=60 fill=\"#778da9\" radius=12 start=0 duration=4\n    box x=140 y=560 width=240 height=16 fill=\"#e0e1dd\" radius=8 start=0 duration=4\n}\n";

// -- overlays -------------------------------------------------------------
const KDL_LETTERBOX: &str = "composition name=\"{name}\" width=1280 height=720 duration=5 fps=30 background=\"#22223b\" version=1 {\n    box x=240 y=140 width=800 height=440 fill=\"#9a8c98\" radius=12 start=0 duration=5\n    box x=0 y=0 width=1280 height=90 fill=\"#0b0b10\" start=0 duration=5\n    box x=0 y=630 width=1280 height=90 fill=\"#0b0b10\" start=0 duration=5\n}\n";
const KDL_KEYLINE: &str = "box x=40 y=40 width=1200 height=8 fill=\"#f1fa8c\" start=0 duration=5\nbox x=40 y=672 width=1200 height=8 fill=\"#f1fa8c\" start=0 duration=5\nbox x=40 y=40 width=8 height=640 fill=\"#f1fa8c\" start=0 duration=5\nbox x=1232 y=40 width=8 height=640 fill=\"#f1fa8c\" start=0 duration=5\n";
const KDL_CORNER_STARTER: &str = "composition name=\"{name}\" width=1280 height=720 duration=3 fps=30 background=\"#100f14\" version=1 {\n    box x=60 y=60 width=160 height=26 fill=\"#e07a5f\" radius=8 start=0 duration=3\n    box x=60 y=634 width=160 height=26 fill=\"#e07a5f\" radius=8 start=0 duration=3\n    box x=1060 y=60 width=160 height=26 fill=\"#e07a5f\" radius=8 start=0 duration=3\n    box x=1060 y=634 width=160 height=26 fill=\"#e07a5f\" radius=8 start=0 duration=3\n}\n";

// -- text -----------------------------------------------------------------
const KDL_READING_PLATE: &str = "composition name=\"{name}\" width=1280 height=720 duration=5 fps=30 background=\"#14213d\" version=1 {\n    box x=190 y=180 width=900 height=360 fill=\"#fdf0d5\" radius=20 start=0 duration=5\n    box x=240 y=240 width=800 height=34 fill=\"#c9ada7\" radius=10 start=0 duration=5\n    box x=240 y=300 width=640 height=34 fill=\"#c9ada7\" radius=10 start=0 duration=5\n    box x=240 y=360 width=720 height=34 fill=\"#c9ada7\" radius=10 start=0 duration=5\n}\n";
const KDL_HIGHLIGHT_BAND: &str = "box x=120 y=320 width=700 height=64 fill=\"#f1fa8c\" radius=14 opacity=0.85 start=0 duration=5\n";
const KDL_PULLQUOTE_STARTER: &str = "composition name=\"{name}\" width=1280 height=720 duration=4 fps=30 background=\"#212529\" version=1 {\n    box x=180 y=200 width=60 height=320 fill=\"#e9c46a\" radius=12 start=0 duration=4\n    box x=280 y=200 width=820 height=140 fill=\"#f8f9fa\" radius=16 start=0 duration=4\n    box x=280 y=370 width=560 height=60 fill=\"#adb5bd\" radius=12 start=0 duration=4\n}\n";

// -- layouts --------------------------------------------------------------
const KDL_DUET_SPLIT: &str = "composition name=\"{name}\" width=1280 height=720 duration=4 fps=30 background=\"#0b0b10\" version=1 {\n    box x=40 y=40 width=590 height=640 fill=\"#457b9d\" radius=16 start=0 duration=4\n    box x=650 y=40 width=590 height=640 fill=\"#e63946\" radius=16 start=0 duration=4\n}\n";
const KDL_SAFE_FRAME: &str = "box x=128 y=72 width=1024 height=6 fill=\"#f1fa8c\" start=0 duration=4\nbox x=128 y=642 width=1024 height=6 fill=\"#f1fa8c\" start=0 duration=4\nbox x=128 y=72 width=6 height=576 fill=\"#f1fa8c\" start=0 duration=4\nbox x=1146 y=72 width=6 height=576 fill=\"#f1fa8c\" start=0 duration=4\n";
const KDL_QUARTET_STARTER: &str = "composition name=\"{name}\" width=1280 height=720 duration=3 fps=30 background=\"#101018\" version=1 {\n    box x=40 y=40 width=590 height=310 fill=\"#e9c46a\" radius=16 start=0 duration=3\n    box x=650 y=40 width=590 height=310 fill=\"#2a9d8f\" radius=16 start=0 duration=3\n    box x=40 y=370 width=590 height=310 fill=\"#e76f51\" radius=16 start=0 duration=3\n    box x=650 y=370 width=590 height=310 fill=\"#264653\" radius=16 start=0 duration=3\n}\n";

// -- storyboards ----------------------------------------------------------
const KDL_BOARD_SIX: &str = "composition name=\"{name}\" width=1280 height=720 duration=6 fps=30 background=\"#f4f1de\" version=1 {\n    box x=60 y=60 width=360 height=280 fill=\"#3d405b\" radius=14 start=0 duration=6\n    box x=460 y=60 width=360 height=280 fill=\"#81b29a\" radius=14 start=0 duration=6\n    box x=860 y=60 width=360 height=280 fill=\"#e07a5f\" radius=14 start=0 duration=6\n    box x=60 y=380 width=360 height=280 fill=\"#f2cc8f\" radius=14 start=0 duration=6\n    box x=460 y=380 width=360 height=280 fill=\"#457b9d\" radius=14 start=0 duration=6\n    box x=860 y=380 width=360 height=280 fill=\"#e9c46a\" radius=14 start=0 duration=6\n}\n";
const KDL_PANEL_STRIP: &str = "box x=60 y=220 width=340 height=280 fill=\"#3d405b\" radius=14 start=0 duration=6\nbox x=470 y=220 width=340 height=280 fill=\"#81b29a\" radius=14 start=0 duration=6\nbox x=880 y=220 width=340 height=280 fill=\"#e07a5f\" radius=14 start=0 duration=6\n";
const KDL_APPROVAL_STARTER: &str = "composition name=\"{name}\" width=1280 height=720 duration=4 fps=30 background=\"#edf2f4\" version=1 {\n    box x=140 y=120 width=460 height=480 fill=\"#2b2d42\" radius=18 start=0 duration=4\n    box x=660 y=120 width=460 height=200 fill=\"#8d99ae\" radius=18 start=0 duration=4\n    box x=660 y=360 width=460 height=120 fill=\"#2a9d8f\" radius=18 start=0 duration=4\n    box x=660 y=520 width=220 height=80 fill=\"#e63946\" radius=14 start=0 duration=4\n}\n";

// -- variables ------------------------------------------------------------
const KDL_PALETTE_TRIO: &str = "composition name=\"{name}\" width=1280 height=720 duration=4 fps=30 background=\"#f8f9fa\" version=1 {\n    box x=90 y=170 width=320 height=380 fill=\"#e63946\" radius=18 start=0 duration=4\n    box x=480 y=170 width=320 height=380 fill=\"#2a9d8f\" radius=18 start=0 duration=4\n    box x=870 y=170 width=320 height=380 fill=\"#1d3557\" radius=18 start=0 duration=4\n}\n";
const KDL_SWATCH_ROW: &str = "box x=140 y=300 width=180 height=120 fill=\"#e9c46a\" radius=12 start=0 duration=4\nbox x=360 y=300 width=180 height=120 fill=\"#e76f51\" radius=12 start=0 duration=4\nbox x=580 y=300 width=180 height=120 fill=\"#2a9d8f\" radius=12 start=0 duration=4\nbox x=800 y=300 width=180 height=120 fill=\"#264653\" radius=12 start=0 duration=4\n";
const KDL_TOKEN_STARTER: &str = "composition name=\"{name}\" width=1280 height=720 duration=3 fps=30 background=\"#100f14\" version=1 {\n    box x=240 y=200 width=800 height=120 fill=\"#f4f1de\" radius=16 start=0 duration=3\n    box x=240 y=360 width=800 height=120 fill=\"#e07a5f\" radius=16 start=0 duration=3\n    box x=240 y=520 width=800 height=40 fill=\"#3d405b\" radius=12 start=0 duration=3\n}\n";

// -- themes ---------------------------------------------------------------
const KDL_DAYBREAK: &str = "composition name=\"{name}\" width=1280 height=720 duration=4 fps=30 background=\"#faf9f6\" version=1 {\n    box x=140 y=160 width=1000 height=240 fill=\"#ffffff\" radius=20 border-width=4 border-color=\"#dee2e6\" start=0 duration=4\n    box x=140 y=440 width=480 height=140 fill=\"#212529\" radius=16 start=0 duration=4\n    box x=660 y=440 width=480 height=140 fill=\"#e9c46a\" radius=16 start=0 duration=4\n}\n";
const KDL_NIGHTFALL_CHIP: &str = "box x=140 y=260 width=1000 height=200 fill=\"#16161d\" radius=18 border-width=4 border-color=\"#3d405b\" start=0 duration=4\n";
const KDL_HEARTH_STARTER: &str = "composition name=\"{name}\" width=1280 height=720 duration=3 fps=30 background=\"#2b2118\" version=1 {\n    box x=240 y=170 width=800 height=380 fill=\"#f2cc8f\" radius=22 start=0 duration=3\n    box x=240 y=170 width=800 height=60 fill=\"#81b29a\" radius=18 start=0 duration=3\n    box x=300 y=580 width=300 height=18 fill=\"#e07a5f\" radius=9 start=0 duration=3\n}\n";

// -- media ----------------------------------------------------------------
const KDL_PILLAR_MATTE: &str = "composition name=\"{name}\" width=1280 height=720 duration=4 fps=30 background=\"#0b0b10\" version=1 {\n    box x=400 y=0 width=480 height=720 fill=\"#343a40\" start=0 duration=4\n    box x=400 y=0 width=480 height=24 fill=\"#f1fa8c\" start=0 duration=4\n    box x=400 y=696 width=480 height=24 fill=\"#f1fa8c\" start=0 duration=4\n}\n";
const KDL_SQUARE_MATTE: &str =
    "box x=280 y=0 width=720 height=720 fill=\"#495057\" start=0 duration=4\n";
const KDL_WIDESCREEN_STARTER: &str = "composition name=\"{name}\" width=1280 height=720 duration=3 fps=30 background=\"#101018\" version=1 {\n    box x=0 y=140 width=1280 height=440 fill=\"#264653\" radius=4 start=0 duration=3\n    box x=80 y=200 width=400 height=320 fill=\"#e9c46a\" radius=14 start=0 duration=3\n    box x=520 y=200 width=680 height=120 fill=\"#f4f1de\" radius=14 start=0 duration=3\n}\n";

// -- compositions ---------------------------------------------------------
const KDL_TRIPTYCH: &str = "composition name=\"{name}\" width=1280 height=720 duration=5 fps=30 background=\"#14141c\" version=1 {\n    box x=60 y=90 width=360 height=540 fill=\"#e07a5f\" radius=16 start=0 duration=5\n    box x=460 y=90 width=360 height=540 fill=\"#f4f1de\" radius=16 start=0 duration=5\n    box x=860 y=90 width=360 height=540 fill=\"#3d405b\" radius=16 start=0 duration=5\n}\n";
const KDL_INSET_PIP: &str = "box x=880 y=440 width=320 height=200 fill=\"#f1fa8c\" radius=14 border-width=5 border-color=\"#0b0b10\" start=0 duration=5\n";
const KDL_ASSEMBLY_STARTER: &str = "composition name=\"{name}\" width=1280 height=720 duration=4 fps=30 background=\"#1d2125\" version=1 {\n    box x=80 y=80 width=1120 height=120 fill=\"#e0e1dd\" radius=14 start=0 duration=4\n    box x=80 y=240 width=540 height=400 fill=\"#778da9\" radius=16 start=0 duration=4\n    box x=660 y=240 width=540 height=400 fill=\"#415a77\" radius=16 start=0 duration=4\n}\n";

// -- beats ----------------------------------------------------------------
const KDL_DOWNBEAT: &str = "composition name=\"{name}\" width=1280 height=720 duration=4 fps=30 background=\"#0d0d12\" version=1 {\n    box x=140 y=300 width=200 height=120 fill=\"#e63946\" radius=12 start=0 duration=1\n    box x=380 y=330 width=140 height=60 fill=\"#6c757d\" radius=12 start=1 duration=1\n    box x=560 y=330 width=140 height=60 fill=\"#6c757d\" radius=12 start=2 duration=1\n    box x=740 y=330 width=140 height=60 fill=\"#6c757d\" radius=12 start=3 duration=1\n}\n";
const KDL_SECTION_MARK: &str = "box x=0 y=120 width=18 height=480 fill=\"#f1fa8c\" start=0 duration=4\nbox x=0 y=120 width=120 height=18 fill=\"#f1fa8c\" start=0 duration=4\n";
const KDL_COUNT_IN_STARTER: &str = "composition name=\"{name}\" width=1280 height=720 duration=4 fps=30 background=\"#101018\" version=1 {\n    box x=560 y=240 width=160 height=160 fill=\"#e63946\" radius=80 start=0 duration=1\n    box x=560 y=240 width=160 height=160 fill=\"#e9c46a\" radius=80 start=1 duration=1\n    box x=560 y=240 width=160 height=160 fill=\"#2a9d8f\" radius=80 start=2 duration=1\n    box x=560 y=240 width=160 height=160 fill=\"#f4f1de\" radius=80 start=3 duration=1\n}\n";

// -- lyrics ---------------------------------------------------------------
const KDL_LINE_PLATE: &str = "composition name=\"{name}\" width=1280 height=720 duration=4 fps=30 background=\"#0f0f16\" version=1 {\n    box x=240 y=260 width=800 height=90 fill=\"#f4f1de\" radius=14 start=0 duration=2\n    box x=240 y=370 width=640 height=90 fill=\"#f4f1de\" radius=14 opacity=0.75 start=2 duration=2\n}\n";
const KDL_LYRIC_UNDERLINE: &str = "box x=240 y=420 width=560 height=12 fill=\"#e63946\" radius=6 start=0 duration=4\nbox x=240 y=450 width=360 height=12 fill=\"#e63946\" radius=6 opacity=0.6 start=0 duration=4\n";
const KDL_VERSE_STARTER: &str = "composition name=\"{name}\" width=1280 height=720 duration=5 fps=30 background=\"#16121c\" version=1 {\n    box x=240 y=180 width=800 height=110 fill=\"#e9c46a\" radius=14 start=0 duration=5\n    box x=240 y=320 width=800 height=110 fill=\"#e9c46a\" radius=14 opacity=0.8 start=0 duration=5\n    box x=240 y=460 width=560 height=110 fill=\"#e9c46a\" radius=14 opacity=0.6 start=0 duration=5\n}\n";

// -- grids ----------------------------------------------------------------
const KDL_THIRDS_GUIDE: &str = "composition name=\"{name}\" width=1280 height=720 duration=4 fps=30 background=\"#101018\" version=1 {\n    box x=0 y=236 width=1280 height=6 fill=\"#f1fa8c\" opacity=0.8 start=0 duration=4\n    box x=0 y=478 width=1280 height=6 fill=\"#f1fa8c\" opacity=0.8 start=0 duration=4\n    box x=422 y=0 width=6 height=720 fill=\"#f1fa8c\" opacity=0.8 start=0 duration=4\n    box x=852 y=0 width=6 height=720 fill=\"#f1fa8c\" opacity=0.8 start=0 duration=4\n}\n";
const KDL_CENTER_CROSS: &str = "box x=0 y=357 width=1280 height=6 fill=\"#f1fa8c\" opacity=0.8 start=0 duration=4\nbox x=637 y=0 width=6 height=720 fill=\"#f1fa8c\" opacity=0.8 start=0 duration=4\n";
const KDL_MARGIN_STARTER: &str = "composition name=\"{name}\" width=1280 height=720 duration=3 fps=30 background=\"#14141c\" version=1 {\n    box x=64 y=36 width=1152 height=648 fill=\"#f4f1de\" radius=10 start=0 duration=3\n    box x=128 y=72 width=1024 height=576 fill=\"#3d405b\" radius=10 start=0 duration=3\n    box x=200 y=300 width=880 height=120 fill=\"#e07a5f\" radius=14 start=0 duration=3\n}\n";

// -- logos ----------------------------------------------------------------
const KDL_BUG_PLATE: &str = "composition name=\"{name}\" width=1280 height=720 duration=5 fps=30 background=\"#0b0b10\" version=1 {\n    box x=60 y=60 width=220 height=120 fill=\"#f4f1de\" radius=16 start=0 duration=5\n    box x=100 y=100 width=140 height=40 fill=\"#e63946\" radius=10 start=0 duration=5\n}\n";
const KDL_LOCKUP_STRIP: &str = "box x=80 y=540 width=180 height=110 fill=\"#f4f1de\" radius=14 start=0 duration=5\nbox x=280 y=565 width=320 height=28 fill=\"#f4f1de\" radius=10 opacity=0.85 start=0 duration=5\n";
const KDL_IDENT_STARTER: &str = "composition name=\"{name}\" width=1280 height=720 duration=3 fps=30 background=\"#f8f9fa\" version=1 {\n    box x=440 y=200 width=400 height=220 fill=\"#212529\" radius=24 start=0 duration=3\n    box x=500 y=260 width=280 height=44 fill=\"#e9c46a\" radius=12 start=0 duration=3\n    box x=500 y=330 width=180 height=24 fill=\"#adb5bd\" radius=8 start=0 duration=3\n}\n";

// -- maps -----------------------------------------------------------------
const KDL_ROUTE_BAR: &str = "composition name=\"{name}\" width=1280 height=720 duration=5 fps=30 background=\"#eef4ef\" version=1 {\n    box x=140 y=330 width=1000 height=60 fill=\"#b7c9b7\" radius=30 start=0 duration=5\n    box x=140 y=330 width=340 height=60 fill=\"#2d6a4f\" radius=30 start=0 duration=5\n    box x=440 y=290 width=60 height=60 fill=\"#d90429\" radius=30 start=0 duration=5\n    box x=1080 y=290 width=60 height=60 fill=\"#2d6a4f\" radius=30 start=0 duration=5\n}\n";
const KDL_PIN_PLATE: &str = "box x=600 y=180 width=80 height=80 fill=\"#d90429\" radius=40 start=0 duration=5\nbox x=628 y=260 width=24 height=60 fill=\"#d90429\" start=0 duration=5\n";
const KDL_JOURNEY_STARTER: &str = "composition name=\"{name}\" width=1280 height=720 duration=4 fps=30 background=\"#e9f0e9\" version=1 {\n    box x=90 y=90 width=1100 height=540 fill=\"#cfe0cf\" radius=20 start=0 duration=4\n    box x=150 y=360 width=980 height=40 fill=\"#2d6a4f\" radius=20 start=0 duration=4\n    box x=150 y=320 width=56 height=56 fill=\"#d90429\" radius=28 start=0 duration=4\n    box x=1074 y=320 width=56 height=56 fill=\"#2d6a4f\" radius=28 start=0 duration=4\n}\n";

// ---------------------------------------------------------------------------
// Bundled items: three originals per shelf (block, component, example).
// The first tag is always the shelf name so listings group back by shelf.
// ---------------------------------------------------------------------------

/// Every bundled item, in shelf order.
pub static BUNDLED: &[BundledItem] = &[
    BundledItem {
        shelf: "titles",
        name: "ember-title-card",
        item_type: ItemType::Block,
        title: "Ember Title Card",
        description: "An original warm title card with an ember bar and a quiet plate.",
        files: &[BundledFile {
            src: "composition.kdl",
            target: "composition.kdl",
            contents: KDL_EMBER_TITLE,
        }],
        width: Some(1280),
        height: Some(720),
        duration: Some(4.0),
        tags: &["titles", "card"],
    },
    BundledItem {
        shelf: "titles",
        name: "hearth-title-plate",
        item_type: ItemType::Component,
        title: "Hearth Title Plate",
        description: "An original paste-in title plate with an ember keyline.",
        files: &[BundledFile {
            src: "snippet.kdl",
            target: "snippet.kdl",
            contents: KDL_HEARTH_PLATE,
        }],
        width: None,
        height: None,
        duration: None,
        tags: &["titles", "plate"],
    },
    BundledItem {
        shelf: "titles",
        name: "kindling-title-starter",
        item_type: ItemType::Example,
        title: "Kindling Title Starter",
        description: "An original starter project opening on a warm title plate.",
        files: &[BundledFile {
            src: "composition.kdl",
            target: "composition.kdl",
            contents: KDL_KINDLING_STARTER,
        }],
        width: Some(1280),
        height: Some(720),
        duration: Some(3.0),
        tags: &["titles", "starter"],
    },
    BundledItem {
        shelf: "captions",
        name: "cinder-caption",
        item_type: ItemType::Block,
        title: "Cinder Caption",
        description: "An original caption block with a white plate and a red rule.",
        files: &[BundledFile {
            src: "composition.kdl",
            target: "composition.kdl",
            contents: KDL_CINDER_CAPTION,
        }],
        width: Some(1280),
        height: Some(720),
        duration: Some(5.0),
        tags: &["captions", "plate"],
    },
    BundledItem {
        shelf: "captions",
        name: "pilot-caption-chip",
        item_type: ItemType::Component,
        title: "Pilot Caption Chip",
        description: "An original dark caption chip with a light keyline.",
        files: &[BundledFile {
            src: "snippet.kdl",
            target: "snippet.kdl",
            contents: KDL_PILOT_CAPTION,
        }],
        width: None,
        height: None,
        duration: None,
        tags: &["captions", "chip"],
    },
    BundledItem {
        shelf: "captions",
        name: "ferry-caption-starter",
        item_type: ItemType::Example,
        title: "Ferry Caption Starter",
        description: "An original starter project with a teal-ruled caption plate.",
        files: &[BundledFile {
            src: "composition.kdl",
            target: "composition.kdl",
            contents: KDL_FERRY_CAPTION,
        }],
        width: Some(1280),
        height: Some(720),
        duration: Some(4.0),
        tags: &["captions", "starter"],
    },
    BundledItem {
        shelf: "transitions",
        name: "dusk-wipe",
        item_type: ItemType::Block,
        title: "Dusk Wipe",
        description: "An original two-field wipe split by a light seam.",
        files: &[BundledFile {
            src: "composition.kdl",
            target: "composition.kdl",
            contents: KDL_DUSK_WIPE,
        }],
        width: Some(1280),
        height: Some(720),
        duration: Some(3.0),
        tags: &["transitions", "wipe"],
    },
    BundledItem {
        shelf: "transitions",
        name: "soft-crossfade-veil",
        item_type: ItemType::Component,
        title: "Soft Crossfade Veil",
        description: "An original translucent veil for pasting between scenes.",
        files: &[BundledFile {
            src: "snippet.kdl",
            target: "snippet.kdl",
            contents: KDL_SOFT_CROSSFADE,
        }],
        width: None,
        height: None,
        duration: None,
        tags: &["transitions", "fade"],
    },
    BundledItem {
        shelf: "transitions",
        name: "tide-wipe-starter",
        item_type: ItemType::Example,
        title: "Tide Wipe Starter",
        description: "An original starter project with a horizontal tide wipe.",
        files: &[BundledFile {
            src: "composition.kdl",
            target: "composition.kdl",
            contents: KDL_TIDE_WIPE,
        }],
        width: Some(1280),
        height: Some(720),
        duration: Some(3.0),
        tags: &["transitions", "starter"],
    },
    BundledItem {
        shelf: "effects",
        name: "grain-wash",
        item_type: ItemType::Block,
        title: "Grain Wash",
        description: "An original dusky treatment wash with a soft inner field.",
        files: &[BundledFile {
            src: "composition.kdl",
            target: "composition.kdl",
            contents: KDL_GRAIN_WASH,
        }],
        width: Some(1280),
        height: Some(720),
        duration: Some(4.0),
        tags: &["effects", "treatment"],
    },
    BundledItem {
        shelf: "effects",
        name: "scanline-band",
        item_type: ItemType::Component,
        title: "Scanline Band",
        description: "An original trio of dark scan bands for a retro pass.",
        files: &[BundledFile {
            src: "snippet.kdl",
            target: "snippet.kdl",
            contents: KDL_SCANLINE_BAND,
        }],
        width: None,
        height: None,
        duration: None,
        tags: &["effects", "retro"],
    },
    BundledItem {
        shelf: "effects",
        name: "halftone-starter",
        item_type: ItemType::Example,
        title: "Halftone Starter",
        description: "An original starter project with a dotted print-style plate.",
        files: &[BundledFile {
            src: "composition.kdl",
            target: "composition.kdl",
            contents: KDL_HALFTONE_STARTER,
        }],
        width: Some(1280),
        height: Some(720),
        duration: Some(3.0),
        tags: &["effects", "starter"],
    },
    BundledItem {
        shelf: "color-grades",
        name: "amber-grade",
        item_type: ItemType::Block,
        title: "Amber Grade",
        description: "An original warm grade wash with a deep ground band.",
        files: &[BundledFile {
            src: "composition.kdl",
            target: "composition.kdl",
            contents: KDL_AMBER_GRADE,
        }],
        width: Some(1280),
        height: Some(720),
        duration: Some(4.0),
        tags: &["color-grades", "warm"],
    },
    BundledItem {
        shelf: "color-grades",
        name: "teal-tint",
        item_type: ItemType::Component,
        title: "Teal Tint",
        description: "An original cool teal tint for pasting over a scene.",
        files: &[BundledFile {
            src: "snippet.kdl",
            target: "snippet.kdl",
            contents: KDL_TEAL_TINT,
        }],
        width: None,
        height: None,
        duration: None,
        tags: &["color-grades", "cool"],
    },
    BundledItem {
        shelf: "color-grades",
        name: "slate-grade-starter",
        item_type: ItemType::Example,
        title: "Slate Grade Starter",
        description: "An original starter project with a slate grade and dark core.",
        files: &[BundledFile {
            src: "composition.kdl",
            target: "composition.kdl",
            contents: KDL_SLATE_GRADE,
        }],
        width: Some(1280),
        height: Some(720),
        duration: Some(4.0),
        tags: &["color-grades", "starter"],
    },
    BundledItem {
        shelf: "audio",
        name: "pulse-guide",
        item_type: ItemType::Block,
        title: "Pulse Guide",
        description: "An original four-beat rhythm guide with a rising final bar.",
        files: &[BundledFile {
            src: "composition.kdl",
            target: "composition.kdl",
            contents: KDL_PULSE_GUIDE,
        }],
        width: Some(1280),
        height: Some(720),
        duration: Some(4.0),
        tags: &["audio", "rhythm"],
    },
    BundledItem {
        shelf: "audio",
        name: "beat-tick",
        item_type: ItemType::Component,
        title: "Beat Tick",
        description: "An original paste-in beat dot for marking a pulse.",
        files: &[BundledFile {
            src: "snippet.kdl",
            target: "snippet.kdl",
            contents: KDL_BEAT_TICK,
        }],
        width: None,
        height: None,
        duration: None,
        tags: &["audio", "tick"],
    },
    BundledItem {
        shelf: "audio",
        name: "metronome-starter",
        item_type: ItemType::Example,
        title: "Metronome Starter",
        description: "An original starter project with a simple metronome plate.",
        files: &[BundledFile {
            src: "composition.kdl",
            target: "composition.kdl",
            contents: KDL_METRONOME_STARTER,
        }],
        width: Some(1280),
        height: Some(720),
        duration: Some(4.0),
        tags: &["audio", "starter"],
    },
    BundledItem {
        shelf: "backgrounds",
        name: "moss-field",
        item_type: ItemType::Block,
        title: "Moss Field",
        description: "An original two-tone field with a warm sky band.",
        files: &[BundledFile {
            src: "composition.kdl",
            target: "composition.kdl",
            contents: KDL_MOSS_FIELD,
        }],
        width: Some(1280),
        height: Some(720),
        duration: Some(5.0),
        tags: &["backgrounds", "field"],
    },
    BundledItem {
        shelf: "backgrounds",
        name: "vignette-shade",
        item_type: ItemType::Component,
        title: "Vignette Shade",
        description: "An original pair of shading bands for framing a shot.",
        files: &[BundledFile {
            src: "snippet.kdl",
            target: "snippet.kdl",
            contents: KDL_VIGNETTE_SHADE,
        }],
        width: None,
        height: None,
        duration: None,
        tags: &["backgrounds", "vignette"],
    },
    BundledItem {
        shelf: "backgrounds",
        name: "pebble-starter",
        item_type: ItemType::Example,
        title: "Pebble Starter",
        description: "An original starter project on a soft neutral ground.",
        files: &[BundledFile {
            src: "composition.kdl",
            target: "composition.kdl",
            contents: KDL_PEBBLE_STARTER,
        }],
        width: Some(1280),
        height: Some(720),
        duration: Some(3.0),
        tags: &["backgrounds", "starter"],
    },
    BundledItem {
        shelf: "motion",
        name: "drift-study",
        item_type: ItemType::Block,
        title: "Drift Study",
        description: "An original three-step drift study across cool tones.",
        files: &[BundledFile {
            src: "composition.kdl",
            target: "composition.kdl",
            contents: KDL_DRIFT_STUDY,
        }],
        width: Some(1280),
        height: Some(720),
        duration: Some(4.0),
        tags: &["motion", "drift"],
    },
    BundledItem {
        shelf: "motion",
        name: "rise-chip",
        item_type: ItemType::Component,
        title: "Rise Chip",
        description: "An original rising chip with a fading echo.",
        files: &[BundledFile {
            src: "snippet.kdl",
            target: "snippet.kdl",
            contents: KDL_RISE_CHIP,
        }],
        width: None,
        height: None,
        duration: None,
        tags: &["motion", "rise"],
    },
    BundledItem {
        shelf: "motion",
        name: "slide-duet-starter",
        item_type: ItemType::Example,
        title: "Slide Duet Starter",
        description: "An original starter project with a paired sliding plate.",
        files: &[BundledFile {
            src: "composition.kdl",
            target: "composition.kdl",
            contents: KDL_SLIDE_STARTER,
        }],
        width: Some(1280),
        height: Some(720),
        duration: Some(3.0),
        tags: &["motion", "starter"],
    },
    BundledItem {
        shelf: "slideshows",
        name: "deck-frame",
        item_type: ItemType::Block,
        title: "Deck Frame",
        description: "An original slide frame with a title band and two fields.",
        files: &[BundledFile {
            src: "composition.kdl",
            target: "composition.kdl",
            contents: KDL_DECK_FRAME,
        }],
        width: Some(1280),
        height: Some(720),
        duration: Some(6.0),
        tags: &["slideshows", "deck"],
    },
    BundledItem {
        shelf: "slideshows",
        name: "slide-number-chip",
        item_type: ItemType::Component,
        title: "Slide Number Chip",
        description: "An original corner chip for marking a slide count.",
        files: &[BundledFile {
            src: "snippet.kdl",
            target: "snippet.kdl",
            contents: KDL_SLIDE_NUMBER,
        }],
        width: None,
        height: None,
        duration: None,
        tags: &["slideshows", "chip"],
    },
    BundledItem {
        shelf: "slideshows",
        name: "talk-starter",
        item_type: ItemType::Example,
        title: "Talk Starter",
        description: "An original starter project for a short deck opener.",
        files: &[BundledFile {
            src: "composition.kdl",
            target: "composition.kdl",
            contents: KDL_TALK_STARTER,
        }],
        width: Some(1280),
        height: Some(720),
        duration: Some(4.0),
        tags: &["slideshows", "starter"],
    },
    BundledItem {
        shelf: "overlays",
        name: "letterbox-frame",
        item_type: ItemType::Block,
        title: "Letterbox Frame",
        description: "An original letterbox frame with a quiet middle field.",
        files: &[BundledFile {
            src: "composition.kdl",
            target: "composition.kdl",
            contents: KDL_LETTERBOX,
        }],
        width: Some(1280),
        height: Some(720),
        duration: Some(5.0),
        tags: &["overlays", "frame"],
    },
    BundledItem {
        shelf: "overlays",
        name: "keyline-frame",
        item_type: ItemType::Component,
        title: "Keyline Frame",
        description: "An original thin light keyline for edging a shot.",
        files: &[BundledFile {
            src: "snippet.kdl",
            target: "snippet.kdl",
            contents: KDL_KEYLINE,
        }],
        width: None,
        height: None,
        duration: None,
        tags: &["overlays", "keyline"],
    },
    BundledItem {
        shelf: "overlays",
        name: "corner-mark-starter",
        item_type: ItemType::Example,
        title: "Corner Mark Starter",
        description: "An original starter project marking all four corners.",
        files: &[BundledFile {
            src: "composition.kdl",
            target: "composition.kdl",
            contents: KDL_CORNER_STARTER,
        }],
        width: Some(1280),
        height: Some(720),
        duration: Some(3.0),
        tags: &["overlays", "starter"],
    },
    BundledItem {
        shelf: "text",
        name: "reading-plate",
        item_type: ItemType::Block,
        title: "Reading Plate",
        description: "An original reading plate with three ruled text bands.",
        files: &[BundledFile {
            src: "composition.kdl",
            target: "composition.kdl",
            contents: KDL_READING_PLATE,
        }],
        width: Some(1280),
        height: Some(720),
        duration: Some(5.0),
        tags: &["text", "plate"],
    },
    BundledItem {
        shelf: "text",
        name: "highlight-band",
        item_type: ItemType::Component,
        title: "Highlight Band",
        description: "An original highlight band for backing a line of type.",
        files: &[BundledFile {
            src: "snippet.kdl",
            target: "snippet.kdl",
            contents: KDL_HIGHLIGHT_BAND,
        }],
        width: None,
        height: None,
        duration: None,
        tags: &["text", "highlight"],
    },
    BundledItem {
        shelf: "text",
        name: "pullquote-starter",
        item_type: ItemType::Example,
        title: "Pullquote Starter",
        description: "An original starter project with a ruled pullquote plate.",
        files: &[BundledFile {
            src: "composition.kdl",
            target: "composition.kdl",
            contents: KDL_PULLQUOTE_STARTER,
        }],
        width: Some(1280),
        height: Some(720),
        duration: Some(4.0),
        tags: &["text", "starter"],
    },
    BundledItem {
        shelf: "layouts",
        name: "duet-split",
        item_type: ItemType::Block,
        title: "Duet Split",
        description: "An original two-field split with rounded panels.",
        files: &[BundledFile {
            src: "composition.kdl",
            target: "composition.kdl",
            contents: KDL_DUET_SPLIT,
        }],
        width: Some(1280),
        height: Some(720),
        duration: Some(4.0),
        tags: &["layouts", "split"],
    },
    BundledItem {
        shelf: "layouts",
        name: "safe-frame",
        item_type: ItemType::Component,
        title: "Safe Frame",
        description: "An original action-safe frame for laying out a shot.",
        files: &[BundledFile {
            src: "snippet.kdl",
            target: "snippet.kdl",
            contents: KDL_SAFE_FRAME,
        }],
        width: None,
        height: None,
        duration: None,
        tags: &["layouts", "frame"],
    },
    BundledItem {
        shelf: "layouts",
        name: "quartet-starter",
        item_type: ItemType::Example,
        title: "Quartet Starter",
        description: "An original starter project with a four-panel grid.",
        files: &[BundledFile {
            src: "composition.kdl",
            target: "composition.kdl",
            contents: KDL_QUARTET_STARTER,
        }],
        width: Some(1280),
        height: Some(720),
        duration: Some(3.0),
        tags: &["layouts", "starter"],
    },
    BundledItem {
        shelf: "storyboards",
        name: "board-six",
        item_type: ItemType::Block,
        title: "Board Six",
        description: "An original six-panel storyboard board.",
        files: &[BundledFile {
            src: "composition.kdl",
            target: "composition.kdl",
            contents: KDL_BOARD_SIX,
        }],
        width: Some(1280),
        height: Some(720),
        duration: Some(6.0),
        tags: &["storyboards", "board"],
    },
    BundledItem {
        shelf: "storyboards",
        name: "panel-strip",
        item_type: ItemType::Component,
        title: "Panel Strip",
        description: "An original three-panel strip for boarding a beat.",
        files: &[BundledFile {
            src: "snippet.kdl",
            target: "snippet.kdl",
            contents: KDL_PANEL_STRIP,
        }],
        width: None,
        height: None,
        duration: None,
        tags: &["storyboards", "strip"],
    },
    BundledItem {
        shelf: "storyboards",
        name: "approval-starter",
        item_type: ItemType::Example,
        title: "Approval Starter",
        description: "An original starter project with a board plus decision plate.",
        files: &[BundledFile {
            src: "composition.kdl",
            target: "composition.kdl",
            contents: KDL_APPROVAL_STARTER,
        }],
        width: Some(1280),
        height: Some(720),
        duration: Some(4.0),
        tags: &["storyboards", "starter"],
    },
    BundledItem {
        shelf: "variables",
        name: "palette-trio",
        item_type: ItemType::Block,
        title: "Palette Trio",
        description: "An original three-swatch palette for variable-driven work.",
        files: &[BundledFile {
            src: "composition.kdl",
            target: "composition.kdl",
            contents: KDL_PALETTE_TRIO,
        }],
        width: Some(1280),
        height: Some(720),
        duration: Some(4.0),
        tags: &["variables", "palette"],
    },
    BundledItem {
        shelf: "variables",
        name: "swatch-row",
        item_type: ItemType::Component,
        title: "Swatch Row",
        description: "An original four-swatch row for token previews.",
        files: &[BundledFile {
            src: "snippet.kdl",
            target: "snippet.kdl",
            contents: KDL_SWATCH_ROW,
        }],
        width: None,
        height: None,
        duration: None,
        tags: &["variables", "swatch"],
    },
    BundledItem {
        shelf: "variables",
        name: "token-starter",
        item_type: ItemType::Example,
        title: "Token Starter",
        description: "An original starter project with a simple token ladder.",
        files: &[BundledFile {
            src: "composition.kdl",
            target: "composition.kdl",
            contents: KDL_TOKEN_STARTER,
        }],
        width: Some(1280),
        height: Some(720),
        duration: Some(3.0),
        tags: &["variables", "starter"],
    },
    BundledItem {
        shelf: "themes",
        name: "daybreak-theme",
        item_type: ItemType::Block,
        title: "Daybreak Theme",
        description: "An original light theme with a dark accent and warm chip.",
        files: &[BundledFile {
            src: "composition.kdl",
            target: "composition.kdl",
            contents: KDL_DAYBREAK,
        }],
        width: Some(1280),
        height: Some(720),
        duration: Some(4.0),
        tags: &["themes", "light"],
    },
    BundledItem {
        shelf: "themes",
        name: "nightfall-chip",
        item_type: ItemType::Component,
        title: "Nightfall Chip",
        description: "An original dark theme chip with a slate border.",
        files: &[BundledFile {
            src: "snippet.kdl",
            target: "snippet.kdl",
            contents: KDL_NIGHTFALL_CHIP,
        }],
        width: None,
        height: None,
        duration: None,
        tags: &["themes", "dark"],
    },
    BundledItem {
        shelf: "themes",
        name: "hearth-theme-starter",
        item_type: ItemType::Example,
        title: "Hearth Theme Starter",
        description: "An original starter project in a warm hearth theme.",
        files: &[BundledFile {
            src: "composition.kdl",
            target: "composition.kdl",
            contents: KDL_HEARTH_STARTER,
        }],
        width: Some(1280),
        height: Some(720),
        duration: Some(3.0),
        tags: &["themes", "starter"],
    },
    BundledItem {
        shelf: "media",
        name: "pillar-matte",
        item_type: ItemType::Block,
        title: "Pillar Matte",
        description: "An original vertical matte with top and bottom guides.",
        files: &[BundledFile {
            src: "composition.kdl",
            target: "composition.kdl",
            contents: KDL_PILLAR_MATTE,
        }],
        width: Some(1280),
        height: Some(720),
        duration: Some(4.0),
        tags: &["media", "matte"],
    },
    BundledItem {
        shelf: "media",
        name: "square-matte",
        item_type: ItemType::Component,
        title: "Square Matte",
        description: "An original square matte for centering footage.",
        files: &[BundledFile {
            src: "snippet.kdl",
            target: "snippet.kdl",
            contents: KDL_SQUARE_MATTE,
        }],
        width: None,
        height: None,
        duration: None,
        tags: &["media", "matte"],
    },
    BundledItem {
        shelf: "media",
        name: "widescreen-starter",
        item_type: ItemType::Example,
        title: "Widescreen Starter",
        description: "An original starter project framing a wide media field.",
        files: &[BundledFile {
            src: "composition.kdl",
            target: "composition.kdl",
            contents: KDL_WIDESCREEN_STARTER,
        }],
        width: Some(1280),
        height: Some(720),
        duration: Some(3.0),
        tags: &["media", "starter"],
    },
    BundledItem {
        shelf: "compositions",
        name: "triptych-assembly",
        item_type: ItemType::Block,
        title: "Triptych Assembly",
        description: "An original three-panel assembly for nested-block work.",
        files: &[BundledFile {
            src: "composition.kdl",
            target: "composition.kdl",
            contents: KDL_TRIPTYCH,
        }],
        width: Some(1280),
        height: Some(720),
        duration: Some(5.0),
        tags: &["compositions", "assembly"],
    },
    BundledItem {
        shelf: "compositions",
        name: "inset-pip",
        item_type: ItemType::Component,
        title: "Inset Pip",
        description: "An original bordered inset for picture-in-picture pasting.",
        files: &[BundledFile {
            src: "snippet.kdl",
            target: "snippet.kdl",
            contents: KDL_INSET_PIP,
        }],
        width: None,
        height: None,
        duration: None,
        tags: &["compositions", "inset"],
    },
    BundledItem {
        shelf: "compositions",
        name: "assembly-starter",
        item_type: ItemType::Example,
        title: "Assembly Starter",
        description: "An original starter project with a header and two fields.",
        files: &[BundledFile {
            src: "composition.kdl",
            target: "composition.kdl",
            contents: KDL_ASSEMBLY_STARTER,
        }],
        width: Some(1280),
        height: Some(720),
        duration: Some(4.0),
        tags: &["compositions", "starter"],
    },
    BundledItem {
        shelf: "beats",
        name: "downbeat-row",
        item_type: ItemType::Block,
        title: "Downbeat Row",
        description: "An original four-step beat row with a strong downbeat.",
        files: &[BundledFile {
            src: "composition.kdl",
            target: "composition.kdl",
            contents: KDL_DOWNBEAT,
        }],
        width: Some(1280),
        height: Some(720),
        duration: Some(4.0),
        tags: &["beats", "row"],
    },
    BundledItem {
        shelf: "beats",
        name: "section-mark",
        item_type: ItemType::Component,
        title: "Section Mark",
        description: "An original corner mark for flagging a track section.",
        files: &[BundledFile {
            src: "snippet.kdl",
            target: "snippet.kdl",
            contents: KDL_SECTION_MARK,
        }],
        width: None,
        height: None,
        duration: None,
        tags: &["beats", "mark"],
    },
    BundledItem {
        shelf: "beats",
        name: "count-in-starter",
        item_type: ItemType::Example,
        title: "Count In Starter",
        description: "An original starter project counting in four colored beats.",
        files: &[BundledFile {
            src: "composition.kdl",
            target: "composition.kdl",
            contents: KDL_COUNT_IN_STARTER,
        }],
        width: Some(1280),
        height: Some(720),
        duration: Some(4.0),
        tags: &["beats", "starter"],
    },
    BundledItem {
        shelf: "lyrics",
        name: "line-plate",
        item_type: ItemType::Block,
        title: "Line Plate",
        description: "An original two-line lyric plate with a fading second line.",
        files: &[BundledFile {
            src: "composition.kdl",
            target: "composition.kdl",
            contents: KDL_LINE_PLATE,
        }],
        width: Some(1280),
        height: Some(720),
        duration: Some(4.0),
        tags: &["lyrics", "plate"],
    },
    BundledItem {
        shelf: "lyrics",
        name: "lyric-underline",
        item_type: ItemType::Component,
        title: "Lyric Underline",
        description: "An original paired underline for a sung line.",
        files: &[BundledFile {
            src: "snippet.kdl",
            target: "snippet.kdl",
            contents: KDL_LYRIC_UNDERLINE,
        }],
        width: None,
        height: None,
        duration: None,
        tags: &["lyrics", "underline"],
    },
    BundledItem {
        shelf: "lyrics",
        name: "verse-starter",
        item_type: ItemType::Example,
        title: "Verse Starter",
        description: "An original starter project stacking three verse plates.",
        files: &[BundledFile {
            src: "composition.kdl",
            target: "composition.kdl",
            contents: KDL_VERSE_STARTER,
        }],
        width: Some(1280),
        height: Some(720),
        duration: Some(5.0),
        tags: &["lyrics", "starter"],
    },
    BundledItem {
        shelf: "grids",
        name: "thirds-guide",
        item_type: ItemType::Block,
        title: "Thirds Guide",
        description: "An original rule-of-thirds guide over a dark field.",
        files: &[BundledFile {
            src: "composition.kdl",
            target: "composition.kdl",
            contents: KDL_THIRDS_GUIDE,
        }],
        width: Some(1280),
        height: Some(720),
        duration: Some(4.0),
        tags: &["grids", "thirds"],
    },
    BundledItem {
        shelf: "grids",
        name: "center-cross",
        item_type: ItemType::Component,
        title: "Center Cross",
        description: "An original center cross for aligning a subject.",
        files: &[BundledFile {
            src: "snippet.kdl",
            target: "snippet.kdl",
            contents: KDL_CENTER_CROSS,
        }],
        width: None,
        height: None,
        duration: None,
        tags: &["grids", "center"],
    },
    BundledItem {
        shelf: "grids",
        name: "margin-starter",
        item_type: ItemType::Example,
        title: "Margin Starter",
        description: "An original starter project with nested margin frames.",
        files: &[BundledFile {
            src: "composition.kdl",
            target: "composition.kdl",
            contents: KDL_MARGIN_STARTER,
        }],
        width: Some(1280),
        height: Some(720),
        duration: Some(3.0),
        tags: &["grids", "starter"],
    },
    BundledItem {
        shelf: "logos",
        name: "bug-plate",
        item_type: ItemType::Block,
        title: "Bug Plate",
        description: "An original corner bug plate with a red mark.",
        files: &[BundledFile {
            src: "composition.kdl",
            target: "composition.kdl",
            contents: KDL_BUG_PLATE,
        }],
        width: Some(1280),
        height: Some(720),
        duration: Some(5.0),
        tags: &["logos", "bug"],
    },
    BundledItem {
        shelf: "logos",
        name: "lockup-strip",
        item_type: ItemType::Component,
        title: "Lockup Strip",
        description: "An original mark-plus-line strip for a lockup.",
        files: &[BundledFile {
            src: "snippet.kdl",
            target: "snippet.kdl",
            contents: KDL_LOCKUP_STRIP,
        }],
        width: None,
        height: None,
        duration: None,
        tags: &["logos", "lockup"],
    },
    BundledItem {
        shelf: "logos",
        name: "ident-starter",
        item_type: ItemType::Example,
        title: "Ident Starter",
        description: "An original starter project with a centered ident card.",
        files: &[BundledFile {
            src: "composition.kdl",
            target: "composition.kdl",
            contents: KDL_IDENT_STARTER,
        }],
        width: Some(1280),
        height: Some(720),
        duration: Some(3.0),
        tags: &["logos", "starter"],
    },
    BundledItem {
        shelf: "maps",
        name: "route-bar",
        item_type: ItemType::Block,
        title: "Route Bar",
        description: "An original route bar with progress and two stops.",
        files: &[BundledFile {
            src: "composition.kdl",
            target: "composition.kdl",
            contents: KDL_ROUTE_BAR,
        }],
        width: Some(1280),
        height: Some(720),
        duration: Some(5.0),
        tags: &["maps", "route"],
    },
    BundledItem {
        shelf: "maps",
        name: "pin-plate",
        item_type: ItemType::Component,
        title: "Pin Plate",
        description: "An original map pin with a stem for dropping on a route.",
        files: &[BundledFile {
            src: "snippet.kdl",
            target: "snippet.kdl",
            contents: KDL_PIN_PLATE,
        }],
        width: None,
        height: None,
        duration: None,
        tags: &["maps", "pin"],
    },
    BundledItem {
        shelf: "maps",
        name: "journey-starter",
        item_type: ItemType::Example,
        title: "Journey Starter",
        description: "An original starter project tracing a simple journey.",
        files: &[BundledFile {
            src: "composition.kdl",
            target: "composition.kdl",
            contents: KDL_JOURNEY_STARTER,
        }],
        width: Some(1280),
        height: Some(720),
        duration: Some(4.0),
        tags: &["maps", "starter"],
    },
];

// ---------------------------------------------------------------------------
// Manifests, counts, and the contribution/coverage check.
// ---------------------------------------------------------------------------

fn escape_json(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// Render the T001 manifest document for a bundled item. The first tag
/// is always the shelf name so listings group back by shelf.
pub fn manifest_json(item: &BundledItem) -> String {
    let mut files = String::from("[");
    for (i, f) in item.files.iter().enumerate() {
        if i > 0 {
            files.push_str(", ");
        }
        files.push_str(&format!(
            "{{\"src\": \"{}\", \"target\": \"{}\"}}",
            escape_json(f.src),
            escape_json(f.target)
        ));
    }
    files.push(']');
    let mut tags = String::from("[");
    for (i, t) in item.tags.iter().enumerate() {
        if i > 0 {
            tags.push_str(", ");
        }
        tags.push_str(&format!("\"{}\"", escape_json(t)));
    }
    tags.push(']');
    let mut doc = format!(
        "{{\n    \"name\": \"{}\",\n    \"type\": \"{}\",\n    \"title\": \"{}\",\n    \"description\": \"{}\",\n    \"files\": {files},\n    \"tags\": {tags},\n    \"author\": \"Framesmith\",\n    \"license\": \"MIT\"",
        escape_json(item.name),
        item.item_type.as_str(),
        escape_json(item.title),
        escape_json(item.description),
    );
    if let (Some(w), Some(h), Some(d)) = (item.width, item.height, item.duration) {
        doc.push_str(&format!(
            ",\n    \"width\": {w},\n    \"height\": {h},\n    \"duration\": {d}"
        ));
    }
    doc.push_str("\n}\n");
    doc
}

/// Items per shelf, in shelf order.
pub fn shelf_counts(items: &[BundledItem]) -> Vec<(&'static str, usize)> {
    SHELVES
        .iter()
        .map(|s| (s.name, items.iter().filter(|i| i.shelf == s.name).count()))
        .collect()
}

/// Items per shelf for a loaded listing, in shelf order. Shelf
/// membership comes from the manifest tags (first tag matching a
/// known shelf name).
pub fn listing_shelf_counts(listing: &RegistryListing) -> Vec<(&'static str, usize)> {
    SHELVES
        .iter()
        .map(|s| {
            (
                s.name,
                listing
                    .items
                    .iter()
                    .filter(|m| shelf_of_manifest(m) == Some(s.name))
                    .count(),
            )
        })
        .collect()
}

/// Printable per-shelf counts, one line per shelf plus a total.
pub fn format_shelf_counts(items: &[BundledItem]) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "Bundled catalog: {} items across {} shelves\n",
        items.len(),
        SHELVES.len()
    ));
    for (shelf, count) in shelf_counts(items) {
        let (mut blocks, mut components, mut examples) = (0, 0, 0);
        for item in items.iter().filter(|i| i.shelf == shelf) {
            match item.item_type {
                ItemType::Block => blocks += 1,
                ItemType::Component => components += 1,
                ItemType::Example => examples += 1,
            }
        }
        out.push_str(&format!(
            "{shelf}: {count} ({blocks} block, {components} component, {examples} example)\n"
        ));
    }
    out
}

/// The shelf a listed manifest belongs to: its first tag matching a
/// known shelf name. Items from other registries may belong nowhere.
pub fn shelf_of_manifest(manifest: &ItemManifest) -> Option<&str> {
    manifest
        .tags
        .iter()
        .find_map(|t| lookup_shelf(t).map(|s| s.name))
}

/// One coverage defect: the shelf plus what is wrong with it. An
/// emptied shelf reports by name so the contribution check never
/// passes a silent empty room.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverageFault {
    pub shelf: String,
    pub message: String,
}

impl std::fmt::Display for CoverageFault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "shelf {:?}: {}", self.shelf, self.message)
    }
}

fn empty_fault(shelf: &str) -> CoverageFault {
    CoverageFault {
        shelf: shelf.to_string(),
        message: format!("shelf {shelf:?} holds no items; every shelf needs original items"),
    }
}

fn thin_fault(shelf: &str, count: usize) -> CoverageFault {
    CoverageFault {
        shelf: shelf.to_string(),
        message: format!(
            "shelf {shelf:?} holds only {count} item(s); every shelf needs at least {MIN_ITEMS_PER_SHELF}"
        ),
    }
}

/// Coverage over a loaded registry listing: every expected shelf must
/// hold at least [`MIN_ITEMS_PER_SHELF`] items. An emptied shelf fails
/// by name. Items from other registries (no known shelf tag) never
/// count against the bundled shelves.
pub fn check_listing_coverage(listing: &RegistryListing) -> Vec<CoverageFault> {
    let counts = listing_shelf_counts(listing);
    let mut faults = Vec::new();
    for (shelf, count) in counts {
        if count == 0 {
            faults.push(empty_fault(shelf));
        } else if count < MIN_ITEMS_PER_SHELF {
            faults.push(thin_fault(shelf, count));
        }
    }
    faults
}

/// Snake-case style spellings our catalog KDL must never use: bundled
/// content stays hyphenated throughout.
pub const BANNED_SNAKE_ATTRS: &[&str] = &[
    "font_family",
    "border_width",
    "border_color",
    "object_fit",
    "object_position",
    "line_height",
    "letter_spacing",
    "flex_direction",
    "flex_grow",
    "align_items",
    "justify_content",
    "stroke_width",
    "stroke_color",
    "stroke_position",
    "start_after",
    "start_offset",
    "mix_blend_mode",
    "box_shadow",
];

/// Distinctive HyperFrames item, template, preset, and shader names
/// that must never appear in bundled content. Generic category words
/// (shelf names) are the only similarity allowed. Checked
/// case-insensitively over names, titles, descriptions, tags, and KDL.
pub const BANNED_HYPERFRAMES_NAMES: &[&str] = &[
    "echo-smear",
    "filmed-screen",
    "warm-grain",
    "play-mode",
    "swiss-grid",
    "vignelli",
    "decision-tree",
    "kinetic-type",
    "product-promo",
    "nyt-graph",
    "domain-warp",
    "ridged-burn",
    "whip-pan",
    "sdf-iris",
    "ripple-waves",
    "gravitational-lens",
    "cinematic-zoom",
    "chromatic-split",
    "swirl-vortex",
    "thermal-distortion",
    "flash-through-white",
    "cross-warp-morph",
    "light-leak",
    "remotion",
    "heygen",
    "lottie",
    "gsap",
    "hyperframes",
];

/// KDL bytes every bundled composition or snippet must satisfy:
/// hyphenated attributes, no `font-family`, no external references.
pub fn kdl_hygiene(contents: &str) -> Vec<String> {
    let mut violations = Vec::new();
    let lower = contents.to_lowercase();
    for snake in BANNED_SNAKE_ATTRS {
        if lower.contains(snake) {
            violations.push(format!("snake-case attribute {snake:?}; use hyphens"));
        }
    }
    for needle in [
        "font-family",
        "font_family",
        "path=",
        "src=",
        "http://",
        "https://",
    ] {
        if lower.contains(needle) {
            violations.push(format!("forbidden {needle:?} in bundled KDL"));
        }
    }
    violations
}

/// Full contribution check over the bundled statics: exact shelf set,
/// per-shelf minimums, unique well-formed names, manifests that parse
/// under the T001 schema, KDL hygiene, and no HyperFrames content.
pub fn check_bundled_coverage() -> Vec<CoverageFault> {
    let mut faults = Vec::new();
    let fault = |shelf: &str, message: String| CoverageFault {
        shelf: shelf.to_string(),
        message,
    };
    let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for item in BUNDLED {
        if lookup_shelf(item.shelf).is_none() {
            faults.push(fault(
                item.shelf,
                format!("item {:?} names an unknown shelf", item.name),
            ));
        }
        if !seen.insert(item.name) {
            faults.push(fault(
                item.shelf,
                format!("duplicate item name {:?}", item.name),
            ));
        }
        if item.tags.first().map(|t| *t) != Some(item.shelf) {
            faults.push(fault(
                item.shelf,
                format!("item {:?} must carry its shelf as its first tag", item.name),
            ));
        }
        match parse_manifest_str(&manifest_json(item)) {
            Ok(parsed) => {
                if parsed.name != item.name {
                    faults.push(fault(
                        item.shelf,
                        format!(
                            "item {:?} manifest round-trips as {:?}",
                            item.name, parsed.name
                        ),
                    ));
                }
            }
            Err(errors) => {
                let fields: Vec<&str> = errors.iter().map(|e| e.field.as_str()).collect();
                faults.push(fault(
                    item.shelf,
                    format!("item {:?} manifest fails fields {fields:?}", item.name),
                ));
            }
        }
        for violation in kdl_hygiene_bundled(item) {
            faults.push(fault(item.shelf, violation));
        }
        for banned in BANNED_HYPERFRAMES_NAMES {
            let haystacks = [item.name, item.title, item.description];
            if haystacks.iter().any(|h| h.to_lowercase().contains(banned))
                || item.tags.iter().any(|t| t.to_lowercase().contains(banned))
                || item
                    .files
                    .iter()
                    .any(|f| f.contents.to_lowercase().contains(banned))
            {
                faults.push(fault(
                    item.shelf,
                    format!("item {:?} copies HyperFrames content {banned:?}", item.name),
                ));
            }
        }
    }
    for (shelf, count) in shelf_counts(BUNDLED) {
        if count == 0 {
            faults.push(empty_fault(shelf));
        } else if count < MIN_ITEMS_PER_SHELF {
            faults.push(thin_fault(shelf, count));
        }
    }
    faults.sort_by(|a, b| (&a.shelf, &a.message).cmp(&(&b.shelf, &b.message)));
    faults
}

fn kdl_hygiene_bundled(item: &BundledItem) -> Vec<String> {
    let mut out = Vec::new();
    for f in item.files {
        for violation in kdl_hygiene(f.contents) {
            out.push(format!(
                "item {:?} file {:?}: {violation}",
                item.name, f.src
            ));
        }
    }
    out
}

/// Write the whole bundled registry to `dir` as
/// `<dir>/<shelf>/<item>/manifest.json` plus content files, so the
/// offline [`crate::load_registry`] lists every item. Returns the
/// item count written. Used by tests and by later installer slices.
pub fn materialize_bundled(dir: &Path) -> io::Result<usize> {
    for item in BUNDLED {
        let folder = dir.join(item.shelf).join(item.name);
        std::fs::create_dir_all(&folder)?;
        std::fs::write(folder.join(crate::MANIFEST_FILE), manifest_json(item))?;
        for f in item.files {
            let path = folder.join(f.src);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(path, f.contents.replace("{name}", item.name))?;
        }
    }
    Ok(BUNDLED.len())
}

/// One file shipped by a bundled item: install `src` to `target`,
/// with the file bytes baked in for fully offline use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BundledFile {
    pub src: &'static str,
    pub target: &'static str,
    pub contents: &'static str,
}

/// One bundled catalog item: its shelf plus every manifest field.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BundledItem {
    pub shelf: &'static str,
    pub name: &'static str,
    pub item_type: ItemType,
    pub title: &'static str,
    pub description: &'static str,
    pub files: &'static [BundledFile],
    pub width: Option<u64>,
    pub height: Option<u64>,
    pub duration: Option<f64>,
    pub tags: &'static [&'static str],
}
