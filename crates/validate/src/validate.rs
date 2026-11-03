//! Folder + composition validation (T003, T004, T005).
//!
//! Every problem is reported as a [`Diagnostic`] carrying file, line,
//! column and a stable code. The verdict mapping is:
//!
//! * no diagnostics → `Valid` (check exit 0)
//! * any `E_*` invalid diagnostic → `Invalid` (check exit 1)
//! * only `E_UNSUPPORTED` diagnostics → `Unsupported` (check exit 2,
//!   render refuses before the first frame and writes no MP4)

use notation::model::{
    AudioClip, BoxClip, Clip, Composition, EaseDecl, Fit, FontDecl, Fps, ImageClip, Placement,
    SetDecl, TextClip, TweenDecl, TweenKind, VideoClip, parse_color, CONFIG_FILE,
};
use notation::parse::{RawNode, RawValue, ValueKind, parse_document};
use std::path::Path;

/// One located, coded problem (or, for unsupported use, one
/// valid-but-not-yet-usable marking).
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub file: String,
    pub line: usize,
    pub column: usize,
    pub code: &'static str,
    pub message: String,
    pub fix: String,
}

impl Diagnostic {
    fn new(
        file: &str,
        line: usize,
        column: usize,
        code: &'static str,
        message: impl Into<String>,
        fix: impl Into<String>,
    ) -> Self {
        Self {
            file: file.into(),
            line,
            column,
            code,
            message: message.into(),
            fix: fix.into(),
        }
    }
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "error[{}] {}:{}:{}: {} (fix: {})",
            self.code, self.file, self.line, self.column, self.message, self.fix
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Valid,
    Invalid,
    Unsupported,
}

#[derive(Debug, Clone)]
pub struct Verdict {
    pub status: Status,
    pub composition: Option<Composition>,
    pub diagnostics: Vec<Diagnostic>,
    /// Optional `framesmith.toml` setting, default true. Later sprints
    /// (T009/T010) own download behavior; the foundation records it so
    /// refusal wording can point at the right fix.
    pub ffmpeg_auto_download: bool,
}

pub const COMPOSITION_FILE: &str = "composition.kdl";

/// Later-sprint elements: well-formed but never rendered yet.
/// Each maps to the sprint that enables it. Boxes and images landed in
/// Sprint 2 (T002); tweens and sets validate since Sprint 2 (T003);
/// `audio` validates since Sprint 3 (T007), so only later sound-shaping
/// elements (Sprint 10), hardware encode (Sprint 25), and similar still
/// wait here — later motion properties and easings are marked at the
/// key level inside `validate_tween`/`validate_set`.
pub fn unsupported_sprint(element: &str) -> Option<u32> {
    match element {
        // Sprint 10 sound proof: effects chains, automation, groups,
        // ducking, and the analysis/normalize verbs.
        "effect" | "effects" | "automation" | "group" | "groups" | "ducking" | "mixer"
        | "equalizer" | "normalize-audio" | "loudness" => Some(10),
        // Sprint 25: hardware encoding.
        "hardware" | "videotoolbox" | "nvenc" | "vaapi" | "amf" | "qsv" => Some(25),
        _ => None,
    }
}

/// Audio keys that wait for Sprint 10 sound proof (effects chains,
/// lane automation, groups, ducking, panning, per-sample curves, and
/// speed/pitch behavior). Well-formed but refused with the enabling
/// sprint, never mixed partially.
const LATER_AUDIO_KEYS: &[&str] = &[
    "automation",
    "compressor",
    "curve",
    "ducking",
    "echo",
    "effect",
    "effects",
    "eq",
    "equalizer",
    "fade-shape",
    "fade_shape",
    "group",
    "groups",
    "limiter",
    "loudness",
    "normalize",
    "pan",
    "panning",
    "pitch",
    "reverb",
    "speed",
    "tempo",
];

/// Audio source extensions Sprint 3 mixes (decoded through the
/// pinned-or-host FFmpeg). Anything else is refused as
/// not-yet-supported, never guessed into the mix.
fn audio_format_supported(path: &str) -> bool {
    const KNOWN: &[&str] = &[
        "wav", "mp3", "flac", "ogg", "oga", "opus", "m4a", "aac", "aiff", "aif", "wma", "mka",
        "ac3", "mp2", "amr",
    ];
    let lower = path.to_lowercase();
    KNOWN.iter().any(|ext| lower.ends_with(&format!(".{ext}")))
}

/// Parse a Sprint 3 `volume` value in dB (shared by `audio` tracks and
/// video sound keys): a finite number at most +12 dB, or `-inf` for
/// exact digital silence. Anything else is refused, never guessed.
fn parse_volume_db(ctx: &mut Ctx, node: &RawNode, what: &str) -> Option<f64> {
    match find(node, "volume") {
        None => Some(0.0),
        Some(v) => match as_f64(v) {
            Some(db) if db == f64::NEG_INFINITY => Some(db),
            Some(db) if db.is_finite() && db <= 12.0 => Some(db),
            _ => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_AUDIO",
                    format!("{what} `volume` must be silence (`-inf`) or at most +12 dB"),
                    "set `volume=<dB at most +12>` (for example `volume=0`, `volume=6`, or `volume=\"-inf\"` for silence)",
                );
                None
            }
        },
    }
}

/// Parse one linear fade duration (Sprint 3 `fade-in`/`fade-out`, either
/// spelling): absent means 0, present must be a finite non-negative
/// number within the clip window. A fade longer than its clip is
/// refused, never clamped silently.
fn parse_fade(
    ctx: &mut Ctx,
    node: &RawNode,
    hyphen: &str,
    underscore: &str,
    duration: f64,
    what: &str,
) -> Option<f64> {
    let found = find(node, hyphen).or_else(|| find(node, underscore));
    match found {
        None => Some(0.0),
        Some(v) => match as_f64(v) {
            Some(x) if x.is_finite() && x >= 0.0 && x <= duration => Some(x),
            _ => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_AUDIO",
                    format!(
                        "{what} `{hyphen}` must run 0 through the clip duration ({duration}s); a fade longer than its clip is refused"
                    ),
                    format!("set `{hyphen}=<seconds from 0 to {duration}>` or remove the key"),
                );
                None
            }
        },
    }
}

fn as_f64(v: &RawValue) -> Option<f64> {
    match v.kind {
        ValueKind::Int(n) => Some(n as f64),
        ValueKind::Float(x) => Some(x),
        ValueKind::Str(ref s) => s.parse::<f64>().ok(),
        _ => None,
    }
}

fn as_str(v: &RawValue) -> Option<&str> {
    match v.kind {
        ValueKind::Str(ref s) => Some(s.as_str()),
        _ => None,
    }
}

fn as_int(v: &RawValue) -> Option<i64> {
    match v.kind {
        ValueKind::Int(n) => Some(n),
        _ => None,
    }
}

struct Ctx<'a> {
    file: String,
    folder: &'a Path,
    diags: Vec<Diagnostic>,
    invalid: bool,
}

impl<'a> Ctx<'a> {
    fn push(
        &mut self,
        line: usize,
        column: usize,
        code: &'static str,
        message: impl Into<String>,
        fix: impl Into<String>,
    ) {
        if code != "E_UNSUPPORTED" {
            self.invalid = true;
        }
        self.diags.push(Diagnostic::new(
            &self.file.clone(),
            line,
            column,
            code,
            message,
            fix,
        ));
    }
}

fn find<'b>(node: &'b RawNode, key: &str) -> Option<&'b RawValue> {
    node.props.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

/// Validate an optional `framesmith.toml` config. Only `key = value`
/// lines are accepted; unknown keys are invalid (never ignored).
fn validate_config(folder: &Path, ctx_file: &str) -> (bool, Vec<Diagnostic>) {
    let path = folder.join(CONFIG_FILE);
    if !path.exists() {
        return (true, Vec::new());
    }
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) => {
            return (
                false,
                vec![Diagnostic::new(
                    ctx_file,
                    1,
                    1,
                    "E_UNREADABLE",
                    format!("cannot read {CONFIG_FILE}: {e}"),
                    "make the config file readable or remove it",
                )],
            );
        }
    };
    let mut auto_download = true;
    let mut diags = Vec::new();
    for (idx, raw) in text.lines().enumerate() {
        let line_no = idx + 1;
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let (k, v) = match line.split_once('=') {
            Some(p) => (p.0.trim(), p.1.trim()),
            None => {
                diags.push(Diagnostic::new(
                    ctx_file,
                    line_no,
                    1,
                    "E_PARSE",
                    format!("malformed config line: {raw:?}"),
                    "use `key = value` lines",
                ));
                continue;
            }
        };
        match k {
            "ffmpeg_auto_download" => match v {
                "true" => auto_download = true,
                "false" => auto_download = false,
                _ => {
                    diags.push(Diagnostic::new(
                        ctx_file,
                        line_no,
                        1,
                        "E_BAD_CONFIG",
                        format!("{k} must be true or false, found {v:?}"),
                        "write `ffmpeg_auto_download = true` or `false`",
                    ));
                }
            },
            _ => {
                diags.push(Diagnostic::new(
                    ctx_file,
                    line_no,
                    1,
                    "E_UNKNOWN_KEY",
                    format!("unknown config key {k:?}"),
                    "remove the key or check the spelling",
                ));
            }
        }
    }
    (auto_download, diags)
}

