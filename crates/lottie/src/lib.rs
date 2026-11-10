//! Native Lottie and dotLottie decoder plus film-time sampler (Sprint 21,
//! T001 / CAP-001).
//!
//! Pure Rust, no browser, no JavaScript, no network: a `.json` file holds
//! one bodymovin animation, a `.lottie` file is a zip holding
//! `manifest.json` plus `animations/<id>.json`. Decoding is fail-closed:
//! an undecodable file, an unsupported layer or shape kind, or a corrupt
//! zip refuses with an `Err` naming the cause, never a guessed picture.
//!
//! Covered subset (everything T001 promises): solid shape layers (`ty=4`)
//! with groups, rects, ellipses, fills, strokes, and transforms;
//! static and linearly-interpolated keyframed transform channels;
//! markers as named ranges; the manifest `loop` flag as the default
//! loop mode. Expressions evaluate to their baked values: any property
//! carrying an expression program (`"x"` beside `"k"`) reads its baked
//! `"k"` channel and never runs the program, so renders stay
//! deterministic. Anything outside the subset (image or text layers,
//! masks, gradients, effects, time remap) refuses at decode with the
//! offending kind named.
//!
//! Time model: the animation window is `[0, len)` animation seconds,
//! where `len = (op - ip) / fr`. [`sample_time`] maps an unwrapped
//! source time to the time-correct animation instant for a loop mode.

use serde::Deserialize;

/// How an animation maps an unwrapped source time to an animation
/// instant (Sprint 21, T001 / CAP-001): `none` holds the edge frames
/// past the window (like non-looping footage), `loop` repeats the
/// window, `ping-pong` alternates direction every pass. Hyphenated
/// spellings, exactly like style attributes; anything else is refused
/// by the caller naming the mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum LoopMode {
    None,
    Loop,
    PingPong,
}

impl LoopMode {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "none" => Some(LoopMode::None),
            "loop" => Some(LoopMode::Loop),
            "ping-pong" => Some(LoopMode::PingPong),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            LoopMode::None => "none",
            LoopMode::Loop => "loop",
            LoopMode::PingPong => "ping-pong",
        }
    }

    /// Accepted spellings, for diagnostics.
    pub const ACCEPTED: [&'static str; 3] = ["none", "loop", "ping-pong"];
}

/// One marker as a named range in animation seconds (Sprint 21, T001 /
/// CAP-001): bodymovin `tm`/`dr` are animation frames, divided by the
/// frame rate here. Only in-window ranges survive decode; out-of-range
/// markers are reported separately as drops.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LottieMarker {
    pub name: String,
    pub start: f64,
    pub end: f64,
}

/// One marker dropped at decode (Sprint 21, T001 / spec section 11):
/// the marker lies outside the animation window (or is empty), so it
/// cannot name a range. Reported with its numbers, never silently.
#[derive(Debug, Clone, PartialEq)]
pub struct DroppedMarker {
    pub name: String,
    pub start: f64,
    pub end: f64,
    pub detail: String,
}

/// One decoded animation (Sprint 21, T001): intrinsic size, timing, the
/// default loop flag from the file, in-window markers, and the shape
/// layers in document order (layer 0 paints on top).
#[derive(Debug, Clone, PartialEq)]
pub struct LottieDoc {
    pub width: f64,
    pub height: f64,
    pub frame_rate: f64,
    pub in_point: f64,
    pub out_point: f64,
    /// Default loop flag from the file (dotLottie manifest `loop`;
    /// plain Lottie JSON declares none, so false). An authored
    /// `loop-mode` overrides this; it never overrides the author.
    pub loop_default: bool,
    pub markers: Vec<LottieMarker>,
    pub dropped_markers: Vec<DroppedMarker>,
    pub layers: Vec<Layer>,
}

impl LottieDoc {
    /// Intrinsic animation length in seconds (`(op - ip) / fr`).
    /// Always positive after decode (refused otherwise).
    pub fn len_secs(&self) -> f64 {
        (self.out_point - self.in_point) / self.frame_rate
    }
}

/// One animated transform channel pair: every channel is static or
/// keyframed sel (Sprint 21, T001). Positions carry x/y (bodymovin
/// z is ignored: layers render flat), scales are percents, rotations
/// are degrees, opacities are 0-100.
#[derive(Debug, Clone, PartialEq)]
pub struct Transform {
    pub position: AnimVec,
    pub anchor: AnimVec,
    pub scale: AnimVec,
    pub rotation: AnimScalar,
    pub opacity: AnimScalar,
}

