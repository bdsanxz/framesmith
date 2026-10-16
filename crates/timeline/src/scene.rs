//! The scene: the root's canvas and the boxes that stand directly in it, as far as this version
//! draws them (`docs/spec/render-command.md` section 3), and the frames drawn from it.

use draw::{Colour, DisplayList, Picture, Renderer, Step};
use layout::{Amount, Beyond, Rect, Written, LIMIT};
use project::{Code, Codec, Element, Encoder, Length, Node, Problem, Project, Quality, Report, Workers};
use timebase::Time;

/// The lowest and the highest bit rate of the sound, in kbit/s, that the system's AAC encoder takes
/// at 48 kHz in stereo (`frx7/docs/spec/os-codecs.md` section 3.1, measured).
const SOUND_KBITS_LOWEST: u32 = 64;
const SOUND_KBITS_HIGHEST: u32 = 320;

/// What the root takes.
const ROOT_TAKES: [&str; 7] = ["id", "version", "width", "height", "fps", "duration", "background"];
/// What a `<box>` takes.
const BOX_TAKES: [&str; 12] = [
    "id",
    "left",
    "top",
    "right",
    "bottom",
    "width",
    "height",
    "position",
    "margin",
    "background",
    "start",
    "duration",
];

/// The frames a box is shown in: from `first` up to `end`, not counting `end`, numbered from 0. A
/// window whose `end` is not after its `first` holds no frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Window {
    first: u64,
    end: u64,
}

impl Window {
    /// Every frame of a scene of `frames` frames.
    fn all(frames: u64) -> Window {
        Window { first: 0, end: frames }
    }

    /// The frames of a clip from `begin` to `finish`, both on the timeline, at `fps` frames a second.
    /// Frame `i` is in the clip when the grid frame of `begin` is at most `i` and the grid frame of
    /// `finish` is above `i` (`docs/spec/frame-time.md`).
    fn of_clip(begin: Time, finish: Time, fps: u32) -> Window {
        Window {
            first: begin.grid_frame(fps).max(0) as u64,
            end: finish.grid_frame(fps).max(0) as u64,
        }
    }

    /// The frames that both windows hold.
    fn within(self, outer: Window) -> Window {
        let first = self.first.max(outer.first);
        Window {
            first,
            end: self.end.min(outer.end).max(first),
        }
    }

    /// Whether frame `number` is in the window.
    fn contains(self, number: u64) -> bool {
        self.first <= number && number < self.end
    }
}

/// Where a box is placed and when it is shown: its rectangle in the frame, the origin its children
/// count their `start` from (the exact start of the box when it is a clip, otherwise the origin of
/// its parent), and the frames it is shown in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Slot {
    rect: Rect,
    origin: Time,
    window: Window,
}

