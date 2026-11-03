//! Error-code catalog (T007 / CAP-007).
//!
//! Every diagnostic code the system can report has one entry with a
//! meaning, a minimal example, and a fix direction. The `errors`
//! subcommand prints this catalog; `check --json` carries the same
//! codes machine-readably. No code is reported without an entry here.

/// One catalog entry: what the code means, what triggers it, how to fix it.
#[derive(Debug, Clone, Copy)]
pub struct CatalogEntry {
    pub code: &'static str,
    pub meaning: &'static str,
    pub example: &'static str,
    pub fix: &'static str,
}

/// All codes the system can emit, sorted by code.
///
/// Validation codes (`validate.rs`) first in spirit, then the FFmpeg
/// bootstrap codes (`bootstrap.rs`). The order below is alphabetical so
/// `errors` output stays stable.
pub const CATALOG: &[CatalogEntry] = &[
    CatalogEntry {
        code: "E_AMBIGUOUS_ROOT",
        meaning: "The project holds several compositions but does not mark exactly one as the render root (`root=true`), or a `root` mark is not `true` or `false`.",
        example: "composition name=\"a\" { }  composition name=\"b\" { }   // no root marked",
        fix: "Mark exactly one composition `root=true` (for example on the main cutdown); `render` uses the root by default and `render --composition <name>` selects another.",
    },
    CatalogEntry {
        code: "E_BAD_AUDIO",
        meaning: "An `audio` track or video sound definition names an unusable trim, a `rate` outside 0.1 to 10, a volume above +12 dB, a fade outside its clip window, or an omitted `duration` with no knowable source end to infer from.",
        example: "audio path=\"s.wav\" start=0 duration=2 volume=20   // above the +12 dB ceiling",
        fix: "Point `path` at a readable supported audio file; keep `trim-start`/`trim-end` ordered seconds from 0, `rate` from 0.1 to 10, `volume` at most +12 dB (`-inf` is silence), each fade within the clip duration, and every duration-less track offering a knowable source end (or its own `trim-end`).",
    },
    CatalogEntry {
        code: "E_BAD_BOX",
        meaning: "A box size, radius, border width, or opacity is missing, not a number, or outside its accepted range.",
        example: "box x=10 y=20 width=0 height=50 fill=\"#ff0000\" start=0 duration=1   // zero width",
        fix: "Give the box a positive `width` and `height`, a non-negative `radius` and `border-width`, and an `opacity` from 0 to 1.",
    },
    CatalogEntry {
        code: "E_BAD_CANVAS",
        meaning: "A composition width or height is not a positive whole number of pixels, or the `canvas` preset is not a known preset name.",
        example: "composition name=\"t\" width=0 height=640 duration=2 fps=30 background=\"#000\" version=1 { }",
        fix: "Set `width` and `height` to positive integers (e.g. `width=1280 height=720`), or set `canvas` to one of `1080p` (alias `fullhd`), `720p`, `square`, `portrait`, `4k` (alias `uhd`); explicit dimensions override the preset.",
    },
    CatalogEntry {
        code: "E_BAD_COLOR",
        meaning: "A color value is not a `#rgb` or `#rrggbb` hex color.",
        example: "composition name=\"t\" width=64 height=64 duration=1 background=\"red\" version=1 { }",
        fix: "Use a hex color such as `background=\"#1a1a2e\"` or `color=\"#ffffff\"`.",
    },
    CatalogEntry {
        code: "E_BAD_CONFIG",
        meaning: "A `framesmith.toml` value has the wrong shape (e.g. `ffmpeg_auto_download` is not `true` or `false`).",
        example: "ffmpeg_auto_download = maybe",
        fix: "Write `ffmpeg_auto_download = true` or `ffmpeg_auto_download = false`.",
    },
    CatalogEntry {
        code: "E_BAD_DURATION",
        meaning: "A composition `duration` is not a number, not positive, below the 1/60s minimum length, or omitted with no finite clip end to infer from.",
        example: "composition name=\"t\" width=64 height=64 duration=-2 background=\"#000\" version=1 { }",
        fix: "Set `duration=<positive seconds>` on the composition root, e.g. `duration=3` (at least 1/60s); omit it only when a clip bounds the timeline so the composition can end at the latest finite clip end.",
    },
    CatalogEntry {
        code: "E_BAD_EASING",
        meaning: "A `tween`, `key`, or `frame` names an unknown easing, or a parameterized easing carries missing, non-numeric, or out-of-range parameters; the system never guesses a curve.",
        example: "tween target=0 property=\"x\" kind=\"to\" start=0 duration=1 to=10 ease=\"fast\"   // unknown easing",
        fix: "Set `ease=` to one of the nineteen named curves (e.g. `linear`, `in-out-sine`), `ease=\"out-back(1.7)\"` with a numeric overshoot, or a full-curve spelling: `cubic-bezier(x1,y1,x2,y2)`, `steps(n)`, `spring` or `spring(stiffness,damping)`, `bounce`, `elastic`, `wiggle` or `wiggle(n)`, or `hold`.",
    },
    CatalogEntry {
        code: "E_BAD_FIT",
        meaning: "A video `fit` is not exactly one of `contain`, `cover`, or `fill`.",
        example: "video path=\"media/clip.mp4\" start=0 duration=2 fit=\"zoom\"",
        fix: "Set `fit=\"contain\"`, `fit=\"cover\"`, or `fit=\"fill\"`.",
    },
    CatalogEntry {
        code: "E_BAD_FONT",
        meaning: "A declared font file is not a supported font format, or a text `font-family` names no usable family; the renderer draws only TrueType/OpenType fonts with `glyf` outlines and never substitutes a different font silently.",
        example: "font path=\"fonts/body.woff\"   // WOFF is a web format the renderer cannot draw",
        fix: "Point `path` at a TrueType (.ttf) or OpenType (.otf) font with TrueType outlines, or remove the `font` to use the host system font (bitmap fallback only when the machine has none).",
    },
    CatalogEntry {
        code: "E_BAD_FOOTAGE",
        meaning: "A `video` clip names unusable footage intent: a backward or empty trim window, a `trim-start` at or past the source end, a `rate` outside 0.1 to 10, a non-boolean `loop`, a `loop=true` clip or infinitely looping GIF without an explicit `duration` (an unbounded length), or an omitted `duration` with no knowable source end to infer from.",
        example: "video path=\"clip.mp4\" start=0 duration=2 trim-start=3 trim-end=1 rate=20   // backward window, rate above the ceiling",
        fix: "Keep `trim-start`/`trim-end` ordered seconds from 0 with `trim-start` below the source end, `rate` from 0.1 to 10 (`rate=2` is double speed, `rate=0.5` is half speed), `loop` as `true`/`false`, every looping clip and infinite GIF carrying an explicit `duration`, and every duration-less clip offering a knowable source end (or its own `trim-end`).",
    },
    CatalogEntry {
        code: "E_BAD_FPS",
        meaning: "The composition `fps` is not an integer or exact rational within 1 to 240; decimal fps is rejected, never rounded.",
        example: "composition name=\"t\" width=64 height=64 duration=1 fps=29.97 background=\"#000\" version=1 { }",
        fix: "Use an integer like `fps=30` or an exact rational like `fps=\"30000/1001\"` within 1 to 240.",
    },
    CatalogEntry {
        code: "E_BAD_HIDDEN",
        meaning: "A clip `hidden` switch is not `true` or `false`; hidden clips are still validated, never guessed.",
        example: "text value=\"Hi\" start=0 duration=1 hidden=\"yes\"   // not a boolean",
        fix: "Set `hidden=true` to hide the clip, or `hidden=false` (or remove the key) to show it.",
    },
    CatalogEntry {
        code: "E_BAD_ID",
        meaning: "A clip or composition `id` is not 1-64 letters, digits, `-` or `_`; a `start-after` names the clip's own id; or a value that must be a quoted id is not one.",
        example: "text value=\"Hi\" start=0 duration=1 id=\"has space\"   // spaces are not id characters",
        fix: "Set `id` to 1-64 letters, digits, `-` or `_` (e.g. `id=\"title-1\"`); point `start-after` at a different declared id.",
    },
    CatalogEntry {
        code: "E_BAD_IMAGE",
        meaning: "An image position, size, or placement is not a usable value, the file is corrupt or not a PNG/JPEG/still-GIF/HEIC picture, or the file is an animated GIF (animations play in a `video` clip, not `image`).",
        example: "image path=\"media/photo.png\" start=0 duration=2 placement=\"north\"   // unknown placement",
        fix: "Use numbers for `x`, `y`, `width`, and `height`, a placement like `placement=\"top-left\"`, an undamaged PNG, JPEG, still GIF, or HEIC file — and a `video` clip for animated GIFs.",
    },
    CatalogEntry {
        code: "E_BAD_LABEL",
        meaning: "A clip `label` is not a quoted editor note.",
        example: "text value=\"Hi\" start=0 duration=1 label=5   // labels must be quoted strings",
        fix: "Set `label=\"...\"` to a quoted editor note, or remove the key; labels never change rendered output.",
    },
    CatalogEntry {
        code: "E_BAD_MOTION",
        meaning: "A `tween`, `set`, or `key` names no target clip, names an unusable property (or a property its clip kind does not carry), uses an unknown form, carries a missing, mistimed, or unreadable window or value, or (for `key`) chains fewer than two stops, unordered stops, or duplicate stop times; or a visual clip carries an unreadable 3D turn (`rotation-x`/`rotation-y` degrees, a non-positive `perspective`, an unknown `origin`) or a malformed `filter`.",
        example: "tween property=\"x\" kind=\"to\" start=0 duration=1 to=10   // missing target",
        fix: "Name a clip with `target=<index from 0>`, a property with `property=\"x\"`, `\"y\"`, `\"scale\"`, `\"rotation\"`, `\"opacity\"`, `\"rotation-x\"`, `\"rotation-y\"`, `\"width\"`, `\"height\"`, `\"fill\"`, `\"radius\"`, `\"border-width\"`, `\"border-color\"`, `\"blur\"`, `\"font-size\"`, or `\"letter-spacing\"`, a form with `kind=\"to\"`, `\"from\"`, or `\"from-to\"`, an absolute `start` with a positive `duration`, and the values the form needs (`to`, `from`, or both; `value` for `set`; hex colors for `fill`/`border-color`). Chain keyframes as `key` elements with at least two `frame at=<seconds> value=<number>` stops in rising time order. Keep `rotation-x`/`rotation-y` to finite degrees, `perspective` to positive pixels (or omit it for flat), `origin` to a placement like `\"center\"` or `\"top-left\"`, and `filter` to `\"none\"` or `\"blur(N)\"`.",
    },
    CatalogEntry {
        code: "E_BAD_ORDER",
        meaning: "A clip paint `order` is not a whole number; the system never guesses a stacking arrangement.",
        example: "text value=\"Hi\" start=0 duration=1 order=\"front\"   // not a number",
        fix: "Set `order=<integer>` (for example `order=1`); clips with equal or absent orders paint in document order.",
    },
    CatalogEntry {
        code: "E_BAD_SPACING",
        meaning: "A text `line-height` or `letter-spacing` is not a non-negative number of canvas pixels.",
        example: "text value=\"Hi\" start=0 duration=1 letter-spacing=-2   // negative spacing",
        fix: "Set `line-height=<pixels >= 0>` and `letter-spacing=<pixels >= 0>`, or remove the key for the font default.",
    },
    CatalogEntry {
        code: "E_BAD_TIMING",
        meaning: "A clip start, duration, or relative start is unusable: a missing or negative absolute start, a non-positive duration, a written or resolved length below the 1/60s minimum, an `end=` key, an absolute `start` combined with `start-after` on one clip, a `start-after` cycle or an effective start before zero, a non-numeric `start-offset` (or one without `start-after`), an unbounded `nest` with no written `duration`, a `nest` trim/rate problem (a backward or empty window, a trim below zero, a `rate` outside 0.1 to 10, or a `trim-start` at or past the child end), a circular `nest` chain, or a `nest` chain deeper than 8 levels.",
        example: "text value=\"Hi\" duration=2   // missing start",
        fix: "Declare an absolute `start=<seconds >= 0>` on every clip; write `duration=<seconds > 0>` (at least 1/60s) on every clip but `video`, `audio`, and the stills (which may omit `duration` to infer it) and on every `nest`; use `start-after` without `start`, keep chains cycle-free and at zero or later, pair `start-offset` with a numeric seconds value, and keep `nest` windows forward inside the child timeline with `rate` from 0.1 to 10, chains cycle-free, and at most 8 levels deep.",
    },
    CatalogEntry {
        code: "E_BAD_TRACK",
        meaning: "A clip `track` is not exactly one of `video`, `audio`, `caption`, or `graphics`. Undeclared tracks default by kind (video to video, audio to audio, everything else to graphics).",
        example: "text value=\"Hi\" start=0 duration=1 track=\"titles\"   // unknown lane",
        fix: "Set `track` to `video`, `audio`, `caption`, or `graphics`, or remove the key for the kind default; tracks never change rendered output.",
    },
    CatalogEntry {
        code: "E_BAD_VERSION",
        meaning: "The notation `version` is missing or is an integer below 1.",
        example: "composition name=\"t\" width=64 height=64 duration=1 background=\"#000\" version=0 { }",
        fix: "Set `version=1` on the composition root.",
    },
    CatalogEntry {
        code: "E_DOWNLOAD_FAILED",
        meaning: "The pinned FFmpeg self-download failed: the transfer was interrupted, the SHA256 did not match, or the downloaded binary failed verification.",
        example: "render refuses: could not self-provide FFmpeg: checksum mismatch for the pinned build ...",
        fix: "Re-run the render to retry the download; if the failure persists, install FFmpeg manually (see the pinned source in the refusal) and re-run.",
    },
    CatalogEntry {
        code: "E_DUP_ID",
        meaning: "An `id` is declared more than once in the project; ids must be unique across the whole project (clips and compositions share one namespace).",
        example: "text value=\"A\" start=0 duration=1 id=\"t\"  text value=\"B\" start=1 duration=1 id=\"t\"   // reused id",
        fix: "Rename all but one `id=\"t\"` to unique ids so every clip and composition has its own name.",
    },
    CatalogEntry {
        code: "E_MISSING_COMPOSITION",
        meaning: "The project folder has no readable `composition.kdl`.",
        example: "(empty folder) $ framesmith check demo  ->  E_MISSING_COMPOSITION",
        fix: "Create the composition with `framesmith init <name>` or restore `composition.kdl` in the folder.",
    },
    CatalogEntry {
        code: "E_MISSING_DIR",
        meaning: "The folder passed to `check` or `render` does not exist.",
        example: "$ framesmith check ghost  ->  E_MISSING_DIR",
        fix: "Point the command at an existing folder, or create one with `framesmith init <name>`.",
    },
    CatalogEntry {
        code: "E_MISSING_FIELD",
        meaning: "A required field is absent: composition `name`, `background`, `version`, clip `value`/`path`, and similar.",
        example: "composition width=64 height=64 duration=1 background=\"#000\" version=1 { }   // missing name",
        fix: "Add the named field, e.g. `name=\"...\"` on the composition root or `value=\"...\"` on a text clip.",
    },
    CatalogEntry {
        code: "E_MISSING_FILE",
        meaning: "A referenced media or font file does not exist or is not readable at the declared path.",
        example: "video path=\"media/gone.mp4\" start=0 duration=2   // file absent",
        fix: "Provide the file at the referenced path relative to the project folder, or fix the `path`.",
    },
    CatalogEntry {
        code: "E_NO_FFMPEG",
        meaning: "No suitable FFmpeg is available and the pinned self-download is disabled or offline, so rendering refuses before the first frame.",
        example: "render refuses: no suitable FFmpeg ... Self-download is disabled by framesmith.toml ...",
        fix: "Install FFmpeg 6+ with libx264 and aac encoders on PATH, or re-enable downloading (`ffmpeg_auto_download = true`).",
    },
    CatalogEntry {
        code: "E_PARSE",
        meaning: "The KDL notation or a `framesmith.toml` config line is malformed at the reported location.",
        example: "composition name=\"t\" width=64 height=64 duration=1 background=\"#000\" version=1 {   // unclosed brace",
        fix: "Fix the notation syntax at the reported line and column; config lines must be `key = value`.",
    },
    CatalogEntry {
        code: "E_UNKNOWN_ELEMENT",
        meaning: "A top-level node is not `composition`, or a child element is not a known element name.",
        example: "composition name=\"t\" ... { }  subtitle value=\"Hi\" start=0 duration=1",
        fix: "Keep a single `composition` root and use known elements (`text`, `font`, `video` in Sprint 1).",
    },
    CatalogEntry {
        code: "E_UNKNOWN_KEY",
        meaning: "A key is not part of the notation (composition, clip, or config keys); unknown keys are errors, never ignored.",
        example: "composition name=\"t\" width=64 height=64 duration=1 bogus=1 background=\"#000\" version=1 { }",
        fix: "Remove the key or check the spelling against the documented keys.",
    },
    CatalogEntry {
        code: "E_UNKNOWN_REF",
        meaning: "A `start-after` or `nest` reference names no declared id, `mark`, or composition in the project, or a `nest` names a clip id instead of a composition.",
        example: "text value=\"B\" start=0 duration=1 start-after=\"ghost\"   // no clip, mark, or composition is called `ghost`",
        fix: "Declare `id=\"ghost\"` on the intended clip or composition, declare a `mark` named `ghost`, fix the spelling, or point `nest` at a composition `name`.",
    },
    CatalogEntry {
        code: "E_UNREADABLE",
        meaning: "A required file exists but cannot be read (permissions or I/O error).",
        example: "$ chmod 000 composition.kdl; framesmith check demo  ->  E_UNREADABLE",
        fix: "Make the file readable, or remove it and restore a readable copy.",
    },
    CatalogEntry {
        code: "E_UNSUPPORTED",
        meaning: "The element, property, key, easing, or look is valid notation but needs a later sprint, so it cannot render yet (audio effects, automation, groups, and ducking need Sprint 10; later motion properties and easings need Sprint 7; rich box looks need Sprint 8; hardware encode needs Sprint 25).",
        example: "audio path=\"s.wav\" start=0 duration=1 effect=\"reverb\"   // valid shape, Sprint 10 behavior",
        fix: "Remove the element or key for the current sprint, or wait for the named sprint; `check` exits 2 and `render` refuses before the first frame.",
    },
];

