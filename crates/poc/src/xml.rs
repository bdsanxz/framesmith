//! Reads a composition written in XML and checks it.
//!
//! Layout, look and text attributes carry the names and meanings CSS gives them (`left`, `top`,
//! `width`, `background`, `border-radius`, `flex-direction`, `font-size`, `color` and so on),
//! written as plain attributes of the element they belong to. There are no selectors, no cascade
//! and no scripts: only the text properties pass from a parent to its children. Motion is written
//! as `<tween>` and `<set>` children of the element it moves.

use crate::anim::{own_values, Curve, Key, Prop, Track, PROPS};
use crate::count::Format;
use crate::lut::Lut;
use crate::model::{
    Align, Audio, Blend, Column, Columns, Composition, Dim, Direction, Fit, FitKind, Flex, Flow, FontSource, Geometry, Inline, Kind, Layer,
    LineHeight, Look, Node, Still, TextAlign, TextStyle,
};
use crate::model::{Finish, NoiseField, Opening, Shader};
use crate::outline;
use crate::shared;
use crate::style::{self, Rgba};
use motion::Ease;
use roxmltree::Document;
use std::path::PathBuf;
use std::sync::Arc;
use timebase::Time;
use vello_cpu::kurbo::{BezPath, Rect};

type Xml<'a, 'input> = roxmltree::Node<'a, 'input>;

/// One thing wrong with the file, with the line it is on.
#[derive(Debug)]
pub struct Problem {
    /// The file the problem is in, when it is not the file that was given but one it shows.
    pub file: Option<String>,
    pub line: u32,
    pub message: String,
}

impl std::fmt::Display for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match &self.file {
            Some(file) => write!(f, "{file}, line {}: {}", self.line, self.message),
            None => write!(f, "line {}: {}", self.line, self.message),
        }
    }
}

/// How deep compositions may show other compositions.
const DEPTH: usize = 16;

/// What the root element of a composition says about the whole of it.
struct Head {
    width: u16,
    height: u16,
    fps: u32,
    duration: Option<Time>,
    background: Rgba,
}

/// A composition read from the file another one names.
struct Nested {
    head: Head,
    children: Vec<Node>,
    audio: Vec<Audio>,
}

/// The attributes every element that is drawn may carry.
const COMMON: [&str; 66] = [
    "id",
    "left",
    "top",
    "right",
    "bottom",
    "width",
    "height",
    "start",
    "duration",
    "x",
    "y",
    "scale",
    "scale-x",
    "rotation",
    "skew-y",
    "opacity",
    "blur",
    "brightness",
    "displace",
    "grain",
    "drop-shadow",
    "rotate-x",
    "rotate-y",
    "reveal",
    "perspective",
    "transition",
    "transform-origin",
    "background",
    "background-size",
    "background-position",
    "background-repeat",
    "border-radius",
    "border",
    "box-shadow",
    "overflow",
    "clip-path",
    "mix-blend-mode",
    "flex-direction",
    "columns",
    "max-width",
    "justify-content",
    "align-items",
    "gap",
    "padding",
    "margin",
    "align-self",
    "flex-grow",
    "position",
    "font-family",
    "font-weight",
    "font-style",
    "font-size",
    "line-height",
    "letter-spacing",
    "color",
    "text-transform",
    "text-align",
    "text-shadow",
    "white-space",
    "font-variant-numeric",
    "text-stroke",
    "typed",
    "count",
    "count-format",
    "draw",
    "dash",
];

struct Reader<'a> {
    document: &'a Document<'a>,
    problems: Vec<Problem>,
    videos: usize,
    stills: Vec<Still>,
    keys: usize,
    /// The project folder, which every `src` is read from.
    folder: PathBuf,
    /// The colour tables read so far, by the name they were written with.
    tables: Vec<(String, Arc<Lut>)>,
    /// The openings written as shaders that were read so far.
    shaders: Vec<Shader>,
    fonts: Vec<FontSource>,
    audio: Vec<Audio>,
    /// The frame rate of the composition being read.
    fps: u32,
    /// When the time of the element being read begins, on the timeline of the whole composition.
    clock: f64,
    /// The file being read, when it is one that another composition shows.
    file: Option<String>,
    /// The files being read, from the one that was given to this one.
    stack: Vec<PathBuf>,
}

