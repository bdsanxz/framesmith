//! Agent skill installer (Sprint 20, T001 / CAP-001, J-01, REC-001).
//!
//! Framesmith ships its own clean-room skill files and installs them
//! into each supported agent host's skill location. The shipped bytes
//! are baked into this binary; `check` compares the hashed manifest
//! (SHA256 over every shipped file) against the host directory and
//! names drifted, missing, and extra files, reporting zero drift
//! explicitly on a clean install. `update` restores files in scope
//! (all, the core set, or one named skill) and only overwrites a
//! drifted file with the maker's say-so (`--yes`).
//!
//! Boundary: the installer writes skill locations only, never project
//! content, and never touches the Sprint 15 patch/history machinery
//! (no staged receipts: there is no project edit to stage). Default
//! renders are unaffected: nothing here reads or writes compositions.
//!
//! The full T002 set lives here: 11 domain skills and 10 creation
//! workflow documents, every one validated against its own schema by
//! `skills validate`, plus the installer, the manifest, and the host
//! targets. Install still ships the core set only; `update --all`
//! pulls in the extended set.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// One supported agent host and its default skill location.
#[derive(Debug, Clone, Copy)]
pub struct Host {
    /// `--host` name (exact, case-sensitive).
    pub name: &'static str,
    /// One-line description of the default location for listings.
    pub blurb: &'static str,
    /// Default skill directory relative to the home directory.
    /// A second `true` element means the path goes under
    /// `$HOME/.config` instead of directly under `$HOME`.
    pub suffix: &'static str,
}

/// Every supported host, in listing order.
pub const HOSTS: &[Host] = &[
    Host {
        name: "claude-code",
        blurb: "Claude Code skills directory (~/.claude/skills)",
        suffix: ".claude/skills",
    },
    Host {
        name: "codex",
        blurb: "Codex skills directory (~/.codex/skills)",
        suffix: ".codex/skills",
    },
    Host {
        name: "cursor",
        blurb: "Cursor skills directory (~/.cursor/skills)",
        suffix: ".cursor/skills",
    },
    Host {
        name: "gemini",
        blurb: "Gemini CLI skills directory (~/.gemini/skills)",
        suffix: ".gemini/skills",
    },
    Host {
        name: "copilot",
        blurb: "Copilot skills directory (~/.copilot/skills)",
        suffix: ".copilot/skills",
    },
    Host {
        name: "opencode",
        blurb: "OpenCode skills directory (~/.config/opencode/skills)",
        suffix: ".config/opencode/skills",
    },
];

/// Comma-separated host list for refusal messages.
pub fn host_list_line() -> String {
    HOSTS
        .iter()
        .map(|h| h.name.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Look up a host by exact name.
pub fn find_host(name: &str) -> Option<&'static Host> {
    HOSTS.iter().find(|h| h.name == name)
}

/// One shipped skill: its file set, purpose, and core membership.
#[derive(Debug, Clone, Copy)]
pub struct Skill {
    /// Skill directory name and `--skill` selector (exact).
    pub name: &'static str,
    /// One-line purpose shown by `skills list`.
    pub purpose: &'static str,
    /// Whether `install` and `update --core` cover this skill.
    pub core: bool,
    /// Domain skills teach one area; workflows walk a whole video.
    pub kind: SkillKind,
    /// Shipped files as `(relative path, bytes)`.
    pub files: &'static [(&'static str, &'static str)],
}

/// The two shapes a shipped skill document takes. The shape selects
/// the schema `validate_skill_body` applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillKind {
    Domain,
    Workflow,
}

// --- Shipped skill documents -------------------------------------------
// All text below is framesmith's own clean-room writing. It describes
// only commands this build actually ships and never promises
// unbuilt capabilities (no hosted generation, narration, avatars,
// or third-party catalogs).

const ROUTER_SKILL: &str = "# Framesmith router skill\n\nPurpose: read the request and project state, then name the workflow and its gates\n\nUse this skill when the maker asks for a video in plain words.\n\nDo:\n- Read the project folder first (`compositions`, `timeline`, `info`).\n- Ask one question per must-have before proposing anything.\n- Name the chosen workflow and its gates before building.\n- Install a creation workflow only when the request needs one;\n  the general-video workflow plus a Not-sure path covers the rest.\n- Stop at any step that needs a capability this build lacks and name\n  the missing capability (never simulate it).\n\nDo not:\n- Render or animate before the maker approves the storyboard.\n- Guess an ambiguous request; offer the general path plus a Not-sure\n  path instead.\n- Shell out to other video tools; domain skills describe framesmith\n  behavior only.\n\nReference map (every other skill; start here, then load one):\n- Domains: core, animation, keyframes, creative, audio, cli,\n  registry, studio, media, design-import.\n- Workflows: product-launch, faceless-explainer, pr-to-video,\n  embedded-captions, talking-head-recut, motion-graphics,\n  music-to-video, slideshow, general-video, migration.\n\nCommands: `compositions`, `timeline`, `info`, `check`, `history`.\n";

const CORE_SKILL: &str = "# Framesmith core skill\n\nPurpose: project shape, KDL vocabulary, and the check-first habit\n\nUse this skill when any edit touches a framesmith project.\n\nDo:\n- Read the project's agent guide (`AGENTS.md` or `CLAUDE.md`) first.\n- Keep style attributes CSS-like and hyphenated\n  (`background-color`, `border-radius`, `border-width`, `object-fit`,\n  `object-position`, `line-height`, `letter-spacing`).\n- Run `check` before and after every edit; fix diagnostics by their\n  file, line, column, code, and fix.\n- Prefer `timeline` verbs for clip edits so history stays undoable.\n\nDo not:\n- Hand-edit files that `timeline` verbs own while a turn is open.\n- Promise commands outside this build; see the CLI skill for the\n  exact command list.\n\nCommands: `check`, `lint`, `timeline`, `info`, `errors`.\n";

const CLI_SKILL: &str = "# Framesmith CLI skill\n\nPurpose: every framesmith command with its flags, exits, and refusal shapes\n\nUse this skill when maker intent must become an exact invocation.\n\nAvailable commands: `init`, `check`, `lint`, `errors`,\n`compositions`, `timeline`, `info`, `waveform`, `history`, `catalog`,\n`preview`, `play`, `preview-status`, `preview-stop`, `preview-list`,\n`preview-stop-all`, `preview-serve`, `snapshot`, `keyframes`,\n`compare`, `grade-compare`, `grade-bake`, `render`, `doctor`,\n`benchmark`, `route`, `storyboard` (`board`, `approve`, `status`), `skills`\n(`list`, `install`, `check`, `update`, `validate`).\n\nDo:\n- Quote the refusal (exit 1 names what is missing; exit 2 names the\n  usage mistake) and propose the exact fix.\n- Use `--json` when piping results into another step.\n\nDo not:\n- Invent flags or subcommands not listed above.\n- Claim behavior this build does not ship; offline-first here means\n  no hosted generation, no accounts, and no third-party catalogs.\n\nCommands: all of the above, via the `framesmith` binary. Route first (`route --request <text>` interviews, pitches five concepts, and names the workflow) before building.\n";

const STUDIO_SKILL: &str = "# Framesmith Studio skill\n\nPurpose: preview and Studio surfaces for review, selection, and staged edits\n\nUse this skill when the maker works in the Studio review page.\n\nDo:\n- Read context first: composition, time, and selected elements.\n- Treat an empty selection as explicitly empty, never as the whole\n  timeline.\n- Stage one edit at a time and report the receipt naming what will\n  apply; the maker undoes turns from shared history (`history undo`).\n- Through Studio, use the twelve staged tools: `look`, `inspect`,\n  `frame`, `select`, `seek`, `set-text`, `set-style`, `transform`,\n  `add-animation`, `update-animation`, `add-keyframe`,\n  `delete-animation`.\n- Approve posed stills before motion through the stills toggle\n  (`POST /api/studio/stills/approve`); motion and render refuse\n  while the board awaits approval.\n\nDo not:\n- Apply an edit that names a missing selection or composition;\n  refuse naming the missing target and change nothing.\n- Render before preview approval.\n\nCommands: `preview`, `play`, `snapshot`, `history`, `timeline`.\n";