/// Parse and validate one project folder. Never writes anything.
pub fn validate_folder(folder: &Path) -> Verdict {
    let file = format!("{}/{COMPOSITION_FILE}", folder.display());
    let config_file = format!("{}/{CONFIG_FILE}", folder.display());

    if !folder.exists() {
        return Verdict {
            status: Status::Invalid,
            composition: None,
            diagnostics: vec![Diagnostic::new(
                &file,
                1,
                1,
                "E_MISSING_DIR",
                format!("project folder {} does not exist", folder.display()),
                "point check at an existing folder or create one with `framesmith init`",
            )],
            ffmpeg_auto_download: true,
        };
    }

    let (ffmpeg_auto_download, config_diags) = validate_config(folder, &config_file);
    let config_invalid = config_diags
        .iter()
        .any(|d| d.code != "E_UNSUPPORTED");

    let text = match std::fs::read_to_string(folder.join(COMPOSITION_FILE)) {
        Ok(t) => t,
        Err(e) => {
            let mut diags = config_diags;
            diags.push(Diagnostic::new(
                &file,
                1,
                1,
                if folder.join(COMPOSITION_FILE).exists() {
                    "E_UNREADABLE"
                } else {
                    "E_MISSING_COMPOSITION"
                },
                format!("cannot read {COMPOSITION_FILE}: {e}"),
                "create the composition with `framesmith init` or restore the file",
            ));
            return Verdict {
                status: Status::Invalid,
                composition: None,
                diagnostics: diags,
                ffmpeg_auto_download,
            };
        }
    };

    let roots = match parse_document(&text) {
        Ok(r) => r,
        Err(e) => {
            let mut diags = config_diags;
            diags.push(Diagnostic::new(
                &file,
                e.line,
                e.column,
                "E_PARSE",
                format!("malformed KDL: {}", e.message),
                "fix the notation syntax at the reported location",
            ));
            return Verdict {
                status: Status::Invalid,
                composition: None,
                diagnostics: diags,
                ffmpeg_auto_download,
            };
        }
    };

    let mut ctx = Ctx {
        file: file.clone(),
        folder,
        diags: config_diags,
        invalid: config_invalid,
    };

    let root = roots.iter().find(|n| n.name == "composition");
    if root.is_none() {
        let (line, col) = roots
            .first()
            .map(|n| (n.name_line, n.name_column))
            .unwrap_or((1, 1));
        ctx.push(
            line,
            col,
            "E_MISSING_FIELD",
            "missing `composition` root node",
            "declare `composition name=... width=... ... { ... }`",
        );
        // Any other top-level node is still located for the author.
        for n in &roots {
            ctx.push(
                n.name_line,
                n.name_column,
                "E_UNKNOWN_ELEMENT",
                format!("unexpected top-level node {:?}", n.name),
                "only a single `composition` root is allowed",
            );
        }
        return finish(ctx, None, ffmpeg_auto_download);
    }
    if roots.len() > 1 {
        for n in roots.iter().filter(|n| n.name != "composition") {
            ctx.push(
                n.name_line,
                n.name_column,
                "E_UNKNOWN_ELEMENT",
                format!("unexpected top-level node {:?}", n.name),
                "keep a single `composition` root",
            );
        }
    }
    let root = root.unwrap();

    // --- root properties -------------------------------------------------
    let name = match find(root, "name").and_then(as_str) {
        Some(s) => s.to_string(),
        None => {
            ctx.push(
                root.name_line,
                root.name_column,
                "E_MISSING_FIELD",
                "composition is missing required `name`",
                "add `name=\"...\"` to the composition root",
            );
            String::new()
        }
    };
    // Unknown root keys are invalid, never ignored (T003).
    const ROOT_KEYS: &[&str] = &[
        "name",
        "width",
        "height",
        "duration",
        "fps",
        "background",
        "version",
    ];
    for (k, v) in &root.props {
        if !ROOT_KEYS.contains(&k.as_str()) {
            ctx.push(
                v.line,
                v.column,
                "E_UNKNOWN_KEY",
                format!("unknown composition key {k:?}"),
                "remove the key or check the spelling",
            );
        }
    }

    let mut need = |key: &str, what: &str| -> Option<f64> {
        match find(root, key).and_then(as_f64) {
            Some(x) => Some(x),
            None => {
                let (line, col) = find(root, key)
                    .map(|v| (v.line, v.column))
                    .unwrap_or((root.name_line, root.name_column));
                ctx.push(
                    line,
                    col,
                    if find(root, key).is_none() {
                        "E_MISSING_FIELD"
                    } else {
                        "E_BAD_DURATION"
                    },
                    format!("composition {what} must be a positive number"),
                    format!("set `{key}=<positive number>` on the composition root"),
                );
                None
            }
        }
    };
    let width = need("width", "width");
    let height = need("height", "height");
    let duration = need("duration", "duration");
    for (key, val) in [("width", width), ("height", height), ("duration", duration)] {
        if let Some(x) = val {
            if x <= 0.0 || (key != "duration" && x.fract() != 0.0) {
                let (line, col) = find(root, key).map(|v| (v.line, v.column)).unwrap();
                ctx.push(
                    line,
                    col,
                    if key == "duration" {
                        "E_BAD_DURATION"
                    } else {
                        "E_BAD_CANVAS"
                    },
                    format!("composition {key} must be positive, found {x}"),
                    format!("set `{key}` to a positive value"),
                );
            }
        }
    }

    // fps: integer or exact rational in 1..=240, default 30.
    // A decimal value is rejected (never silently rounded).
    let fps = match find(root, "fps") {
        None => Fps::from_int(30),
        Some(v) => match &v.kind {
            ValueKind::Int(n) => {
                if *n < 1 || *n > 240 {
                    ctx.push(
                        v.line,
                        v.column,
                        "E_BAD_FPS",
                        format!("fps {n} is outside the accepted range 1 to 240"),
                        "use an integer or exact rational fps within 1 to 240",
                    );
                    Fps::from_int(30)
                } else {
                    Fps::from_int(*n as u64)
                }
            }
            ValueKind::Ratio(num, den) => {
                if *den == 0 {
                    ctx.push(
                        v.line,
                        v.column,
                        "E_BAD_FPS",
                        "fps rational has a zero denominator",
                        "use `<num>/<den>` with a non-zero denominator",
                    );
                    Fps::from_int(30)
                } else {
                    let value = *num as f64 / *den as f64;
                    if value < 1.0 || value > 240.0 {
                        ctx.push(
                            v.line,
                            v.column,
                            "E_BAD_FPS",
                            format!("fps {num}/{den} is outside the accepted range 1 to 240"),
                            "use an integer or exact rational fps within 1 to 240",
                        );
                        Fps::from_int(30)
                    } else {
                        Fps { num: *num, den: *den }
                    }
                }
            }
            ValueKind::Str(s) if s.contains('/') => {
                match s.split_once('/').map(|(a, b)| (a.parse::<u64>(), b.parse::<u64>())) {
                    Some((Ok(num), Ok(den))) if den != 0 => {
                        let value = num as f64 / den as f64;
                        if value < 1.0 || value > 240.0 {
                            ctx.push(
                                v.line,
                                v.column,
                                "E_BAD_FPS",
                                format!("fps {s:?} is outside the accepted range 1 to 240"),
                                "use an integer or exact rational fps within 1 to 240",
                            );
                            Fps::from_int(30)
                        } else {
                            Fps { num, den }
                        }
                    }
                    _ => {
                        ctx.push(
                            v.line,
                            v.column,
                            "E_BAD_FPS",
                            format!("fps {s:?} is not an exact rational"),
                            "use an integer like `fps=30` or a rational like `fps=\"30000/1001\"`",
                        );
                        Fps::from_int(30)
                    }
                }
            }
            _ => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_FPS",
                    "fps must be an integer or an exact rational; decimal fps is rejected",
                    "use an integer like `fps=30` or a rational like `fps=\"30000/1001\"`",
                );
                Fps::from_int(30)
            }
        },
    };

    let background = match find(root, "background").and_then(as_str) {
        Some(s) => match parse_color(s) {
            Some(c) => c,
            None => {
                let v = find(root, "background").unwrap();
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_COLOR",
                    format!("background {s:?} is not a `#rgb` or `#rrggbb` color"),
                    "use a hex color like `background=\"#1a1a2e\"`",
                );
                notation::model::Rgb { r: 0, g: 0, b: 0 }
            }
        },
        None => {
            ctx.push(
                root.name_line,
                root.name_column,
                "E_MISSING_FIELD",
                "composition is missing required `background`",
                "add `background=\"#rrggbb\"` to the composition root",
            );
            notation::model::Rgb { r: 0, g: 0, b: 0 }
        }
    };

    let version = match find(root, "version").and_then(as_int) {
        Some(n) if n >= 1 => n as u64,
        Some(n) => {
            let v = find(root, "version").unwrap();
            ctx.push(
                v.line,
                v.column,
                "E_BAD_VERSION",
                format!("notation version {n} is invalid; versions start at 1"),
                "set `version=1` on the composition root",
            );
            1
        }
        None => {
            let is_present = find(root, "version").is_some();
            let (line, col) = find(root, "version")
                .map(|v| (v.line, v.column))
                .unwrap_or((root.name_line, root.name_column));
            ctx.push(
                line,
                col,
                if is_present {
                    "E_BAD_VERSION"
                } else {
                    "E_MISSING_FIELD"
                },
                "composition is missing required integer `version`",
                "add `version=1` to the composition root",
            );
            1
        }
    };

    // --- child elements --------------------------------------------------
    let mut clips: Vec<Clip> = Vec::new();
    let mut fonts: Vec<FontDecl> = Vec::new();
    // Motion directives validate after the clip pass so `target`
    // indexes resolve against the declared clip count (T003).
    let mut motion_nodes: Vec<(bool, &RawNode)> = Vec::new();
    for child in &root.children {
        match child.name.as_str() {
            "text" => {
                if let Some(c) = validate_text(&mut ctx, child) {
                    clips.push(Clip::Text(c));
                }
            }
            "video" => {
                if let Some(c) = validate_video(&mut ctx, child) {
                    clips.push(Clip::Video(c));
                }
            }
            "font" => {
                if let Some(f) = validate_font(&mut ctx, child) {
                    fonts.push(f);
                }
            }
            "box" => {
                if let Some(c) = validate_box(&mut ctx, child) {
                    clips.push(Clip::Box(c));
                }
            }
            "image" => {
                if let Some(c) = validate_image(&mut ctx, child) {
                    clips.push(Clip::Image(c));
                }
            }
            "audio" => {
                if let Some(c) = validate_audio(&mut ctx, child) {
                    clips.push(Clip::Audio(c));
                }
            }
            "tween" => motion_nodes.push((true, child)),
            "set" => motion_nodes.push((false, child)),
            other if unsupported_sprint(other).is_some() => {
                let sprint = unsupported_sprint(other).unwrap();
                ctx.diags.push(Diagnostic::new(
                    &ctx.file.clone(),
                    child.name_line,
                    child.name_column,
                    "E_UNSUPPORTED",
                    format!("element `{other}` is valid notation but needs Sprint {sprint}; it cannot render yet"),
                    format!("remove `{other}` or wait for Sprint {sprint}"),
                ));
            }
            other => {
                ctx.push(
                    child.name_line,
                    child.name_column,
                    "E_UNKNOWN_ELEMENT",
                    format!("unknown element `{other}`"),
                    "use `text`, `font`, `video`, `box`, `image`, `audio`, `tween`, or `set`, or remove the element",
                );
            }
        }
    }

    // --- motion directives (T003) --------------------------------------
    let clip_count = clips.len();
    let mut tweens: Vec<TweenDecl> = Vec::new();
    let mut sets: Vec<SetDecl> = Vec::new();
    for (is_tween, node) in motion_nodes {
        if is_tween {
            if let Some(t) = validate_tween(&mut ctx, node, clip_count) {
                tweens.push(t);
            }
        } else if let Some(s) = validate_set(&mut ctx, node, clip_count) {
            sets.push(s);
        }
    }

    let composition = Composition {
        name,
        width: width.unwrap_or(0.0) as u32,
        height: height.unwrap_or(0.0) as u32,
        duration: duration.unwrap_or(0.0),
        fps,
        background,
        version,
        clips,
        fonts,
        tweens,
        sets,
    };
    finish(ctx, Some(composition), ffmpeg_auto_download)
}