impl Reader<'_> {
    fn line(&self, node: Xml) -> u32 {
        self.document.text_pos_at(node.range().start).row
    }

    fn problem(&mut self, node: Xml, message: String) {
        let line = self.line(node);
        self.problems.push(Problem {
            file: self.file.clone(),
            line,
            message,
        });
    }

    /// Takes what is wrong with the styles of the file being read and with their use.
    fn faults(&mut self, faults: Vec<shared::Fault>) {
        for (line, message) in faults {
            self.problems.push(Problem {
                file: self.file.clone(),
                line,
                message,
            });
        }
    }

    /// Reports every attribute of `node` that is not in `known`. `class` is known everywhere:
    /// the attributes of the styles it names are on the element by now.
    fn only(&mut self, node: Xml, known: &[&str]) {
        for attribute in node.attributes() {
            if attribute.name() != "class" && !known.contains(&attribute.name()) {
                let message = format!("<{}> has no attribute `{}`", node.tag_name().name(), attribute.name());
                self.problem(node, message);
            }
        }
    }

    fn number(&mut self, node: Xml, name: &str, default: Option<f64>) -> f64 {
        match (node.attribute(name), default) {
            (Some(written), _) => match written.trim().parse::<f64>() {
                Ok(value) if value.is_finite() => value,
                _ => {
                    self.problem(node, format!("`{name}=\"{written}\"` is not a number"));
                    0.0
                }
            },
            (None, Some(default)) => default,
            (None, None) => {
                self.problem(node, format!("<{}> needs the attribute `{name}`", node.tag_name().name()));
                0.0
            }
        }
    }

    /// A number of pixels, or a percentage of the parent's.
    fn dim(&mut self, node: Xml, name: &str) -> Option<Dim> {
        let written = node.attribute(name)?.trim();
        let read = match written.strip_suffix('%') {
            Some(share) => share.trim().parse::<f64>().ok().map(|value| Dim::Share(value / 100.0)),
            None => written.parse::<f64>().ok().map(Dim::Px),
        };
        if read.is_none() {
            self.problem(node, format!("`{name}=\"{written}\"` is not a number or a percentage"));
        }
        read
    }

    fn time(&mut self, node: Xml, name: &str) -> Option<Time> {
        let written = node.attribute(name)?;
        let time = Time::parse(written).filter(|time| !time.is_negative());
        if time.is_none() {
            self.problem(node, format!("`{name}=\"{written}\"` is not a time in seconds"));
        }
        time
    }

    fn color(&mut self, node: Xml, name: &str, default: Rgba) -> Rgba {
        let Some(written) = node.attribute(name) else { return default };
        style::color(written).unwrap_or_else(|| {
            self.problem(
                node,
                format!("`{name}=\"{written}\"` is not a colour written as #rrggbb or #rrggbbaa"),
            );
            default
        })
    }

    /// One of the written forms of an attribute, or the default when it is absent.
    fn choice<T: Copy>(&mut self, node: Xml, name: &str, forms: &[(&str, T)], default: T) -> T {
        let Some(written) = node.attribute(name) else { return default };
        match forms.iter().find(|(form, _)| *form == written.trim()) {
            Some((_, value)) => *value,
            None => {
                let known: Vec<&str> = forms.iter().map(|(form, _)| *form).collect();
                self.problem(node, format!("`{name}=\"{written}\"` is not one of: {}", known.join(", ")));
                default
            }
        }
    }

    /// One to four numbers, read as CSS reads a shorthand: top, right, bottom, left.
    fn sides(&mut self, node: Xml, name: &str) -> [f64; 4] {
        let Some(written) = node.attribute(name) else { return [0.0; 4] };
        let numbers: Option<Vec<f64>> = written.split_whitespace().map(|word| word.parse().ok()).collect();
        match numbers.as_deref() {
            Some([all]) => [*all; 4],
            Some([vertical, horizontal]) => [*vertical, *horizontal, *vertical, *horizontal],
            Some([top, horizontal, bottom]) => [*top, *horizontal, *bottom, *horizontal],
            Some([top, right, bottom, left]) => [*top, *right, *bottom, *left],
            _ => {
                self.problem(node, format!("`{name}=\"{written}\"` is not one to four numbers"));
                [0.0; 4]
            }
        }
    }

    /// A width and a colour, as `border` and `text-stroke` are written.
    fn line_style(&mut self, node: Xml, name: &str) -> Option<(f64, Rgba)> {
        let written = node.attribute(name)?;
        let read = written.split_once(' ').and_then(|(width, color)| {
            let width = width.trim().parse::<f64>().ok().filter(|width| *width >= 0.0)?;
            Some((width, style::color(color)?))
        });
        if read.is_none() {
            self.problem(node, format!("`{name}=\"{written}\"` is not a width and a colour"));
        }
        read
    }

    /// The tracks of `element`, from its `<tween>` and `<set>` children.
    fn tracks(&mut self, element: Xml, base: &[f64; PROPS.len()]) -> Vec<Track> {
        let mut keys: Vec<Vec<Key>> = PROPS.iter().map(|_| Vec::new()).collect();
        for child in element.children().filter(Xml::is_element) {
            let name = child.tag_name().name();
            let mut touched: Vec<(Prop, Option<f64>, Option<f64>)> = Vec::new();
            match name {
                "tween" => {
                    let mut known = vec!["at".to_string(), "duration".to_string(), "ease".to_string()];
                    for prop in PROPS {
                        let from_name = format!("from-{}", prop.name());
                        let to_name = format!("to-{}", prop.name());
                        let from = child
                            .has_attribute(from_name.as_str())
                            .then(|| self.number(child, &from_name, None));
                        let to = child.has_attribute(to_name.as_str()).then(|| self.number(child, &to_name, None));
                        if from.is_some() || to.is_some() {
                            touched.push((prop, from, to));
                        }
                        known.push(from_name);
                        known.push(to_name);
                    }
                    self.only(child, &known.iter().map(String::as_str).collect::<Vec<_>>());
                }
                "set" => {
                    let mut known = vec!["at"];
                    for prop in PROPS {
                        if child.has_attribute(prop.name()) {
                            touched.push((prop, None, Some(self.number(child, prop.name(), None))));
                        }
                        known.push(prop.name());
                    }
                    self.only(child, &known);
                }
                _ => continue,
            }
            if touched.is_empty() {
                self.problem(child, format!("<{name}> changes no property"));
                continue;
            }
            let curve = match child.attribute("ease") {
                None => Curve::Std(Ease::Linear),
                Some(written) => Curve::from_name(written).unwrap_or_else(|| {
                    self.problem(child, format!("`ease=\"{written}\"` is not a known easing"));
                    Curve::Std(Ease::Linear)
                }),
            };
            let at = self.time(child, "at").map_or(0.0, Time::to_f64);
            let duration = self.time(child, "duration").map_or(0.0, Time::to_f64);
            if name == "tween" && duration == 0.0 {
                self.problem(child, "<tween> needs a `duration` above 0".to_string());
            }
            for (prop, from, to) in touched {
                keys[prop.index()].push(Key {
                    at,
                    duration,
                    from,
                    to,
                    curve,
                });
            }
        }
        PROPS
            .into_iter()
            .filter(|prop| !keys[prop.index()].is_empty())
            .map(|prop| Track::build(prop, base[prop.index()], &keys[prop.index()]))
            .collect()
    }

    /// The text properties of `node`: its parent's, changed by its own attributes.
    fn text_style(&mut self, node: Xml, parent: &TextStyle) -> TextStyle {
        let mut text = parent.clone();
        if let Some(family) = node.attribute("font-family") {
            text.family = family.to_string();
        }
        text.weight = self.number(node, "font-weight", Some(f64::from(text.weight))) as f32;
        text.italic = self.choice(node, "font-style", &[("normal", false), ("italic", true)], text.italic);
        text.size = self.number(node, "font-size", Some(f64::from(text.size))) as f32;
        if let Some(written) = node.attribute("line-height") {
            let written = written.trim();
            let read = match (written, written.strip_suffix("px")) {
                ("normal", _) => Some(LineHeight::Normal),
                (_, Some(px)) => px.parse().ok().map(LineHeight::Px),
                _ => written.parse().ok().map(LineHeight::Multiple),
            };
            match read {
                Some(line_height) => text.line_height = line_height,
                None => self.problem(node, format!("`line-height=\"{written}\"` is not `normal`, a number or pixels")),
            }
        }
        if let Some(written) = node.attribute("letter-spacing") {
            let written = written.trim();
            let read = match written.strip_suffix("em") {
                Some(em) => em.parse::<f32>().ok().map(|em| em * text.size),
                None => written.parse::<f32>().ok(),
            };
            match read {
                Some(spacing) => text.letter_spacing = spacing,
                None => self.problem(node, format!("`letter-spacing=\"{written}\"` is not a number or ems")),
            }
        }
        text.color = self.color(node, "color", text.color);
        text.upper = self.choice(node, "text-transform", &[("none", false), ("uppercase", true)], text.upper);
        let aligns = [
            ("left", TextAlign::Left),
            ("center", TextAlign::Centre),
            ("right", TextAlign::Right),
        ];
        text.align = self.choice(node, "text-align", &aligns, text.align);
        if let Some(written) = node.attribute("text-shadow") {
            match style::shadows(written) {
                Some(shadows) => text.shadows = shadows,
                None if written.trim() == "none" => text.shadows.clear(),
                None => self.problem(node, format!("`text-shadow=\"{written}\"` is not a list of shadows")),
            }
        }
        text.nowrap = self.choice(node, "white-space", &[("normal", false), ("nowrap", true)], text.nowrap);
        if node.has_attribute("text-stroke") {
            text.stroke = self.line_style(node, "text-stroke");
        }
        if let Some(written) = node.attribute("font-variant-numeric") {
            let kinds = [
                ("lining-nums", "lnum"),
                ("oldstyle-nums", "onum"),
                ("tabular-nums", "tnum"),
                ("proportional-nums", "pnum"),
            ];
            let feature = |word: &str| kinds.iter().find(|(kind, _)| *kind == word).map(|(_, tag)| format!("\"{tag}\" 1"));
            let features: Option<Vec<String>> = written.split_whitespace().map(feature).collect();
            match features {
                _ if written.trim() == "normal" => text.figures.clear(),
                Some(features) if !features.is_empty() => text.figures = features.join(", "),
                _ => {
                    let known: Vec<&str> = kinds.iter().map(|(kind, _)| *kind).collect();
                    let form = format!("is not `normal` or a list of: {}", known.join(", "));
                    self.problem(node, format!("`font-variant-numeric=\"{written}\"` {form}"));
                }
            }
        }
        text
    }

    fn look(&mut self, node: Xml) -> Look {
        let paints = match node.attribute("background") {
            None => Vec::new(),
            Some(written) => style::paints(written).unwrap_or_else(|| {
                self.problem(node, format!("`background=\"{written}\"` is not a colour or a list of gradients"));
                Vec::new()
            }),
        };
        let background = self.layers(node, paints);
        let shadows = match node.attribute("box-shadow") {
            None => Vec::new(),
            Some(written) => style::shadows(written).unwrap_or_else(|| {
                self.problem(node, format!("`box-shadow=\"{written}\"` is not a list of shadows"));
                Vec::new()
            }),
        };
        let blends = Blend::NAMED;
        let clip_path = node.attribute("clip-path").and_then(|written| {
            let corners = written.trim().strip_prefix("polygon(").and_then(|rest| rest.strip_suffix(')'));
            let read: Option<Vec<(f64, f64)>> = corners.and_then(|corners| {
                let share = |word: &str| word.strip_suffix('%').unwrap_or(word).parse::<f64>().ok().map(|v| v / 100.0);
                corners
                    .split(',')
                    .map(|corner| {
                        let mut words = corner.split_whitespace();
                        Some((share(words.next()?)?, share(words.next()?)?))
                    })
                    .collect()
            });
            if read.as_ref().is_none_or(|corners| corners.len() < 3) {
                self.problem(
                    node,
                    format!("`clip-path=\"{written}\"` is not a polygon of three or more corners in percent"),
                );
                return None;
            }
            read
        });
        let displace = node.attribute("displace").and_then(|written| {
            let read = written.trim().split_once(char::is_whitespace).and_then(|(amount, noise)| {
                let amount = amount.parse::<f64>().ok().filter(|amount| *amount > 0.0 && amount.is_finite())?;
                Some((amount, noise_field(noise)?))
            });
            if read.is_none() {
                self.problem(
                    node,
                    format!("`displace=\"{written}\"` is not an amount above 0 and a noise: `14 noise(0.035 3 9)`"),
                );
            }
            read
        });
        let grain = node.attribute("grain").and_then(|written| {
            let read = written.trim().split_once(')').and_then(|(noise, rest)| {
                let mut numbers = rest
                    .split_whitespace()
                    .map(|word| word.parse::<f64>().ok().filter(|number| number.is_finite()));
                let (slope, offset) = (numbers.next()??, numbers.next()??);
                numbers
                    .next()
                    .is_none()
                    .then_some((noise_field(&format!("{noise})"))?, slope, offset))
            });
            if read.is_none() {
                self.problem(
                    node,
                    format!("`grain=\"{written}\"` is not a noise, a slope and an offset: `noise(0.9 2 5) -1.7 1.75`"),
                );
            }
            read
        });
        // The shadow of a shape follows its outlines, and is the shape's own to read.
        let shadow = node
            .attribute("drop-shadow")
            .filter(|_| node.tag_name().name() != "shape")
            .and_then(|written| {
                let read = style::shadows(written)
                    .filter(|shadows| shadows.len() == 1)
                    .map(|shadows| shadows[0]);
                if read.is_none() {
                    self.problem(
                        node,
                        format!("`drop-shadow=\"{written}\"` is not one shadow: x, y, blur and a colour"),
                    );
                }
                read
            });
        // A border of two lines names itself between its width and its colour.
        let (border, double) = match node
            .attribute("border")
            .map(|written| written.split_whitespace().collect::<Vec<_>>())
        {
            Some(words) if words.len() > 2 && words[1] == "double" => {
                let line = format!("{} {}", words[0], words[2..].join(" "));
                let read = line
                    .split_once(' ')
                    .and_then(|(width, color)| Some((width.parse::<f64>().ok().filter(|width| *width >= 0.0)?, style::color(color)?)));
                if read.is_none() {
                    let written = node.attribute("border").unwrap_or("");
                    self.problem(node, format!("`border=\"{written}\"` is not a width and a colour"));
                }
                (read, read.is_some())
            }
            _ => (self.line_style(node, "border"), false),
        };
        let look = Look {
            finish: Finish { displace, grain, shadow },
            clip_path,
            background,
            radius: self.number(node, "border-radius", Some(0.0)),
            border,
            double,
            shadows,
            clip: self.choice(node, "overflow", &[("visible", false), ("hidden", true)], false),
            blend: self.choice(node, "mix-blend-mode", &blends, Blend::Normal),
            perspective: node.attribute("perspective").and_then(|written| {
                let distance = self.number(node, "perspective", None);
                // A text that is no number was complained of already.
                if distance <= 0.0 && written.trim().parse::<f64>().is_ok() {
                    self.problem(node, format!("`perspective` must be above 0, not {distance}"));
                }
                (distance > 0.0).then_some(distance)
            }),
            transition: match node.attribute("transition").map(str::trim) {
                Some(written) if written.ends_with(".wgsl") => self.shader(node, written),
                _ => self.choice(
                    node,
                    "transition",
                    &[
                        ("none", None),
                        ("wipe", Some(Opening::Wipe)),
                        ("cut", Some(Opening::Cut)),
                        ("iris", Some(Opening::Iris)),
                    ],
                    None,
                ),
            },
        };
        // The hard-edged opening is the element's one cut: it cannot be joined with another shape.
        if look.transition == Some(Opening::Cut) && (look.clip_path.is_some() || (look.clip && look.radius > 0.0)) {
            let other = if look.clip_path.is_some() {
                "`clip-path`"
            } else {
                "`overflow=\"hidden\"` and rounded corners"
            };
            self.problem(node, format!("`transition=\"cut\"` cannot be joined with {other}"));
        }
        look
    }

    /// The opening a `transition` names as a shader file of the project.
    fn shader(&mut self, node: Xml, written: &str) -> Option<Opening> {
        if let Some(known) = self.shaders.iter().position(|shader| shader.name == written) {
            return Some(Opening::Shader(known));
        }
        let path = self.folder.join(written);
        match std::fs::read_to_string(&path) {
            Ok(source) => {
                self.shaders.push(Shader {
                    name: written.to_string(),
                    source,
                    line: self.line(node),
                });
                Some(Opening::Shader(self.shaders.len() - 1))
            }
            Err(error) => {
                self.problem(node, format!("the transition `{}` could not be read: {error}", path.display()));
                None
            }
        }
    }

    /// The widths `columns` lists: pixels, or shares such as `1fr`.
    fn columns(&mut self, node: Xml) -> Option<Vec<Column>> {
        let written = node.attribute("columns")?;
        if node.tag_name().name() != "box" {
            self.problem(node, "`columns` is for a <box>".to_string());
            return None;
        }
        let width = |word: &str| {
            let (number, share) = match word.strip_suffix("fr") {
                Some(number) => (number, true),
                None => (word, false),
            };
            let number = number.parse::<f64>().ok().filter(|number| number.is_finite() && *number > 0.0)?;
            Some(if share { Column::Share(number) } else { Column::Px(number) })
        };
        let widths: Option<Vec<Column>> = written.split_whitespace().map(width).collect();
        match widths.filter(|widths| !widths.is_empty()) {
            Some(widths) => Some(widths),
            None => {
                let form = "is not a list of column widths: pixels, or shares such as `1fr`";
                self.problem(node, format!("`columns=\"{written}\"` {form}"));
                None
            }
        }
    }

    fn flow(&mut self, node: Xml) -> Flow {
        let aligns = [
            ("flex-start", Align::Start),
            ("center", Align::Centre),
            ("flex-end", Align::End),
            ("stretch", Align::Stretch),
            ("baseline", Align::Baseline),
        ];
        let justifies = [aligns[0], aligns[1], aligns[2], ("space-between", Align::Between)];
        let columns = self.columns(node).map(|widths| Columns {
            widths,
            align: self.choice(node, "align-items", &aligns, Align::Stretch),
            gap: self.number(node, "gap", Some(0.0)),
        });
        if columns.is_some() && node.has_attribute("flex-direction") {
            self.problem(node, "a <box> has `columns` or `flex-direction`, not both".to_string());
        }
        let flex = node.has_attribute("flex-direction").then(|| Flex {
            direction: self.choice(
                node,
                "flex-direction",
                &[("row", Direction::Row), ("column", Direction::Column)],
                Direction::Row,
            ),
            justify: self.choice(node, "justify-content", &justifies, Align::Start),
            align: self.choice(node, "align-items", &aligns, Align::Stretch),
            gap: self.number(node, "gap", Some(0.0)),
        });
        Flow {
            flex: flex.filter(|_| columns.is_none()),
            columns,
            padding: self.sides(node, "padding"),
            margin: self.sides(node, "margin"),
            align_self: node
                .has_attribute("align-self")
                .then(|| self.choice(node, "align-self", &aligns, Align::Start)),
            grow: self.number(node, "flex-grow", Some(0.0)),
            absolute: self.choice(node, "position", &[("static", false), ("absolute", true)], false),
        }
    }

    /// The entries of an attribute that places background layers: one for each of `count`
    /// layers. A list of one entry applies to every layer.
    fn placed<T: Clone>(&mut self, node: Xml, name: &str, count: usize, form: &str, read: impl Fn(&str) -> Option<T>) -> Option<Vec<T>> {
        let written = node.attribute(name)?;
        if count == 0 {
            self.problem(node, format!("`{name}` needs a `background` to place"));
            return None;
        }
        let entries: Option<Vec<T>> = style::split_top(written, ',').into_iter().map(read).collect();
        match entries {
            None => {
                self.problem(node, format!("`{name}=\"{written}\"` is not a list of {form}"));
                None
            }
            Some(entries) if entries.len() == count => Some(entries),
            Some(entries) if entries.len() == 1 => Some(vec![entries[0].clone(); count]),
            Some(entries) => {
                let found = entries.len();
                self.problem(
                    node,
                    format!("`{name}` has {found} entries for {count} layers; it takes one for each layer, or one for all"),
                );
                None
            }
        }
    }

    /// The layers of a background: its paints, each with its size, its place and its repeat.
    fn layers(&mut self, node: Xml, paints: Vec<style::Paint>) -> Vec<Layer> {
        let count = paints.len();
        let dim = |word: &str| -> Option<Dim> {
            match word.strip_suffix('%') {
                Some(percent) => percent.parse::<f64>().ok().map(|percent| Dim::Share(percent / 100.0)),
                None => word.parse::<f64>().ok().map(Dim::Px),
            }
        };
        let pair = |entry: &str| -> Option<(Dim, Dim)> {
            let words: Vec<&str> = entry.split_whitespace().collect();
            let [x, y] = words[..] else { return None };
            Some((dim(x)?, dim(y)?))
        };
        let sizes = self.placed(node, "background-size", count, "`auto` or a width and a height", |entry| {
            if entry == "auto" {
                return Some(None);
            }
            let (width, height) = pair(entry)?;
            let positive = |dim: Dim| matches!(dim, Dim::Px(v) | Dim::Share(v) if v > 0.0);
            (positive(width) && positive(height)).then_some(Some((width, height)))
        });
        let positions = self.placed(node, "background-position", count, "an x and a y", pair);
        let repeats = self.placed(node, "background-repeat", count, "`repeat` or `no-repeat`", |entry| match entry {
            "repeat" => Some(true),
            "no-repeat" => Some(false),
            _ => None,
        });
        paints
            .into_iter()
            .enumerate()
            .map(|(index, paint)| Layer {
                paint,
                size: sizes.as_ref().and_then(|sizes| sizes[index]),
                position: positions
                    .as_ref()
                    .map_or((Dim::Px(0.0), Dim::Px(0.0)), |positions| positions[index]),
                repeat: repeats.as_ref().is_none_or(|repeats| repeats[index]),
            })
            .collect()
    }

    fn color_filter(&mut self, node: Xml) -> Vec<style::ColorOp> {
        let Some(written) = node.attribute("filter") else {
            return Vec::new();
        };
        style::color_ops(written).unwrap_or_else(|| {
            let known = "brightness(), contrast(), saturate(), grayscale()";
            self.problem(node, format!("`filter=\"{written}\"` is not a list of: {known}"));
            Vec::new()
        })
    }

    /// The colour table a `<video>` names with `lut`, as the first step of its filter.
    fn color_table(&mut self, node: Xml) -> Option<style::ColorOp> {
        let written = node.attribute("lut")?;
        if let Some((_, table)) = self.tables.iter().find(|(name, _)| name == written) {
            return Some(style::ColorOp::Table(Arc::clone(table)));
        }
        let path = self.folder.join(written);
        let read = std::fs::read_to_string(&path)
            .map_err(|error| format!("could not be read: {error}"))
            .and_then(|text| Lut::read(&text));
        match read {
            Ok(table) => {
                let table = Arc::new(table);
                self.tables.push((written.to_string(), Arc::clone(&table)));
                Some(style::ColorOp::Table(table))
            }
            Err(reason) => {
                self.problem(node, format!("the colour table `{}`: {reason}", path.display()));
                None
            }
        }
    }

    /// The outlines of a `<shape>`: its own `d`, or the paths of the SVG file it names.
    fn shape(&mut self, node: Xml) -> Kind {
        let color = self.color(node, "color", [0, 0, 0, 255]);
        let mut stroke = node.has_attribute("stroke-width").then(|| self.number(node, "stroke-width", None));
        let mut view = Rect::new(0.0, 0.0, 24.0, 24.0);
        let mut view_written = node.attribute("view-box").map(str::to_string);
        let mut outlines: Vec<String> = node.attribute("d").map(str::to_string).into_iter().collect();
        if let Some(source) = node.attribute("src") {
            let path = self.folder.join(source);
            match std::fs::read_to_string(&path) {
                Err(error) => self.problem(node, format!("the shape `{}` could not be read: {error}", path.display())),
                Ok(text) => match Document::parse(&text) {
                    Err(error) => self.problem(node, format!("the shape `{}` is not well-formed: {error}", path.display())),
                    Ok(svg) => {
                        let root = svg.root_element();
                        view_written = root.attribute("viewBox").map(str::to_string);
                        if stroke.is_none() && root.attribute("fill") == Some("none") {
                            stroke = root.attribute("stroke-width").and_then(|width| width.parse().ok());
                        }
                        let paths = root.descendants().filter(|n| n.has_tag_name("path"));
                        outlines.extend(paths.filter_map(|n| n.attribute("d")).map(str::to_string));
                    }
                },
            }
        }
        if let Some(written) = view_written {
            let numbers: Vec<f64> = written.split_whitespace().filter_map(|word| word.parse().ok()).collect();
            match numbers[..] {
                [x, y, width, height] if width > 0.0 && height > 0.0 => view = Rect::new(x, y, x + width, y + height),
                _ => self.problem(node, format!("`{written}` is not a view box of four numbers")),
            }
        }
        let mut paths = Vec::with_capacity(outlines.len());
        for outline in outlines {
            match BezPath::from_svg(&outline) {
                Ok(path) => paths.push(path),
                Err(error) => self.problem(node, format!("an outline of the shape is not a path: {error}")),
            }
        }
        if paths.is_empty() {
            self.problem(node, "<shape> needs `d` or a `src` with paths".to_string());
        }
        let shadow = node.attribute("drop-shadow").and_then(|written| {
            let read = style::shadows(written)
                .filter(|shadows| shadows.len() == 1)
                .map(|shadows| shadows[0]);
            if read.is_none() {
                self.problem(
                    node,
                    format!("`drop-shadow=\"{written}\"` is not one shadow: x, y, blur and a colour"),
                );
            }
            read
        });
        let dash = self.dash(node);
        let dashed = match dash.is_empty() {
            true => Vec::new(),
            false => paths.iter().map(|path| outline::dashed(path, &dash)).collect(),
        };
        Kind::Shape {
            paths,
            view,
            stroke,
            color,
            shadow,
            dash,
            dashed,
        }
    }

    /// The dashes of a `<shape>`: pairs of lengths, a dash and then a gap.
    fn dash(&mut self, node: Xml) -> Vec<f64> {
        let Some(written) = node.attribute("dash") else { return Vec::new() };
        let lengths: Option<Vec<f64>> = written.split_whitespace().map(|word| word.parse().ok()).collect();
        let pairs = lengths.filter(|lengths| {
            let sound = lengths.iter().all(|length| length.is_finite() && *length >= 0.0);
            sound && !lengths.is_empty() && lengths.len() % 2 == 0 && lengths.iter().sum::<f64>() > 0.0
        });
        pairs.unwrap_or_else(|| {
            let message = format!("`dash=\"{written}\"` is not pairs of lengths: a dash and a gap, in view-box units");
            self.problem(node, message);
            Vec::new()
        })
    }

    fn element(&mut self, node: Xml, parent_text: &TextStyle, inside_text: bool) -> Option<Node> {
        let name = node.tag_name().name();
        let common: Vec<&str> = COMMON.to_vec();
        let mut nested: Option<Nested> = None;
        let kind = match (name, inside_text) {
            ("composition", false) => {
                nested = Some(self.nested(node)?);
                Kind::Box
            }
            ("box" | "rect", false) => {
                self.only(node, &common);
                Kind::Box
            }
            ("text", false) => {
                self.only(node, &common);
                Kind::Text
            }
            ("span", true) => {
                self.only(node, &common);
                Kind::Span
            }
            ("image", false) => {
                self.only(
                    node,
                    &[&common[..], &["src", "filter", "mask", "object-fit", "object-position"]].concat(),
                );
                let source = PathBuf::from(node.attribute("src").unwrap_or(""));
                if source.as_os_str().is_empty() {
                    self.problem(node, "<image> needs the attribute `src`".to_string());
                }
                let mask = node.attribute("mask").and_then(|written| {
                    let read = style::paints(written)
                        .filter(|paints| paints.len() == 1)
                        .and_then(|mut paints| paints.pop());
                    if read.is_none() {
                        self.problem(node, format!("`mask=\"{written}\"` is not one gradient"));
                    }
                    read
                });
                let wanted = Still {
                    source,
                    filter: self.color_filter(node),
                    mask,
                };
                let still = self.stills.iter().position(|still| *still == wanted).unwrap_or_else(|| {
                    self.stills.push(wanted);
                    self.stills.len() - 1
                });
                let kinds = [("fill", FitKind::Fill), ("cover", FitKind::Cover), ("contain", FitKind::Contain)];
                let kind = self.choice(node, "object-fit", &kinds, FitKind::Fill);
                let mut hold = (0.5, 0.5);
                if let Some(written) = node.attribute("object-position") {
                    let share = |word: &str| word.strip_suffix('%')?.parse::<f64>().ok().filter(|share| share.is_finite());
                    let shares: Vec<Option<f64>> = written.split_whitespace().map(share).collect();
                    match shares[..] {
                        [Some(across), Some(down)] => hold = (across / 100.0, down / 100.0),
                        _ => self.problem(
                            node,
                            format!("`object-position=\"{written}\"` is not two percentages, across and down"),
                        ),
                    }
                }
                Kind::Image {
                    still,
                    fit: Fit { kind, hold },
                }
            }
            ("video", false) => {
                self.only(node, &[&common[..], &["src", "media-start", "filter", "lut"]].concat());
                let source = PathBuf::from(node.attribute("src").unwrap_or(""));
                if source.as_os_str().is_empty() {
                    self.problem(node, "<video> needs the attribute `src`".to_string());
                }
                for size in ["width", "height"] {
                    match self.number(node, size, None) {
                        value if value.fract() == 0.0 && value >= 2.0 => {}
                        value => {
                            let message = format!("`{size}` of a <video> must be a whole number of 2 or more, not {value}");
                            self.problem(node, message);
                        }
                    }
                }
                let video = self.videos;
                self.videos += 1;
                Kind::Video {
                    source,
                    video,
                    media_start: self.time(node, "media-start").unwrap_or(Time::ZERO),
                    filter: self.color_table(node).into_iter().chain(self.color_filter(node)).collect(),
                }
            }
            ("shape", false) => {
                self.only(node, &[&common[..], &["src", "d", "view-box", "stroke-width"]].concat());
                self.shape(node)
            }
            (other, true) => {
                self.problem(node, format!("<{other}> is not known inside a text; only <span> is"));
                return None;
            }
            ("style", false) => {
                self.problem(
                    node,
                    "a <style> stands directly in the <composition>, not inside an element".to_string(),
                );
                return None;
            }
            (other, false) => {
                self.problem(node, format!("<{other}> is not a known element"));
                return None;
            }
        };
        // A number that counts is a <text>; an outline drawn in part or in dashes is a <shape>
        // with a line.
        let stroked = matches!(kind, Kind::Shape { stroke: Some(_), .. });
        for attribute in ["draw", "dash"] {
            if node.has_attribute(attribute) && !stroked {
                self.problem(node, format!("`{attribute}` is for a <shape> with `stroke-width`"));
            }
        }
        let is_block = matches!(kind, Kind::Text);
        for attribute in ["count", "count-format"] {
            if node.has_attribute(attribute) && !is_block {
                self.problem(node, format!("`{attribute}` is for a <text>, not a <{name}>"));
            }
        }
        if is_block && node.has_attribute("count-format") && !node.has_attribute("count") {
            self.problem(node, "`count-format` needs `count`".to_string());
        }
        let count = (is_block && node.has_attribute("count")).then(|| match node.attribute("count-format") {
            None => Format::default(),
            Some(written) => Format::from_name(written).unwrap_or_else(|| {
                let forms = "the forms are `0`, `0.00`, `0,0`, `0,0.00`, `m:ss` and `m:ss.0`, with up to six decimals";
                self.problem(node, format!("`count-format=\"{written}\"` is not a form of number; {forms}"));
                Format::default()
            }),
        });
        let window = match (self.time(node, "start"), self.time(node, "duration")) {
            // A composition from another file is always a clip.
            (start, duration) if nested.is_some() => {
                let whole = nested.as_ref().and_then(|nested| nested.head.duration).unwrap_or(Time::ZERO);
                Some((start.unwrap_or(Time::ZERO), duration.unwrap_or(whole)))
            }
            (Some(start), Some(duration)) => Some((start, duration)),
            (None, None) => None,
            _ => {
                self.problem(node, format!("<{name}> needs both `start` and `duration`, or neither"));
                None
            }
        };
        let mut base = own_values();
        for prop in PROPS {
            if prop != Prop::Typed && node.has_attribute(prop.name()) {
                base[prop.index()] = self.number(node, prop.name(), None);
            }
        }
        if node.has_attribute("typed") {
            base[Prop::Typed.index()] = self.number(node, "typed", None);
        }
        let origin = match node.attribute("transform-origin") {
            None => (Dim::Share(0.5), Dim::Share(0.5)),
            Some(written) => {
                let parts: Vec<Option<Dim>> = written
                    .split_whitespace()
                    .map(|word| match word.strip_suffix('%') {
                        Some(share) => share.parse::<f64>().ok().map(|value| Dim::Share(value / 100.0)),
                        None => word.parse::<f64>().ok().map(Dim::Px),
                    })
                    .collect();
                match parts[..] {
                    [Some(x), Some(y)] => (x, y),
                    _ => {
                        self.problem(node, format!("`transform-origin=\"{written}\"` is not two lengths"));
                        (Dim::Share(0.5), Dim::Share(0.5))
                    }
                }
            }
        };
        let mut text = self.text_style(node, parent_text);
        let key = self.keys;
        self.keys += 1;
        let is_text = matches!(kind, Kind::Text | Kind::Span);
        let mut inline = Vec::new();
        let mut children = Vec::new();
        // The children count their time from the start of a clip.
        let outer_clock = self.clock;
        self.clock += window.map_or(0.0, |(start, _)| start.to_f64());
        for child in node.children() {
            if child.is_text() {
                if is_text {
                    inline.push(Inline::Words(child.text().unwrap_or("").to_string()));
                }
            } else if child.is_element() && !matches!(child.tag_name().name(), "tween" | "set") {
                if nested.is_some() {
                    let found = child.tag_name().name();
                    self.problem(
                        child,
                        format!("<{found}> is not known in a <composition> that names a file; only <tween> and <set> are"),
                    );
                } else if let Some(read) = self.element(child, &text, is_text) {
                    if is_text {
                        inline.push(Inline::Child(children.len()));
                    }
                    children.push(read);
                }
            }
        }
        if count.is_some() {
            let words = inline
                .iter()
                .any(|part| matches!(part, Inline::Words(words) if !words.trim().is_empty()));
            if words || !children.is_empty() {
                let message = "a <text> that counts holds no words of its own: its text is the number";
                self.problem(node, message.to_string());
            }
            inline.clear();
            children.clear();
            // One line, never broken.
            text.nowrap = true;
        }
        let mut geometry = Geometry {
            left: self.dim(node, "left"),
            top: self.dim(node, "top"),
            right: self.dim(node, "right"),
            bottom: self.dim(node, "bottom"),
            width: self.dim(node, "width"),
            height: self.dim(node, "height"),
            max_width: node.has_attribute("max-width").then(|| self.number(node, "max-width", None)),
        };
        let mut look = self.look(node);
        if let Some(nested) = nested {
            // The file's own frame: its size, and nothing shows outside it.
            geometry.width = Some(Dim::Px(f64::from(nested.head.width)));
            geometry.height = Some(Dim::Px(f64::from(nested.head.height)));
            look.clip = true;
            children = nested.children;
            // Its sounds play from the clip's start and stop at the clip's end.
            let (begins, lasts) = window.map_or((0.0, 0.0), |(start, duration)| (start.to_f64(), duration.to_f64()));
            for mut sound in nested.audio {
                let left = lasts - sound.start;
                if left > 0.0 {
                    sound.duration = sound.duration.min(left);
                    sound.start += outer_clock + begins;
                    self.audio.push(sound);
                }
            }
        }
        self.clock = outer_clock;
        // The tweens are read after the element's own attributes, so that problems come in the
        // order of their lines.
        let tracks = self.tracks(node, &base);
        if count.is_none() && tracks.iter().any(|track| track.prop == Prop::Count) {
            self.problem(node, "a tween of `count` needs the attribute `count` on its <text>".to_string());
        }
        if !stroked && tracks.iter().any(|track| track.prop == Prop::Draw) {
            self.problem(node, "`draw` is for a <shape> with `stroke-width`".to_string());
        }
        Some(Node {
            id: node.attribute("id").unwrap_or("").to_string(),
            line: self.line(node),
            key,
            kind,
            geometry,
            window,
            tracks,
            base,
            origin,
            look,
            flow: self.flow(node),
            text,
            inline,
            children,
            rect: Rect::ZERO,
            runs: Vec::new(),
            baseline: 0.0,
            isolates: false,
            typed: Vec::new(),
            count,
            counted: Vec::new(),
        })
    }

    fn audio(&mut self, node: Xml) -> Audio {
        self.only(node, &["id", "src", "start", "duration", "media-start", "volume"]);
        if node.attribute("src").is_none_or(str::is_empty) {
            self.problem(node, "<audio> needs the attribute `src`".to_string());
        }
        let mut automation = Vec::new();
        for point in node.children().filter(Xml::is_element) {
            if point.tag_name().name() != "volume" {
                self.problem(point, format!("<{}> is not known inside <audio>", point.tag_name().name()));
                continue;
            }
            self.only(point, &["at", "value"]);
            let at = self.time(point, "at").map_or(0.0, Time::to_f64);
            automation.push((at, self.number(point, "value", None)));
        }
        if automation.windows(2).any(|pair| pair[1].0 < pair[0].0) {
            self.problem(node, "the <volume> points of an <audio> must be in order of time".to_string());
        }
        let duration = self.time(node, "duration");
        if duration.is_none() {
            self.problem(node, "<audio> needs a `duration`".to_string());
        }
        Audio {
            id: node.attribute("id").unwrap_or("").to_string(),
            source: PathBuf::from(node.attribute("src").unwrap_or("")),
            start: self.time(node, "start").map_or(0.0, Time::to_f64),
            duration: duration.map_or(0.0, Time::to_f64),
            media_start: self.time(node, "media-start").map_or(0.0, Time::to_f64),
            volume: self.number(node, "volume", Some(1.0)),
            automation,
            line: self.line(node),
        }
    }
}

