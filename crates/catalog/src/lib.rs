//! Framesmith catalog and registry (Sprint 19, T001 / CAP-001).
//!
//! Every registry item ships a `manifest.json` declaring its name,
//! type, title, description, and files with per-file targets. Blocks
//! and examples additionally carry dimensions and duration, while
//! components never do. Optional fields (tags, author, license,
//! `minCliVersion`, deprecated, `registryDependencies`, preview,
//! `relatedSkill`, params, variables) read back truthfully when
//! present.
//!
//! A manifest failing its own schema is never installed: registry
//! listing reports it as broken, naming the item and the failing
//! field, beside the healthy items it never blocks.
//!
//! Layout: a registry is a directory whose item directories each hold
//! one [`MANIFEST_FILE`]. Listing works fully offline from those
//! files alone. All bundled item content is our own clean-room work;
//! no HyperFrames item is ever copied.
//!
//! Later slices layer on this core: shelf coverage (T002) fills the
//! bundled registry, installs (T003/T004) resolve and gate these
//! manifests, and meaning search (T006) layers over the word ranking
//! in [`search`] that this slice ships for the CLI, the picker, and
//! the Studio panel.

pub mod install;
pub mod manifest;
pub mod meaning;
pub mod registry;
pub mod search;
pub mod sha256;
pub mod shelves;

pub use install::{
    adopted_audio, check_version_gates, install, install_merged, install_merged_with_options,
    install_with_options, is_audio_target, mount_snippet_for, parse_var_assign, project_registries,
    project_remaps, registry_sources, remap_target, resolve, resolve_merged, write_plan,
    write_plan_with_options, AdoptedAudio, CustomRegistry, InstallError, InstallOptions,
    InstallReport, InstalledFile, LockEntry, LockFile, MergedPlan, PlannedItem, ResolvedPlan,
    Selection, VarAssign, AUDIO_EXTENSIONS, CATALOG_LOCK_FILE, CATALOG_LOCK_VERSION,
    CURRENT_CLI_VERSION, HIDDEN_TOOL_DIR, PROJECT_CONFIG_FILE,
};
pub use manifest::{
    parse_manifest_str, AudioRole, ItemFile, ItemManifest, ItemType, ManifestError, MANIFEST_FILE,
};
pub use meaning::{
    builtin_model_bytes, consent_explanation, consent_path, declined_json, declined_notice,
    default_fetch, download_failed_json, download_failed_report, embed_text, ensure_model,
    meaning_dir, meaning_ranked_json, meaning_search, model_path, needs_consent_json, rank_meaning,
    render_meaning_picker, render_word_fallback, revoke_consent, revoked_report, word_hits,
    ConsentAnswer, MeaningHit, MeaningOutcome, WordHit, EMBED_DIM, MEANING_ENV_DIR,
    MEANING_ENV_FAIL, MEANING_ENV_SOURCE, MEANING_MODEL_NAME, MEANING_MODEL_VERSION, MEANING_TOP_K,
};
pub use registry::{
    load_merged, load_registry, locate_items, locate_items_detailed, BrokenItem, CollisionNote,
    DuplicateClaim, LocatedItem, MergedItem, MergedListing, RegistryListing, RegistrySource,
    BUNDLED_ORIGIN,
};
pub use search::{
    append_miss_report, empty_json, empty_json_with_warnings, empty_registry_report, hits_json,
    hits_json_with_warnings, load_merged_search_index, load_search_index, miss_report_offer,
    no_hits_json, no_hits_json_with_warnings, no_hits_report, render_picker, resolve_registry_dir,
    stems_of, tokenize, RankedHit, SearchIndex, MISS_REPORTS_FILE, REGISTRY_ENV,
};
pub use sha256::{hex as sha256_hex_of, sha256, sha256_hex};
pub use shelves::{
    check_bundled_coverage, check_listing_coverage, format_shelf_counts, kdl_hygiene,
    listing_shelf_counts, lookup_shelf, manifest_json, materialize_bundled, shelf_counts,
    shelf_of_manifest, BundledFile, BundledItem, CoverageFault, Shelf, BANNED_HYPERFRAMES_NAMES,
    BANNED_SNAKE_ATTRS, BUNDLED, MIN_ITEMS_PER_SHELF, SHELVES,
};
