//! Inspector row properties (Sprint 17, T007 / CAP-007, J-04, REC-001).
//!
//! Rows are the precise editing surface for everything gestures
//! cannot reach. Every row edit applies through the Sprint 15 patch
//! and history machinery ([`EditOp::SetRow`](crate::EditOp::SetRow)),
//! so row edits share the one undo system with canvas gestures,
//! timeline verbs, and headless edits: the forward op persists in the
//! SDK delta and [`RevertOp::RestoreRow`](crate::RevertOp::RestoreRow)
//! restores the exact prior rendered value, never an approximation.
//!
//! Row keys are CSS-like hyphenated style attributes (`font-size`,
//! `background-color`, `border-radius`, `object-fit`, `z-index`,
//! `line-height`, `letter-spacing`), exactly like the Sprint 2 style
//! vocabulary. Underscores read as hyphens; upper case folds. The
//! `font-family` try-order list passes through untouched — every
//! entry must be non-empty and comma-separated order is preserved,
//! while unknown families are never resolved or substituted here:
//! fail-closed resolution stays at render time, exactly like
//! `check`.
//!
//! Availability boundary: rows cover the render-supported 2D
//! property set only. Render-deferred rows (`filter`, `transform-3d`
//! and its `rotate`/`perspective`/`origin` keys) refuse with
//! [`E_SDK_UNSUPPORTED`](crate::E_SDK_UNSUPPORTED) naming their
//! milestone, so the inspector can mark them unavailable instead of
//! faking them.
//!
//! Refusal boundary: capability and value checks run before anything
//! changes. A refused row names the rule and the clip location, and
//! the row keeps its prior value — the staged composition and both
//! undo stacks stay exactly as they were.

use notation::model::{
    Align, BlendMode, Clip, Fit, FontStyle, Grade, DELAY_FEEDBACK_MAX, DELAY_MAX_SECS,
    GRADE_CONTRAST_RANGE, GRADE_EXPOSURE_RANGE, GRADE_SATURATION_RANGE, GRADE_TEMPERATURE_RANGE,
    GRADE_TINT_RANGE,
};

use crate::edits::{ClipRef, EditRefusal, EditRefusalKind, E_SDK_BAD_VALUE, E_SDK_UNSUPPORTED};

/// Milestone naming the render work that unlocks 3D rows: the
/// `transform-3d`, `rotate`, `perspective`, and `origin` keys stay
/// unavailable until the 3D milestone lands them.
pub const ROW_MILESTONE_3D: &str = "3D milestone";

/// Milestone naming the render work that unlocks filter rows: the
/// `filter` function syntax stays unavailable until the filter
/// milestone lands it.
pub const ROW_MILESTONE_FILTER: &str = "filter milestone";

/// Whether one row key may be attempted on the open composition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowAvailability {
    /// The row writes through [`EditOp::SetRow`](crate::EditOp::SetRow).
    Available,
    /// The row is real but render-deferred: show it unavailable with
    /// this milestone named, never fake it.
    Unavailable {
        /// Milestone that unlocks the row.
        milestone: &'static str,
    },
    /// The row key names nothing settable.
    Unknown,
}

/// Render-deferred row keys with the milestone each waits for, in
/// catalog order. These are genuine KDL keys the render milestones
/// have not landed yet — not guesses.
pub const DEFERRED_ROWS: &[(&str, &str)] = &[
    ("filter", ROW_MILESTONE_FILTER),
    ("transform-3d", ROW_MILESTONE_3D),
    ("rotate-x", ROW_MILESTONE_3D),
    ("rotate-y", ROW_MILESTONE_3D),
    ("rotation-x", ROW_MILESTONE_3D),
    ("rotation-y", ROW_MILESTONE_3D),
    ("perspective", ROW_MILESTONE_3D),
    ("origin", ROW_MILESTONE_3D),
];

/// Accepted row keys in catalog order (Text, Style, Layout, Motion,
/// Grade, Effects, Overlays, Media, Audio FX). Timing rows
/// (`start`, `duration`, `trim-start`) ride the existing
/// [`EditOp::Retiming`](crate::EditOp::Retiming) op instead and are
/// not settable here.
pub const ACCEPTED_ROWS: &[&str] = &[
    "value",
    "font-size",
    "color",
    "text-align",
    "font-style",
    "font-family",
    "line-height",
    "letter-spacing",
    "background-color",
    "border-width",
    "border-color",
    "border-radius",
    "opacity",
    "mix-blend-mode",
    "x",
    "y",
    "width",
    "height",
    "z-index",
    "object-fit",
    "filter-blur",
    "grade-exposure",
    "grade-contrast",
    "grade-saturation",
    "grade-temperature",
    "grade-tint",
    "effect-amount",
    "waveform",
    "visible",
    "loop",
    "rate",
    "source",
    "volume",
    "fade-in",
    "fade-out",
    "delay",
    "delay-feedback",
    "delay-mix",
    "reverb",
];

/// Fold one raw row key to its canonical CSS-like hyphenated
/// spelling: surrounding whitespace trims, upper case folds, and
/// underscores read as hyphens.
pub fn canonical_row_property(raw: &str) -> String {
    raw.trim().to_lowercase().replace('_', "-")
}

/// Classify one canonical row key: render-deferred rows name their
/// milestone, accepted rows are available, anything else is unknown.
pub fn row_availability(property: &str) -> RowAvailability {
    for (key, milestone) in DEFERRED_ROWS {
        if *key == property {
            return RowAvailability::Unavailable { milestone };
        }
    }
    if ACCEPTED_ROWS.contains(&property) {
        return RowAvailability::Available;
    }
    RowAvailability::Unknown
}

/// The reset default of one accepted row: a static authored value, an
/// inherited clear (the KDL omission default — the row resets by
/// clearing to `None`), or no default (required keys like `value`,
/// `width`, `height`, and `source` keep no default, so resetting
/// them refuses with the rule and the location).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowDefault {
    /// Reset writes this static value.
    Value(&'static str),
    /// Reset clears the row to its inherited KDL omission default.
    Inherit,
    /// The row keeps no default; reset refuses with rule + location.
    None,
}