/// One shape-layer group: geometry plus its style plus the transform
/// applied to everything the group holds (Sprint 21, T001). A group
/// holds at most one fill and one stroke; more is refused at decode
/// (fail-closed), so rendering never guesses which paint wins.
#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    pub items: Vec<ShapeItem>,
    pub fill: Option<Fill>,
    pub stroke: Option<Stroke>,
    pub transform: Transform,
}

/// One drawable shape item (Sprint 21, T001 subset): groups nest,
/// rects and ellipses carry static geometry (their motion rides the
/// group transform, exactly like bodymovin authors it).
#[derive(Debug, Clone, PartialEq)]
pub enum ShapeItem {
    Group(Group),
    Rect {
        pos: [f64; 2],
        size: [f64; 2],
        round: f64,
    },
    Ellipse {
        pos: [f64; 2],
        size: [f64; 2],
    },
}

/// One solid fill (Sprint 21, T001): straight RGBA channels 0-1 plus
/// opacity 0-100, multiplied at draw time.
#[derive(Debug, Clone, PartialEq)]
pub struct Fill {
    pub color: [f64; 4],
    pub opacity: f64,
}

/// One solid stroke (Sprint 21, T001): straight RGB channels 0-1,
/// opacity 0-100, width in layer pixels.
#[derive(Debug, Clone, PartialEq)]
pub struct Stroke {
    pub color: [f64; 3],
    pub opacity: f64,
    pub width: f64,
}

/// One shape layer (Sprint 21, T001 subset, `ty=4`): a local time
/// window plus the layer transform plus groups in document order.
#[derive(Debug, Clone, PartialEq)]
pub struct Layer {
    pub name: String,
    /// Layer in/out points in animation frames; `start` offsets the
    /// layer clock (`st`).
    pub in_frame: f64,
    pub out_frame: f64,
    pub start_offset: f64,
    pub transform: Transform,
    pub shapes: Vec<ShapeItem>,
}

/// One scalar channel: a baked constant or keyframed stops
/// (Sprint 21, T001). An expression program (`"x"`) never runs: the
/// baked `"k"` channel below it is the value.
#[derive(Debug, Clone, PartialEq)]
pub enum AnimScalar {
    Static(f64),
    Keyed(Vec<ScalarKey>),
}

/// One vector channel: a baked constant or keyframed stops
/// (Sprint 21, T001). Same expression rule as scalars.
#[derive(Debug, Clone, PartialEq)]
pub enum AnimVec {
    Static(Vec<f64>),
    Keyed(Vec<VecKey>),
}

/// One scalar keyframe stop (Sprint 21, T001): `frame` is the
/// animation frame, `value` the stop value, `end` the segment end
/// (absent means the next stop's value), `hold` freezes the stop.
#[derive(Debug, Clone, PartialEq)]
pub struct ScalarKey {
    pub frame: f64,
    pub value: f64,
    pub end: Option<f64>,
    pub hold: bool,
}

/// One vector keyframe stop (Sprint 21, T001): same shape as scalars.
#[derive(Debug, Clone, PartialEq)]
pub struct VecKey {
    pub frame: f64,
    pub value: Vec<f64>,
    pub end: Option<Vec<f64>>,
    pub hold: bool,
}

impl AnimScalar {
    /// Value at animation frame `f` (Sprint 21, T001): the last stop
    /// at or before `f`, linearly interpolated across its segment
    /// (bodymovin bezier easing is out of subset: segments run
    /// linear, documented at decode). Held stops and past-end times
    /// freeze; before the first stop the first value applies.
    pub fn at(&self, f: f64) -> f64 {
        match self {
            AnimScalar::Static(v) => *v,
            AnimScalar::Keyed(keys) => {
                let mut prev: Option<&ScalarKey> = None;
                for k in keys {
                    if k.frame <= f {
                        prev = Some(k);
                    } else {
                        let Some(p) = prev else { return k.value };
                        let span = k.frame - p.frame;
                        if p.hold || span <= 0.0 {
                            return p.value;
                        }
                        let end = p.end.unwrap_or(k.value);
                        let t = ((f - p.frame) / span).clamp(0.0, 1.0);
                        return p.value + (end - p.value) * t;
                    }
                }
                prev.map(|p| p.value).unwrap_or(0.0)
            }
        }
    }
}