fn finish(ctx: Ctx<'_>, composition: Option<Composition>, ffmpeg_auto_download: bool) -> Verdict {
    let status = if ctx.invalid {
        Status::Invalid
    } else if ctx.diags.iter().any(|d| d.code == "E_UNSUPPORTED") {
        Status::Unsupported
    } else {
        Status::Valid
    };
    Verdict {
        status,
        composition: if status == Status::Invalid {
            None
        } else {
            composition
        },
        diagnostics: ctx.diags,
        ffmpeg_auto_download,
    }
}

/// Shared absolute start/duration check (T005). Returns the window on
/// success; every missing/negative/zero/relative shape is a diagnostic.
fn clip_window(ctx: &mut Ctx, node: &RawNode, what: &str) -> Option<(f64, f64)> {
    let mut bad = false;
    // `end=`-derived durations stay invalid in Sprint 1.
    if let Some(v) = find(node, "end") {
        ctx.push(
            v.line,
            v.column,
            "E_BAD_TIMING",
            format!("{what} uses `end=`; end-derived durations are not part of Sprint 1"),
            "declare an absolute `start=` and a written `duration=` instead",
        );
        bad = true;
    }
    let start = match find(node, "start") {
        Some(v) => match as_f64(v) {
            Some(x) if x.is_finite() && x >= 0.0 => x,
            Some(x) => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_TIMING",
                    format!("{what} start {x} is invalid; starts are absolute seconds >= 0"),
                    "set `start=` to an absolute non-negative time in seconds",
                );
                bad = true;
                0.0
            }
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_TIMING",
                    format!("{what} start is not an absolute time; relative starts are not part of Sprint 1"),
                    "set `start=` to an absolute non-negative time in seconds",
                );
                bad = true;
                0.0
            }
        },
        None => {
            ctx.push(
                node.name_line,
                node.name_column,
                "E_BAD_TIMING",
                format!("{what} is missing required `start`; every clip needs an absolute start"),
                "add `start=<seconds>` to the clip",
            );
            bad = true;
            0.0
        }
    };
    let duration = match find(node, "duration") {
        Some(v) => match as_f64(v) {
            Some(x) if x.is_finite() && x > 0.0 => x,
            Some(x) => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_TIMING",
                    format!("{what} duration {x} is invalid; durations must be positive"),
                    "set `duration=` to a positive time in seconds",
                );
                bad = true;
                0.0
            }
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_TIMING",
                    format!("{what} duration is not a plain duration in seconds"),
                    "set `duration=` to a positive time in seconds",
                );
                bad = true;
                0.0
            }
        },
        None => {
            ctx.push(
                node.name_line,
                node.name_column,
                "E_BAD_TIMING",
                format!("{what} is missing required `duration`; every clip needs a written duration"),
                "add `duration=<seconds>` to the clip",
            );
            bad = true;
            0.0
        }
    };
    if bad {
        return None;
    }
    Some((start, duration))
}

/// Explicit paint order: a bare integer (negative allowed); anything
/// else is a validation error, never a guessed arrangement.
fn parse_order(ctx: &mut Ctx, node: &RawNode) -> Option<i64> {
    match find(node, "order") {
        None => Some(0),
        Some(v) => match &v.kind {
            ValueKind::Int(n) => Some(*n),
            _ => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_ORDER",
                    "paint `order` must be a whole number; ties fall back to document order",
                    "set `order=<integer>` (for example `order=1`), or remove the key",
                );
                None
            }
        },
    }
}

/// A declared spacing value (`line-height` or `letter-spacing`, hyphen
/// or underscore spelling): absent means the font default, present must
/// be a non-negative finite number. Zero letter spacing is default
/// advance. Anything else is a validation error.
fn parse_spacing(ctx: &mut Ctx, node: &RawNode, hyphen: &str, underscore: &str) -> Option<Option<f64>> {
    let found = find(node, hyphen).or_else(|| find(node, underscore));
    match found {
        None => Some(None),
        Some(v) => match as_f64(v) {
            Some(x) if x.is_finite() && x >= 0.0 => Some(Some(x)),
            _ => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_SPACING",
                    format!("text `{hyphen}` must be a non-negative number of canvas pixels"),
                    format!("set `{hyphen}=<pixels >= 0>` or remove the key"),
                );
                None
            }
        },
    }
}