/// A box that is placed and drawn, or placed and drawn as nothing: its slot and the colour it fills
/// with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Placed {
    slot: Slot,
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
    /// draw or apply (`FS-0035`), of the lengths and the words that are wrong (`FS-0032`, `FS-0034`,
    /// `FS-0039`): first those of the project file, then those of the composition, each in the order
    /// of the file.
    pub fn from_project(project: &Project) -> Result<Scene, Vec<Report>> {
        let mut reports = settings(project);
        let (boxes, found) = walk(project, false);
        reports.extend(found);
        if !reports.is_empty() {
            return Err(reports);
        }
        let root = project.root();
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
        // A box is drawn in the frames of its window and in no other, so it appears and disappears only
        // with its window; the steps of a frame are in document order (`docs/spec/render-command.md`,
        // section 4.2).
        let steps = self
            .boxes
            .iter()
            .filter(|placed| placed.slot.window.contains(number))
            .filter_map(|placed| {
                placed.colour.map(|colour| Step::Fill {
                    rect: placed.slot.rect,
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

/// The reports of the forms of a place, a size, a margin and a position that this version does not
/// list, in the order of the file. They are the reports `Scene::from_project` makes of those forms,
/// by the same code, without the settings of the project file and without the refusals of what is
/// drawn: `check` judges the forms of a place and does not judge the rest (`docs/spec/check-command.md`,
/// section 3b).
pub fn place_reports(project: &Project) -> Vec<Report> {
    walk(project, true).1
}

/// Walks the project's tree in document order: the root, then each element after its parent. A box
/// is placed (`place_box`) unless it, or an element it stands in, is refused. Gives the boxes that are
/// placed, in document order, and the reports of the refusals. With `place_only`, the reports are those
/// of a place form alone.
fn walk(project: &Project, place_only: bool) -> (Vec<Placed>, Vec<Report>) {
    let root = project.root();
    let tree = project.tree().nodes();
    let file = project.composition();
    let mut reports: Vec<Report> = Vec::new();
    let mut refuse = |refusal: Refusal| {
        if refusal.place || !place_only {
            reports.push(Report::new(file, Problem::new(refusal.code, refusal.line, refusal.message)));
        }
    };

    let frame = layout::frame(root.width, root.height);
    let mut boxes: Vec<Placed> = Vec::new();
    // For each element, in document order: the slot its box is placed in. The root's is the frame,
    // shown in every frame. `None` is for an element that is reported and for one inside it, which
    // is not looked into. An element comes after its parent in the tree, so the parent's entry is
    // already known.
    let mut places: Vec<Option<Slot>> = Vec::with_capacity(tree.len());

    for node in tree {
        let Some(parent) = node.parent() else {
            // The root.
            for attribute in node.attributes() {
                if !ROOT_TAKES.contains(&attribute.name()) {
                    refuse(Refusal::of_drawing(
                        Code::FormNotSupported,
                        attribute.line(),
                        unsupported_attribute(node, attribute.name()),
                    ));
                }
            }
            if let (Some(attribute), true) = (node.attribute("background"), root.background.alpha != 255) {
                refuse(Refusal::of_drawing(
                    Code::FormNotSupported,
                    attribute.line(),
                    unsupported_value(node, attribute.name(), &attribute.shown(), "one opaque colour, #rrggbb"),
                ));
            }
            places.push(Some(Slot {
                rect: frame,
                origin: Time::ZERO,
                window: Window::all(root.frames),
            }));
            continue;
        };
        let Some(parent_place) = places[parent] else {
            // Inside an element that is reported: nothing more is said of it.
            places.push(None);
            continue;
        };
        if node.element() != Element::Box {
            let message = if parent == 0 {
                unsupported_element(node.element())
            } else {
                unsupported_inside_box(node.element())
            };
            refuse(Refusal::of_drawing(Code::FormNotSupported, node.line(), message));
            places.push(None);
            continue;
        }
        let (placed, refusals) = place_box(node, parent_place, root.fps);
        for refusal in refusals {
            refuse(refusal);
        }
        places.push(placed.map(|placed| {
            boxes.push(placed);
            placed.slot
        }));
    }
    (boxes, reports)
}

/// A refusal of the root or of a box: its code, its line, its message, and whether it is of a form
/// of a place (`place`: a length, a size, a margin or a position), which `check` judges as well.
struct Refusal {
    code: Code,
    line: u32,
    message: String,
    place: bool,
}

impl Refusal {
    /// A refusal of a form of a place, a size, a margin or a position.
    fn of_place(code: Code, line: u32, message: String) -> Refusal {
        Refusal {
            code,
            line,
            message,
            place: true,
        }
    }

    /// A refusal of anything else that this version does not draw or apply.
    fn of_drawing(code: Code, line: u32, message: String) -> Refusal {
        Refusal {
            code,
            line,
            message,
            place: false,
        }
    }
}

/// A `<box>` in its parent's slot: the place it is drawn at, its colour, the frames it is shown in,
/// and every refusal of its attributes. A box with a refusal, or whose place cannot be worked out
/// within the limit, has no place; what is inside it is not looked into.
fn place_box(node: &Node, parent: Slot, fps: u32) -> (Option<Placed>, Vec<Refusal>) {
    let mut refusals: Vec<Refusal> = Vec::new();
    let mut written = Written::default();
    let mut colour = None;
    let mut start: Option<Time> = None;
    let mut duration: Option<Time> = None;
    for attribute in node.attributes() {
        let name = attribute.name();
        if !BOX_TAKES.contains(&name) {
            refusals.push(Refusal::of_drawing(
                Code::FormNotSupported,
                attribute.line(),
                unsupported_attribute(node, name),
            ));
            continue;
        }
        let value = attribute.shown();
        match name {
            "left" | "top" | "right" | "bottom" | "width" | "height" => {
                match amount(attribute.length(), &value, name, node.element().tag()) {
                    Ok(found) => match name {
                        "left" => written.left = Some(found),
                        "top" => written.top = Some(found),
                        "right" => written.right = Some(found),
                        "bottom" => written.bottom = Some(found),
                        "width" => written.width = Some(found),
                        _ => written.height = Some(found),
                    },
                    Err((code, message)) => refusals.push(Refusal::of_place(code, attribute.line(), message)),
                }
            }
            "position" => {
                if !matches!(attribute.value(), "static" | "absolute") {
                    refusals.push(Refusal::of_place(
                        Code::WordNotTaken,
                        attribute.line(),
                        format!("`position=\"{value}\"` is not one of: static, absolute"),
                    ));
                }
            }
            "margin" => {
                // The sides are read and their values held; no margin moves a box (N10).
                if attribute.sides().is_none() {
                    refusals.push(Refusal::of_place(
                        Code::NotANumber,
                        attribute.line(),
                        format!("`margin=\"{value}\"` is not one to four numbers, such as 4 or 4 8"),
                    ));
                }
            }
            "background" => match attribute.colour() {
                Some(found) if found.alpha == 255 => {
                    colour = Some(Colour {
                        red: found.red,
                        green: found.green,
                        blue: found.blue,
                    })
                }
                _ => refusals.push(Refusal::of_drawing(
                    Code::FormNotSupported,
                    attribute.line(),
                    unsupported_value(node, name, &value, "one opaque colour, #rrggbb"),
                )),
            },
            "start" | "duration" => match attribute.time() {
                Some(time) if name == "start" => start = Some(time),
                Some(time) => duration = Some(time),
                None => refusals.push(Refusal::of_drawing(
                    Code::NotATime,
                    attribute.line(),
                    format!("`{name}=\"{value}\"` is not a time in seconds: digits with an optional point, such as 5 or 1.25"),
                )),
            },
            _ => {}
        }
    }
    if !refusals.is_empty() {
        return (None, refusals);
    }
    // The notation judges the pair (FS-0040): a box has both or neither. One without the other is
    // refused here as well, so that no window is made of it.
    let clip = match (start, duration) {
        (Some(start), Some(duration)) => Some((parent.origin + start, duration)),
        (None, None) => None,
        _ => {
            let message = format!("{} needs both start and duration, or neither", node.element().tag());
            return (None, vec![Refusal::of_drawing(Code::ClipNotPaired, node.line(), message)]);
        }
    };
    match layout::place(parent.rect, written) {
        Ok(rect) => {
            let slot = match clip {
                Some((begin, duration)) => Slot {
                    rect,
                    origin: begin,
                    window: Window::of_clip(begin, begin + duration, fps).within(parent.window),
                },
                None => Slot {
                    rect,
                    origin: parent.origin,
                    window: parent.window,
                },
            };
            (Some(Placed { slot, colour }), refusals)
        }
        Err(Beyond) => {
            let message = format!(
                "a place or a size of {} works out beyond {LIMIT} pixels, which this version does not place",
                node.element().tag()
            );
            (None, vec![Refusal::of_place(Code::FormNotSupported, node.line(), message)])
        }
    }
}

/// A place or a size as written (`notation.md` section 4): a length. A length in pixels is a whole
/// number within the limit (from -1000000 to 1000000 for an edge, from 0 to 1000000 for a size); a
/// percentage is any value of its kind; a size below 0 is wrong (N6), whatever its form.
fn amount(length: Option<Length>, value: &str, name: &str, tag: &str) -> Result<Amount, (Code, String)> {
    let sized = matches!(name, "width" | "height");
    match length {
        None => Err((Code::LengthNotAccepted, not_a_length(name, value))),
        Some(Length::Pixels(pixels) | Length::Percent(pixels)) if sized && pixels < 0.0 => Err((
            Code::LengthNotAccepted,
            format!("`{name}=\"{value}\"` is below 0; a size is 0 or more"),
        )),
        Some(Length::Pixels(pixels)) if pixels.fract() == 0.0 && pixels.abs() <= LIMIT as f64 => Ok(Amount::Pixels(pixels)),
        Some(Length::Percent(percent)) => Ok(Amount::Percent(percent)),
        Some(_) => Err((
            Code::FormNotSupported,
            format!(
                "`{name}=\"{value}\"` on {tag} is not a form this version supports: {}",
                if sized {
                    "a whole number of pixels from 0 to 1000000, or a percentage such as 50%"
                } else {
                    "a whole number of pixels from -1000000 to 1000000, or a percentage such as 50%"
                }
            ),
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
        "{} inside a <box> is not supported by this version, which draws a <box> inside a <box> and no other element",
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

/// Settings in use that this version does not apply: the renderer is the processor's, the encoder
/// the system's hardware encoder at `standard`, one thread draws and encodes, and the system's AAC
/// encoder takes 64 to 320 kbit/s. A value an option gave is reported on the command line; a value
/// of the project file on the line of the value.
fn settings(project: &Project) -> Vec<Report> {
    let settings = project.settings();
    let mut from_options: Vec<String> = Vec::new();
    let mut found: Vec<(u32, String)> = Vec::new();
    let mut check = |key: &str, option: Option<&str>, accepted: bool, value: String, takes: &str| {
        if accepted {
            return;
        }
        if let (true, Some(option)) = (settings.from_option(key), option) {
            from_options.push(format!("`{option} {value}` is not supported by this version, which takes {takes}"));
        } else if let Some(line) = settings.line_of(key) {
            found.push((
                line,
                format!("`{key}: {value}` is not supported by this version, which takes {takes}"),
            ));
        }
    };
    check(
        "render.quality",
        Some("--quality"),
        settings.quality() == Quality::Standard,
        settings.quality().to_string(),
        "standard",
    );
    check(
        "render.encoder",
        Some("--encoder"),
        settings.encoder() == Encoder::Hardware,
        settings.encoder().to_string(),
        "hardware",
    );
    check(
        "render.codec",
        Some("--codec"),
        settings.codec() == Codec::H264,
        settings.codec().to_string(),
        "h264",
    );
    check(
        "render.workers",
        Some("--workers"),
        matches!(settings.workers(), Workers::Auto | Workers::Count(1)),
        settings.workers().to_string(),
        "auto or 1",
    );
    check(
        "render.audio-bitrate",
        // No option sets the sound's bit rate: only the project file does.
        None,
        (SOUND_KBITS_LOWEST..=SOUND_KBITS_HIGHEST).contains(&settings.audio_bitrate()),
        settings.audio_bitrate().to_string(),
        "64 to 320",
    );
    found.sort_by_key(|(line, _)| *line);
    // The command line is read before the project file, so its reports come first.
    from_options
        .into_iter()
        .map(|message| Report::new(project::COMMAND_LINE, Problem::new(Code::FormNotSupported, 0, message)))
        .chain(
            found
                .into_iter()
                .map(|(line, message)| Report::new(project::PROJECT_FILE, Problem::new(Code::FormNotSupported, line, message))),
        )
        .collect()
}