/// Reset default of one accepted row key.
pub fn row_default_value(property: &str) -> Option<RowDefault> {
    let default = match property {
        "font-size" => RowDefault::Value("48"),
        "color" => RowDefault::Value("#ffffff"),
        "text-align" => RowDefault::Value("left"),
        "font-style" => RowDefault::Value("regular"),
        "font-family" => RowDefault::Inherit,
        "line-height" => RowDefault::Inherit,
        "letter-spacing" => RowDefault::Inherit,
        "background-color" => RowDefault::Value("#ffffff"),
        "border-width" => RowDefault::Value("0"),
        "border-color" => RowDefault::Value("#000000"),
        "border-radius" => RowDefault::Value("0"),
        "opacity" => RowDefault::Value("1"),
        "mix-blend-mode" => RowDefault::Value("normal"),
        "x" => RowDefault::Value("0"),
        "y" => RowDefault::Value("0"),
        "z-index" => RowDefault::Value("0"),
        "object-fit" => RowDefault::Value("contain"),
        "filter-blur" => RowDefault::Value("0"),
        "grade-exposure" => RowDefault::Inherit,
        "grade-contrast" => RowDefault::Inherit,
        "grade-saturation" => RowDefault::Inherit,
        "grade-temperature" => RowDefault::Inherit,
        "grade-tint" => RowDefault::Inherit,
        "effect-amount" => RowDefault::Value("1"),
        "waveform" => RowDefault::Value("false"),
        "visible" => RowDefault::Value("true"),
        "loop" => RowDefault::Value("false"),
        "rate" => RowDefault::Value("1"),
        "volume" => RowDefault::Value("0"),
        "fade-in" => RowDefault::Value("0"),
        "fade-out" => RowDefault::Value("0"),
        "delay" => RowDefault::Value("0"),
        "delay-feedback" => RowDefault::Value("0"),
        "delay-mix" => RowDefault::Value("0"),
        "reverb" => RowDefault::Value("0"),
        "value" | "width" | "height" | "source" => RowDefault::None,
        _ => return Option::None,
    };
    Option::Some(default)
}

fn bad_value(message: String) -> EditRefusal {
    EditRefusal {
        kind: EditRefusalKind::BadValue,
        code: E_SDK_BAD_VALUE,
        message,
    }
}

fn unsupported(message: String) -> EditRefusal {
    EditRefusal {
        kind: EditRefusalKind::Unsupported,
        code: E_SDK_UNSUPPORTED,
        message,
    }
}

/// Locate one refusal on its clip: the kind, the stable `id` when
/// declared (else the document index), and the source line and
/// column the clip parsed from.
pub(crate) fn row_location(clip: &Clip, idx: usize, target: &ClipRef) -> String {
    let (line, column) = clip.line_col();
    let name = match clip.id() {
        Some(id) => format!("{} {id:?}", clip.what()),
        None => format!("{} (clip index {idx})", clip.what()),
    };
    if line > 0 {
        format!("{name} at {target} (composition.kdl:{line}:{column})")
    } else {
        format!("{name} at {target}")
    }
}

/// Render one number the way rows show it: whole values read as
/// integers, anything else as written.
fn num_text(x: f64) -> String {
    if x.is_finite() && x == x.trunc() && x.abs() < 1e15 {
        format!("{}", x as i64)
    } else {
        format!("{x}")
    }
}

/// Render one color the way rows show it: lowercase `#rrggbb`.
fn rgb_text(c: &notation::model::Rgb) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
}

/// Parse one authored number: finite, else a bad value naming the
/// row and the location.
fn row_num(raw: &str, property: &str, loc: &str) -> Result<f64, EditRefusal> {
    match raw.trim().parse::<f64>() {
        Ok(x) if x.is_finite() => Ok(x),
        _ => Err(bad_value(format!(
            "sdk refused: `{property}` value {raw:?} on {loc} is not a finite number; write a plain number. Nothing was applied, previewed, or recorded."
        ))),
    }
}

/// Parse one authored color exactly like `check`: `#rgb` or
/// `#rrggbb`, else a bad value naming the row and the location.
fn row_rgb(raw: &str, property: &str, loc: &str) -> Result<notation::model::Rgb, EditRefusal> {
    notation::model::parse_color(raw.trim()).ok_or_else(|| {
        bad_value(format!(
            "sdk refused: `{property}` color {raw:?} on {loc} is not `#rgb` or `#rrggbb`; write a hex color. Nothing was applied, previewed, or recorded."
        ))
    })
}

/// Parse one authored boolean: exactly `true` or `false`.
fn row_bool(raw: &str, property: &str, loc: &str) -> Result<bool, EditRefusal> {
    match raw.trim() {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(bad_value(format!(
            "sdk refused: `{property}` value {raw:?} on {loc} is not `true` or `false`; write a boolean. Nothing was applied, previewed, or recorded."
        ))),
    }
}

/// Parse one `font-family` try-order list: comma-separated, every
/// entry non-empty, order preserved, never resolved or substituted —
/// unknown families pass through and fail closed at render time. An
/// empty entry refuses naming the list.
fn row_font_family(raw: &str, loc: &str) -> Result<Vec<String>, EditRefusal> {
    let names: Vec<String> = raw.split(',').map(|s| s.trim().to_string()).collect();
    if names.iter().any(|s| s.is_empty()) {
        return Err(bad_value(format!(
            "sdk refused: `font-family` list {raw:?} on {loc} carries an empty entry; write comma-separated family names like `\"Alpha, Beta\"`. Nothing was applied, previewed, or recorded."
        )));
    }
    Ok(names)
}

/// Parse one authored integer: whole finite numbers only.
fn row_int(raw: &str, property: &str, loc: &str) -> Result<i64, EditRefusal> {
    let x = row_num(raw, property, loc)?;
    if x != x.trunc() {
        return Err(bad_value(format!(
            "sdk refused: `{property}` value {raw:?} on {loc} is not a whole number; write an integer. Nothing was applied, previewed, or recorded."
        )));
    }
    Ok(x as i64)
}