impl AnimVec {
    /// Value at animation frame `f` (Sprint 21, T001): same segment
    /// rule as scalars, per channel; ragged stops blend over their
    /// shared prefix and hold the longer tail.
    pub fn at(&self, f: f64) -> Vec<f64> {
        match self {
            AnimVec::Static(v) => v.clone(),
            AnimVec::Keyed(keys) => {
                let mut prev = keys.first();
                for k in keys {
                    if k.frame <= f {
                        prev = Some(k);
                    } else {
                        let Some(p) = prev else {
                            return k.value.clone();
                        };
                        let span = k.frame - p.frame;
                        if p.hold || span <= 0.0 {
                            return p.value.clone();
                        }
                        let end = p.end.as_ref().unwrap_or(&k.value);
                        let t = ((f - p.frame) / span).clamp(0.0, 1.0);
                        let n = p.value.len().max(end.len());
                        return (0..n)
                            .map(|i| {
                                let a = *p.value.get(i).or(end.get(i)).unwrap_or(&0.0);
                                let b = *end.get(i).unwrap_or(&a);
                                a + (b - a) * t
                            })
                            .collect();
                    }
                }
                prev.map(|p| p.value.clone()).unwrap_or_default()
            }
        }
    }
}

/// Map an unwrapped source time in animation seconds to the
/// time-correct animation instant in `[0, len]` for a loop mode
/// (Sprint 21, T001): `none` clamps past the window (the edge frame
/// holds, like non-looping footage), `loop` wraps with the same
/// epsilon snap as footage looping, `ping-pong` mirrors every second
/// pass so the turnarounds hold their edge frame for one instant.
/// A non-positive window always yields 0. Pure function of its
/// inputs, so reruns are byte-identical by construction.
pub fn sample_time(len: f64, mode: LoopMode, t: f64) -> f64 {
    if !len.is_finite() || len <= 0.0 || !t.is_finite() {
        return 0.0;
    }
    match mode {
        LoopMode::None => t.clamp(0.0, len),
        LoopMode::Loop => {
            let mut off = t % len;
            if off < 0.0 {
                off += len;
            }
            if (len - off) < 1e-9 {
                off = 0.0;
            }
            off
        }
        LoopMode::PingPong => {
            let period = 2.0 * len;
            let mut phase = t % period;
            if phase < 0.0 {
                phase += period;
            }
            if phase <= len {
                if (len - phase) < 1e-9 && t >= 0.0 {
                    // Hold the far edge for the turnaround instant.
                    len
                } else {
                    phase
                }
            } else {
                period - phase
            }
        }
    }
}

// --- decode -------------------------------------------------------------

/// Decode one source by file name (Sprint 21, T001): `.json`
/// (case-insensitive) parses one bodymovin animation, `.lottie`
/// unzips a dotLottie pack and decodes its first manifest animation.
/// Anything else, an unreadable zip, a missing manifest, or a schema
/// violation refuses with the cause — fail-closed before render work.
pub fn decode_source(path: &str, bytes: &[u8]) -> Result<LottieDoc, String> {
    let lower = path.to_lowercase();
    if lower.ends_with(".json") {
        decode_json(bytes).map_err(|e| format!("{path:?} is not a readable Lottie animation ({e})"))
    } else if lower.ends_with(".lottie") {
        decode_dotlottie(path, bytes)
    } else {
        Err(format!(
            "{path:?} must name a `.json` Lottie file or a `.lottie` dotLottie pack; a lottie clip plays animations only"
        ))
    }
}

/// Decode one dotLottie pack (Sprint 21, T001): the zip must hold a
/// readable `manifest.json` naming at least one animation, and
/// `animations/<id>.json` must decode. The manifest `loop` flag of
/// the chosen (first) animation becomes the default loop mode.
fn decode_dotlottie(path: &str, bytes: &[u8]) -> Result<LottieDoc, String> {
    let fail = |why: String| format!("{path:?} is not a readable dotLottie pack ({why})");
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|e| fail(format!("it is not a readable zip: {e}")))?;
    let manifest_bytes = {
        let mut entry = zip
            .by_name("manifest.json")
            .map_err(|_| fail("it holds no `manifest.json`".to_string()))?;
        let mut buf = Vec::new();
        std::io::Read::read_to_end(&mut entry, &mut buf)
            .map_err(|e| fail(format!("`manifest.json` cannot be read: {e}")))?;
        buf
    };
    let manifest: Manifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|e| fail(format!("`manifest.json` is not JSON: {e}")))?;
    let first = manifest
        .animations
        .first()
        .ok_or_else(|| fail("`manifest.json` names no animations".to_string()))?;
    let anim_name = format!("animations/{}.json", first.id);
    let anim_bytes = {
        let mut entry = zip
            .by_name(&anim_name)
            .map_err(|_| fail(format!("it holds no `{anim_name}`")))?;
        let mut buf = Vec::new();
        std::io::Read::read_to_end(&mut entry, &mut buf)
            .map_err(|e| fail(format!("`{anim_name}` cannot be read: {e}")))?;
        buf
    };
    let mut doc = decode_json(&anim_bytes)
        .map_err(|e| format!("{path:?} holds an unreadable animation `{anim_name}` ({e})"))?;
    doc.loop_default = first.loop_flag;
    Ok(doc)
}