fn validate_text(ctx: &mut Ctx, node: &RawNode) -> Option<TextClip> {
    const KEYS: &[&str] = &[
        "value",
        "start",
        "duration",
        "x",
        "y",
        "size",
        "color",
        "position",
        "align",
        "wrap",
        "font-style",
        "font-family",
        "font_family",
        "order",
        "line-height",
        "line_height",
        "letter-spacing",
        "letter_spacing",
    ];
    for (k, v) in &node.props {
        if !KEYS.contains(&k.as_str()) && k != "end" {
            ctx.push(
                v.line,
                v.column,
                "E_UNKNOWN_KEY",
                format!("unknown text key {k:?}"),
                "remove the key or check the spelling",
            );
        }
    }
    let (start, duration) = clip_window(ctx, node, "text clip")?;
    let value = match find(node, "value").and_then(as_str) {
        Some(s) => s.to_string(),
        None => {
            ctx.push(
                node.name_line,
                node.name_column,
                "E_MISSING_FIELD",
                "text clip is missing required `value`",
                "add `value=\"...\"` to the text clip",
            );
            return None;
        }
    };
    // Plain helper (not a closure) so `ctx` stays freely reborrowable.
    fn num_prop(ctx: &mut Ctx, node: &RawNode, key: &str, default: f64) -> f64 {
        match find(node, key) {
            None => default,
            Some(v) => match as_f64(v) {
                Some(x) if x.is_finite() => x,
                _ => {
                    ctx.push(
                        v.line,
                        v.column,
                        "E_BAD_TIMING",
                        format!("text `{key}` must be a number"),
                        format!("set `{key}=<number>`"),
                    );
                    default
                }
            },
        }
    }
    let (x, y, size) = (
        num_prop(ctx, node, "x", 0.0),
        num_prop(ctx, node, "y", 0.0),
        num_prop(ctx, node, "size", 48.0),
    );
    if !size.is_finite() || size <= 0.0 {
        let (line, col) = find(node, "size")
            .map(|v| (v.line, v.column))
            .unwrap_or((node.name_line, node.name_column));
        ctx.push(
            line,
            col,
            "E_BAD_TIMING",
            format!("text `size` {size} is invalid; sizes must be positive"),
            "set `size=<positive number>` on the text clip",
        );
        return None;
    }
    let color = match find(node, "color") {
        None => notation::model::Rgb {
            r: 255,
            g: 255,
            b: 255,
        },
        Some(v) => match as_str(v).and_then(parse_color) {
            Some(c) => c,
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_COLOR",
                    "text color is not a `#rgb` or `#rrggbb` color",
                    "use a hex color like `color=\"#ffffff\"`",
                );
                return None;
            }
        },
    };
    // Optional placement sanity: negative placement is suspicious input.
    for key in ["x", "y"] {
        if let Some(v) = find(node, key).and_then(as_f64) {
            if v < 0.0 {
                let loc = find(node, key).unwrap();
                ctx.push(
                    loc.line,
                    loc.column,
                    "E_BAD_TIMING",
                    format!("text `{key}` {v} places content off-canvas"),
                    format!("set `{key}` to a non-negative canvas position"),
                );
            }
        }
    }
    // Alignment: exactly `left`, `center`, or `right`.
    let align = match find(node, "align") {
        None => notation::model::Align::Left,
        Some(v) => match as_str(v).and_then(notation::model::Align::parse) {
            Some(a) => a,
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_TIMING",
                    "text `align` must be exactly `left`, `center`, or `right`",
                    "set `align=\"left\"`, `align=\"center\"`, or `align=\"right\"`",
                );
                return None;
            }
        },
    };
    // Font style: exactly `regular`, `bold`, `italic`, or `bold-italic`.
    let style = match find(node, "font-style") {
        None => notation::model::FontStyle::Regular,
        Some(v) => match as_str(v).and_then(notation::model::FontStyle::parse) {
            Some(s) => s,
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_TIMING",
                    "text `font-style` must be exactly `regular`, `bold`, `italic`, or `bold-italic`",
                    "set `font-style=\"bold\"`, `font-style=\"italic\"`, or `font-style=\"bold-italic\"`",
                );
                return None;
            }
        },
    };
    // Wrapping: `off`, or a positive wrap width in canvas pixels.
    let wrap = match find(node, "wrap") {
        None => notation::model::Wrap::Off,
        Some(v) => {
            if let Some(s) = as_str(v) {
                if s == "off" {
                    notation::model::Wrap::Off
                } else if let Ok(w) = s.parse::<f64>() {
                    if w.is_finite() && w > 0.0 {
                        notation::model::Wrap::Width(w)
                    } else {
                        ctx.push(
                            v.line,
                            v.column,
                            "E_BAD_TIMING",
                            format!("text `wrap` {s:?} is invalid; use `\"off\"` or a positive width in pixels"),
                            "set `wrap=\"off\"` or `wrap=<positive pixels>`",
                        );
                        return None;
                    }
                } else {
                    ctx.push(
                        v.line,
                        v.column,
                        "E_BAD_TIMING",
                        format!("text `wrap` {s:?} is invalid; use `\"off\"` or a positive width in pixels"),
                        "set `wrap=\"off\"` or `wrap=<positive pixels>`",
                    );
                    return None;
                }
            } else if let Some(w) = as_f64(v) {
                if w.is_finite() && w > 0.0 {
                    notation::model::Wrap::Width(w)
                } else {
                    ctx.push(
                        v.line,
                        v.column,
                        "E_BAD_TIMING",
                        format!("text `wrap` {w} is invalid; widths must be positive"),
                        "set `wrap=\"off\"` or `wrap=<positive pixels>`",
                    );
                    return None;
                }
            } else {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_TIMING",
                    "text `wrap` must be `\"off\"` or a positive width in pixels",
                    "set `wrap=\"off\"` or `wrap=<positive pixels>`",
                );
                return None;
            }
        }
    };
    // Position: an `"X,Y"` pair feeding the `x`/`y` defaults; explicit
    // `x`/`y` win when both are present.
    if let Some(v) = find(node, "position") {
        match as_str(v) {
            Some(s) => {
                let bad = |ctx: &mut Ctx| {
                    ctx.push(
                        v.line,
                        v.column,
                        "E_BAD_TIMING",
                        format!("text `position` {s:?} is invalid; use `\"X,Y\"` canvas pixels"),
                        "set `position=\"80,320\"` or use `x=` and `y=`",
                    );
                };
                match s.split_once(',') {
                    Some((a, b)) => match (a.trim().parse::<f64>(), b.trim().parse::<f64>()) {
                        (Ok(px), Ok(py)) if px.is_finite() && py.is_finite() => {
                            // Accepted shape; explicit x/y override below.
                            let _ = (px, py);
                        }
                        _ => {
                            bad(ctx);
                            return None;
                        }
                    },
                    None => {
                        bad(ctx);
                        return None;
                    }
                }
            }
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_TIMING",
                    "text `position` must be an `\"X,Y\"` pair in canvas pixels",
                    "set `position=\"80,320\"` or use `x=` and `y=`",
                );
                return None;
            }
        }
    }
    // Resolve position defaults (explicit x/y already won above).
    let (mut px, mut py) = (x, y);
    if find(node, "x").is_none() || find(node, "y").is_none() {
        if let Some(s) = find(node, "position").and_then(as_str) {
            if let Some((a, b)) = s.split_once(',') {
                if let (Ok(vx), Ok(vy)) = (a.trim().parse::<f64>(), b.trim().parse::<f64>()) {
                    if find(node, "x").is_none() {
                        px = vx;
                    }
                    if find(node, "y").is_none() {
                        py = vy;
                    }
                }
            }
        }
    }
    let order = parse_order(ctx, node)?;
    let line_height = parse_spacing(ctx, node, "line-height", "line_height")?;
    let letter_spacing = parse_spacing(ctx, node, "letter-spacing", "letter_spacing")?;
    // Font family: CSS-like comma-separated list in try order. Only
    // syntax is validated here (a machine family may serve a name the
    // project does not declare); render refuses a list nothing resolves.
    let font_family = match find(node, "font-family").or_else(|| find(node, "font_family")) {
        None => None,
        Some(v) => match as_str(v) {
            Some(s) => {
                let names: Vec<String> = s
                    .split(',')
                    .map(str::trim)
                    .filter(|part| !part.is_empty())
                    .map(str::to_string)
                    .collect();
                if names.is_empty() {
                    ctx.push(
                        v.line,
                        v.column,
                        "E_BAD_FONT",
                        "text `font-family` names no family",
                        "set `font-family=\"Family Name\"` or `font-family=\"First, Fallback\"`, or remove the key for the project font",
                    );
                    return None;
                }
                Some(names)
            }
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_FONT",
                    "text `font-family` must be a quoted family list",
                    "set `font-family=\"Family Name\"` or `font-family=\"First, Fallback\"`, or remove the key for the project font",
                );
                return None;
            }
        },
    };
    Some(TextClip {
        value,
        start,
        duration,
        x: px,
        y: py,
        size,
        color,
        align,
        wrap,
        style,
        order,
        line_height,
        letter_spacing,
        font_family,
        line: node.name_line,
        column: node.name_column,
    })
}