/// Mutable grade of one clip, or `None` while audio grades rest
/// inert: audio clips carry a grade that never paints, so grade rows
/// refuse on them instead of staging a silent no-op.
fn clip_grade_mut(clip: &mut Clip) -> Option<&mut Grade> {
    match clip {
        Clip::Text(c) => Some(&mut c.grade),
        Clip::Video(c) => Some(&mut c.grade),
        Clip::Box(c) => Some(&mut c.grade),
        Clip::Flex(c) => Some(&mut c.grade),
        Clip::Image(c) => Some(&mut c.grade),
        Clip::Path(c) => Some(&mut c.grade),
        Clip::Svg(c) => Some(&mut c.grade),
        Clip::Chart(c) => Some(&mut c.grade),
        Clip::Map(c) => Some(&mut c.grade),
        Clip::Scene3d(c) => Some(&mut c.grade),
        Clip::Lottie(c) => Some(&mut c.grade),
        Clip::Audio(_) => None,
        Clip::Nest(c) => Some(&mut c.grade),
    }
}

/// Shared grade of one clip, or `None` on audio (see
/// [`clip_grade_mut`]).
fn clip_grade(clip: &Clip) -> Option<&Grade> {
    match clip {
        Clip::Text(c) => Some(&c.grade),
        Clip::Video(c) => Some(&c.grade),
        Clip::Box(c) => Some(&c.grade),
        Clip::Flex(c) => Some(&c.grade),
        Clip::Image(c) => Some(&c.grade),
        Clip::Path(c) => Some(&c.grade),
        Clip::Svg(c) => Some(&c.grade),
        Clip::Chart(c) => Some(&c.grade),
        Clip::Map(c) => Some(&c.grade),
        Clip::Scene3d(c) => Some(&c.grade),
        Clip::Lottie(c) => Some(&c.grade),
        Clip::Audio(_) => None,
        Clip::Nest(c) => Some(&c.grade),
    }
}

/// Render one optional number the way rows show it: a set value
/// reads as a number, a cleared (inherited) row reads empty.
fn opt_text(x: &Option<f64>) -> String {
    match x {
        Some(v) => num_text(*v),
        Option::None => String::new(),
    }
}