const ANIMATION_SKILL: &str = "# Framesmith animation skill\n\nPurpose: motion, keyframes, stagger, and timing over the timeline.\n\nUse this skill when clips must move, ease, or stagger.\n\nDo:\n- List computed motion with `keyframes --clip <id> --property <name>`\n  before tuning it.\n- Keep stagger overflows clamped to the target clip window; the\n  `check` notes name every clamp.\n- Validate with `check` after retiming; past-end trims clamp to the\n  source end.\n\nDo not:\n- Animate before storyboard approval in a creation workflow.\n- Promise motion presets this build does not ship; see `errors` for\n  the exact diagnostic catalog.\n\nCommands: `keyframes`, `timeline`, `check`, `snapshot`, `info`.\n";

const AUDIO_SKILL: &str = "# Framesmith audio skill\n\nPurpose: sound clips, mixes, peaks, and loudness gates.\n\nUse this skill when the project carries sound.\n\nDo:\n- Inspect the mix with `waveform` (per-bucket peaks plus the master\n  peak) before release decisions.\n- Respect the loudness gates: silent normalizing mixes fail `check`\n  naming the composition, and hot masters warn above -1.0 dBFS.\n- Keep crossfade overlaps and effect tails inside the composition;\n  `check` names every clamp.\n\nDo not:\n- Promise narration, voice cloning, or hosted media; name the missing\n  capability and stop at that step.\n- Ship a hot master without telling the maker the peak.\n\nCommands: `waveform`, `check`, `info`, `timeline`, `render`.\n";

const KEYFRAMES_SKILL: &str = "# Framesmith keyframes skill\n\nPurpose: computed motion readouts and retiming rules per clip and property\n\nUse this skill when motion values must be read, compared, or retimed.\n\nDo:\n- List computed motion first: `keyframes --clip <id> --property <name>`.\n- Retiming edits go through `timeline` verbs so history stays undoable.\n- Re-run `keyframes` after a retime; past-end trims clamp to the\n  source end and `check` names the clamp.\n\nDo not:\n- Animate before storyboard approval in a creation workflow.\n- Invent motion presets; this build computes motion from clips only.\n- Hand back anything outside computed motion to the router skill\n  instead of improvising.\n\nCommands: `keyframes`, `timeline`, `check`, `snapshot`, `info`.\n";

const CREATIVE_SKILL: &str = "# Framesmith creative skill\n\nPurpose: look, palette, and paint choices that stay inside the offline catalog\n\nUse this skill when the maker asks for a look, a palette, or a grade.\n\nDo:\n- Build the look from paint the build can draw: fills, borders,\n  opacity, and hyphenated style attributes.\n- Compare stills with `compare` and grades with `grade-compare`;\n  bake a final look with `grade-bake`.\n- Theme from the project's own variables (`render --theme`,\n  `check --theme`); keep every value local.\n\nDo not:\n- Promise generated imagery or third-party looks; this build draws\n  from its own catalog and variables only.\n- Hand back anything outside look and paint to the router skill\n  instead of improvising.\n\nCommands: `compare`, `grade-compare`, `grade-bake`, `render`, `check`.\n";

const REGISTRY_SKILL: &str = "# Framesmith registry skill\n\nPurpose: bundled and custom catalog shelves, gated installs, and consent-gated meaning search\n\nUse this skill when the maker wants a shelf piece or a catalog lookup.\n\nDo:\n- Search offline first: `catalog search <words>` names bundled pieces.\n- Install gated and pinned: `catalog install <project> <name>` records\n  the manifest hash; dependency order is enforced.\n- Meaning search only with consent: `catalog meaning-search <words>`\n  asks first, and `catalog meaning-revoke` withdraws it.\n\nDo not:\n- Promise hosted registries or account-backed catalogs; installs here\n  are offline and manifest-pinned.\n- Hand back anything outside the catalog to the router skill instead\n  of improvising.\n\nCommands: `catalog`, `info`, `compositions`, `check`.\n";

const MEDIA_SKILL: &str = "# Framesmith media skill\n\nPurpose: footage windows, loops, trims, and container rules for video and stills\n\nUse this skill when a clip carries a `video` source or stills.\n\nDo:\n- Read the effective window first: source start and end, inferred\n  length when the source end is unknown, and the clamp note when a\n  past-end trim is cut back.\n- Keep loops and crossfade overlaps inside the composition; `check`\n  names every clamp.\n- Review frames with `snapshot` before approving the cut.\n\nDo not:\n- Promise narration, hosted footage, or container conversions this\n  build does not ship; name the missing capability and stop.\n- Hand back anything outside footage handling to the router skill\n  instead of improvising.\n\nCommands: `check`, `info`, `timeline`, `snapshot`, `waveform`.\n";

const DESIGN_IMPORT_SKILL: &str = "# Framesmith design-import skill\n\nPurpose: outside design files mapped onto native KDL boxes without unbuilt promises\n\nUse this skill when the maker brings an outside design file.\n\nDo:\n- Read the outside file's boxes and text, then rebuild them as native\n  KDL `box` and `text` clips with hyphenated style attributes.\n- Validate the rebuild with `check`; review it with `snapshot`.\n- Keep every value local: no external `path` or `src` references.\n\nDo not:\n- Promise one-click importers or hosted conversion; the mapping is\n  manual and native-only.\n- Hand back anything outside the mapping to the router skill instead\n  of improvising.\n\nCommands: `check`, `snapshot`, `info`, `timeline`, `compositions`.\n";

const PRODUCT_LAUNCH_SKILL: &str = "# Framesmith product-launch workflow\n\nPurpose: a launch film from brief to approved storyboard to rendered MP4\n\nInput: maker brief (product, audience, one promise), optional starter.\nOutput: one approved MP4 plus its locked storyboard.\n\nSteps:\n- Interview: one question per must-have, then a five-concept pitch.\n- Board: one board of stills; the film stays unchanged until approval.\n- Build: staged edits with receipts, each turn undoable by the maker.\n- Preview approval, then `render` once.\n\nCapabilities: `compositions`, `timeline`, `keyframes`, `check`,\n`snapshot`, `compare`, `waveform`, `history`, `render`.\n\nGates: storyboard approval before any motion; preview approval\nbefore `render`.\n\nDeferred: narration and voice steps wait for the caption sprint\n(not in this build); hosted distribution is excluded, never simulated.\n\nStop: when a step needs a capability this build lacks (here narration\nand voice work), stop at that step naming the capability\nand its sprint (the caption sprint); never simulate the missing\nstep. Everything before the stop stays valid and resumable.\n";

const FACELESS_EXPLAINER_SKILL: &str = "# Framesmith faceless-explainer workflow\n\nPurpose: a voice-free explainer from outline to captioned motion\n\nInput: outline of points, palette, duration budget.\nOutput: one approved MP4 with timed caption boxes.\n\nSteps:\n- Interview: one question per must-have, then a five-concept pitch.\n- Board: one board of stills; the film stays unchanged until approval.\n- Build: boxes and `text` clips only, timed to the outline.\n- Preview approval, then `render` once.\n\nCapabilities: `compositions`, `timeline`, `keyframes`, `check`,\n`snapshot`, `waveform`, `history`, `render`.\n\nGates: storyboard approval before any motion; preview approval\nbefore `render`.\n\nDeferred: spoken narration waits for the caption sprint (not in this\nbuild); captions here are timed text boxes, never synthesized speech.\n\nStop: when a step needs a capability this build lacks (here spoken\nnarration), stop at that step naming the capability and its sprint\n(the caption sprint); never simulate the missing step. Everything\nbefore the stop stays valid and resumable.\n";