fn validate_video(ctx: &mut Ctx, node: &RawNode) -> Option<VideoClip> {
    const KEYS: &[&str] = &[
        "path",
        "start",
        "duration",
        "fit",
        "order",
        "volume",
        "fade-in",
        "fade_in",
        "fade-out",
        "fade_out",
    ];
    for (k, v) in &node.props {
        if LATER_AUDIO_KEYS.contains(&k.as_str()) {
            ctx.diags.push(Diagnostic::new(
                &ctx.file.clone(),
                v.line,
                v.column,
                "E_UNSUPPORTED",
                format!("video clip `{k}` needs Sprint 10; Sprint 3 mixes video sound with volume and linear fades only"),
                "use `volume` with `fade-in`/`fade-out`, or wait for Sprint 10 sound proof",
            ));
            return None;
        }
        if !KEYS.contains(&k.as_str()) && k != "end" {
            ctx.push(
                v.line,
                v.column,
                "E_UNKNOWN_KEY",
                format!("unknown video key {k:?}"),
                "remove the key or check the spelling",
            );
        }
    }
    let (start, duration) = clip_window(ctx, node, "video clip")?;
    let path = match find(node, "path").and_then(as_str) {
        Some(s) => s.to_string(),
        None => {
            ctx.push(
                node.name_line,
                node.name_column,
                "E_MISSING_FIELD",
                "video clip is missing required `path`",
                "add `path=\"media/clip.mp4\"` to the video clip",
            );
            return None;
        }
    };
    // Referenced media must exist and be readable (T003).
    let resolved = ctx.folder.join(&path);
    if !resolved.is_file() {
        ctx.push(
            node.name_line,
            node.name_column,
            "E_MISSING_FILE",
            format!("video source {path:?} does not exist or is not readable"),
            "provide the file at the referenced path or fix the `path`",
        );
        return None;
    }
    let fit = match find(node, "fit") {
        None => Fit::Contain,
        Some(v) => match as_str(v).and_then(Fit::parse) {
            Some(f) => f,
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_FIT",
                    "video `fit` must be exactly one of `contain`, `cover`, `fill`",
                    "set `fit=\"contain\"`, `\"cover\"`, or `\"fill\"`",
                );
                return None;
            }
        },
    };
    let order = parse_order(ctx, node)?;
    // Sprint 3 video sound keys (T007): volume and linear fades shape the
    // clip's sound in the mix; timing validation for fades uses the clip
    // window validated above. Mixing itself belongs to T010.
    let volume_db = parse_volume_db(ctx, node, "video clip")?;
    let fade_in = parse_fade(ctx, node, "fade-in", "fade_in", duration, "video clip")?;
    let fade_out = parse_fade(ctx, node, "fade-out", "fade_out", duration, "video clip")?;
    Some(VideoClip {
        path,
        start,
        duration,
        fit,
        order,
        volume_db,
        fade_in,
        fade_out,
        line: node.name_line,
        column: node.name_column,
    })
}

/// Validate one Sprint 3 `audio` track (T007): a readable sound source
/// in a decodable format, an absolute half-open window, ordered
/// trim seconds into the source, a volume of silence through +12 dB,
/// and linear fades within the clip window. Every problem is a located
/// diagnostic; mixing and render-time decode refusal belong to T008.
fn validate_audio(ctx: &mut Ctx, node: &RawNode) -> Option<AudioClip> {
    const KEYS: &[&str] = &[
        "path",
        "start",
        "duration",
        "trim-start",
        "trim_start",
        "trim-end",
        "trim_end",
        "volume",
        "fade-in",
        "fade_in",
        "fade-out",
        "fade_out",
    ];
    for (k, v) in &node.props {
        if LATER_AUDIO_KEYS.contains(&k.as_str()) {
            ctx.diags.push(Diagnostic::new(
                &ctx.file.clone(),
                v.line,
                v.column,
                "E_UNSUPPORTED",
                format!("audio track `{k}` needs Sprint 10; Sprint 3 mixes trim, volume, and linear fades only"),
                "remove the key, or wait for Sprint 10 sound proof",
            ));
            return None;
        }
        if !KEYS.contains(&k.as_str()) && k != "end" {
            ctx.push(
                v.line,
                v.column,
                "E_UNKNOWN_KEY",
                format!("unknown audio key {k:?}"),
                "remove the key or check the spelling",
            );
        }
    }
    let (start, duration) = clip_window(ctx, node, "audio track")?;
    let path = match find(node, "path").and_then(as_str) {
        Some(s) => s.to_string(),
        None => {
            ctx.push(
                node.name_line,
                node.name_column,
                "E_MISSING_FIELD",
                "audio track is missing required `path`",
                "add `path=\"audio/music.wav\"` to the audio track",
            );
            return None;
        }
    };
    // Presence and readability come before format: a missing file names
    // the file, a format outside the Sprint 3 set names the file and
    // waits for a later sprint.
    if !ctx.folder.join(&path).is_file() {
        ctx.push(
            node.name_line,
            node.name_column,
            "E_MISSING_FILE",
            format!("audio source {path:?} does not exist or is not readable"),
            "provide the file at the referenced path or fix the `path`",
        );
        return None;
    }
    if !audio_format_supported(&path) {
        ctx.diags.push(Diagnostic::new(
            &ctx.file.clone(),
            node.name_line,
            node.name_column,
            "E_UNSUPPORTED",
            format!("audio source {path:?} needs a later sprint: Sprint 3 decodes WAV, MP3, FLAC, Ogg Opus, M4A/AAC, AIFF, WMA, and other FFmpeg-decodable audio only"),
            "point `path` at a supported audio file, or wait for the later audio-format sprint",
        ));
        return None;
    }
    let trim_of = |ctx: &mut Ctx, hyphen: &str, underscore: &str| -> Option<Option<f64>> {
        let found = find(node, hyphen).or_else(|| find(node, underscore));
        match found {
            None => Some(None),
            Some(v) => match as_f64(v) {
                Some(x) if x.is_finite() && x >= 0.0 => Some(Some(x)),
                _ => {
                    ctx.push(
                        v.line,
                        v.column,
                        "E_BAD_AUDIO",
                        format!("audio `{hyphen}` must be seconds into the source from 0"),
                        format!("set `{hyphen}=<seconds >= 0>` or remove the key"),
                    );
                    None
                }
            },
        }
    };
    let trim_start = trim_of(ctx, "trim-start", "trim_start")?.unwrap_or(0.0);
    let trim_end = trim_of(ctx, "trim-end", "trim_end")?;
    if let Some(end) = trim_end {
        // An empty window (end == start) is valid silence; a backward
        // window is refused, never swapped silently.
        if end < trim_start {
            let v = find(node, "trim-end").or_else(|| find(node, "trim_end")).unwrap();
            ctx.push(
                v.line,
                v.column,
                "E_BAD_AUDIO",
                format!("audio trim window {trim_start}..{end} runs backward; trims are seconds into the source"),
                "set `trim-end` after `trim-start`, or equal for an empty (silent) window",
            );
            return None;
        }
    }
    let volume_db = parse_volume_db(ctx, node, "audio track")?;
    let fade_in = parse_fade(ctx, node, "fade-in", "fade_in", duration, "audio track")?;
    let fade_out = parse_fade(ctx, node, "fade-out", "fade_out", duration, "audio track")?;
    Some(AudioClip {
        path,
        start,
        duration,
        trim_start,
        trim_end,
        volume_db,
        fade_in,
        fade_out,
        line: node.name_line,
        column: node.name_column,
    })
}

/// A finite box number with a default; non-numbers fail as `E_BAD_BOX`.
fn box_num(ctx: &mut Ctx, node: &RawNode, key: &str, default: f64) -> Option<f64> {
    match find(node, key) {
        None => Some(default),
        Some(v) => match as_f64(v) {
            Some(x) if x.is_finite() => Some(x),
            _ => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_BOX",
                    format!("box `{key}` must be a number"),
                    format!("set `{key}=<number>` on the box clip"),
                );
                None
            }
        },
    }
}