/// Live value of one accepted row on one clip, or `None` while the
/// row does not apply to this clip kind (the inspector skips those
/// rows). Accepted rows on the wrong kind never read here — they
/// refuse in [`apply_row_value`] instead, so a direct op still names
/// its rule.
pub fn row_live_value(clip: &Clip, property: &str) -> Option<String> {
    let value = match property {
        "value" => match clip {
            Clip::Text(c) => c.value.clone(),
            _ => return Option::None,
        },
        "font-size" => match clip {
            Clip::Text(c) => num_text(c.size),
            _ => return Option::None,
        },
        "color" => match clip {
            Clip::Text(c) => rgb_text(&c.color),
            _ => return Option::None,
        },
        "text-align" => match clip {
            Clip::Text(c) => match c.align {
                Align::Left => "left",
                Align::Center => "center",
                Align::Right => "right",
            }
            .to_string(),
            _ => return Option::None,
        },
        "font-style" => match clip {
            Clip::Text(c) => match c.style {
                FontStyle::Regular => "regular",
                FontStyle::Bold => "bold",
                FontStyle::Italic => "italic",
                FontStyle::BoldItalic => "bold-italic",
            }
            .to_string(),
            _ => return Option::None,
        },
        "font-family" => match clip {
            Clip::Text(c) => c.font_family.clone().unwrap_or_default().join(", "),
            _ => return Option::None,
        },
        "line-height" => match clip {
            Clip::Text(c) => opt_text(&c.line_height),
            _ => return Option::None,
        },
        "letter-spacing" => match clip {
            Clip::Text(c) => opt_text(&c.letter_spacing),
            _ => return Option::None,
        },
        "background-color" => match clip {
            Clip::Box(c) => rgb_text(&c.fill),
            Clip::Path(c) => rgb_text(&c.fill),
            _ => return Option::None,
        },
        "border-width" => match clip {
            Clip::Box(c) => num_text(c.border_width),
            Clip::Path(c) => num_text(c.border_width),
            _ => return Option::None,
        },
        "border-color" => match clip {
            Clip::Box(c) => rgb_text(&c.border_color),
            Clip::Path(c) => rgb_text(&c.border_color),
            _ => return Option::None,
        },
        "border-radius" => match clip {
            Clip::Box(c) => num_text(c.radius),
            _ => return Option::None,
        },
        "opacity" => match clip {
            Clip::Box(c) => num_text(c.opacity),
            Clip::Path(c) => num_text(c.opacity),
            _ => return Option::None,
        },
        "mix-blend-mode" => match clip {
            Clip::Box(c) => c.blend_mode.name().to_string(),
            Clip::Path(c) => c.blend_mode.name().to_string(),
            _ => return Option::None,
        },
        "x" => match clip {
            Clip::Text(c) => num_text(c.x),
            Clip::Box(c) => num_text(c.x),
            Clip::Image(c) => num_text(c.x),
            Clip::Svg(c) => num_text(c.x),
            Clip::Path(c) => num_text(c.x),
            Clip::Flex(c) => num_text(c.x),
            _ => return Option::None,
        },
        "y" => match clip {
            Clip::Text(c) => num_text(c.y),
            Clip::Box(c) => num_text(c.y),
            Clip::Image(c) => num_text(c.y),
            Clip::Svg(c) => num_text(c.y),
            Clip::Path(c) => num_text(c.y),
            Clip::Flex(c) => num_text(c.y),
            _ => return Option::None,
        },
        "width" => match clip {
            Clip::Box(c) => num_text(c.width),
            Clip::Image(c) => opt_text(&c.width),
            Clip::Svg(c) => num_text(c.width),
            Clip::Flex(c) => num_text(c.width),
            _ => return Option::None,
        },
        "height" => match clip {
            Clip::Box(c) => num_text(c.height),
            Clip::Image(c) => opt_text(&c.height),
            Clip::Svg(c) => num_text(c.height),
            Clip::Flex(c) => num_text(c.height),
            _ => return Option::None,
        },
        "z-index" => match clip {
            Clip::Audio(_) => return Option::None,
            _ => num_text(clip.order() as f64),
        },
        "object-fit" => match clip {
            Clip::Video(c) => match c.fit {
                Fit::Contain => "contain",
                Fit::Cover => "cover",
                Fit::Fill => "fill",
            }
            .to_string(),
            Clip::Image(c) => match c.fit {
                Fit::Contain => "contain",
                Fit::Cover => "cover",
                Fit::Fill => "fill",
            }
            .to_string(),
            Clip::Svg(c) => match c.fit {
                Fit::Contain => "contain",
                Fit::Cover => "cover",
                Fit::Fill => "fill",
            }
            .to_string(),
            _ => return Option::None,
        },
        "filter-blur" => match clip {
            Clip::Audio(_) => return Option::None,
            Clip::Text(c) => num_text(c.filter_blur),
            Clip::Video(c) => num_text(c.filter_blur),
            Clip::Box(c) => num_text(c.filter_blur),
            Clip::Image(c) => num_text(c.filter_blur),
            Clip::Path(c) => num_text(c.filter_blur),
            Clip::Svg(c) => num_text(c.filter_blur),
            _ => return Option::None,
        },
        "grade-exposure" => opt_text(&clip_grade(clip)?.exposure),
        "grade-contrast" => opt_text(&clip_grade(clip)?.contrast),
        "grade-saturation" => opt_text(&clip_grade(clip)?.saturation),
        "grade-temperature" => opt_text(&clip_grade(clip)?.temperature),
        "grade-tint" => opt_text(&clip_grade(clip)?.tint),
        "effect-amount" => match clip {
            Clip::Audio(_) => return Option::None,
            _ => num_text(clip.effects().first()?.amount),
        },
        "waveform" => match clip {
            Clip::Video(c) => c.waveform.enabled.to_string(),
            Clip::Audio(c) => c.waveform.enabled.to_string(),
            _ => return Option::None,
        },
        "visible" => (!clip.hidden()).to_string(),
        "loop" => match clip {
            Clip::Video(c) => c.loop_clip.to_string(),
            _ => return Option::None,
        },
        "rate" => match clip {
            Clip::Video(c) => num_text(c.rate),
            Clip::Audio(c) => num_text(c.rate),
            Clip::Nest(c) => num_text(c.rate),
            _ => return Option::None,
        },
        "source" => match clip {
            Clip::Video(c) => c.path.clone(),
            Clip::Image(c) => c.path.clone(),
            Clip::Audio(c) => c.path.clone(),
            Clip::Svg(c) => c.path.clone(),
            _ => return Option::None,
        },
        "volume" => match clip {
            Clip::Video(c) => num_text(c.volume_db),
            Clip::Audio(c) => num_text(c.volume_db),
            _ => return Option::None,
        },
        "fade-in" => match clip {
            Clip::Video(c) => num_text(c.fade_in),
            Clip::Audio(c) => num_text(c.fade_in),
            _ => return Option::None,
        },
        "fade-out" => match clip {
            Clip::Video(c) => num_text(c.fade_out),
            Clip::Audio(c) => num_text(c.fade_out),
            _ => return Option::None,
        },
        "delay" => match clip {
            Clip::Video(c) => num_text(c.fx.delay),
            Clip::Audio(c) => num_text(c.fx.delay),
            _ => return Option::None,
        },
        "delay-feedback" => match clip {
            Clip::Video(c) => num_text(c.fx.delay_feedback),
            Clip::Audio(c) => num_text(c.fx.delay_feedback),
            _ => return Option::None,
        },
        "delay-mix" => match clip {
            Clip::Video(c) => num_text(c.fx.delay_mix),
            Clip::Audio(c) => num_text(c.fx.delay_mix),
            _ => return Option::None,
        },
        "reverb" => match clip {
            Clip::Video(c) => num_text(c.fx.reverb),
            Clip::Audio(c) => num_text(c.fx.reverb),
            _ => return Option::None,
        },
        _ => return Option::None,
    };
    Option::Some(value)
}