const PR_TO_VIDEO_SKILL: &str = "# Framesmith pr-to-video workflow\n\nPurpose: a pull-request summary video from diff notes to approved stills\n\nInput: diff notes or change list, audience (reviewers or makers).\nOutput: one approved MP4 summarizing the change.\n\nSteps:\n- Interview: one question per must-have, then a five-concept pitch.\n- Board: one board of stills; the film stays unchanged until approval.\n- Build: title card, change beats as text boxes, closing card.\n- Preview approval, then `render` once.\n\nCapabilities: `compositions`, `timeline`, `check`, `snapshot`,\n`history`, `render`.\n\nGates: storyboard approval before any motion; preview approval\nbefore `render`.\n\nDeferred: narration waits for the caption sprint (not in this build);\nhosted publishing is excluded, never simulated.\n\nStop: when a step needs a capability this build lacks (here narration\nand hosted publishing), stop at that step naming the\ncapability and its sprint (the caption sprint for narration;\npublishing stays excluded); never simulate the missing step.\nEverything before the stop stays valid and resumable.\n";

const EMBEDDED_CAPTIONS_SKILL: &str = "# Framesmith embedded-captions workflow\n\nPurpose: a captioned cut with timed caption boxes held for readability\n\nInput: locked cut plus caption lines with times.\nOutput: one approved MP4 with burned-in caption boxes.\n\nSteps:\n- Interview: one question per must-have (language, line length).\n- Board: captioned stills showing placement and hold times.\n- Build: `text` caption boxes timed to the lines, held for\n  readability, clamped to the composition.\n- Preview approval, then `render` once.\n\nCapabilities: `compositions`, `timeline`, `check`, `snapshot`,\n`history`, `render`.\n\nGates: captioned-stills approval before timing passes; preview\napproval before `render`.\n\nDeferred: speech-to-text waits for the caption sprint (not in this\nbuild); lines here are maker-supplied, never transcribed.\n\nStop: when a step needs a capability this build lacks (here speech-to-text), stop at that step naming the capability and its\nsprint (the caption sprint); never simulate the missing step.\nEverything before the stop stays valid and resumable.\n";

const TALKING_HEAD_RECUT_SKILL: &str = "# Framesmith talking-head-recut workflow\n\nPurpose: a trimmed talking-head cut with silent gaps removed and peaks safe\n\nInput: talking-head footage plus keep and cut notes.\nOutput: one approved MP4, tighter and loudness-safe.\n\nSteps:\n- Interview: one question per must-have (keep list, target length).\n- Board: before and after stills of the kept beats.\n- Build: `timeline split`, `trim`, and `delete` on the gaps; check\n  the mix with `waveform` and keep the master peak at or under\n  -1.0 dBFS.\n- Preview approval, then `render` once.\n\nCapabilities: `timeline`, `waveform`, `check`, `snapshot`, `history`,\n`render`.\n\nGates: stills approval before cutting; preview approval before\n`render`.\n\nDeferred: automatic silence detection waits for the ML sprints (not\nin this build); gaps here are cut by maker notes, never detected.\n\nStop: when a step needs a capability this build lacks (here automatic\nsilence detection), stop at that step naming the\ncapability and its sprint (an ML sprint); never simulate the missing\nstep. Everything before the stop stays valid and resumable.\n";

const MOTION_GRAPHICS_SKILL: &str = "# Framesmith motion-graphics workflow\n\nPurpose: a motion-graphics piece from keyframed boxes to preview approval\n\nInput: beat list or script, palette, duration budget.\nOutput: one approved MP4 of keyframed boxes and text.\n\nSteps:\n- Interview: one question per must-have, then a five-concept pitch.\n- Board: one board of stills; the film stays unchanged until approval.\n- Build: keyframed `box` and `text` clips; read computed motion with\n  `keyframes` and clamp stagger overflows to the clip window.\n- Preview approval, then `render` once.\n\nCapabilities: `compositions`, `timeline`, `keyframes`, `check`,\n`snapshot`, `compare`, `history`, `render`.\n\nGates: storyboard approval before any motion; preview approval\nbefore `render`.\n\nDeferred: narration waits for the caption sprint (not in this build);\npreset packs from outside this build are excluded, never simulated.\n\nStop: when a step needs a capability this build lacks (here narration), stop at that step naming the capability and its sprint\n(the caption sprint); never simulate the missing step. Everything\nbefore the stop stays valid and resumable.\n";

const MUSIC_TO_VIDEO_SKILL: &str = "# Framesmith music-to-video workflow\n\nPurpose: a music-led cut with beats marking edits and loudness gates held\n\nInput: music track plus edit beats, duration budget.\nOutput: one approved MP4 cut to the music.\n\nSteps:\n- Interview: one question per must-have (track, beats, target length).\n- Board: one board of stills on the beat marks.\n- Build: cuts land on the marks; keep crossfades inside the\n  composition and the master peak at or under -1.0 dBFS (`waveform`).\n- Preview approval, then `render` once.\n\nCapabilities: `waveform`, `timeline`, `check`, `snapshot`, `history`,\n`render`.\n\nGates: storyboard approval before any motion; preview approval\nbefore `render`.\n\nDeferred: beat detection waits for the ML sprints (not in this\nbuild); marks here are maker-supplied, never detected.\n\nStop: when a step needs a capability this build lacks (here beat\ndetection), stop at that step naming the capability and its sprint\n(an ML sprint); never simulate the missing step. Everything before\nthe stop stays valid and resumable.\n";

const SLIDESHOW_SKILL: &str = "# Framesmith slideshow workflow\n\nPurpose: a stills slideshow with clamped holds and safe crossfades\n\nInput: stills in order, hold times, palette.\nOutput: one approved MP4 holding each still, then moving on.\n\nSteps:\n- Interview: one question per must-have (order, holds, target length).\n- Board: one board of stills in final order.\n- Build: holds clamped to the clip window; crossfade overlaps kept\n  inside the composition (`check` names every clamp).\n- Preview approval, then `render` once.\n\nCapabilities: `compositions`, `timeline`, `check`, `snapshot`,\n`history`, `render`.\n\nGates: order approval before timing; preview approval before `render`.\n\nDeferred: narration waits for the caption sprint (not in this build);\nken-burns-style moves beyond keyframed boxes are excluded.\n\nStop: when a step needs a capability this build lacks (here narration), stop at that step naming the capability and its sprint\n(the caption sprint); never simulate the missing step. Everything\nbefore the stop stays valid and resumable.\n";

const GENERAL_VIDEO_SKILL: &str = "# Framesmith general-video workflow\n\nPurpose: the fallback path for any request no other workflow fits\n\nInput: any video request plus a Not-sure path when it stays ambiguous.\nOutput: one approved MP4, or a named stop when the build cannot do it.\n\nSteps:\n- Interview: one question per must-have; when the request stays\n  ambiguous, offer this workflow plus a Not-sure path, never a guess.\n- Board: one board of stills; the film stays unchanged until approval.\n- Build: the smallest staged edit set that answers the request.\n- Preview approval, then `render` once.\n\nCapabilities: `compositions`, `timeline`, `keyframes`, `check`,\n`snapshot`, `waveform`, `history`, `render`.\n\nGates: storyboard approval before any motion; preview approval\nbefore `render`.\n\nDeferred: whatever the request needs that this build lacks stops at\nthat step naming the capability and its sprint (not in this build),\nnever simulated.\n\nStop: when a step needs a capability this build lacks, stop at that\nstep naming the capability and the sprint that owns it (the caption\nsprint for voice work, an ML sprint for model work, a future sprint\notherwise); never simulate the missing step. Everything before the\nstop stays valid and resumable.\n";