#[derive(Debug, Deserialize)]
struct Manifest {
    #[serde(default)]
    animations: Vec<ManifestAnimation>,
}

#[derive(Debug, Deserialize)]
struct ManifestAnimation {
    id: String,
    #[serde(default, deserialize_with = "de_opt_bool")]
    #[serde(rename = "loop")]
    loop_flag: bool,
}

/// A JSON boolean or `"true"`/`"false"` string (some exporters quote
/// the manifest loop flag): anything else means false, never a guess
/// that loops an animation the maker did not mark.
fn de_opt_bool<'de, D>(d: D) -> Result<bool, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{self, Visitor};
    struct Loose;
    impl<'de> Visitor<'de> for Loose {
        type Value = bool;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("a boolean")
        }
        fn visit_bool<E: de::Error>(self, v: bool) -> Result<bool, E> {
            Ok(v)
        }
        fn visit_str<E: de::Error>(self, v: &str) -> Result<bool, E> {
            Ok(v.eq_ignore_ascii_case("true"))
        }
    }
    d.deserialize_any(Loose)
}

/// Decode one bodymovin animation (Sprint 21, T001): required header
/// (`fr`, `ip`, `op`, `w`, `h`), shape layers, and markers. An
/// expression program beside a baked channel never runs (the baked
/// value wins, deterministically). Out-of-window markers drop with a
/// named note on the doc; anything structurally unusable refuses.
fn decode_json(bytes: &[u8]) -> Result<LottieDoc, String> {
    let v: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|e| format!("it is not JSON: {e}"))?;
    let num = |key: &str| -> Result<f64, String> {
        v.get(key)
            .and_then(serde_json::Value::as_f64)
            .filter(|x| x.is_finite())
            .ok_or_else(|| format!("it carries no usable `{key}` number"))
    };
    let frame_rate = num("fr").and_then(|x| {
        if x > 0.0 {
            Ok(x)
        } else {
            Err("its frame rate `fr` is not positive".to_string())
        }
    })?;
    let in_point = num("ip")?;
    let out_point = num("op")?;
    if !(out_point > in_point) {
        return Err("its `op` is not past its `ip` (the animation has no length)".to_string());
    }
    let width = num("w").and_then(|x| {
        if x > 0.0 {
            Ok(x)
        } else {
            Err("its width `w` is not positive".to_string())
        }
    })?;
    let height = num("h").and_then(|x| {
        if x > 0.0 {
            Ok(x)
        } else {
            Err("its height `h` is not positive".to_string())
        }
    })?;
    let len = (out_point - in_point) / frame_rate;
    let mut markers = Vec::new();
    let mut dropped_markers = Vec::new();
    if let Some(list) = v.get("markers").and_then(|m| m.as_array()) {
        for m in list {
            let name = m
                .get("cm")
                .and_then(|c| c.as_str())
                .unwrap_or("marker")
                .to_string();
            let tm = m
                .get("tm")
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(f64::NAN);
            let dr = m
                .get("dr")
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(0.0);
            if !tm.is_finite() || !dr.is_finite() {
                dropped_markers.push(DroppedMarker {
                    name,
                    start: 0.0,
                    end: 0.0,
                    detail: "its `tm`/`dr` are not numbers".to_string(),
                });
                continue;
            }
            let start = (tm - in_point) / frame_rate;
            let end = (tm + dr - in_point) / frame_rate;
            if !start.is_finite()
                || !end.is_finite()
                || end <= start
                || start < 0.0
                || end > len + 1e-9
            {
                dropped_markers.push(DroppedMarker {
                    name,
                    start,
                    end,
                    detail: "it lies outside the animation window".to_string(),
                });
                continue;
            }
            markers.push(LottieMarker {
                name,
                start,
                end: end.min(len),
            });
        }
    }
    let mut layers = Vec::new();
    if let Some(list) = v.get("layers").and_then(|l| l.as_array()) {
        for (i, l) in list.iter().enumerate() {
            layers.push(decode_layer(i, l)?);
        }
    }
    Ok(LottieDoc {
        width,
        height,
        frame_rate,
        in_point,
        out_point,
        loop_default: false,
        markers,
        dropped_markers,
        layers,
    })
}