impl Reader<'_> {
    /// What the root element says about the whole composition.
    fn head(&mut self, root: Xml) -> Head {
        self.only(root, &["id", "width", "height", "fps", "duration", "background"]);
        let mut size = |name: &str| match self.number(root, name, None) {
            value if value.fract() == 0.0 && (2.0..=8192.0).contains(&value) && value % 2.0 == 0.0 => value as u16,
            value => {
                self.problem(root, format!("`{name}` must be an even whole number from 2 to 8192, not {value}"));
                2
            }
        };
        let (width, height) = (size("width"), size("height"));
        let fps = match self.number(root, "fps", Some(30.0)) {
            value if value.fract() == 0.0 && (1.0..=240.0).contains(&value) => value as u32,
            value => {
                self.problem(root, format!("`fps` must be a whole number from 1 to 240, not {value}"));
                30
            }
        };
        let duration = self.time(root, "duration").filter(|time| time.is_positive());
        if duration.is_none() {
            self.problem(root, "<composition> needs a `duration` above 0".to_string());
        }
        Head {
            width,
            height,
            fps,
            duration,
            background: self.color(root, "background", [0, 0, 0, 255]),
        }
    }

    /// The elements of a composition, in document order. Its fonts and sounds go to the reader.
    fn body(&mut self, root: Xml) -> Vec<Node> {
        let text = TextStyle::default();
        let mut children = Vec::new();
        for node in root.children().filter(Xml::is_element) {
            match node.tag_name().name() {
                "font" => {
                    self.only(node, &["src", "family"]);
                    match node.attribute("src") {
                        Some(source) if !source.is_empty() => {
                            let font = FontSource {
                                source: PathBuf::from(source),
                                family: node.attribute("family").map(str::to_string),
                                line: self.line(node),
                            };
                            let known = |other: &FontSource| other.source == font.source && other.family == font.family;
                            if !self.fonts.iter().any(known) {
                                self.fonts.push(font);
                            }
                        }
                        _ => self.problem(node, "<font> needs the attribute `src`".to_string()),
                    }
                }
                "audio" => {
                    let read = self.audio(node);
                    self.audio.push(read);
                }
                // A style draws nothing; its attributes are on the elements that take it.
                "style" => {}
                _ => children.extend(self.element(node, &text, false)),
            }
        }
        children
    }

    /// The composition in the file a `<composition>` element names.
    fn nested(&mut self, node: Xml) -> Option<Nested> {
        let allowed: Vec<&str> = COMMON
            .iter()
            .copied()
            .filter(|name| !matches!(*name, "width" | "height"))
            .chain(["src"])
            .collect();
        self.only(node, &allowed);
        let Some(written) = node.attribute("src").filter(|written| !written.is_empty()) else {
            self.problem(node, "a <composition> inside a composition needs the attribute `src`".to_string());
            return None;
        };
        let path = self.folder.join(written);
        let identity = path.canonicalize().unwrap_or_else(|_| path.clone());
        if self.stack.contains(&identity) {
            self.problem(node, format!("`{written}` shows itself, directly or through the files it shows"));
            return None;
        }
        if self.stack.len() >= DEPTH {
            self.problem(node, format!("`{written}` is more than {DEPTH} compositions deep"));
            return None;
        }
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) => {
                self.problem(node, format!("the composition `{}` could not be read: {error}", path.display()));
                return None;
            }
        };
        let (text, faults) = shared::expand(&text);
        let document = match Document::parse(&text) {
            Ok(document) => document,
            Err(error) => {
                self.problems.push(Problem {
                    file: Some(written.to_string()),
                    line: error.pos().row,
                    message: format!("the file is not well-formed XML: {error}"),
                });
                return None;
            }
        };
        let mut stack = self.stack.clone();
        stack.push(identity);
        let mut inner = Reader {
            document: &document,
            problems: Vec::new(),
            videos: self.videos,
            stills: std::mem::take(&mut self.stills),
            keys: self.keys,
            folder: self.folder.clone(),
            tables: std::mem::take(&mut self.tables),
            shaders: std::mem::take(&mut self.shaders),
            fonts: std::mem::take(&mut self.fonts),
            audio: Vec::new(),
            fps: self.fps,
            clock: 0.0,
            file: Some(written.to_string()),
            stack,
        };
        inner.faults(faults);
        let root = document.root_element();
        let read = if root.tag_name().name() == "composition" {
            let head = inner.head(root);
            inner.fps = head.fps;
            Some((head, inner.body(root)))
        } else {
            inner.problem(root, format!("the root element is <{}>, not <composition>", root.tag_name().name()));
            None
        };
        self.videos = inner.videos;
        self.stills = inner.stills;
        self.tables = inner.tables;
        self.shaders = inner.shaders;
        self.keys = inner.keys;
        self.fonts = inner.fonts;
        self.problems.append(&mut inner.problems);
        let (head, children) = read?;
        if head.fps != self.fps {
            let (theirs, ours) = (head.fps, self.fps);
            self.problem(
                node,
                format!("`{written}` has {theirs} frames a second and the composition that shows it has {ours}; they must be the same"),
            );
        }
        Some(Nested {
            head,
            children,
            audio: inner.audio,
        })
    }
}