/// Look up one code. Returns `None` for unknown codes.
pub fn lookup(code: &str) -> Option<&'static CatalogEntry> {
    CATALOG.iter().find(|e| e.code == code)
}

/// Render one entry as human text.
pub fn render_entry(e: &CatalogEntry) -> String {
    format!(
        "{code}\n  meaning: {meaning}\n  example: {example}\n  fix: {fix}\n",
        code = e.code,
        meaning = e.meaning,
        example = e.example,
        fix = e.fix
    )
}

/// Escape a string for JSON output.
pub fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// Every code the validator can emit, plus the bootstrap codes.
    /// Keep in sync with `grep -rho '"E_[A-Z_]*"' src/`.
    const KNOWN_EMITTED: &[&str] = &[
        "E_AMBIGUOUS_ROOT",
        "E_BAD_AUDIO",
        "E_BAD_BOX",
        "E_BAD_CANVAS",
        "E_BAD_COLOR",
        "E_BAD_CONFIG",
        "E_BAD_DURATION",
        "E_BAD_EASING",
        "E_BAD_FIT",
        "E_BAD_FONT",
        "E_BAD_FOOTAGE",
        "E_BAD_FPS",
        "E_BAD_HIDDEN",
        "E_BAD_ID",
        "E_BAD_IMAGE",
        "E_BAD_LABEL",
        "E_BAD_MOTION",
        "E_BAD_ORDER",
        "E_BAD_SPACING",
        "E_BAD_TIMING",
        "E_BAD_TRACK",
        "E_BAD_VERSION",
        "E_DOWNLOAD_FAILED",
        "E_DUP_ID",
        "E_MISSING_COMPOSITION",
        "E_MISSING_DIR",
        "E_MISSING_FIELD",
        "E_MISSING_FILE",
        "E_NO_FFMPEG",
        "E_PARSE",
        "E_UNKNOWN_ELEMENT",
        "E_UNKNOWN_KEY",
        "E_UNKNOWN_REF",
        "E_UNREADABLE",
        "E_UNSUPPORTED",
    ];

    #[test]
    fn catalog_covers_every_emitted_code() {
        let cataloged: HashSet<&str> = CATALOG.iter().map(|e| e.code).collect();
        for code in KNOWN_EMITTED {
            assert!(cataloged.contains(code), "catalog is missing {code}");
        }
        assert_eq!(
            cataloged.len(),
            KNOWN_EMITTED.len(),
            "catalog has extra codes beyond the emitted set"
        );
    }

    #[test]
    fn entries_are_complete_and_ordered() {
        let mut prev = "";
        let mut seen = HashSet::new();
        for e in CATALOG {
            assert!(!e.meaning.is_empty(), "{} needs a meaning", e.code);
            assert!(!e.example.is_empty(), "{} needs an example", e.code);
            assert!(!e.fix.is_empty(), "{} needs a fix direction", e.code);
            assert!(seen.insert(e.code), "duplicate catalog entry {}", e.code);
            assert!(e.code > prev, "catalog must stay sorted for stable output");
            prev = e.code;
        }
    }

    #[test]
    fn lookup_round_trips() {
        assert_eq!(lookup("E_PARSE").unwrap().code, "E_PARSE");
        assert!(lookup("E_BOGUS").is_none());
        assert!(lookup("").is_none());
    }

    #[test]
    fn json_escape_shapes() {
        assert_eq!(json_escape("a\"b\\c"), "a\\\"b\\\\c");
        assert_eq!(json_escape("x\ny\tz"), "x\\ny\\tz");
        assert_eq!(json_escape("é"), "é");
        assert_eq!(json_escape("\u{1}"), "\\u0001");
    }
}