/// Decode one layer (Sprint 21, T001 subset): shape layers (`ty=4`)
/// only. Any other layer type refuses naming the layer and its type,
/// so picture kinds outside the subset never render a guess.
fn decode_layer(index: usize, l: &serde_json::Value) -> Result<Layer, String> {
    let ty = l
        .get("ty")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(-1);
    let name = l
        .get("nm")
        .and_then(|n| n.as_str())
        .unwrap_or("layer")
        .to_string();
    if ty != 4 {
        return Err(format!(
            "layer #{index} {name:?} has type `ty={ty}`; only shape layers (`ty=4`) play natively"
        ));
    }
    let num = |key: &str, default: f64| -> f64 {
        l.get(key)
            .and_then(serde_json::Value::as_f64)
            .filter(|x| x.is_finite())
            .unwrap_or(default)
    };
    let ks = l
        .get("ks")
        .ok_or_else(|| format!("layer #{index} {name:?} carries no `ks` transform"))?;
    let transform = decode_transform(ks, &format!("layer #{index} {name:?}"))?;
    let mut shapes = Vec::new();
    if let Some(list) = l.get("shapes").and_then(|s| s.as_array()) {
        for s in list {
            match decode_shape_item(s)? {
                Some(item) => shapes.push(item),
                None => {}
            }
        }
    }
    Ok(Layer {
        name,
        in_frame: num("ip", 0.0),
        out_frame: num("op", f64::INFINITY),
        start_offset: num("st", 0.0),
        transform,
        shapes,
    })
}

/// Decode one shape item (Sprint 21, T001 subset): groups, rects, and
/// ellipses draw; fills, strokes, and transforms style them.
/// `None` means a style-only item already folded into its group.
/// Anything else refuses naming its `ty`, fail-closed.
fn decode_shape_item(v: &serde_json::Value) -> Result<Option<ShapeItem>, String> {
    let ty = v.get("ty").and_then(|t| t.as_str()).unwrap_or("");
    match ty {
        "gr" => {
            let items = v
                .get("it")
                .and_then(|i| i.as_array())
                .ok_or_else(|| "a group carries no `it` items".to_string())?;
            let mut group = Group {
                items: Vec::new(),
                fill: None,
                stroke: None,
                transform: Transform {
                    position: AnimVec::Static(vec![0.0, 0.0]),
                    anchor: AnimVec::Static(vec![0.0, 0.0]),
                    scale: AnimVec::Static(vec![100.0, 100.0]),
                    rotation: AnimScalar::Static(0.0),
                    opacity: AnimScalar::Static(100.0),
                },
            };
            for item in items {
                let ity = item.get("ty").and_then(|t| t.as_str()).unwrap_or("");
                match ity {
                    "gr" | "rc" | "el" => {
                        if let Some(shape) = decode_shape_item(item)? {
                            group.items.push(shape);
                        }
                    }
                    "fl" => {
                        if group.fill.is_some() {
                            return Err("a group holds two fills; at most one fill paints natively".to_string());
                        }
                        group.fill = Some(decode_fill(item)?);
                    }
                    "st" => {
                        if group.stroke.is_some() {
                            return Err("a group holds two strokes; at most one stroke paints natively".to_string());
                        }
                        group.stroke = Some(decode_stroke(item)?);
                    }
                    "tr" => group.transform = decode_transform(item, "a group")?,
                    "tm" | "sr" | "mm" | "mg" => {
                        return Err(format!(
                            "shape `{ity}` is outside the native subset; only groups, rects, ellipses, fills, strokes, and transforms play"
                        ));
                    }
                    _ => {
                        return Err(format!(
                            "shape `{ity}` is unknown; only groups, rects, ellipses, fills, strokes, and transforms play natively"
                        ));
                    }
                }
            }
            Ok(Some(ShapeItem::Group(group)))
        }
        "rc" => {
            let pos = decode_pair(v, "p")?;
            let size = decode_pair(v, "s")?;
            let round = v
                .get("r")
                .and_then(decode_baked_scalar)
                .unwrap_or(0.0);
            Ok(Some(ShapeItem::Rect { pos, size, round }))
        }
        "el" => {
            let pos = decode_pair(v, "p")?;
            let size = decode_pair(v, "s")?;
            Ok(Some(ShapeItem::Ellipse { pos, size }))
        }
        _ => Err(format!(
            "shape `{ty}` is outside the native subset; only groups, rects, ellipses, fills, strokes, and transforms play natively"
        )),
    }
}

fn decode_fill(v: &serde_json::Value) -> Result<Fill, String> {
    let color = v
        .get("c")
        .and_then(decode_baked_color)
        .ok_or_else(|| "a fill carries no usable `c` color".to_string())?;
    let opacity = v.get("o").and_then(decode_baked_scalar).unwrap_or(100.0);
    Ok(Fill { color, opacity })
}

