//! Bundled starter examples for `init --example` (Sprint 19, T008 / CAP-008).
//!
//! Starters are clean-room Framesmith content baked into the binary: no
//! network, no registry download, fully offline. Each starter is a single
//! self-contained `composition.kdl` — boxes only, hyphenated style
//! attributes throughout, no `font-family` (fail-closed try order is never
//! exercised because no family is named), and no external `path`/`src`
//! references — so every starter checks, opens in Studio, and snapshots
//! deterministically on any machine.
//!
//! `preview_digest` is the SHA256 hex of the starter's `snapshot --at 0`
//! PNG bytes (default CPU backend). Project names never affect pixels
//! (no text clips), so one digest covers every project name.

/// One bundled starter example.
#[derive(Debug, Clone, Copy)]
pub struct Starter {
    /// `init --example` name (exact, case-sensitive).
    pub name: &'static str,
    /// Human title shown in `AGENTS.md` and listings.
    pub title: &'static str,
    /// One-line description for refusal listings.
    pub description: &'static str,
    /// Catalog shelf this starter belongs to.
    pub shelf: &'static str,
    /// Composition template with one `{name}` placeholder for the project
    /// (composition) name. Project names cannot contain braces (see
    /// [`crate::commands::name_error`]), so substitution is exact.
    pub composition: &'static str,
    /// SHA256 hex of the `snapshot --at 0` PNG of the untouched starter.
    pub preview_digest: &'static str,
}

/// Title card: warm card with an accent border and an underline bar.
const TITLE_CARD: &str = "composition name=\"{name}\" width=640 height=360 duration=3 fps=30 background=\"#1a1a2e\" version=1 {\n    box x=40 y=110 width=560 height=140 fill=\"#f4f1de\" radius=16 border-width=4 border-color=\"#e07a5f\" start=0 duration=3\n    box x=40 y=266 width=180 height=12 fill=\"#e07a5f\" radius=6 start=0 duration=3\n}\n";

/// Lower third: white plate with a red keyline over a dark frame.
const LOWER_THIRD: &str = "composition name=\"{name}\" width=1280 height=720 duration=4 fps=30 background=\"#101018\" version=1 {\n    box x=80 y=520 width=560 height=120 fill=\"#ffffff\" radius=12 opacity=0.96 start=0 duration=4\n    box x=80 y=520 width=16 height=120 fill=\"#e63946\" radius=6 start=0 duration=4\n    box x=80 y=660 width=240 height=10 fill=\"#e63946\" radius=5 start=0 duration=4\n}\n";

/// Color grid: four paint-variant boxes (radius, border, opacity).
const COLOR_GRID: &str = "composition name=\"{name}\" width=640 height=360 duration=2 fps=30 background=\"#0b0b10\" version=1 {\n    box x=40 y=40 width=260 height=130 fill=\"#e63946\" radius=14 start=0 duration=2\n    box x=340 y=40 width=260 height=130 fill=\"#457b9d\" border-width=4 border-color=\"#f1fa8c\" start=0 duration=2\n    box x=40 y=190 width=260 height=130 fill=\"#2a9d8f\" radius=14 start=0 duration=2\n    box x=340 y=190 width=260 height=130 fill=\"#e9c46a\" opacity=0.9 start=0 duration=2\n}\n";

/// Every bundled starter, in listing order.
pub const STARTERS: &[Starter] = &[
    Starter {
        name: "title-card",
        title: "Title card",
        description: "warm card with an accent border and underline bar (640x360, 3s)",
        shelf: "titles",
        composition: TITLE_CARD,
        preview_digest: "2c70dd83cdcc85012c78c9e53cf9f3622545c86daa814887c8c07f7a6f749f69",
    },
    Starter {
        name: "lower-third",
        title: "Lower third",
        description: "white plate with a red keyline over a dark frame (1280x720, 4s)",
        shelf: "titles",
        composition: LOWER_THIRD,
        preview_digest: "621a04b26f5abad83ea10a489d17c26f3fe5ea9233e75fb745410fc7d00d1b5d",
    },
    Starter {
        name: "color-grid",
        title: "Color grid",
        description: "four paint-variant boxes: radius, border, opacity (640x360, 2s)",
        shelf: "backgrounds",
        composition: COLOR_GRID,
        preview_digest: "d041656284e8ba55b5b32b5d5606f0a9dad81f74ef4ac2e015fa9c07fc2950ca",
    },
];

/// Find a starter by exact (case-sensitive) name.
pub fn find_starter(name: &str) -> Option<&'static Starter> {
    STARTERS.iter().find(|s| s.name == name)
}

/// Starter names in listing order.
pub fn starter_names() -> Vec<&'static str> {
    STARTERS.iter().map(|s| s.name).collect()
}