/// Apply one row value to one clip, returning the prior rendered
/// value for the inverse. Capability and value checks run before
/// anything changes: a wrong-kind row refuses as unsupported, a bad
/// value refuses naming the accepted shape, and render-deferred
/// rows refuse naming their milestone — each with the rule and the
/// clip location, keeping the prior value.
pub(crate) fn apply_row_value(
    idx: usize,
    clip: &mut Clip,
    property: &str,
    value: &str,
    target: &ClipRef,
) -> Result<String, EditRefusal> {
    let loc = row_location(clip, idx, target);
    let what = clip.what();
    let prior = row_live_value(clip, property);
    // A row that reads no live value on this kind never writes
    // either: name the kinds it does write.
    let prior = prior.ok_or_else(|| {
        unsupported(format!(
            "sdk refused: `{property}` on {loc} writes {} only; this {what} keeps its prior value. Nothing was applied, previewed, or recorded.",
            row_kinds(property),
        ))
    })?;
    let text_only = || {
        unsupported(format!(
            "sdk refused: `{property}` on {loc} writes text clips only; this {what} keeps its prior value. Nothing was applied, previewed, or recorded."
        ))
    };
    match property {
        "value" => {
            let spans_empty = match clip {
                Clip::Text(c) => c.spans.is_empty(),
                _ => return Err(text_only()),
            };
            let slot = match clip {
                Clip::Text(c) => &mut c.value,
                _ => return Err(text_only()),
            };
            let next = value.to_string();
            if next.is_empty() && spans_empty {
                return Err(bad_value(format!(
                    "sdk refused: `value` on {loc} is empty and the clip carries no `span` runs; a text clip needs a `value` or spans. Write quoted text. Nothing was applied, previewed, or recorded."
                )));
            }
            *slot = next;
        }
        "font-size" => {
            let slot = match clip {
                Clip::Text(c) => &mut c.size,
                _ => return Err(text_only()),
            };
            let next = row_num(value, property, &loc)?;
            if next <= 0.0 {
                return Err(bad_value(format!(
                    "sdk refused: `font-size` {next} on {loc} is not positive; write positive canvas pixels. Nothing was applied, previewed, or recorded."
                )));
            }
            *slot = next;
        }
        "color" => {
            let slot = match clip {
                Clip::Text(c) => &mut c.color,
                _ => return Err(text_only()),
            };
            *slot = row_rgb(value, property, &loc)?;
        }
        "text-align" => {
            let slot = match clip {
                Clip::Text(c) => &mut c.align,
                _ => return Err(text_only()),
            };
            *slot = match value.trim() {
                "left" => Align::Left,
                "center" => Align::Center,
                "right" => Align::Right,
                _ => {
                    return Err(bad_value(format!(
                        "sdk refused: `text-align` {value:?} on {loc} is unknown; write `left`, `center`, or `right`. Nothing was applied, previewed, or recorded."
                    )))
                }
            };
        }
        "font-style" => {
            let slot = match clip {
                Clip::Text(c) => &mut c.style,
                _ => return Err(text_only()),
            };
            *slot = match value.trim() {
                "regular" => FontStyle::Regular,
                "bold" => FontStyle::Bold,
                "italic" => FontStyle::Italic,
                "bold-italic" => FontStyle::BoldItalic,
                _ => {
                    return Err(bad_value(format!(
                        "sdk refused: `font-style` {value:?} on {loc} is unknown; write `regular`, `bold`, `italic`, or `bold-italic`. Nothing was applied, previewed, or recorded."
                    )))
                }
            };
        }
        "font-family" => {
            let slot = match clip {
                Clip::Text(c) => &mut c.font_family,
                _ => return Err(text_only()),
            };
            if value.trim().is_empty() {
                *slot = Option::None;
            } else {
                *slot = Option::Some(row_font_family(value, &loc)?);
            }
        }
        "line-height" => {
            let slot = match clip {
                Clip::Text(c) => &mut c.line_height,
                _ => return Err(text_only()),
            };
            if value.trim().is_empty() {
                *slot = Option::None;
            } else {
                let next = row_num(value, property, &loc)?;
                if next < 0.0 {
                    return Err(bad_value(format!(
                        "sdk refused: `line-height` {next} on {loc} runs negative; write canvas pixels from 0, or clear the row to inherit. Nothing was applied, previewed, or recorded."
                    )));
                }
                *slot = Option::Some(next);
            }
        }
        "letter-spacing" => {
            let slot = match clip {
                Clip::Text(c) => &mut c.letter_spacing,
                _ => return Err(text_only()),
            };
            if value.trim().is_empty() {
                *slot = Option::None;
            } else {
                let next = row_num(value, property, &loc)?;
                if next < 0.0 {
                    return Err(bad_value(format!(
                        "sdk refused: `letter-spacing` {next} on {loc} runs negative; write pixels from 0, or clear the row to inherit. Nothing was applied, previewed, or recorded."
                    )));
                }
                *slot = Option::Some(next);
            }
        }
        "background-color" => {
            let slot = match clip {
                Clip::Box(c) => &mut c.fill,
                Clip::Path(c) => &mut c.fill,
                _ => {
                    return Err(unsupported(format!(
                        "sdk refused: `background-color` on {loc} writes `box` and `path` clips only; this {} keeps its prior value. Nothing was applied, previewed, or recorded.",
                        what
                    )))
                }
            };
            *slot = row_rgb(value, property, &loc)?;
        }
        "border-width" => {
            let slot = match clip {
                Clip::Box(c) => &mut c.border_width,
                Clip::Path(c) => &mut c.border_width,
                _ => {
                    return Err(unsupported(format!(
                        "sdk refused: `border-width` on {loc} writes `box` and `path` clips only; this {} keeps its prior value. Nothing was applied, previewed, or recorded.",
                        what
                    )))
                }
            };
            let next = row_num(value, property, &loc)?;
            if next < 0.0 {
                return Err(bad_value(format!(
                    "sdk refused: `border-width` {next} on {loc} runs negative; border widths cannot be negative. Nothing was applied, previewed, or recorded."
                )));
            }
            *slot = next;
        }
        "border-color" => {
            let slot = match clip {
                Clip::Box(c) => &mut c.border_color,
                Clip::Path(c) => &mut c.border_color,
                _ => {
                    return Err(unsupported(format!(
                        "sdk refused: `border-color` on {loc} writes `box` and `path` clips only; this {} keeps its prior value. Nothing was applied, previewed, or recorded.",
                        what
                    )))
                }
            };
            *slot = row_rgb(value, property, &loc)?;
        }
        "border-radius" => {
            let slot = match clip {
                Clip::Box(c) => &mut c.radius,
                _ => {
                    return Err(unsupported(format!(
                        "sdk refused: `border-radius` on {loc} writes `box` clips only; this {} keeps its prior value. Nothing was applied, previewed, or recorded.",
                        what
                    )))
                }
            };
            let next = row_num(value, property, &loc)?;
            if next < 0.0 {
                return Err(bad_value(format!(
                    "sdk refused: `border-radius` {next} on {loc} runs negative; corner radii cannot be negative. Nothing was applied, previewed, or recorded."
                )));
            }
            *slot = next;
        }
        "opacity" => {
            let slot = match clip {
                Clip::Box(c) => &mut c.opacity,
                Clip::Path(c) => &mut c.opacity,
                _ => {
                    return Err(unsupported(format!(
                        "sdk refused: `opacity` on {loc} writes `box` and `path` clips only; this {} keeps its prior value. Nothing was applied, previewed, or recorded.",
                        what
                    )))
                }
            };
            let next = row_num(value, property, &loc)?;
            if !(0.0..=1.0).contains(&next) {
                return Err(bad_value(format!(
                    "sdk refused: `opacity` {next} on {loc} leaves 0 to 1; write an opacity from 0 to 1. Nothing was applied, previewed, or recorded."
                )));
            }
            *slot = next;
        }
        "mix-blend-mode" => {
            let slot = match clip {
                Clip::Box(c) => &mut c.blend_mode,
                Clip::Path(c) => &mut c.blend_mode,
                _ => {
                    return Err(unsupported(format!(
                        "sdk refused: `mix-blend-mode` on {loc} writes `box` and `path` clips only; this {} keeps its prior value. Nothing was applied, previewed, or recorded.",
                        what
                    )))
                }
            };
            *slot = match value.trim() {
                "normal" => BlendMode::Normal,
                "multiply" => BlendMode::Multiply,
                "screen" => BlendMode::Screen,
                "overlay" => BlendMode::Overlay,
                _ => {
                    return Err(bad_value(format!(
                        "sdk refused: `mix-blend-mode` {value:?} on {loc} is unknown; write one of {}. Nothing was applied, previewed, or recorded.",
                        BlendMode::ACCEPTED.join(", ")
                    )))
                }
            };
        }
        "x" | "y" => {
            let slot = match clip {
                Clip::Text(c) => {
                    if property == "x" {
                        &mut c.x
                    } else {
                        &mut c.y
                    }
                }
                Clip::Box(c) => {
                    if property == "x" {
                        &mut c.x
                    } else {
                        &mut c.y
                    }
                }
                Clip::Image(c) => {
                    if property == "x" {
                        &mut c.x
                    } else {
                        &mut c.y
                    }
                }
                Clip::Svg(c) => {
                    if property == "x" {
                        &mut c.x
                    } else {
                        &mut c.y
                    }
                }
                Clip::Path(c) => {
                    if property == "x" {
                        &mut c.x
                    } else {
                        &mut c.y
                    }
                }
                Clip::Flex(c) => {
                    if property == "x" {
                        &mut c.x
                    } else {
                        &mut c.y
                    }
                }
                _ => {
                    return Err(unsupported(format!(
                        "sdk refused: `{property}` on {loc} writes placed clips only; this {} carries no canvas position and keeps its prior value. Nothing was applied, previewed, or recorded.",
                        what
                    )))
                }
            };
            *slot = row_num(value, property, &loc)?;
        }
        "width" | "height" => {
            // Image sizes are optional (omitted means the source
            // size), so clearing the row inherits; every other sized
            // clip requires a positive size with no default.
            if let Clip::Image(c) = clip {
                if value.trim().is_empty() {
                    if property == "width" {
                        c.width = Option::None;
                    } else {
                        c.height = Option::None;
                    }
                    return Ok(prior);
                }
                let next = row_num(value, property, &loc)?;
                if next <= 0.0 {
                    return Err(bad_value(format!(
                        "sdk refused: `{property}` {next} on {loc} is not positive; write positive canvas pixels. Nothing was applied, previewed, or recorded."
                    )));
                }
                if property == "width" {
                    c.width = Option::Some(next);
                } else {
                    c.height = Option::Some(next);
                }
                return Ok(prior);
            }
            let slot = match clip {
                Clip::Box(c) => {
                    if property == "width" {
                        &mut c.width
                    } else {
                        &mut c.height
                    }
                }
                Clip::Svg(c) => {
                    if property == "width" {
                        &mut c.width
                    } else {
                        &mut c.height
                    }
                }
                Clip::Flex(c) => {
                    if property == "width" {
                        &mut c.width
                    } else {
                        &mut c.height
                    }
                }
                _ => {
                    return Err(unsupported(format!(
                        "sdk refused: `{property}` on {loc} writes sized clips (`box`, `image`, `svg`, `flex`) only; this {} keeps its prior value. Nothing was applied, previewed, or recorded.",
                        what
                    )))
                }
            };
            if value.trim().is_empty() {
                return Err(bad_value(format!(
                    "sdk refused: `{property}` on {loc} is required and keeps no default; write positive canvas pixels. Nothing was applied, previewed, or recorded."
                )));
            }
            let next = row_num(value, property, &loc)?;
            if next <= 0.0 {
                return Err(bad_value(format!(
                    "sdk refused: `{property}` {next} on {loc} is not positive; write positive canvas pixels. Nothing was applied, previewed, or recorded."
                )));
            }
            *slot = next;
        }
        "z-index" => {
            if matches!(clip, Clip::Audio(_)) {
                return Err(unsupported(format!(
                    "sdk refused: `z-index` on {loc} writes picture clips only; audio tracks draw no picture and keep paint order 0. Nothing was applied, previewed, or recorded."
                )));
            }
            let next = row_int(value, property, &loc)?;
            match clip {
                Clip::Text(c) => c.order = next,
                Clip::Video(c) => c.order = next,
                Clip::Box(c) => c.order = next,
                Clip::Flex(c) => c.order = next,
                Clip::Image(c) => c.order = next,
                Clip::Path(c) => c.order = next,
                Clip::Svg(c) => c.order = next,
                Clip::Chart(c) => c.order = next,
                Clip::Map(c) => c.order = next,
                Clip::Scene3d(c) => c.order = next,
                Clip::Lottie(c) => c.order = next,
                Clip::Nest(c) => c.order = next,
                Clip::Audio(_) => unreachable!("audio refused above"),
            }
        }
        "object-fit" => {
            let slot = match clip {
                Clip::Video(c) => &mut c.fit,
                Clip::Image(c) => &mut c.fit,
                Clip::Svg(c) => &mut c.fit,
                _ => {
                    return Err(unsupported(format!(
                        "sdk refused: `object-fit` on {loc} writes `video`, `image`, and `svg` clips only; this {} keeps its prior value. Nothing was applied, previewed, or recorded.",
                        what
                    )))
                }
            };
            *slot = match value.trim() {
                "contain" => Fit::Contain,
                "cover" => Fit::Cover,
                "fill" => Fit::Fill,
                _ => {
                    return Err(bad_value(format!(
                        "sdk refused: `object-fit` {value:?} on {loc} is unknown; write `contain`, `cover`, or `fill`. Nothing was applied, previewed, or recorded."
                    )))
                }
            };
        }
        "filter-blur" => {
            let slot = match clip {
                Clip::Audio(_) => {
                    return Err(unsupported(format!(
                        "sdk refused: `filter-blur` on {loc} writes picture clips only; this {} keeps its prior value. Nothing was applied, previewed, or recorded.",
                        what
                    )))
                }
                Clip::Text(c) => &mut c.filter_blur,
                Clip::Video(c) => &mut c.filter_blur,
                Clip::Box(c) => &mut c.filter_blur,
                Clip::Image(c) => &mut c.filter_blur,
                Clip::Path(c) => &mut c.filter_blur,
                Clip::Svg(c) => &mut c.filter_blur,
                _ => {
                    return Err(unsupported(format!(
                        "sdk refused: `filter-blur` on {loc} writes picture clips with a blur stage only; this {} keeps its prior value. Nothing was applied, previewed, or recorded.",
                        what
                    )))
                }
            };
            let next = row_num(value, property, &loc)?;
            if next < 0.0 {
                return Err(bad_value(format!(
                    "sdk refused: `filter-blur` {next} on {loc} runs negative; write a blur radius from 0. Nothing was applied, previewed, or recorded."
                )));
            }
            *slot = next;
        }
        "grade-exposure" | "grade-contrast" | "grade-saturation" | "grade-temperature"
        | "grade-tint" => {
            let (slot, range) = match clip_grade_mut(clip) {
                Option::Some(grade) => match property {
                    "grade-exposure" => (&mut grade.exposure, GRADE_EXPOSURE_RANGE),
                    "grade-contrast" => (&mut grade.contrast, GRADE_CONTRAST_RANGE),
                    "grade-saturation" => (&mut grade.saturation, GRADE_SATURATION_RANGE),
                    "grade-temperature" => (&mut grade.temperature, GRADE_TEMPERATURE_RANGE),
                    _ => (&mut grade.tint, GRADE_TINT_RANGE),
                },
                Option::None => {
                    return Err(unsupported(format!(
                        "sdk refused: `{property}` on {loc} writes picture clips only; audio grades rest inert and this {} keeps its prior value. Nothing was applied, previewed, or recorded.",
                        what
                    )))
                }
            };
            if value.trim().is_empty() {
                *slot = Option::None;
            } else {
                let next = row_num(value, property, &loc)?;
                if next < range.0 || next > range.1 {
                    return Err(bad_value(format!(
                        "sdk refused: `{property}` {next} on {loc} leaves {} to {}; write a value in range. Nothing was applied, previewed, or recorded.",
                        range.0, range.1
                    )));
                }
                *slot = Option::Some(next);
            }
        }
        "effect-amount" => {
            if matches!(clip, Clip::Audio(_)) {
                return Err(unsupported(format!(
                    "sdk refused: `effect-amount` on {loc} writes picture clips only; audio effects rest inert and this {} keeps its prior value. Nothing was applied, previewed, or recorded.",
                    what
                )));
            }
            if clip.effects().is_empty() {
                return Err(unsupported(format!(
                    "sdk refused: `effect-amount` on {loc} tunes the first effect but the clip carries none; add an `effect` child first. Nothing was applied, previewed, or recorded."
                )));
            }
            let next = row_num(value, property, &loc)?;
            if !(0.0..=1.0).contains(&next) {
                return Err(bad_value(format!(
                    "sdk refused: `effect-amount` {next} on {loc} leaves 0 to 1; write an amount from 0 (pass-through) to 1 (full look). Nothing was applied, previewed, or recorded."
                )));
            }
            let effect = match clip {
                Clip::Text(c) => &mut c.effects[0],
                Clip::Video(c) => &mut c.effects[0],
                Clip::Box(c) => &mut c.effects[0],
                Clip::Flex(c) => &mut c.effects[0],
                Clip::Image(c) => &mut c.effects[0],
                Clip::Path(c) => &mut c.effects[0],
                Clip::Svg(c) => &mut c.effects[0],
                Clip::Chart(c) => &mut c.effects[0],
                Clip::Map(c) => &mut c.effects[0],
                Clip::Scene3d(c) => &mut c.effects[0],
                Clip::Lottie(c) => &mut c.effects[0],
                Clip::Nest(c) => &mut c.effects[0],
                Clip::Audio(_) => unreachable!("audio refused above"),
            };
            effect.amount = next;
        }
        "waveform" => {
            let slot = match clip {
                Clip::Video(c) => &mut c.waveform.enabled,
                Clip::Audio(c) => &mut c.waveform.enabled,
                _ => {
                    return Err(unsupported(format!(
                        "sdk refused: `waveform` on {loc} writes `video` and `audio` clips only; this {} keeps its prior value. Nothing was applied, previewed, or recorded.",
                        what
                    )))
                }
            };
            *slot = row_bool(value, property, &loc)?;
        }
        "visible" => {
            let next = row_bool(value, property, &loc)?;
            let hidden = !next;
            match clip {
                Clip::Text(c) => c.hidden = hidden,
                Clip::Video(c) => c.hidden = hidden,
                Clip::Box(c) => c.hidden = hidden,
                Clip::Flex(c) => c.hidden = hidden,
                Clip::Image(c) => c.hidden = hidden,
                Clip::Path(c) => c.hidden = hidden,
                Clip::Svg(c) => c.hidden = hidden,
                Clip::Chart(c) => c.hidden = hidden,
                Clip::Map(c) => c.hidden = hidden,
                Clip::Scene3d(c) => c.hidden = hidden,
                Clip::Lottie(c) => c.hidden = hidden,
                Clip::Audio(c) => c.hidden = hidden,
                Clip::Nest(c) => c.hidden = hidden,
            }
        }
        "loop" => {
            let slot = match clip {
                Clip::Video(c) => &mut c.loop_clip,
                _ => {
                    return Err(unsupported(format!(
                        "sdk refused: `loop` on {loc} writes `video` clips only; this {} keeps its prior value. Nothing was applied, previewed, or recorded.",
                        what
                    )))
                }
            };
            *slot = row_bool(value, property, &loc)?;
        }
        "rate" => {
            let slot = match clip {
                Clip::Video(c) => &mut c.rate,
                Clip::Audio(c) => &mut c.rate,
                Clip::Nest(c) => &mut c.rate,
                _ => {
                    return Err(unsupported(format!(
                        "sdk refused: `rate` on {loc} writes `video`, `audio`, and `nest` clips only; this {} keeps its prior value. Nothing was applied, previewed, or recorded.",
                        what
                    )))
                }
            };
            let next = row_num(value, property, &loc)?;
            if !(0.1..=10.0).contains(&next) {
                return Err(bad_value(format!(
                    "sdk refused: `rate` {next} on {loc} is outside the accepted range 0.1 to 10; write a playback rate in range. Nothing was applied, previewed, or recorded."
                )));
            }
            *slot = next;
        }
        "source" => {
            let slot = match clip {
                Clip::Video(c) => &mut c.path,
                Clip::Image(c) => &mut c.path,
                Clip::Audio(c) => &mut c.path,
                Clip::Svg(c) => &mut c.path,
                _ => {
                    return Err(unsupported(format!(
                        "sdk refused: `source` on {loc} writes `video`, `image`, `audio`, and `svg` clips only; this {} keeps its prior value. Nothing was applied, previewed, or recorded.",
                        what
                    )))
                }
            };
            if value.trim().is_empty() {
                return Err(bad_value(format!(
                    "sdk refused: `source` on {loc} is required and keeps no default; write a media path. Nothing was applied, previewed, or recorded."
                )));
            }
            *slot = value.trim().to_string();
        }
        "volume" => {
            let slot = match clip {
                Clip::Video(c) => &mut c.volume_db,
                Clip::Audio(c) => &mut c.volume_db,
                _ => {
                    return Err(unsupported(format!(
                        "sdk refused: `volume` on {loc} writes `video` and `audio` clips only; this {} keeps its prior value. Nothing was applied, previewed, or recorded.",
                        what
                    )))
                }
            };
            let text = value.trim();
            if text == "-inf" {
                *slot = f64::NEG_INFINITY;
            } else {
                let next = row_num(value, property, &loc)?;
                if next > 12.0 {
                    return Err(bad_value(format!(
                        "sdk refused: `volume` {next} on {loc} passes +12 dB; write silence (`-inf`) or at most +12 dB. Nothing was applied, previewed, or recorded."
                    )));
                }
                *slot = next;
            }
        }
        "fade-in" | "fade-out" => {
            let duration = clip.duration();
            let slot = match clip {
                Clip::Video(c) => {
                    if property == "fade-in" {
                        &mut c.fade_in
                    } else {
                        &mut c.fade_out
                    }
                }
                Clip::Audio(c) => {
                    if property == "fade-in" {
                        &mut c.fade_in
                    } else {
                        &mut c.fade_out
                    }
                }
                _ => {
                    return Err(unsupported(format!(
                        "sdk refused: `{property}` on {loc} writes `video` and `audio` clips only; this {} keeps its prior value. Nothing was applied, previewed, or recorded.",
                        what
                    )))
                }
            };
            let next = row_num(value, property, &loc)?;
            if !(0.0..=duration).contains(&next) {
                return Err(bad_value(format!(
                    "sdk refused: `{property}` {next} on {loc} leaves the clip window 0 to {duration}; a fade longer than its clip is refused. Nothing was applied, previewed, or recorded."
                )));
            }
            *slot = next;
        }
        "delay" | "delay-feedback" | "delay-mix" | "reverb" => {
            let (slot, max) = match clip {
                Clip::Video(c) => match property {
                    "delay" => (&mut c.fx.delay, DELAY_MAX_SECS),
                    "delay-feedback" => (&mut c.fx.delay_feedback, DELAY_FEEDBACK_MAX),
                    "delay-mix" => (&mut c.fx.delay_mix, 1.0),
                    _ => (&mut c.fx.reverb, 1.0),
                },
                Clip::Audio(c) => match property {
                    "delay" => (&mut c.fx.delay, DELAY_MAX_SECS),
                    "delay-feedback" => (&mut c.fx.delay_feedback, DELAY_FEEDBACK_MAX),
                    "delay-mix" => (&mut c.fx.delay_mix, 1.0),
                    _ => (&mut c.fx.reverb, 1.0),
                },
                _ => {
                    return Err(unsupported(format!(
                        "sdk refused: `{property}` on {loc} writes `video` and `audio` clips only; this {} keeps its prior value. Nothing was applied, previewed, or recorded.",
                        what
                    )))
                }
            };
            let next = row_num(value, property, &loc)?;
            if !(0.0..=max).contains(&next) {
                return Err(bad_value(format!(
                    "sdk refused: `{property}` {next} on {loc} leaves 0 to {max}; write a value in range. Nothing was applied, previewed, or recorded."
                )));
            }
            *slot = next;
        }
        _ => {
            return Err(bad_value(format!(
                "sdk refused: row key {property:?} on {loc} names nothing settable; write one of {}. Nothing was applied, previewed, or recorded.",
                ACCEPTED_ROWS.join(", ")
            )));
        }
    }
    Ok(prior)
}

