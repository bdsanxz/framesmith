//! The scene: the root's canvas and the boxes that stand directly in it, as far as this version
//! draws them (`docs/spec/render-command.md` section 3), and the frames drawn from it.

use draw::{Colour, DisplayList, Picture, Renderer, Step};
use layout::{Written, LIMIT};
use project::{Backend, Code, Codec, Element, Encoder, Length, Node, Problem, Project, Quality, Report, Workers};

/// What the root takes.
const ROOT_TAKES: [&str; 7] = ["id", "version", "width", "height", "fps", "duration", "background"];
/// What a `<box>` takes.
const BOX_TAKES: [&str; 6] = ["id", "left", "top", "width", "height", "background"];

/// A box that draws something, or nothing: where it lies and the colour it fills with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Placed {
    written: Written,
    colour: Option<Colour>,
}

/// A composition ready to draw.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scene {
    width: u32,
    height: u32,
    fps: u32,
    frames: u64,
    background: Colour,
    boxes: Vec<Placed>,
}

/// Why a picture was not made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PictureError {
    /// The frame asked for is not one of the scene: `number` counted from 0, `frames` in all.
    NoSuchFrame { number: u64, frames: u64 },
    /// The renderer could not draw the frame.
    Renderer(draw::Error),
}

impl std::fmt::Display for PictureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PictureError::NoSuchFrame { number, frames } => {
                write!(f, "there is no frame {number}: the frames are numbered from 0 to {}", frames - 1)
            }
            PictureError::Renderer(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for PictureError {}

impl Scene {
    /// Makes the scene of an opened project, or gives every report of what this version does not
    /// draw or apply (`FS-0035`) and of the lengths that are wrong (`FS-0034`): first those of the
    /// project file, then those of the composition, each in the order of the file.
    pub fn from_project(project: &Project) -> Result<Scene, Vec<Report>> {
        let mut reports = settings(project);
        let root = project.root();
        let tree = project.tree().nodes();
        let file = project.composition();
        let mut refuse = |code: Code, line: u32, message: String| reports.push(Report::new(file, Problem::new(code, line, message)));

        let mut boxes: Vec<Placed> = Vec::new();
        // For each element: the place of its box in `boxes` when it is a box that is drawn.
        let mut drawn: Vec<Option<usize>> = Vec::with_capacity(tree.len());

        for (index, node) in tree.iter().enumerate() {
            let Some(parent) = node.parent() else {
                // The root.
                for attribute in node.attributes() {
                    if !ROOT_TAKES.contains(&attribute.name()) {
                        refuse(
                            Code::FormNotSupported,
                            attribute.line(),
                            unsupported_attribute(node, attribute.name()),
                        );
                    }
                }
                if let (Some(attribute), true) = (node.attribute("background"), root.background.alpha != 255) {
                    refuse(
                        Code::FormNotSupported,
                        attribute.line(),
                        unsupported_value(node, attribute.name(), &attribute.shown(), "one opaque colour, #rrggbb"),
                    );
                }
                drawn.push(None);
                continue;
            };
            if parent == 0 && node.element() == Element::Box {
                let (placed, problems) = place_box(node);
                for (code, line, message) in problems {
                    refuse(code, line, message);
                }
                drawn.push(Some(boxes.len()));
                boxes.push(placed);
            } else if parent == 0 {
                refuse(Code::FormNotSupported, node.line(), unsupported_element(node.element()));
                drawn.push(None);
            } else if drawn[parent].is_some() {
                refuse(Code::FormNotSupported, node.line(), unsupported_inside_box(node.element()));
                drawn.push(None);
            } else {
                // Inside an element that is already reported: nothing more is said of it.
                drawn.push(None);
            }
            debug_assert_eq!(drawn.len(), index + 1);
        }

        if !reports.is_empty() {
            return Err(reports);
        }
        let background = Colour {
            red: root.background.red,
            green: root.background.green,
            blue: root.background.blue,
        };
        // The component stays the one `docs/spec/run-log.md` section 4 names for this line.
        tracing::debug!(target: "render::scene", boxes = boxes.len(), frames = root.frames, "scene made");
        Ok(Scene {
            width: root.width,
            height: root.height,
            fps: root.fps,
            frames: root.frames,
            background,
            boxes,
        })
    }

    /// The frame's width in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// The frame's height in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Frames a second.
    pub fn fps(&self) -> u32 {
        self.fps
    }

    /// The number of frames; they are numbered from 0.
    pub fn frames(&self) -> u64 {
        self.frames
    }

    /// How many boxes the scene has, drawn or not.
    pub fn boxes(&self) -> usize {
        self.boxes.len()
    }

    /// The steps that draw frame `number`: the same list for the same number, whatever was asked
    /// for before.
    pub fn list(&self, number: u64) -> Result<DisplayList, PictureError> {
        if number >= self.frames {
            return Err(PictureError::NoSuchFrame {
                number,
                frames: self.frames,
            });
        }
        // Nothing in this version moves or appears: every frame has the same steps.
        let steps = self
            .boxes
            .iter()
            .filter_map(|placed| {
                placed.colour.map(|colour| Step::Fill {
                    rect: layout::place(self.width, self.height, placed.written),
                    colour,
                })
            })
            .collect();
        Ok(DisplayList {
            width: self.width,
            height: self.height,
            background: self.background,
            steps,
        })
    }

    /// Draws frame `number` with `renderer`.
    pub fn picture(&self, number: u64, renderer: &mut dyn Renderer) -> Result<Picture, PictureError> {
        renderer.draw(&self.list(number)?).map_err(PictureError::Renderer)
    }
}

type Refusal = (Code, u32, String);

/// A `<box>` that stands in the frame: its place and colour, and every refusal of its attributes.
fn place_box(node: &Node) -> (Placed, Vec<Refusal>) {
    let mut refusals: Vec<Refusal> = Vec::new();
    let mut written = Written::default();
    let mut colour = None;
    for attribute in node.attributes() {
        let name = attribute.name();
        if !BOX_TAKES.contains(&name) {
            refusals.push((Code::FormNotSupported, attribute.line(), unsupported_attribute(node, name)));
            continue;
        }
        let value = attribute.shown();
        match name {
            "left" | "top" => match edge(attribute.length(), &value, name, node.element().tag()) {
                Ok(pixels) => {
                    if name == "left" {
                        written.left = Some(pixels);
                    } else {
                        written.top = Some(pixels);
                    }
                }
                Err((code, message)) => refusals.push((code, attribute.line(), message)),
            },
            "width" | "height" => match size(attribute.length(), &value, name, node.element().tag()) {
                Ok(pixels) => {
                    if name == "width" {
                        written.width = Some(pixels);
                    } else {
                        written.height = Some(pixels);
                    }
                }
                Err((code, message)) => refusals.push((code, attribute.line(), message)),
            },
            "background" => match attribute.colour() {
                Some(found) if found.alpha == 255 => {
                    colour = Some(Colour {
                        red: found.red,
                        green: found.green,
                        blue: found.blue,
                    })
                }
                _ => refusals.push((
                    Code::FormNotSupported,
                    attribute.line(),
                    unsupported_value(node, name, &value, "one opaque colour, #rrggbb"),
                )),
            },
            _ => {}
        }
    }
    (Placed { written, colour }, refusals)
}

/// `left` or `top`: a whole number of pixels within the limit.
fn edge(length: Option<Length>, value: &str, name: &str, tag: &str) -> Result<i32, (Code, String)> {
    match length {
        None => Err((Code::LengthNotAccepted, not_a_length(name, value))),
        Some(Length::Pixels(pixels)) if pixels.fract() == 0.0 && pixels.abs() <= LIMIT as f64 => Ok(pixels as i32),
        Some(_) => Err((
            Code::FormNotSupported,
            format!("`{name}=\"{value}\"` on {tag} is not a form this version supports: a whole number of pixels from -{LIMIT} to {LIMIT}"),
        )),
    }
}

/// `width` or `height`: a whole number of pixels, 0 or more, within the limit.
fn size(length: Option<Length>, value: &str, name: &str, tag: &str) -> Result<u32, (Code, String)> {
    match length {
        None => Err((Code::LengthNotAccepted, not_a_length(name, value))),
        Some(Length::Pixels(pixels) | Length::Percent(pixels)) if pixels < 0.0 => Err((
            Code::LengthNotAccepted,
            format!("`{name}=\"{value}\"` is below 0; a size is 0 or more"),
        )),
        Some(Length::Pixels(pixels)) if pixels.fract() == 0.0 && pixels <= LIMIT as f64 => Ok(pixels as u32),
        Some(_) => Err((
            Code::FormNotSupported,
            format!("`{name}=\"{value}\"` on {tag} is not a form this version supports: a whole number of pixels from 0 to {LIMIT}"),
        )),
    }
}

fn not_a_length(name: &str, value: &str) -> String {
    format!("`{name}=\"{value}\"` is not a length: a number of pixels or a percentage, such as 40 or 50%")
}

fn unsupported_element(element: Element) -> String {
    format!("{} is not supported by this version, which draws <box> only", element.tag())
}

fn unsupported_inside_box(element: Element) -> String {
    format!(
        "{} inside a <box> is not supported by this version, which draws a <box> with no element inside it",
        element.tag()
    )
}

fn unsupported_attribute(node: &Node, name: &str) -> String {
    format!(
        "{} has an attribute this version does not support: `{}`",
        node.element().tag(),
        project::shown(name)
    )
}

fn unsupported_value(node: &Node, name: &str, value: &str, takes: &str) -> String {
    format!(
        "`{name}=\"{value}\"` on {} is not a form this version supports: {takes}",
        node.element().tag()
    )
}

/// Settings of the project file that this version does not apply: the renderer is the processor's,
/// the encoder the system's hardware encoder at `standard`, one thread draws and encodes.
fn settings(project: &Project) -> Vec<Report> {
    let settings = project.settings();
    let mut found: Vec<(u32, String)> = Vec::new();
    let mut check = |key: &str, line: Option<u32>, accepted: bool, value: String, takes: &str| {
        if let (Some(line), false) = (line, accepted) {
            found.push((
                line,
                format!("`{key}: {value}` is not supported by this version, which takes {takes}"),
            ));
        }
    };
    check(
        "render.backend",
        settings.line_of("render.backend"),
        matches!(settings.backend(), Backend::Auto | Backend::Cpu),
        settings.backend().to_string(),
        "auto or cpu",
    );
    check(
        "render.quality",
        settings.line_of("render.quality"),
        settings.quality() == Quality::Standard,
        settings.quality().to_string(),
        "standard",
    );
    check(
        "render.encoder",
        settings.line_of("render.encoder"),
        settings.encoder() == Encoder::Hardware,
        settings.encoder().to_string(),
        "hardware",
    );
    check(
        "render.codec",
        settings.line_of("render.codec"),
        settings.codec() == Codec::H264,
        settings.codec().to_string(),
        "h264",
    );
    check(
        "render.workers",
        settings.line_of("render.workers"),
        matches!(settings.workers(), Workers::Auto | Workers::Count(1)),
        settings.workers().to_string(),
        "auto or 1",
    );
    check(
        "render.audio-bitrate",
        settings.line_of("render.audio-bitrate"),
        settings.audio_bitrate() == 192,
        settings.audio_bitrate().to_string(),
        "192",
    );
    found.sort_by_key(|(line, _)| *line);
    found
        .into_iter()
        .map(|(line, message)| Report::new(project::PROJECT_FILE, Problem::new(Code::FormNotSupported, line, message)))
        .collect()
}