/// Box keys that name Sprint 8 looks: per-side borders, border images,
/// and non-solid fills. Well-formed but refused with the enabling
/// sprint (T004), never rendered partially.
const RICH_BOX_KEYS: &[&str] = &[
    "border-top-width",
    "border_top_width",
    "border-right-width",
    "border_right_width",
    "border-bottom-width",
    "border_bottom_width",
    "border-left-width",
    "border_left_width",
    "border-image",
    "border_image",
    "gradient",
    "fill-image",
    "fill_image",
];

fn validate_box(ctx: &mut Ctx, node: &RawNode) -> Option<BoxClip> {
    const KEYS: &[&str] = &[
        "start",
        "duration",
        "x",
        "y",
        "width",
        "height",
        "fill",
        "radius",
        "border-width",
        "border_width",
        "border-color",
        "border_color",
        "opacity",
        "order",
    ];
    for (k, v) in &node.props {
        if RICH_BOX_KEYS.contains(&k.as_str()) {
            ctx.diags.push(Diagnostic::new(
                &ctx.file.clone(),
                v.line,
                v.column,
                "E_UNSUPPORTED",
                format!("box `{k}` needs Sprint 8; Sprint 2 boxes use one fill color, one corner-radius set, one border width with one border color, and one opacity"),
                "use a single `fill`, `radius`, `border-width` with one `border-color`, and `opacity`, or wait for Sprint 8 layout-and-look proof",
            ));
            return None;
        }
        if !KEYS.contains(&k.as_str()) && k != "end" {
            ctx.push(
                v.line,
                v.column,
                "E_UNKNOWN_KEY",
                format!("unknown box key {k:?}"),
                "remove the key or check the spelling",
            );
        }
    }
    let (start, duration) = clip_window(ctx, node, "box clip")?;
    let x = box_num(ctx, node, "x", 0.0)?;
    let y = box_num(ctx, node, "y", 0.0)?;
    // Size is required and must be positive: a missing, negative, or
    // zero size is refused, never drawn as a guessed rectangle.
    let mut size = |key: &str| -> Option<f64> {
        match find(node, key) {
            None => {
                ctx.push(
                    node.name_line,
                    node.name_column,
                    "E_BAD_BOX",
                    format!("box clip is missing required `{key}`"),
                    format!("add `{key}=<positive pixels>` to the box clip"),
                );
                None
            }
            Some(v) => match as_f64(v) {
                Some(val) if val.is_finite() && val > 0.0 => Some(val),
                _ => {
                    ctx.push(
                        v.line,
                        v.column,
                        "E_BAD_BOX",
                        format!("box `{key}` must be a positive number of canvas pixels"),
                        format!("set `{key}=<positive pixels>` on the box clip"),
                    );
                    None
                }
            },
        }
    };
    let width = size("width")?;
    let height = size("height")?;
    let fill = match find(node, "fill") {
        None => notation::model::Rgb { r: 255, g: 255, b: 255 },
        Some(v) => match as_str(v).and_then(parse_color) {
            Some(c) => c,
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_COLOR",
                    "box `fill` is not a `#rgb` or `#rrggbb` color",
                    "use a hex color like `fill=\"#ff0000\"`",
                );
                return None;
            }
        },
    };
    let radius = box_num(ctx, node, "radius", 0.0)?;
    if radius < 0.0 {
        let v = find(node, "radius").unwrap();
        ctx.push(
            v.line,
            v.column,
            "E_BAD_BOX",
            format!("box `radius` {radius} is invalid; corner radii cannot be negative"),
            "set `radius=<pixels >= 0>` on the box clip",
        );
        return None;
    }
    let border_key = if find(node, "border-width").is_some() {
        "border-width"
    } else {
        "border_width"
    };
    let border_width = box_num(ctx, node, border_key, 0.0)?;
    if border_width < 0.0 {
        let v = find(node, border_key).unwrap();
        ctx.push(
            v.line,
            v.column,
            "E_BAD_BOX",
            format!("box `border-width` {border_width} is invalid; border widths cannot be negative"),
            "set `border-width=<pixels >= 0>` on the box clip",
        );
        return None;
    }
    let color_key = if find(node, "border-color").is_some() {
        "border-color"
    } else {
        "border_color"
    };
    let border_color = match find(node, color_key) {
        None => notation::model::Rgb { r: 0, g: 0, b: 0 },
        Some(v) => match as_str(v).and_then(parse_color) {
            Some(c) => c,
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_COLOR",
                    "box `border-color` is not a `#rgb` or `#rrggbb` color",
                    "use a hex color like `border-color=\"#000000\"`",
                );
                return None;
            }
        },
    };
    let opacity = box_num(ctx, node, "opacity", 1.0)?;
    if !(0.0..=1.0).contains(&opacity) {
        let (line, col) = find(node, "opacity")
            .map(|v| (v.line, v.column))
            .unwrap_or((node.name_line, node.name_column));
        ctx.push(
            line,
            col,
            "E_BAD_BOX",
            format!("box `opacity` {opacity} is invalid; opacity runs 0 (transparent) to 1 (opaque)"),
            "set `opacity=<number from 0 to 1>` on the box clip",
        );
        return None;
    }
    let order = parse_order(ctx, node)?;
    Some(BoxClip {
        start,
        duration,
        x,
        y,
        width,
        height,
        fill,
        radius,
        border_width,
        border_color,
        opacity,
        order,
        line: node.name_line,
        column: node.name_column,
    })
}

/// Image file extensions the Sprint 2 renderer decodes. Anything else
/// (animated images, SVG, and every other format) is refused as
/// not-yet-supported with its enabling sprint, never rendered guessed.
fn image_format_supported(path: &str) -> bool {
    let lower = path.to_lowercase();
    lower.ends_with(".png") || lower.ends_with(".jpg") || lower.ends_with(".jpeg")
}

fn image_magic_ok(bytes: &[u8]) -> bool {
    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n";
    // PNG signature, or a JPEG start-of-image marker.
    (bytes.len() >= 8 && &bytes[..8] == PNG)
        || (bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] == 0xD8)
}

fn validate_image(ctx: &mut Ctx, node: &RawNode) -> Option<ImageClip> {
    const KEYS: &[&str] = &[
        "path",
        "start",
        "duration",
        "x",
        "y",
        "width",
        "height",
        "fit",
        "placement",
        "order",
    ];
    for (k, v) in &node.props {
        if !KEYS.contains(&k.as_str()) && k != "end" {
            ctx.push(
                v.line,
                v.column,
                "E_UNKNOWN_KEY",
                format!("unknown image key {k:?}"),
                "remove the key or check the spelling",
            );
        }
    }
    let (start, duration) = clip_window(ctx, node, "image clip")?;
    let path = match find(node, "path").and_then(as_str) {
        Some(s) => s.to_string(),
        None => {
            ctx.push(
                node.name_line,
                node.name_column,
                "E_MISSING_FIELD",
                "image clip is missing required `path`",
                "add `path=\"media/photo.png\"` to the image clip",
            );
            return None;
        }
    };
    // Presence and readability come before format: a missing file names
    // the file, an unsupported format names the file and its sprint.
    let abs = ctx.folder.join(&path);
    if !abs.is_file() {
        ctx.push(
            node.name_line,
            node.name_column,
            "E_MISSING_FILE",
            format!("image source {path:?} does not exist or is not readable"),
            "provide the file at the referenced path or fix the `path`",
        );
        return None;
    }
    if !image_format_supported(&path) {
        ctx.diags.push(Diagnostic::new(
            &ctx.file.clone(),
            node.name_line,
            node.name_column,
            "E_UNSUPPORTED",
            format!("image source {path:?} needs a later sprint: Sprint 2 decodes PNG and JPEG still images only"),
            "point `path` at a `.png`, `.jpg`, or `.jpeg` file, or wait for the later footage sprint",
        ));
        return None;
    }
    match std::fs::read(&abs) {
        Ok(bytes) => {
            if !image_magic_ok(&bytes) {
                ctx.push(
                    node.name_line,
                    node.name_column,
                    "E_BAD_IMAGE",
                    format!("image source {path:?} is corrupt or not a PNG/JPEG picture"),
                    "restore an undamaged PNG or JPEG file at the referenced path",
                );
                return None;
            }
        }
        Err(e) => {
            ctx.push(
                node.name_line,
                node.name_column,
                "E_UNREADABLE",
                format!("image source {path:?} cannot be read: {e}"),
                "make the image file readable or fix the `path`",
            );
            return None;
        }
    }
    let mut pos = |key: &str, default: f64| -> Option<f64> {
        match find(node, key) {
            None => Some(default),
            Some(v) => match as_f64(v) {
                Some(x) if x.is_finite() => Some(x),
                _ => {
                    ctx.push(
                        v.line,
                        v.column,
                        "E_BAD_IMAGE",
                        format!("image `{key}` must be a number of canvas pixels"),
                        format!("set `{key}=<number>` on the image clip"),
                    );
                    None
                }
            },
        }
    };
    let x = pos("x", 0.0)?;
    let y = pos("y", 0.0)?;
    let mut extent = |key: &str| -> Option<Option<f64>> {
        match find(node, key) {
            None => Some(None),
            Some(v) => match as_f64(v) {
                Some(val) if val.is_finite() && val > 0.0 => Some(Some(val)),
                _ => {
                    ctx.push(
                        v.line,
                        v.column,
                        "E_BAD_IMAGE",
                        format!("image `{key}` must be a positive number of canvas pixels"),
                        format!("set `{key}=<positive pixels>` on the image clip or remove the key"),
                    );
                    None
                }
            },
        }
    };
    let width = extent("width")?;
    let height = extent("height")?;
    let fit = match find(node, "fit") {
        None => Fit::Contain,
        Some(v) => match as_str(v).and_then(Fit::parse) {
            Some(f) => f,
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_FIT",
                    "image `fit` must be exactly one of `contain`, `cover`, `fill`",
                    "set `fit=\"contain\"`, `\"cover\"`, or `\"fill\"`",
                );
                return None;
            }
        },
    };
    let placement = match find(node, "placement") {
        None => Placement::center(),
        Some(v) => match as_str(v).and_then(Placement::parse) {
            Some(p) => p,
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_IMAGE",
                    "image `placement` must be a horizontal and vertical placement like `\"center\"`, `\"top-left\"`, or `\"left top\"`",
                    "set `placement=\"center\"` or a pair such as `placement=\"top-left\"`",
                );
                return None;
            }
        },
    };
    let order = parse_order(ctx, node)?;
    Some(ImageClip {
        path,
        start,
        duration,
        x,
        y,
        width,
        height,
        fit,
        placement,
        order,
        line: node.name_line,
        column: node.name_column,
    })
}