const MIGRATION_SKILL: &str = "# Framesmith migration workflow\n\nPurpose: a foreign project mapped onto native KDL with an SSIM-checked still\n\nInput: a foreign project shape (boxes, text, timing) to leave behind.\nOutput: a native framesmith project plus an SSIM-checked still.\n\nSteps:\n- Interview: one question per must-have (sources, timing, palette).\n- Board: side-by-side stills of source shape and native rebuild.\n- Build: rebuild as native `box` and `text` clips with hyphenated\n  style attributes; no external references.\n- Check the rebuild still with `compare` against the source still;\n  preview approval, then `render` once.\n\nCapabilities: `compositions`, `timeline`, `check`, `snapshot`,\n`compare`, `history`, `render`.\n\nGates: side-by-side approval before timing passes; preview approval\nbefore `render`.\n\nDeferred: converter tooling waits for a future sprint (not in this\nbuild); the mapping here is manual, never simulated.\n\nStop: when a step needs a capability this build lacks (here converter\ntooling), stop at that step naming the capability and its\nsprint (a future sprint); never simulate the missing step.\nEverything before the stop stays valid and resumable.\n";

/// Every shipped skill, in listing order: the 11 domain skills, then
/// the 10 creation workflows. The installer and manifest follow this
/// table: `install` ships the core set, `update --all` pulls the rest.
pub const SKILLS: &[Skill] = &[
    Skill {
        name: "router",
        purpose: "read the request and project state, then name the workflow and its gates",
        core: true,
        kind: SkillKind::Domain,
        files: &[("router/SKILL.md", ROUTER_SKILL)],
    },
    Skill {
        name: "core",
        purpose: "project shape, KDL vocabulary, and the check-first habit",
        core: true,
        kind: SkillKind::Domain,
        files: &[("core/SKILL.md", CORE_SKILL)],
    },
    Skill {
        name: "animation",
        purpose: "motion, keyframes, stagger, and timing over the timeline",
        core: false,
        kind: SkillKind::Domain,
        files: &[("animation/SKILL.md", ANIMATION_SKILL)],
    },
    Skill {
        name: "keyframes",
        purpose: "computed motion readouts and retiming rules per clip and property",
        core: false,
        kind: SkillKind::Domain,
        files: &[("keyframes/SKILL.md", KEYFRAMES_SKILL)],
    },
    Skill {
        name: "creative",
        purpose: "look, palette, and paint choices that stay inside the offline catalog",
        core: false,
        kind: SkillKind::Domain,
        files: &[("creative/SKILL.md", CREATIVE_SKILL)],
    },
    Skill {
        name: "audio",
        purpose: "sound clips, mixes, peaks, and loudness gates",
        core: false,
        kind: SkillKind::Domain,
        files: &[("audio/SKILL.md", AUDIO_SKILL)],
    },
    Skill {
        name: "cli",
        purpose: "every framesmith command with its flags, exits, and refusal shapes",
        core: true,
        kind: SkillKind::Domain,
        files: &[("cli/SKILL.md", CLI_SKILL)],
    },
    Skill {
        name: "registry",
        purpose:
            "bundled and custom catalog shelves, gated installs, and consent-gated meaning search",
        core: false,
        kind: SkillKind::Domain,
        files: &[("registry/SKILL.md", REGISTRY_SKILL)],
    },
    Skill {
        name: "studio",
        purpose: "preview and Studio surfaces for review, selection, and staged edits",
        core: true,
        kind: SkillKind::Domain,
        files: &[("studio/SKILL.md", STUDIO_SKILL)],
    },
    Skill {
        name: "media",
        purpose: "footage windows, loops, trims, and container rules for video and stills",
        core: false,
        kind: SkillKind::Domain,
        files: &[("media/SKILL.md", MEDIA_SKILL)],
    },
    Skill {
        name: "design-import",
        purpose: "outside design files mapped onto native KDL boxes without unbuilt promises",
        core: false,
        kind: SkillKind::Domain,
        files: &[("design-import/SKILL.md", DESIGN_IMPORT_SKILL)],
    },
    Skill {
        name: "product-launch",
        purpose: "a launch film from brief to approved storyboard to rendered MP4",
        core: false,
        kind: SkillKind::Workflow,
        files: &[("product-launch/SKILL.md", PRODUCT_LAUNCH_SKILL)],
    },
    Skill {
        name: "faceless-explainer",
        purpose: "a voice-free explainer from outline to captioned motion",
        core: false,
        kind: SkillKind::Workflow,
        files: &[("faceless-explainer/SKILL.md", FACELESS_EXPLAINER_SKILL)],
    },
    Skill {
        name: "pr-to-video",
        purpose: "a pull-request summary video from diff notes to approved stills",
        core: false,
        kind: SkillKind::Workflow,
        files: &[("pr-to-video/SKILL.md", PR_TO_VIDEO_SKILL)],
    },
    Skill {
        name: "embedded-captions",
        purpose: "a captioned cut with timed caption boxes held for readability",
        core: false,
        kind: SkillKind::Workflow,
        files: &[("embedded-captions/SKILL.md", EMBEDDED_CAPTIONS_SKILL)],
    },
    Skill {
        name: "talking-head-recut",
        purpose: "a trimmed talking-head cut with silent gaps removed and peaks safe",
        core: false,
        kind: SkillKind::Workflow,
        files: &[("talking-head-recut/SKILL.md", TALKING_HEAD_RECUT_SKILL)],
    },
    Skill {
        name: "motion-graphics",
        purpose: "a motion-graphics piece from keyframed boxes to preview approval",
        core: false,
        kind: SkillKind::Workflow,
        files: &[("motion-graphics/SKILL.md", MOTION_GRAPHICS_SKILL)],
    },
    Skill {
        name: "music-to-video",
        purpose: "a music-led cut with beats marking edits and loudness gates held",
        core: false,
        kind: SkillKind::Workflow,
        files: &[("music-to-video/SKILL.md", MUSIC_TO_VIDEO_SKILL)],
    },
    Skill {
        name: "slideshow",
        purpose: "a stills slideshow with clamped holds and safe crossfades",
        core: false,
        kind: SkillKind::Workflow,
        files: &[("slideshow/SKILL.md", SLIDESHOW_SKILL)],
    },
    Skill {
        name: "general-video",
        purpose: "the fallback path for any request no other workflow fits",
        core: false,
        kind: SkillKind::Workflow,
        files: &[("general-video/SKILL.md", GENERAL_VIDEO_SKILL)],
    },
    Skill {
        name: "migration",
        purpose: "a foreign project mapped onto native KDL with an SSIM-checked still",
        core: false,
        kind: SkillKind::Workflow,
        files: &[("migration/SKILL.md", MIGRATION_SKILL)],
    },
];

/// Look up a skill by exact name.
pub fn find_skill(name: &str) -> Option<&'static Skill> {
    SKILLS.iter().find(|s| s.name == name)
}

/// Comma-separated skill list for refusal messages.
pub fn skill_list_line() -> String {
    SKILLS
        .iter()
        .map(|s| s.name.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

/// The hashed manifest: every shipped relative path to its SHA256 hex.
/// Computed from the baked bytes, so installs verify every written
/// file against the same digest `check` uses.
pub fn manifest_entries() -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for skill in SKILLS {
        for (rel, body) in skill.files {
            out.insert(rel.to_string(), catalog::sha256_hex(body.as_bytes()));
        }
    }
    out
}

/// Relative paths shipped for one update scope.
pub fn scope_paths(scope: &UpdateScope) -> Vec<(&'static str, &'static str)> {
    let mut out = Vec::new();
    for skill in SKILLS {
        let wanted = match scope {
            UpdateScope::All => true,
            UpdateScope::Core => skill.core,
            UpdateScope::One(name) => skill.name == *name,
        };
        if wanted {
            out.extend(skill.files.iter().copied());
        }
    }
    out
}

/// Update scope selected by exactly one of `--all`, `--core`,
/// `--skill <name>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateScope {
    All,
    Core,
    One(String),
}