/// `noise(0.035 3 9)`: the base frequency, the octaves and the seed.
fn noise_field(written: &str) -> Option<NoiseField> {
    let inner = written.trim().strip_prefix("noise(")?.strip_suffix(')')?;
    let mut words = inner.split_whitespace();
    let frequency = words
        .next()?
        .parse::<f64>()
        .ok()
        .filter(|frequency| *frequency > 0.0 && frequency.is_finite())?;
    let octaves = words.next()?.parse::<u32>().ok().filter(|octaves| (1..=8).contains(octaves))?;
    let seed = words.next()?.parse::<i32>().ok()?;
    words.next().is_none().then_some(NoiseField { frequency, octaves, seed })
}

/// The composition in `file`, a file of the project in `folder`, or everything that is wrong
/// with it and with the files it shows.
pub fn parse_file(file: &std::path::Path, folder: &std::path::Path) -> Result<Composition, Vec<Problem>> {
    let source = std::fs::read_to_string(file).map_err(|error| {
        vec![Problem {
            file: None,
            line: 0,
            message: format!("{} could not be read: {error}", file.display()),
        }]
    })?;
    let identity = file.canonicalize().unwrap_or_else(|_| file.to_path_buf());
    read(&source, folder, vec![identity])
}

/// The composition a text describes, or everything that is wrong with it. `folder` is the
/// project folder: every `src` is read from there.
pub fn parse(source: &str, folder: &std::path::Path) -> Result<Composition, Vec<Problem>> {
    read(source, folder, Vec::new())
}