/// Parse a `tween` easing value (T003): the nineteen named curves, or
/// parameterized `out-back(n)`. Later easings are marked, never parsed.
enum EaseParse {
    Ok(EaseDecl),
    /// Well-formed but waiting for Sprint 7 motion proof.
    Later(String),
    /// Unknown name or malformed `out-back` parameter.
    Bad(String),
}

fn parse_ease_value(s: &str) -> EaseParse {
    let t = s.trim();
    match t {
        "linear" => EaseParse::Ok(EaseDecl::Linear),
        "in-quad" => EaseParse::Ok(EaseDecl::InQuad),
        "out-quad" => EaseParse::Ok(EaseDecl::OutQuad),
        "in-out-quad" => EaseParse::Ok(EaseDecl::InOutQuad),
        "in-cubic" => EaseParse::Ok(EaseDecl::InCubic),
        "out-cubic" => EaseParse::Ok(EaseDecl::OutCubic),
        "in-out-cubic" => EaseParse::Ok(EaseDecl::InOutCubic),
        "in-quart" => EaseParse::Ok(EaseDecl::InQuart),
        "out-quart" => EaseParse::Ok(EaseDecl::OutQuart),
        "in-out-quart" => EaseParse::Ok(EaseDecl::InOutQuart),
        "in-quint" => EaseParse::Ok(EaseDecl::InQuint),
        "out-quint" => EaseParse::Ok(EaseDecl::OutQuint),
        "in-out-quint" => EaseParse::Ok(EaseDecl::InOutQuint),
        "in-sine" => EaseParse::Ok(EaseDecl::InSine),
        "out-sine" => EaseParse::Ok(EaseDecl::OutSine),
        "in-out-sine" => EaseParse::Ok(EaseDecl::InOutSine),
        "in-expo" => EaseParse::Ok(EaseDecl::InExpo),
        "out-expo" => EaseParse::Ok(EaseDecl::OutExpo),
        "in-out-expo" => EaseParse::Ok(EaseDecl::InOutExpo),
        _ => {
            if t == "out-back" || t.starts_with("out-back") {
                // `out-back` without a parameter, or a malformed one, is
                // a validation error, never a guessed overshoot.
                match t.strip_prefix("out-back").and_then(|r| {
                    r.strip_prefix('(')
                        .and_then(|r| r.strip_suffix(')'))
                }) {
                    Some(inner) => match inner.trim().parse::<f64>() {
                        Ok(n) if n.is_finite() && n >= 0.0 => EaseParse::Ok(EaseDecl::OutBack(n)),
                        _ => EaseParse::Bad(format!(
                            "easing {t:?} needs a numeric overshoot, e.g. `ease=\"out-back(1.7)\"`"
                        )),
                    },
                    None => EaseParse::Bad(format!(
                        "easing {t:?} needs a numeric overshoot, e.g. `ease=\"out-back(1.7)\"`"
                    )),
                }
            } else if notation::model::EaseDecl::LATER.contains(&t)
                || t.starts_with("cubic-bezier(")
            {
                EaseParse::Later(t.to_string())
            } else {
                EaseParse::Bad(format!("unknown easing {t:?}"))
            }
        }
    }
}

/// Comma-joined accepted easing spellings for diagnostics.
fn accepted_easings() -> String {
    let mut names: Vec<&str> = notation::model::EaseDecl::NAMED.to_vec();
    names.push("out-back(n)");
    names.join(", ")
}

/// Shared motion target check (T003): a required bare integer naming a
/// clip by document order. Missing, non-integer, negative, or
/// out-of-range targets are validation errors, never guessed clips.
fn motion_target(ctx: &mut Ctx, node: &RawNode, what: &str, clip_count: usize) -> Option<usize> {
    match find(node, "target") {
        None => {
            ctx.push(
                node.name_line,
                node.name_column,
                "E_BAD_MOTION",
                format!("{what} is missing required `target`; every directive names one clip"),
                "add `target=<clip index>` (0 names the first clip in the document)",
            );
            None
        }
        Some(v) => match &v.kind {
            ValueKind::Int(n) if *n >= 0 && (*n as usize) < clip_count => Some(*n as usize),
            ValueKind::Int(n) => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_MOTION",
                    format!(
                        "{what} target {n} names no clip; the composition declares {clip_count} clip(s), indexed from 0"
                    ),
                    "set `target=` to a clip index from 0 to the last clip",
                );
                None
            }
            _ => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_MOTION",
                    format!("{what} `target` must be a whole clip index, indexed from 0"),
                    "set `target=<clip index>` (0 names the first clip in the document)",
                );
                None
            }
        },
    }
}

/// Shared motion property check (T003): exactly one of the five Sprint 2
/// properties. Later properties are marked with Sprint 7, never drawn.
fn motion_property(ctx: &mut Ctx, node: &RawNode, what: &str) -> Option<String> {
    match find(node, "property").and_then(as_str) {
        None => {
            ctx.push(
                node.name_line,
                node.name_column,
                "E_BAD_MOTION",
                format!("{what} is missing required `property`; every directive names what it moves"),
                "add `property=\"x\"`, `\"y\"`, `\"scale\"`, `\"rotation\"`, or `\"opacity\"`",
            );
            None
        }
        Some(s) if notation::model::MOTION_PROPERTIES.contains(&s) => Some(s.to_string()),
        Some(s) if notation::model::LATER_PROPERTIES.contains(&s) => {
            ctx.diags.push(Diagnostic::new(
                &ctx.file.clone(),
                node.name_line,
                node.name_column,
                "E_UNSUPPORTED",
                format!(
                    "{what} property {s:?} needs Sprint 7; Sprint 2 tweens only `x`, `y`, `scale`, `rotation`, and `opacity`"
                ),
                "move one of the five Sprint 2 properties, or wait for Sprint 7 motion proof",
            ));
            None
        }
        Some(s) => {
            ctx.push(
                find(node, "property").map(|v| v.line).unwrap_or(node.name_line),
                find(node, "property").map(|v| v.column).unwrap_or(node.name_column),
                "E_BAD_MOTION",
                format!(
                    "{what} property {s:?} is not animatable; Sprint 2 moves `x`, `y`, `scale`, `rotation`, and `opacity`"
                ),
                "set `property=` to one of `x`, `y`, `scale`, `rotation`, or `opacity`",
            );
            None
        }
    }
}

/// Per-property value range check (T003): opacity runs 0 to 1, every
/// other value must simply be a finite number. Out-of-range values are
/// refused, never clamped silently.
fn motion_value_ok(ctx: &mut Ctx, node: &RawNode, what: &str, property: &str, key: &str, v: f64) -> bool {
    if !v.is_finite() {
        let loc = find(node, key).unwrap();
        ctx.push(
            loc.line,
            loc.column,
            "E_BAD_MOTION",
            format!("{what} `{key}` is not a readable number"),
            format!("set `{key}=<number>` on the directive"),
        );
        return false;
    }
    if property == "opacity" && !(0.0..=1.0).contains(&v) {
        let loc = find(node, key).unwrap();
        ctx.push(
            loc.line,
            loc.column,
            "E_BAD_MOTION",
            format!("{what} `{key}` {v} is invalid; opacity runs 0 (transparent) to 1 (opaque)"),
            format!("set `{key}=<number from 0 to 1>` on the directive"),
        );
        return false;
    }
    true
}