/// Scope label used in human output.
pub fn scope_label(scope: &UpdateScope) -> String {
    match scope {
        UpdateScope::All => "all".to_string(),
        UpdateScope::Core => "core set".to_string(),
        UpdateScope::One(name) => format!("skill {name:?}"),
    }
}

// --- Destination and drift --------------------------------------------

/// Resolve the host skill directory: `--dest` wins (primarily for
/// tests and custom locations), otherwise the host default under
/// `$HOME`. Refuses (with `None`) when the host is unknown or the
/// home directory cannot be found; the caller reports those by name.
pub fn resolve_dest(host: &str, dest_override: Option<&str>) -> Option<PathBuf> {
    if dest_override.is_some() {
        return dest_override.map(PathBuf::from);
    }
    let host = find_host(host)?;
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(host.suffix))
}

/// One drift finding: paths are relative to the skill directory.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Drift {
    /// Present on disk but bytes differ from the manifest.
    pub drifted: Vec<String>,
    /// In the manifest scope but absent on disk.
    pub missing: Vec<String>,
    /// On disk under the scope but in no manifest entry.
    pub extra: Vec<String>,
}

impl Drift {
    pub fn is_clean(&self) -> bool {
        self.drifted.is_empty() && self.missing.is_empty() && self.extra.is_empty()
    }
}

/// Compare `dest` against the manifest entries for `scope`.
/// Unreadable files count as drifted (they cannot match); a
/// missing destination directory means every scoped file is missing.
pub fn compute_drift(dest: &Path, scope_paths: &[(&str, &str)]) -> Drift {
    let manifest = manifest_entries();
    let mut drift = Drift::default();
    for (rel, _body) in scope_paths {
        let path = dest.join(rel);
        match std::fs::read(&path) {
            Ok(bytes) => {
                let want = manifest.get(*rel).cloned().unwrap_or_default();
                if catalog::sha256_hex(&bytes) != want {
                    drift.drifted.push(rel.to_string());
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                drift.missing.push(rel.to_string());
            }
            Err(_) => {
                drift.drifted.push(rel.to_string());
            }
        }
    }
    // Extra files: anything on disk under the scope's skill
    // directories that no manifest entry names. The walk stays
    // inside the scope so `check` (core scope) never reports a
    // fully-installed extended skill as extra.
    let mut scope_dirs: Vec<&str> = scope_paths
        .iter()
        .map(|(rel, _)| rel.split('/').next().unwrap_or(""))
        .collect();
    scope_dirs.sort();
    scope_dirs.dedup();
    let mut stack: Vec<PathBuf> = scope_dirs
        .iter()
        .map(|d| dest.join(d))
        .filter(|p| p.is_dir())
        .collect();
    while let Some(dir) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(it) => it,
            Err(_) => {
                continue;
            }
        };
        let mut entries: Vec<_> = entries.flatten().collect();
        entries.sort_by_key(|e| e.path());
        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let rel = match path.strip_prefix(dest) {
                Ok(r) => r.to_string_lossy().replace('\\', "/"),
                Err(_) => continue,
            };
            if !manifest.contains_key(&rel) {
                drift.extra.push(rel);
            }
        }
    }
    drift.drifted.sort();
    drift.missing.sort();
    drift.extra.sort();
    drift
}

/// Write `files` into `dest`, creating directories as needed. Every
/// write is verified against the manifest afterwards; the first
/// failure refuses naming the path and the error.
pub fn write_files(dest: &Path, files: &[(&str, &str)]) -> Result<Vec<String>, (String, String)> {
    let manifest = manifest_entries();
    let mut written = Vec::new();
    for (rel, body) in files {
        let path = dest.join(rel);
        if let Some(parent) = path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                return Err((path.display().to_string(), e.to_string()));
            }
        }
        if let Err(e) = std::fs::write(&path, body) {
            return Err((path.display().to_string(), e.to_string()));
        }
        match std::fs::read(&path) {
            Ok(bytes) => {
                let want = manifest.get(*rel).cloned().unwrap_or_default();
                if catalog::sha256_hex(&bytes) != want {
                    return Err((
                        path.display().to_string(),
                        "written bytes do not match the manifest".to_string(),
                    ));
                }
            }
            Err(e) => return Err((path.display().to_string(), e.to_string())),
        }
        written.push(rel.to_string());
    }
    written.sort();
    Ok(written)
}

// --- Verbs --------------------------------------------------------------

/// Usage error for the `skills` verbs: prints usage, exits 2 like
/// every other CLI usage error.
fn skills_usage_error(msg: &str) -> ! {
    eprintln!("framesmith skills: {msg}.");
    eprintln!(
        "usage: framesmith skills list [--json]\n\
         \x20       framesmith skills install --host <host> [--dest <dir>]\n\
         \x20       framesmith skills check --host <host> [--dest <dir>] [--json]\n\
         \x20       framesmith skills update (--all | --core | --skill <name>) --host <host> [--dest <dir>] [--yes]\n\
         \x20       framesmith skills validate [--host <host>] [--dest <dir>] [--json]"
    );
    std::process::exit(2);
}

/// Refusal for an unknown host: exit 1 listing every supported host.
fn refuse_unknown_host(host: &str, verb: &str) -> i32 {
    eprintln!(
        "framesmith skills {verb} refused: unknown host {host:?}.\n\
         supported hosts: {}.\n\
         fix: pass one of those names with `--host`; nothing was installed or changed.",
        host_list_line()
    );
    1
}

/// Refusal for an unwritable location: exit 1 naming the path.
fn refuse_unwritable(path: &str, detail: &str, verb: &str) -> i32 {
    eprintln!(
        "framesmith skills {verb} refused: cannot write to {path}: {detail}.\n\
         fix: choose a writable location (or pass `--dest <dir>`); nothing was installed or changed."
    );
    1
}