fn read(source: &str, folder: &std::path::Path, stack: Vec<PathBuf>) -> Result<Composition, Vec<Problem>> {
    let (source, faults) = shared::expand(source);
    let document = Document::parse(&source).map_err(|error| {
        vec![Problem {
            file: None,
            line: error.pos().row,
            message: format!("the file is not well-formed XML: {error}"),
        }]
    })?;
    let mut reader = Reader {
        document: &document,
        problems: Vec::new(),
        videos: 0,
        stills: Vec::new(),
        keys: 1,
        folder: folder.to_path_buf(),
        tables: Vec::new(),
        shaders: Vec::new(),
        fonts: Vec::new(),
        audio: Vec::new(),
        fps: 30,
        clock: 0.0,
        file: None,
        stack,
    };
    reader.faults(faults);
    let root = document.root_element();
    if root.tag_name().name() != "composition" {
        reader.problem(root, format!("the root element is <{}>, not <composition>", root.tag_name().name()));
        return Err(reader.problems);
    }
    let head = reader.head(root);
    reader.fps = head.fps;
    let (width, height, fps, duration, background) = (head.width, head.height, head.fps, head.duration, head.background);
    let children = reader.body(root);
    let text = TextStyle::default();
    if !reader.problems.is_empty() {
        return Err(reader.problems);
    }
    let frame = Rect::new(0.0, 0.0, f64::from(width), f64::from(height));
    let root = Node {
        id: root.attribute("id").unwrap_or("").to_string(),
        line: reader.line(root),
        key: 0,
        kind: Kind::Box,
        geometry: Geometry::default(),
        window: None,
        base: own_values(),
        origin: (Dim::Share(0.5), Dim::Share(0.5)),
        tracks: Vec::new(),
        look: Look {
            background: Vec::new(),
            radius: 0.0,
            border: None,
            double: false,
            shadows: Vec::new(),
            clip: false,
            clip_path: None,
            blend: Blend::Normal,
            perspective: None,
            transition: None,
            finish: Finish::default(),
        },
        flow: Flow {
            flex: None,
            columns: None,
            padding: [0.0; 4],
            margin: [0.0; 4],
            align_self: None,
            grow: 0.0,
            absolute: false,
        },
        text,
        inline: Vec::new(),
        children,
        rect: frame,
        runs: Vec::new(),
        baseline: 0.0,
        isolates: false,
        typed: Vec::new(),
        count: None,
        counted: Vec::new(),
    };
    Ok(Composition {
        width,
        height,
        fps,
        duration: duration.unwrap_or(Time::ZERO),
        background,
        root,
        fonts: reader.fonts,
        audio: reader.audio,
        videos: reader.videos,
        stills: reader.stills,
        shaders: reader.shaders,
    })
}