fn decode_stroke(v: &serde_json::Value) -> Result<Stroke, String> {
    let color3 = v
        .get("c")
        .and_then(decode_baked_color)
        .ok_or_else(|| "a stroke carries no usable `c` color".to_string())?;
    let opacity = v.get("o").and_then(decode_baked_scalar).unwrap_or(100.0);
    let width = v.get("w").and_then(decode_baked_scalar).unwrap_or(1.0);
    Ok(Stroke {
        color: [color3[0], color3[1], color3[2]],
        opacity,
        width,
    })
}

/// Decode one `ks`/`tr` transform block (Sprint 21, T001): position,
/// anchor, scale, rotation, and opacity, each static or keyframed sel.
fn decode_transform(v: &serde_json::Value, owner: &str) -> Result<Transform, String> {
    Ok(Transform {
        position: decode_vec_prop(v.get("p"), &format!("{owner} `p`"))?,
        anchor: decode_vec_prop(v.get("a"), &format!("{owner} `a`"))?,
        scale: decode_vec_prop(v.get("s"), &format!("{owner} `s`"))?,
        rotation: decode_scalar_prop(v.get("r"), &format!("{owner} `r`"))?,
        opacity: decode_scalar_prop(v.get("o"), &format!("{owner} `o`"))?,
    })
}

/// The baked value of one scalar property: its `"k"` channel, whether
/// constant or keyframed sel. An expression program (`"x"`) beside it
/// never runs — the baked value is the frame (Sprint 21, T001).
fn decode_scalar_prop(v: Option<&serde_json::Value>, what: &str) -> Result<AnimScalar, String> {
    let Some(v) = v else {
        return Err(format!("{what} is missing"));
    };
    if let Some(n) = v.as_f64().filter(|x| x.is_finite()) {
        return Ok(AnimScalar::Static(n));
    }
    let k = v
        .get("k")
        .ok_or_else(|| format!("{what} carries no usable `k` channel"))?;
    if let Some(n) = k.as_f64().filter(|x| x.is_finite()) {
        return Ok(AnimScalar::Static(n));
    }
    let stops = k
        .as_array()
        .ok_or_else(|| format!("{what} `k` is neither a number nor keyframes"))?;
    let mut keys = Vec::with_capacity(stops.len());
    for s in stops {
        let frame = s
            .get("t")
            .and_then(serde_json::Value::as_f64)
            .filter(|x| x.is_finite())
            .ok_or_else(|| format!("{what} has a keyframe with no `t` time"))?;
        let value = s
            .get("s")
            .and_then(|x| first_number(x))
            .ok_or_else(|| format!("{what} has a keyframe with no `s` value"))?;
        let end = s.get("e").and_then(first_number);
        let hold = s
            .get("h")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        keys.push(ScalarKey {
            frame,
            value,
            end,
            hold,
        });
    }
    if keys.is_empty() {
        return Err(format!("{what} carries no keyframes"));
    }
    Ok(AnimScalar::Keyed(keys))
}

/// The baked value of one vector property (Sprint 21, T001): same
/// expression rule as scalars.
fn decode_vec_prop(v: Option<&serde_json::Value>, what: &str) -> Result<AnimVec, String> {
    let Some(v) = v else {
        return Err(format!("{what} is missing"));
    };
    if let Some(vec) = as_numbers(v) {
        return Ok(AnimVec::Static(vec));
    }
    let k = v
        .get("k")
        .ok_or_else(|| format!("{what} carries no usable `k` channel"))?;
    if let Some(vec) = as_numbers(k) {
        return Ok(AnimVec::Static(vec));
    }
    let stops = k
        .as_array()
        .ok_or_else(|| format!("{what} `k` is neither numbers nor keyframes"))?;
    let mut keys = Vec::with_capacity(stops.len());
    for s in stops {
        let frame = s
            .get("t")
            .and_then(serde_json::Value::as_f64)
            .filter(|x| x.is_finite())
            .ok_or_else(|| format!("{what} has a keyframe with no `t` time"))?;
        let value = s
            .get("s")
            .and_then(as_numbers)
            .ok_or_else(|| format!("{what} has a keyframe with no `s` value"))?;
        let end = s.get("e").and_then(as_numbers);
        let hold = s
            .get("h")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        keys.push(VecKey {
            frame,
            value,
            end,
            hold,
        });
    }
    if keys.is_empty() {
        return Err(format!("{what} carries no keyframes"));
    }
    Ok(AnimVec::Keyed(keys))
}