/// Kinds one accepted row writes, for wrong-kind refusals.
fn row_kinds(property: &str) -> &'static str {
    match property {
        "value" | "font-size" | "color" | "text-align" | "font-style" | "font-family"
        | "line-height" | "letter-spacing" => "text clips",
        "background-color" | "border-width" | "border-color" | "opacity" | "mix-blend-mode" => {
            "`box` and `path` clips"
        }
        "border-radius" => "`box` clips",
        "x" | "y" => "placed clips",
        "width" | "height" => "`box`, `image`, `svg`, and `flex` clips",
        "z-index" => "picture clips",
        "object-fit" => "`video`, `image`, and `svg` clips",
        "filter-blur" => "`text`, `video`, `box`, `image`, `path`, and `svg` clips",
        "grade-exposure" | "grade-contrast" | "grade-saturation" | "grade-temperature"
        | "grade-tint" => "picture clips",
        "effect-amount" => "picture clips carrying an `effect`",
        "waveform" | "volume" | "fade-in" | "fade-out" | "delay" | "delay-feedback"
        | "delay-mix" | "reverb" => "`video` and `audio` clips",
        "visible" => "every clip",
        "loop" => "`video` clips",
        "rate" => "`video`, `audio`, and `nest` clips",
        "source" => "`video`, `image`, `audio`, and `svg` clips",
        _ => "its clips",
    }
}