fn validate_tween(ctx: &mut Ctx, node: &RawNode, clip_count: usize) -> Option<TweenDecl> {
    const KEYS: &[&str] = &[
        "target", "property", "kind", "start", "duration", "ease", "from", "to",
    ];
    for (k, v) in &node.props {
        if !KEYS.contains(&k.as_str()) && k != "end" {
            ctx.push(
                v.line,
                v.column,
                "E_UNKNOWN_KEY",
                format!("unknown tween key {k:?}"),
                "remove the key or check the spelling",
            );
        }
    }
    let mut ok = true;
    let target = motion_target(ctx, node, "tween", clip_count);
    ok &= target.is_some();
    let property = motion_property(ctx, node, "tween");
    ok &= property.is_some();
    let kind = match find(node, "kind").and_then(as_str) {
        None => TweenKind::To,
        Some(s) => match TweenKind::parse(s) {
            Some(k) => k,
            None => {
                let v = find(node, "kind").unwrap();
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_MOTION",
                    format!("tween `kind` {s:?} is not a tween form; use `to`, `from`, or `from-to`"),
                    "set `kind=\"to\"`, `kind=\"from\"`, or `kind=\"from-to\"` on the tween",
                );
                ok = false;
                TweenKind::To
            }
        },
    };
    // Window: absolute start plus positive duration, like clips.
    let window = clip_window(ctx, node, "tween");
    ok &= window.is_some();
    // Easing: nineteen names plus `out-back(n)`; anything else names
    // the accepted set, later easings wait for Sprint 7.
    let ease = match find(node, "ease").and_then(as_str) {
        None => EaseDecl::Linear,
        Some(s) => match parse_ease_value(s) {
            EaseParse::Ok(e) => e,
            EaseParse::Later(name) => {
                let v = find(node, "ease").unwrap();
                ctx.diags.push(Diagnostic::new(
                    &ctx.file.clone(),
                    v.line,
                    v.column,
                    "E_UNSUPPORTED",
                    format!("tween easing {name:?} needs Sprint 7; Sprint 2 eases with the nineteen named curves plus `out-back(n)`"),
                    "ease with a Sprint 2 curve, or wait for Sprint 7 motion proof",
                ));
                ok = false;
                EaseDecl::Linear
            }
            EaseParse::Bad(detail) => {
                let v = find(node, "ease").unwrap();
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_EASING",
                    format!("{detail}; accepted easings are: {}", accepted_easings()),
                    "set `ease=` to a named curve or `ease=\"out-back(1.7)\"`",
                );
                ok = false;
                EaseDecl::Linear
            }
        },
    };
    // Values: `to` needs `to`, `from` needs `from`, `from-to` needs
    // both; a value under the wrong form is refused, never reinterpreted.
    let need_from = matches!(kind, TweenKind::From | TweenKind::FromTo);
    let need_to = matches!(kind, TweenKind::To | TweenKind::FromTo);
    let mut read_value = |key: &str, needed: bool| -> Option<Option<f64>> {
        let present = find(node, key);
        if !needed {
            if let Some(v) = present {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_MOTION",
                    format!("tween `{key}` needs kind `{}`; a `{}` tween carries no `{key}`", kind.name(), kind.name()),
                    format!(
                        "remove `{key}`, or set `kind=\"from-to\"` to move between two given values"
                    ),
                );
                ok = false;
                return Some(None);
            }
        }
        if needed && present.is_none() {
            ctx.push(
                node.name_line,
                node.name_column,
                "E_BAD_MOTION",
                format!("tween kind `{}` is missing required `{key}`", kind.name()),
                format!("add `{key}=<number>` to the tween"),
            );
            ok = false;
            return Some(None);
        }
        match present {
            None => Some(None),
            Some(v) => match as_f64(v) {
                Some(x) => {
                    if let Some(p) = property.as_deref() {
                        if !motion_value_ok(ctx, node, "tween", p, key, x) {
                            ok = false;
                            return Some(None);
                        }
                    }
                    Some(Some(x))
                }
                None => {
                    ctx.push(
                        v.line,
                        v.column,
                        "E_BAD_MOTION",
                        format!("tween `{key}` is not a readable number"),
                        format!("set `{key}=<number>` on the tween"),
                    );
                    ok = false;
                    Some(None)
                }
            },
        }
    };
    let from = read_value("from", need_from)?;
    let to = read_value("to", need_to)?;
    if !ok {
        return None;
    }
    let (start, duration) = window.unwrap();
    Some(TweenDecl {
        target: target.unwrap(),
        property: property.unwrap(),
        kind,
        from,
        to,
        start,
        duration,
        ease,
        line: node.name_line,
        column: node.name_column,
    })
}

fn validate_set(ctx: &mut Ctx, node: &RawNode, clip_count: usize) -> Option<SetDecl> {
    const KEYS: &[&str] = &["target", "property", "start", "duration", "value"];
    for (k, v) in &node.props {
        if !KEYS.contains(&k.as_str()) && k != "end" {
            ctx.push(
                v.line,
                v.column,
                "E_UNKNOWN_KEY",
                format!("unknown set key {k:?}; a `set` holds one value with no easing"),
                "remove the key or check the spelling",
            );
        }
    }
    let mut ok = true;
    let target = motion_target(ctx, node, "set", clip_count);
    ok &= target.is_some();
    let property = motion_property(ctx, node, "set");
    ok &= property.is_some();
    let window = clip_window(ctx, node, "set");
    ok &= window.is_some();
    let value = match find(node, "value") {
        None => {
            ctx.push(
                node.name_line,
                node.name_column,
                "E_BAD_MOTION",
                "set is missing required `value`; every hold names the value it holds",
                "add `value=<number>` to the set directive",
            );
            ok = false;
            0.0
        }
        Some(v) => match as_f64(v) {
            Some(x) => {
                if let Some(p) = property.as_deref() {
                    if !motion_value_ok(ctx, node, "set", p, "value", x) {
                        ok = false;
                    }
                }
                x
            }
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_MOTION",
                    "set `value` is not a readable number",
                    "set `value=<number>` on the set directive",
                );
                ok = false;
                0.0
            }
        },
    };
    if !ok {
        return None;
    }
    let (start, duration) = window.unwrap();
    Some(SetDecl {
        target: target.unwrap(),
        property: property.unwrap(),
        value,
        start,
        duration,
        line: node.name_line,
        column: node.name_column,
    })
}

fn validate_font(ctx: &mut Ctx, node: &RawNode) -> Option<FontDecl> {
    for (k, v) in &node.props {
        if k != "path" {
            ctx.push(
                v.line,
                v.column,
                "E_UNKNOWN_KEY",
                format!("unknown font key {k:?}"),
                "a font declares only `path`",
            );
        }
    }
    let path = match find(node, "path").and_then(as_str) {
        Some(s) => s.to_string(),
        None => {
            ctx.push(
                node.name_line,
                node.name_column,
                "E_MISSING_FIELD",
                "font is missing required `path`",
                "add `path=\"fonts/Body.ttf\"` to the font",
            );
            return None;
        }
    };
    let abs = ctx.folder.join(&path);
    if !abs.is_file() {
        ctx.push(
            node.name_line,
            node.name_column,
            "E_MISSING_FILE",
            format!("font file {path:?} does not exist or is not readable"),
            "provide the font file at the referenced path or fix the `path`",
        );
        return None;
    }
    // A present font must be a format the CPU renderer can draw: no
    // silent substitution of a different font ever happens.
    match std::fs::read(&abs) {
        Ok(bytes) => {
            if let Err(reason) = glyph::font::check_supported(&bytes) {
                ctx.push(
                    node.name_line,
                    node.name_column,
                    "E_BAD_FONT",
                    format!("font file {path:?} is unsupported: {reason}"),
                    "point `path` at a TrueType (.ttf) or OpenType (.otf) font with TrueType outlines",
                );
                return None;
            }
        }
        Err(e) => {
            ctx.push(
                node.name_line,
                node.name_column,
                "E_UNREADABLE",
                format!("font file {path:?} cannot be read: {e}"),
                "make the font file readable or fix the `path`",
            );
            return None;
        }
    }
    Some(FontDecl {
        path,
        line: node.name_line,
        column: node.name_column,
    })
}