/// `skills list`: every shipped skill with its one-line purpose,
/// kind, and core membership.
pub fn cmd_skills_list(json: bool) -> i32 {
    if json {
        let items: Vec<serde_json::Value> = SKILLS
            .iter()
            .map(|s| {
                serde_json::json!({
                    "name": s.name,
                    "purpose": s.purpose,
                    "core": s.core,
                    "kind": match s.kind {
                        SkillKind::Domain => "domain",
                        SkillKind::Workflow => "workflow",
                    },
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::json!({ "skills": items, "count": items.len() })
        );
        return 0;
    }
    for s in SKILLS {
        let core = if s.core { " (core)" } else { "" };
        let kind = match s.kind {
            SkillKind::Domain => "",
            SkillKind::Workflow => " [workflow]",
        };
        println!("{} — {}{}{}", s.name, s.purpose, core, kind);
    }
    0
}

// --- Skill schemas (T002 / CAP-002, CAP-006; T005 / CAP-005) ---------
// A domain skill teaches one area: title, one-line purpose, trigger,
// Do, Do-not, and the exact commands it may use. A creation workflow
// walks a whole video: title, purpose, input, output, steps, the
// capabilities it may call (only members of the local menu below),
// its approval gates, its deferred steps with the owning sprint
// named, and its stop rule: a step whose capability is absent stops
// at that step naming the capability and sprint, never simulated.
// A failing document is reported naming the file and the field,
// never silently.

/// One schema fault: the shipped file and the field that fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaFault {
    /// Shipped relative path, e.g. `router/SKILL.md`.
    pub file: String,
    /// Schema field that fails, e.g. `purpose`, `gates`.
    pub field: &'static str,
    /// What is wrong with it.
    pub detail: String,
}

/// Required fields for a domain skill document.
const DOMAIN_FIELDS: &[(&str, &str)] = &[
    ("trigger", "Use this skill when"),
    ("do", "Do:"),
    ("do-not", "Do not:"),
    ("commands", "Commands:"),
];

/// Required fields for a creation workflow document.
const WORKFLOW_FIELDS: &[(&str, &str)] = &[
    ("input", "Input:"),
    ("output", "Output:"),
    ("steps", "Steps:"),
    ("capabilities", "Capabilities:"),
    ("gates", "Gates:"),
    ("deferred", "Deferred:"),
    ("stop", "Stop:"),
];

/// The local capability menu (T005 / CAP-005, REC-003): every command
/// verb a creation workflow may call. Each member ships in this build
/// and runs offline against the local project; hosted or generative
/// steps are never members, so a workflow naming anything else fails
/// validation instead of improvising at run time.
pub const WORKFLOW_CAPABILITY_MENU: &[&str] = &[
    "check",
    "compare",
    "compositions",
    "grade-bake",
    "grade-compare",
    "history",
    "info",
    "keyframes",
    "render",
    "snapshot",
    "timeline",
    "waveform",
];

/// Backticked capability tokens inside one document section, in
/// order. Only the first word of each span counts, so trailing
/// punctuation never becomes a capability name.
fn capabilities_tokens(section: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = section;
    while let Some(open) = rest.find('`') {
        rest = &rest[open + 1..];
        let Some(close) = rest.find('`') else {
            break;
        };
        let span = rest[..close].trim().to_string();
        rest = &rest[close + 1..];
        let first = span.split_whitespace().next().unwrap_or("");
        let name = first.trim_matches(|c: char| ",.:;)".contains(c));
        if !name.is_empty() {
            out.push(name.to_string());
        }
    }
    out
}

/// Every capability a workflow calls must come from the local menu.
/// Returns one fault per off-menu token, each naming the file and
/// the `capabilities` field.
fn workflow_menu_faults(rel: &str, body: &str) -> Vec<SchemaFault> {
    // The Capabilities section runs from its marker to the next
    // section marker (Gates, Deferred, Stop) or the end of file.
    let mut section = String::new();
    let mut in_capabilities = false;
    for line in body.lines() {
        let t = line.trim_start();
        if t.starts_with("Capabilities:") {
            in_capabilities = true;
            section.push_str(line);
            section.push('\n');
            continue;
        }
        if in_capabilities
            && (t.starts_with("Gates:") || t.starts_with("Deferred:") || t.starts_with("Stop:"))
        {
            break;
        }
        if in_capabilities {
            section.push_str(line);
            section.push('\n');
        }
    }
    let mut faults = Vec::new();
    for token in capabilities_tokens(&section) {
        if !WORKFLOW_CAPABILITY_MENU.contains(&token.as_str()) {
            faults.push(SchemaFault {
                file: rel.to_string(),
                field: "capabilities",
                detail: format!(
                    "capability {token:?} is not on the local menu ({}); \
                     workflows call only locally available capabilities",
                    WORKFLOW_CAPABILITY_MENU.join(", ")
                ),
            });
        }
    }
    faults
}

/// Validate one baked skill body against its own schema. Returns every
/// fault, each naming the file and the field.
pub fn validate_skill_body(skill: &Skill) -> Vec<SchemaFault> {
    let (rel, body) = skill.files[0];
    validate_doc_body(skill.name, skill.kind, rel, body)
}

/// Validate one document body (baked or installed on disk) against the
/// schema for `name` and `kind`.
pub fn validate_doc_body(name: &str, kind: SkillKind, rel: &str, body: &str) -> Vec<SchemaFault> {
    let mut faults = Vec::new();
    let fault = |field: &'static str, detail: &str| SchemaFault {
        file: rel.to_string(),
        field,
        detail: detail.to_string(),
    };
    let lines: Vec<&str> = body.lines().collect();
    let first = lines
        .iter()
        .map(|l| l.trim())
        .find(|l| !l.is_empty())
        .unwrap_or("");
    let want_title = match kind {
        SkillKind::Domain => format!("# Framesmith {name} skill"),
        SkillKind::Workflow => format!("# Framesmith {name} workflow"),
    };
    // Display case may differ (`CLI` reads better than `cli`), but the
    // title must name exactly this skill or workflow.
    if first.to_lowercase() != want_title.to_lowercase() {
        faults.push(fault(
            "title",
            &format!("first line must name this document ({want_title:?})"),
        ));
    }
    match lines
        .iter()
        .find(|l| l.trim_start().starts_with("Purpose:"))
    {
        Some(line) => {
            if line.trim_start()["Purpose:".len()..].trim().is_empty() {
                faults.push(fault("purpose", "Purpose: needs a one-line purpose"));
            }
        }
        None => faults.push(fault("purpose", "missing Purpose: line")),
    }
    let fields = match kind {
        SkillKind::Domain => DOMAIN_FIELDS,
        SkillKind::Workflow => WORKFLOW_FIELDS,
    };
    for (field, marker) in fields {
        let hit = lines.iter().any(|l| {
            let t = l.trim_start();
            t == *marker || t.starts_with(&format!("{marker} ")) || t.starts_with(marker)
        });
        if !hit {
            faults.push(fault(field, &format!("missing {marker} section")));
        }
    }
    // Marker lines must not be empty shells: a bare marker with no
    // content line after it fails the same field.
    for (field, marker) in fields {
        for (i, line) in lines.iter().enumerate() {
            if line.trim() == *marker {
                let rest = lines[i + 1..].iter().any(|l| !l.trim().is_empty());
                if !rest {
                    faults.push(fault(field, &format!("{marker} has no content")));
                }
                break;
            }
        }
    }
    // Workflows call only the local menu: any other capability is a
    // stop-now refusal at validation time, naming file and field.
    if kind == SkillKind::Workflow {
        faults.extend(workflow_menu_faults(rel, body));
    }
    faults.sort_by(|a, b| (a.file.clone(), a.field).cmp(&(b.file.clone(), b.field)));
    faults
}

/// Validate every baked skill document. Clean means all 21 pass.
pub fn validate_all_baked() -> Vec<SchemaFault> {
    let mut faults = Vec::new();
    for skill in SKILLS {
        faults.extend(validate_skill_body(skill));
    }
    faults.sort_by(|a, b| (a.file.clone(), a.field).cmp(&(b.file.clone(), b.field)));
    faults
}

/// Validate installed `*/SKILL.md` files on disk: known skill names
/// use their own schema, a missing `SKILL.md` fails `presence`, and an
/// unknown skill directory fails `skill`. Files no manifest names are
/// `check`'s business and are ignored here.
pub fn validate_installed(dest: &Path) -> Vec<SchemaFault> {
    let mut faults = Vec::new();
    let mut seen = 0usize;
    let entries: Vec<_> = std::fs::read_dir(dest)
        .map(|it| it.flatten().collect())
        .unwrap_or_default();
    let mut dirs: Vec<PathBuf> = entries
        .iter()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    for dir in dirs {
        let Some(dir_name) = dir.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let candidate = dir.join("SKILL.md");
        let rel = format!("{dir_name}/SKILL.md");
        match find_skill(dir_name) {
            Some(skill) => {
                seen += 1;
                match std::fs::read_to_string(&candidate) {
                    Ok(body) => {
                        faults.extend(validate_doc_body(skill.name, skill.kind, &rel, &body));
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                        faults.push(SchemaFault {
                            file: rel,
                            field: "presence",
                            detail: "SKILL.md is missing".to_string(),
                        });
                    }
                    Err(e) => {
                        faults.push(SchemaFault {
                            file: rel,
                            field: "presence",
                            detail: format!("SKILL.md is unreadable: {e}"),
                        });
                    }
                }
            }
            None => {
                // Only unknown directories that carry a SKILL.md fail
                // here; plain extras stay `check`'s business.
                if candidate.is_file() {
                    seen += 1;
                    faults.push(SchemaFault {
                        file: rel,
                        field: "skill",
                        detail: format!("unknown skill {dir_name:?}"),
                    });
                }
            }
        }
    }
    if seen == 0 {
        faults.push(SchemaFault {
            file: "<none>".to_string(),
            field: "presence",
            detail: format!("no skill files found at {}", dest.display()),
        });
    }
    faults.sort_by(|a, b| (a.file.clone(), a.field).cmp(&(b.file.clone(), b.field)));
    faults
}

/// Render one schema fault as human text, naming file and field.
fn format_fault_human(f: &SchemaFault) -> String {
    format!("broken: {} (field: {}): {}", f.file, f.field, f.detail)
}

/// `skills validate [--host <host>] [--dest <dir>] [--json]`: every
/// baked skill document passes its own schema, or every installed
/// `*/SKILL.md` under the host location does. Clean passes exit 0
/// naming the count; a failing document refuses exit 1 naming the
/// file and the field.
pub fn cmd_skills_validate(host: Option<&str>, dest_override: Option<&str>, json: bool) -> i32 {
    let (faults, scope_label) = match host {
        None => {
            if dest_override.is_some() {
                skills_usage_error("validate takes `--dest` only with `--host`");
            }
            let faults = validate_all_baked();
            (faults, "shipped set".to_string())
        }
        Some(h) => {
            if find_host(h).is_none() {
                return refuse_unknown_host(h, "validate");
            }
            let Some(dest) = resolve_dest(h, dest_override) else {
                eprintln!(
                    "framesmith skills validate refused: cannot locate the skill directory for host {h:?} \
                     (no home directory found).\n\
                     fix: pass `--dest <dir>`; nothing was installed or changed."
                );
                return 1;
            };
            let faults = validate_installed(&dest);
            (faults, format!("host {h:?} at {}", dest.display()))
        }
    };
    if json {
        let items: Vec<serde_json::Value> = faults
            .iter()
            .map(|f| {
                serde_json::json!({
                    "file": f.file,
                    "field": f.field,
                    "detail": f.detail,
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::json!({ "scope": scope_label, "clean": faults.is_empty(), "faults": items })
        );
    } else if faults.is_empty() {
        println!("skills validate: {scope_label}: every skill file passes its schema.");
    } else {
        eprintln!(
            "skills validate: {scope_label}: {} broken file(s).",
            faults.len()
        );
        for f in &faults {
            eprintln!("{}", format_fault_human(f));
        }
        eprintln!(
            "fix: restore the shipped file (`skills update --yes`) or repair the named field."
        );
    }
    if faults.is_empty() {
        0
    } else {
        1
    }
}

/// `skills install --host <host> [--dest <dir>]`: install the core
/// set into the host's skill location.
pub fn cmd_skills_install(host: &str, dest_override: Option<&str>) -> i32 {
    if find_host(host).is_none() {
        return refuse_unknown_host(host, "install");
    }
    let Some(dest) = resolve_dest(host, dest_override) else {
        eprintln!(
            "framesmith skills install refused: cannot locate the skill directory for host {host:?} \
             (no home directory found).\n\
             fix: pass `--dest <dir>` with a writable location; nothing was installed."
        );
        return 1;
    };
    let files: Vec<(&str, &str)> = SKILLS
        .iter()
        .filter(|s| s.core)
        .flat_map(|s| s.files.iter().copied())
        .collect();
    match write_files(&dest, &files) {
        Ok(written) => {
            println!(
                "installed {} core skill file(s) for host {host:?} at {}: {}.",
                written.len(),
                dest.display(),
                written.join(", ")
            );
            println!("next: run `framesmith skills check --host {host}` to verify zero drift.");
            0
        }
        Err((path, detail)) => refuse_unwritable(&path, &detail, "install"),
    }
}

/// Render one drift report as human text.
fn format_drift_human(host: &str, dest: &Path, scope: &str, drift: &Drift) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    if drift.is_clean() {
        let _ = writeln!(
            out,
            "skills check: host {host:?} at {} ({scope}): 0 drifted, 0 missing, 0 extra — zero drift.",
            dest.display()
        );
        return out;
    }
    let _ = writeln!(
        out,
        "skills check: host {host:?} at {} ({scope}): {} drifted, {} missing, {} extra.",
        dest.display(),
        drift.drifted.len(),
        drift.missing.len(),
        drift.extra.len()
    );
    for rel in &drift.drifted {
        let _ = writeln!(out, "drifted: {rel}");
    }
    for rel in &drift.missing {
        let _ = writeln!(out, "missing: {rel}");
    }
    for rel in &drift.extra {
        let _ = writeln!(out, "extra: {rel}");
    }
    out
}

/// `skills check --host <host> [--dest <dir>] [--json]`: compare the
/// installed core set against the hashed manifest. A clean install
/// reports zero drift explicitly; any drift refuses (exit 1) naming
/// every drifted, missing, and extra file.
pub fn cmd_skills_check(host: &str, dest_override: Option<&str>, json: bool) -> i32 {
    if find_host(host).is_none() {
        return refuse_unknown_host(host, "check");
    }
    let Some(dest) = resolve_dest(host, dest_override) else {
        eprintln!(
            "framesmith skills check refused: cannot locate the skill directory for host {host:?} \
             (no home directory found).\n\
             fix: pass `--dest <dir>`; nothing was installed or changed."
        );
        return 1;
    };
    // Check covers the installed set: the core skills `install`
    // writes. Extra files are reported relative to those skill
    // directories, so a clean install reports zero drift explicitly.
    let scope = UpdateScope::Core;
    let paths = scope_paths(&scope);
    let drift = compute_drift(&dest, &paths);
    if json {
        println!(
            "{}",
            serde_json::json!({
                "host": host,
                "dest": dest.display().to_string(),
                "scope": "core",
                "drifted": drift.drifted,
                "missing": drift.missing,
                "extra": drift.extra,
                "clean": drift.is_clean(),
            })
        );
    } else {
        print!("{}", format_drift_human(host, &dest, "core set", &drift));
    }
    if drift.is_clean() {
        0
    } else {
        1
    }
}

/// `skills update (--all | --core | --skill <name>) --host <host>
/// [--dest <dir>] [--yes]`: restore missing files and, with the
/// maker's say-so, drifted files in scope. Without `--yes` nothing
/// is written: pending restores refuse naming every file. Extra
/// files are only ever named by `check`, never removed here. Only
/// the selected scope is touched.
pub fn cmd_skills_update(
    scope: &UpdateScope,
    host: &str,
    dest_override: Option<&str>,
    yes: bool,
) -> i32 {
    if find_host(host).is_none() {
        return refuse_unknown_host(host, "update");
    }
    if let UpdateScope::One(name) = scope {
        if find_skill(name).is_none() {
            eprintln!(
                "framesmith skills update refused: unknown skill {name:?}.\n\
                 available skills: {}.\n\
                 fix: pass one of those names with `--skill`; nothing was installed or changed.",
                skill_list_line()
            );
            return 1;
        }
    }
    let Some(dest) = resolve_dest(host, dest_override) else {
        eprintln!(
            "framesmith skills update refused: cannot locate the skill directory for host {host:?} \
             (no home directory found).\n\
             fix: pass `--dest <dir>` with a writable location; nothing was changed."
        );
        return 1;
    };
    let label = scope_label(scope);
    let paths = scope_paths(scope);
    let drift = compute_drift(&dest, &paths);
    if drift.is_clean() {
        println!(
            "skills update: host {host:?} at {} ({label}): already in sync, nothing to restore.",
            dest.display()
        );
        return 0;
    }
    if !yes {
        eprintln!(
            "framesmith skills update refused: restoring the {label} for host {host:?} needs the maker's say-so.",
        );
        for rel in drift.missing.iter().chain(drift.drifted.iter()) {
            let kind = if drift.missing.contains(rel) {
                "missing"
            } else {
                "drifted"
            };
            eprintln!("{kind}: {rel}");
        }
        if !drift.extra.is_empty() {
            eprintln!(
                "note: {} extra file(s) are left untouched; `skills check` names them.",
                drift.extra.len()
            );
        }
        eprintln!(
            "fix: re-run with `--yes` to restore {}; nothing was changed.",
            drift
                .missing
                .iter()
                .chain(drift.drifted.iter())
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
        return 1;
    }
    // With say-so, restore exactly the scoped drifted-plus-missing
    // set: touch only their set, never extras, never other scopes.
    let wanted: Vec<(&str, &str)> = paths
        .iter()
        .copied()
        .filter(|(rel, _)| {
            drift.drifted.iter().any(|d| d == rel) || drift.missing.iter().any(|m| m == rel)
        })
        .collect();
    match write_files(&dest, &wanted) {
        Ok(written) => {
            println!(
                "updated {} file(s) for host {host:?} at {} ({label}): {}.",
                written.len(),
                dest.display(),
                written.join(", ")
            );
            if !drift.extra.is_empty() {
                println!(
                    "note: {} extra file(s) left untouched: {}.",
                    drift.extra.len(),
                    drift.extra.join(", ")
                );
            }
            0
        }
        Err((path, detail)) => refuse_unwritable(&path, &detail, "update"),
    }
}

/// Entry point for `framesmith skills ...`: parse the verb and run it.
pub fn cmd_skills(args: &[String]) -> i32 {
    if args.is_empty() {
        skills_usage_error("expected a verb: list, install, check, update, or validate");
    }
    match args[0].as_str() {
        "list" => {
            let mut json = false;
            for arg in &args[1..] {
                match arg.as_str() {
                    "--json" => {
                        if json {
                            skills_usage_error("duplicate --json flag");
                        }
                        json = true;
                    }
                    other => skills_usage_error(&format!("unknown flag {other:?}")),
                }
            }
            cmd_skills_list(json)
        }
        "install" => {
            let (host, dest) = parse_host_args(&args[1..], "install");
            cmd_skills_install(&host, dest.as_deref())
        }
        "check" => {
            let (host, dest, json) = parse_check_args(&args[1..]);
            cmd_skills_check(&host, dest.as_deref(), json)
        }
        "update" => {
            let (scope, host, dest, yes) = parse_update_args(&args[1..]);
            cmd_skills_update(&scope, &host, dest.as_deref(), yes)
        }
        "validate" => {
            let (host, dest, json) = parse_validate_args(&args[1..]);
            cmd_skills_validate(host.as_deref(), dest.as_deref(), json)
        }
        other => skills_usage_error(&format!("unknown skills verb {other:?}")),
    }
}

/// Parse `[--host <host>] [--dest <dir>] [--json]` for `validate`.
/// Both flags are optional: bare `validate` checks the baked set,
/// while `--host` (optionally with `--dest`) checks installed files.
fn parse_validate_args(args: &[String]) -> (Option<String>, Option<String>, bool) {
    let mut host: Option<String> = None;
    let mut dest: Option<String> = None;
    let mut json = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--host" => {
                i += 1;
                if i >= args.len() {
                    skills_usage_error("--host needs a value (an agent host name)");
                }
                if host.is_some() {
                    skills_usage_error("duplicate --host flag");
                }
                host = Some(args[i].clone());
            }
            "--dest" => {
                i += 1;
                if i >= args.len() {
                    skills_usage_error("--dest needs a value (a skill directory)");
                }
                if dest.is_some() {
                    skills_usage_error("duplicate --dest flag");
                }
                dest = Some(args[i].clone());
            }
            "--json" => {
                if json {
                    skills_usage_error("duplicate --json flag");
                }
                json = true;
            }
            other if other.starts_with('-') => {
                skills_usage_error(&format!("unknown flag {other:?}"));
            }
            other => skills_usage_error(&format!("unexpected argument {other:?}")),
        }
        i += 1;
    }
    (host, dest, json)
}

/// Parse `--host <host> [--dest <dir>]` for `install`.
fn parse_host_args(args: &[String], verb: &str) -> (String, Option<String>) {
    let mut host: Option<String> = None;
    let mut dest: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--host" => {
                i += 1;
                if i >= args.len() {
                    skills_usage_error("--host needs a value (an agent host name)");
                }
                if host.is_some() {
                    skills_usage_error("duplicate --host flag");
                }
                host = Some(args[i].clone());
            }
            "--dest" => {
                i += 1;
                if i >= args.len() {
                    skills_usage_error("--dest needs a value (a skill directory)");
                }
                if dest.is_some() {
                    skills_usage_error("duplicate --dest flag");
                }
                dest = Some(args[i].clone());
            }
            other if other.starts_with('-') => {
                skills_usage_error(&format!("unknown flag {other:?}"));
            }
            other => skills_usage_error(&format!("unexpected argument {other:?}")),
        }
        i += 1;
    }
    match host {
        Some(h) => (h, dest),
        None => skills_usage_error(&format!("{verb} needs --host <host>")),
    }
}