/// First finite number of a scalar-or-vector JSON value (keyframe `s`
////`e` endpoints may carry either shape).
fn first_number(v: &serde_json::Value) -> Option<f64> {
    if let Some(n) = v.as_f64().filter(|x| x.is_finite()) {
        return Some(n);
    }
    as_numbers(v)?.into_iter().next()
}

/// Every finite number of a JSON scalar-or-array value.
fn as_numbers(v: &serde_json::Value) -> Option<Vec<f64>> {
    if let Some(n) = v.as_f64().filter(|x| x.is_finite()) {
        return Some(vec![n]);
    }
    let arr = v.as_array()?;
    let out: Vec<f64> = arr
        .iter()
        .filter_map(|x| x.as_f64().filter(|y| y.is_finite()))
        .collect();
    if out.len() == arr.len() && !out.is_empty() {
        Some(out)
    } else {
        None
    }
}

/// Baked scalar of a static paint field (`r`, `o`, `w`): the `k`
/// number, or the single static stop.
fn decode_baked_scalar(v: &serde_json::Value) -> Option<f64> {
    if let Some(n) = v.as_f64().filter(|x| x.is_finite()) {
        return Some(n);
    }
    let k = v.get("k")?;
    if let Some(n) = k.as_f64().filter(|x| x.is_finite()) {
        return Some(n);
    }
    k.as_array()?
        .iter()
        .find_map(|s| s.get("s").and_then(first_number))
}

/// Baked color of a `c` field: 3 or 4 channels, 0-1.
fn decode_baked_color(v: &serde_json::Value) -> Option<[f64; 4]> {
    let nums = decode_baked_vec(v)?;
    Some(match nums.as_slice() {
        [r, g, b, a, ..] => [*r, *g, *b, *a],
        [r, g, b] => [*r, *g, *b, 1.0],
        _ => return None,
    })
}

fn decode_baked_vec(v: &serde_json::Value) -> Option<Vec<f64>> {
    if let Some(vec) = as_numbers(v) {
        return Some(vec);
    }
    let k = v.get("k")?;
    if let Some(vec) = as_numbers(k) {
        return Some(vec);
    }
    k.as_array()?
        .iter()
        .find_map(|s| s.get("s").and_then(as_numbers))
}