/// `init --example` refusal line listing every available starter.
pub fn starter_list_line() -> String {
    STARTERS
        .iter()
        .map(|s| format!("{} ({})", s.name, s.description))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Render the composition for `project` from a starter template.
pub fn starter_composition(starter: &Starter, project: &str) -> String {
    starter.composition.replace("{name}", project)
}

/// Agent guide file written into every `init` project (Sprint 19, T008).
/// The agents sprint deepens this content; here it names the layout, the
/// starter (when one was used), and the commands that keep the project
/// valid. Deterministic for a given project and starter.
pub fn agent_guide(project: &str, starter: Option<&Starter>) -> String {
    let from = match starter {
        Some(s) => format!(
            "Started from the `{}` starter ({}).\nThe composition below renders unchanged: `snapshot --at 0 --out <dir>` matches its preview digest.\n",
            s.name, s.title
        ),
        None => "Started from the minimal scaffold (no starter).\n".to_string(),
    };
    format!(
        "# {project} — Framesmith project\n\n{from}\n\
         ## Layout\n\n\
         - `composition.kdl` is the project: compositions with clips (`box`, `text`, `video`, `audio`).\n\
         - Keep style attributes hyphenated (`border-width`, `border-color`, `object-fit`, `font-family`).\n\
         - A `font-family` names usable families in try order and fails closed on unknown names.\n\
         - Extra helper files are ignored by check and render.\n\n\
         ## Commands\n\n\
         - `framesmith check {project}` validates without rendering.\n\
         - `framesmith snapshot --at 0 --out <dir> {project}` draws frame PNGs without writing any MP4.\n\
         - `framesmith compositions {project}` lists compositions (what Studio opens).\n\
         - `framesmith render {project}` validates, then writes the standard MP4.\n\
         - `framesmith init <name> --example <starter>` starts another project from a starter.\n\
         - Available starters: {}.\n",
        starter_list_line()
    )
}

/// Name of the agent guide file written by `init`.
pub const AGENT_GUIDE_FILE: &str = "AGENTS.md";

#[cfg(test)]
mod tests {
    use super::*;

    /// Snake-case style spellings our own catalog KDL must never use:
    /// bundled content stays hyphenated throughout.
    const BANNED_SNAKE_ATTRS: &[&str] = &[
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

    #[test]
    fn starters_cover_more_than_one_shelf() {
        let mut shelves: Vec<&str> = STARTERS.iter().map(|s| s.shelf).collect();
        shelves.sort_unstable();
        shelves.dedup();
        assert!(
            shelves.len() >= 2,
            "starters should span shelves: {shelves:?}"
        );
    }

    #[test]
    fn starter_names_are_unique_and_well_formed() {
        let names = starter_names();
        assert!(names.len() >= 2, "need several starters, have {names:?}");
        let mut seen = std::collections::HashSet::new();
        for n in &names {
            assert!(
                crate::commands::name_error(n).is_none(),
                "starter name {n:?} must itself be a valid project name"
            );
            assert!(seen.insert(*n), "duplicate starter name {n:?}");
        }
    }

    #[test]
    fn starter_content_is_clean_room_offline_and_hyphenated() {
        for s in STARTERS {
            let body = starter_composition(s, "probe");
            // No external files, no network, no fonts: fully offline and
            // machine-independent (fail-closed font-family never triggers).
            for needle in [
                "path=",
                "src=",
                "http://",
                "https://",
                "font-family",
                "font_family",
                "font=",
            ] {
                assert!(
                    !body.contains(needle),
                    "starter {:?} must not contain {needle:?}:\n{body}",
                    s.name
                );
            }
            for snake in BANNED_SNAKE_ATTRS {
                assert!(
                    !body.contains(snake),
                    "starter {:?} must use hyphenated attributes, found {snake:?}:\n{body}",
                    s.name
                );
            }
            // Every box carries paint: no unpainted (invisible) clips.
            for line in body.lines().filter(|l| l.trim_start().starts_with("box ")) {
                assert!(
                    line.contains("fill="),
                    "starter {:?} box must carry paint:\n{line}",
                    s.name
                );
            }
        }
    }

    #[test]
    fn starter_composition_substitutes_project_name_exactly_once() {
        for s in STARTERS {
            assert_eq!(
                s.composition.matches("{name}").count(),
                1,
                "starter {:?} must carry exactly one {{name}} placeholder",
                s.name
            );
            let body = starter_composition(s, "my-proj_1");
            assert!(body.contains("composition name=\"my-proj_1\""), "{body}");
            assert!(!body.contains("{name}"), "{body}");
        }
    }

    #[test]
    fn unknown_starter_lookup_fails() {
        assert!(find_starter("no-such-starter").is_none());
        assert!(
            find_starter("Title-Card").is_none(),
            "lookup is exact and case-sensitive"
        );
        assert!(find_starter("").is_none());
    }

    #[test]
    fn starter_listing_names_every_starter() {
        let line = starter_list_line();
        for s in STARTERS {
            assert!(
                line.contains(s.name),
                "listing must name {:?}: {line}",
                s.name
            );
        }
    }

    #[test]
    fn agent_guide_names_project_starter_and_commands() {
        for s in STARTERS {
            let guide = agent_guide("demo", Some(s));
            assert!(guide.contains("demo"), "{guide}");
            assert!(guide.contains(s.name), "{guide}");
            assert!(guide.contains("check"), "{guide}");
            assert!(guide.contains("render"), "{guide}");
        }
        let minimal = agent_guide("demo", None);
        assert!(minimal.contains("demo"), "{minimal}");
        assert!(minimal.contains("no starter"), "{minimal}");
    }

    #[test]
    fn preview_digests_are_sha256_hex() {
        for s in STARTERS {
            assert_eq!(
                s.preview_digest.len(),
                64,
                "starter {:?} digest must be SHA256 hex",
                s.name
            );
            assert!(
                s.preview_digest.chars().all(|c| c.is_ascii_hexdigit()),
                "starter {:?} digest must be hex: {}",
                s.name,
                s.preview_digest
            );
            assert_ne!(
                s.preview_digest, "PENDING",
                "starter {:?} digest must be recorded",
                s.name
            );
        }
    }
}