/// Parse `--host <host> [--dest <dir>] [--json]` for `check`.
fn parse_check_args(args: &[String]) -> (String, Option<String>, bool) {
    let mut host: Option<String> = None;
    let mut dest: Option<String> = None;
    let mut json = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--host" => {
                i += 1;
                if i >= args.len() {
                    skills_usage_error("--host needs a value (an agent host name)");
                }
                if host.is_some() {
                    skills_usage_error("duplicate --host flag");
                }
                host = Some(args[i].clone());
            }
            "--dest" => {
                i += 1;
                if i >= args.len() {
                    skills_usage_error("--dest needs a value (a skill directory)");
                }
                if dest.is_some() {
                    skills_usage_error("duplicate --dest flag");
                }
                dest = Some(args[i].clone());
            }
            "--json" => {
                if json {
                    skills_usage_error("duplicate --json flag");
                }
                json = true;
            }
            other if other.starts_with('-') => {
                skills_usage_error(&format!("unknown flag {other:?}"));
            }
            other => skills_usage_error(&format!("unexpected argument {other:?}")),
        }
        i += 1;
    }
    match host {
        Some(h) => (h, dest, json),
        None => skills_usage_error("check needs --host <host>"),
    }
}

/// Parse `(--all | --core | --skill <name>) --host <host> [--dest
/// <dir>] [--yes]` for `update`. Exactly one scope is required.
fn parse_update_args(args: &[String]) -> (UpdateScope, String, Option<String>, bool) {
    let mut scope: Option<UpdateScope> = None;
    let mut host: Option<String> = None;
    let mut dest: Option<String> = None;
    let mut yes = false;
    let mut set_scope = |next: UpdateScope| {
        if scope.is_some() {
            skills_usage_error("pass exactly one of --all, --core, or --skill <name>");
        }
        scope = Some(next);
    };
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--all" => set_scope(UpdateScope::All),
            "--core" => set_scope(UpdateScope::Core),
            "--skill" => {
                i += 1;
                if i >= args.len() {
                    skills_usage_error("--skill needs a value (a skill name)");
                }
                let name = args[i].clone();
                set_scope(UpdateScope::One(name));
            }
            "--host" => {
                i += 1;
                if i >= args.len() {
                    skills_usage_error("--host needs a value (an agent host name)");
                }
                if host.is_some() {
                    skills_usage_error("duplicate --host flag");
                }
                host = Some(args[i].clone());
            }
            "--dest" => {
                i += 1;
                if i >= args.len() {
                    skills_usage_error("--dest needs a value (a skill directory)");
                }
                if dest.is_some() {
                    skills_usage_error("duplicate --dest flag");
                }
                dest = Some(args[i].clone());
            }
            "--yes" => {
                if yes {
                    skills_usage_error("duplicate --yes flag");
                }
                yes = true;
            }
            other if other.starts_with('-') => {
                skills_usage_error(&format!("unknown flag {other:?}"));
            }
            other => skills_usage_error(&format!("unexpected argument {other:?}")),
        }
        i += 1;
    }
    let scope = match scope {
        Some(s) => s,
        None => skills_usage_error("update needs exactly one of --all, --core, or --skill <name>"),
    };
    match host {
        Some(h) => (scope, h, dest, yes),
        None => skills_usage_error("update needs --host <host>"),
    }
}