/// Static x/y pair of a geometry field (`p`, `s`): the `k` numbers.
fn decode_pair(v: &serde_json::Value, key: &str) -> Result<[f64; 2], String> {
    let nums = v
        .get(key)
        .and_then(decode_baked_vec)
        .ok_or_else(|| format!("a shape carries no usable `{key}` pair"))?;
    Ok([nums[0], *nums.get(1).unwrap_or(&0.0)])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Smallest time-varying animation: a red rect sliding left to
    /// right over 2s at 30fps, one in-window marker, one dropped
    /// marker, and an expression on opacity (baked to 100).
    pub fn slide_fixture() -> Vec<u8> {
        serde_json::json!({
            "v": "5.7.4", "fr": 30.0, "ip": 0.0, "op": 60.0, "w": 64.0, "h": 64.0,
            "nm": "slide",
            "markers": [
                {"tm": 0.0, "cm": "intro", "dr": 30.0},
                {"tm": 90.0, "cm": "past-end", "dr": 10.0}
            ],
            "layers": [{
                "ddd": 0, "ind": 1, "ty": 4, "nm": "box", "sr": 1.0,
                "ks": {
                    "o": {"a": 0, "k": 100.0, "x": "wiggle(2,50)"},
                    "r": {"a": 0, "k": 0.0},
                    "p": {"a": 1, "k": [
                        {"t": 0.0, "s": [10.0, 32.0, 0.0], "e": [50.0, 32.0, 0.0]},
                        {"t": 30.0, "s": [50.0, 32.0, 0.0]}
                    ]},
                    "a": {"a": 0, "k": [0.0, 0.0, 0.0]},
                    "s": {"a": 0, "k": [100.0, 100.0, 100.0]}
                },
                "shapes": [{
                    "ty": "gr", "it": [
                        {"ty": "rc", "p": {"a": 0, "k": [0.0, 0.0]}, "s": {"a": 0, "k": [20.0, 20.0]}, "r": {"a": 0, "k": 0.0}},
                        {"ty": "fl", "c": {"a": 0, "k": [1.0, 0.0, 0.0, 1.0]}, "o": {"a": 0, "k": 100.0}},
                        {"ty": "tr", "p": {"a": 0, "k": [0.0, 0.0]}, "a": {"a": 0, "k": [0.0, 0.0]},
                         "s": {"a": 0, "k": [100.0, 100.0]}, "r": {"a": 0, "k": 0.0}, "o": {"a": 0, "k": 100.0}}
                    ]
                }]
            }]
        })
        .to_string()
        .into_bytes()
    }

    #[test]
    fn decodes_slide_with_marker_and_drop() {
        let doc = decode_source("slide.json", &slide_fixture()).expect("fixture decodes");
        assert_eq!((doc.width, doc.height), (64.0, 64.0));
        assert!((doc.len_secs() - 2.0).abs() < 1e-12);
        assert!(!doc.loop_default);
        assert_eq!(doc.markers.len(), 1);
        assert_eq!(doc.markers[0].name, "intro");
        assert!((doc.markers[0].start - 0.0).abs() < 1e-12);
        assert!((doc.markers[0].end - 1.0).abs() < 1e-12);
        assert_eq!(doc.dropped_markers.len(), 1);
        assert_eq!(doc.dropped_markers[0].name, "past-end");
        // The expression on opacity bakes to 100, never runs.
        let layer = &doc.layers[0];
        assert_eq!(layer.transform.opacity, AnimScalar::Static(100.0));
        // Position seeks linearly: frame 15 lands halfway.
        match &layer.transform.position {
            AnimVec::Keyed(_) => {
                let p = layer.transform.position.at(15.0);
                assert!((p[0] - 30.0).abs() < 1e-9, "midpoint x is 30, got {p:?}");
                assert!((layer.transform.position.at(0.0)[0] - 10.0).abs() < 1e-9);
                assert!((layer.transform.position.at(45.0)[0] - 50.0).abs() < 1e-9);
            }
            other => panic!("position must be keyed, got {other:?}"),
        }
    }

    #[test]
    fn refuses_garbage_naming_cause() {
        let err = decode_source("bad.json", b"{oops").unwrap_err();
        assert!(err.contains("bad.json"), "names the file: {err}");
        let err = decode_source("note.txt", b"{}").unwrap_err();
        assert!(err.contains("note.txt"), "names the file: {err}");
    }

    #[test]
    fn refuses_unsupported_layer_naming_it() {
        let mut v: serde_json::Value = serde_json::from_slice(&slide_fixture()).unwrap();
        v["layers"][0]["ty"] = serde_json::json!(2);
        let bytes = serde_json::to_vec(&v).unwrap();
        let err = decode_source("img.json", &bytes).unwrap_err();
        assert!(err.contains("\"box\""), "names the layer: {err}");
        assert!(err.contains("ty=2"), "names the cause: {err}");
    }

    #[test]
    fn loop_modes_map_time() {
        assert_eq!(sample_time(2.0, LoopMode::None, 5.0), 2.0);
        assert_eq!(sample_time(2.0, LoopMode::None, -1.0), 0.0);
        let wrapped = sample_time(2.0, LoopMode::Loop, 2.5);
        assert!((wrapped - 0.5).abs() < 1e-9, "loops: {wrapped}");
        assert_eq!(sample_time(2.0, LoopMode::Loop, 2.0), 0.0);
        assert!((sample_time(2.0, LoopMode::PingPong, 3.0) - 1.0).abs() < 1e-9);
        assert!((sample_time(2.0, LoopMode::PingPong, 2.0) - 2.0).abs() < 1e-9);
        assert_eq!(LoopMode::parse("quadratic"), None);
    }

    #[test]
    fn dotlottie_roundtrip_carries_loop_default() {
        let anim = slide_fixture();
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let opts = zip::write::SimpleFileOptions::default();
        zip.start_file("manifest.json", opts).unwrap();
        std::io::Write::write_all(&mut zip, br#"{"animations":[{"id":"slide","loop":true}]}"#)
            .unwrap();
        zip.start_file("animations/slide.json", opts).unwrap();
        std::io::Write::write_all(&mut zip, &anim).unwrap();
        let bytes = zip.finish().unwrap().into_inner();
        let doc = decode_source("pack.lottie", &bytes).expect("pack decodes");
        assert!(doc.loop_default);
        assert_eq!(doc.markers.len(), 1);
        // Uppercase extension still sniffs.
        let doc2 = decode_source("PACK.LOTTIE", &bytes).expect("case-insensitive");
        assert!(doc2.loop_default);
    }

    #[test]
    fn dotlottie_without_manifest_refuses() {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        zip.start_file(
            "animations/slide.json",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        std::io::Write::write_all(&mut zip, &slide_fixture()).unwrap();
        let bytes = zip.finish().unwrap().into_inner();
        let err = decode_source("pack.lottie", &bytes).unwrap_err();
        assert!(err.contains("pack.lottie"), "names the file: {err}");
        assert!(err.contains("manifest.json"), "names the cause: {err}");
    }
}
