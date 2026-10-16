//! The scene: the root's canvas and the boxes that stand directly in it, as far as this version
//! draws them (`docs/spec/render-command.md` section 3), and the frames drawn from it.

use draw::{Colour, DisplayList, Picture, Renderer, Step};
use layout::{Across, Along, Amount, Beyond, Direction, Flex, Item, Measured, Rect, Sides, Written, LIMIT};
use project::{Code, Codec, Element, Encoder, Length, Node, Problem, Project, Quality, Report, Workers};
use timebase::Time;

/// The lowest and the highest bit rate of the sound, in kbit/s, that the system's AAC encoder takes
/// at 48 kHz in stereo (`frx7/docs/spec/os-codecs.md` section 3.1, measured).
const SOUND_KBITS_LOWEST: u32 = 64;
const SOUND_KBITS_HIGHEST: u32 = 320;

/// What the root takes.
const ROOT_TAKES: [&str; 7] = ["id", "version", "width", "height", "fps", "duration", "background"];
/// What a `<box>` takes: its place, its margin and its position, what it draws and its window, and
/// the words and numbers of the row or column it lays out (`docs/spec/render-command.md` section 4.3).
const BOX_TAKES: [&str; 18] = [
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
    "flex-direction",
    "justify-content",
    "align-items",
    "align-self",
    "gap",
    "padding",
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

/// The reports of the forms of a place, a size, a margin, a position and the words and numbers of a
/// row or column that this version does not list, in the order of the file. They are the reports
/// `Scene::from_project` makes of those forms, by the same code, without the settings of the project
/// file and without the refusals of what is drawn: `check` judges the forms of a place and does not
/// judge the rest (`docs/spec/check-command.md`, section 3b).
pub fn place_reports(project: &Project) -> Vec<Report> {
    walk(project, true).1
}

/// Walks the project's tree in document order: the root, then each element after its parent. A box
/// is placed unless it, or an element it stands in, is refused. A row or column reads the boxes inside
/// it ahead, since its flow places them (`docs/spec/render-command.md` section 4.3). Gives the boxes
/// that are placed, in document order, and the reports of the refusals in the order of the file. With
/// `place_only`, the reports are those of a place form alone.
fn walk(project: &Project, place_only: bool) -> (Vec<Placed>, Vec<Report>) {
    let root = project.root();
    let tree = project.tree().nodes();
    let file = project.composition();
    let frame = layout::frame(root.width, root.height);
    // The elements inside each element, in document order.
    let mut inside: Vec<Vec<usize>> = vec![Vec::new(); tree.len()];
    for (index, node) in tree.iter().enumerate() {
        if let Some(parent) = node.parent() {
            inside[parent].push(index);
        }
    }

    let mut refusals: Vec<Refusal> = Vec::new();
    let mut boxes: Vec<Placed> = Vec::new();
    // For each element, in document order: the slot its box is placed in. The root's is the frame,
    // shown in every frame. `None` is for an element that is reported and for one inside it, which
    // is not looked into. An element comes after its parent in the tree, so the parent's entry is
    // already known.
    let mut places: Vec<Option<Slot>> = Vec::with_capacity(tree.len());
    // A box that its row or column has read ahead, and the rectangle that row or column gave it: a box
    // inside a row or column is placed by the flow of its parent, not by its own edges.
    let mut reads: Vec<Option<Read>> = vec![None; tree.len()];
    // Each box's place is given by its row or column's flow, with the axes that flow measures for it.
    let mut assigned: Vec<Option<(Rect, Measured)>> = vec![None; tree.len()];
    // The size of each row or column measured from its items, from the inside out.
    let sizes = measures(tree, &inside);

    for (index, node) in tree.iter().enumerate() {
        let Some(parent) = node.parent() else {
            // The root.
            for attribute in node.attributes() {
                if !ROOT_TAKES.contains(&attribute.name()) {
                    refusals.push(Refusal::of_drawing(
                        Code::FormNotSupported,
                        attribute.line(),
                        unsupported_attribute(node, attribute.name()),
                    ));
                }
            }
            if let (Some(attribute), true) = (node.attribute("background"), root.background.alpha != 255) {
                refusals.push(Refusal::of_drawing(
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
            refusals.push(Refusal::of_drawing(Code::FormNotSupported, node.line(), message));
            places.push(None);
            continue;
        }
        let read = match reads[index].take() {
            Some(read) => read,
            None => {
                let (read, found) = read_box(node);
                refusals.extend(found);
                read
            }
        };
        let Read::Valid(form) = read else {
            places.push(None);
            continue;
        };

        // The boxes inside a row or column are read now: they are its items, and their places come from
        // its flow. Their refusals are kept until this box is placed.
        let mut pending: Vec<Refusal> = Vec::new();
        let mut items: Vec<Item> = Vec::new();
        let mut item_nodes: Vec<(usize, Measured)> = Vec::new();
        if let Some(flex) = form.flex {
            for &inner in &inside[index] {
                let inner_node = &tree[inner];
                if inner_node.element() != Element::Box {
                    continue;
                }
                let (read, found) = read_box(inner_node);
                pending.extend(found);
                if let Read::Valid(inner_form) = &read {
                    items.push(item_in(inner_form, sizes[inner], &flex));
                    item_nodes.push((inner, measured_axes(inner_form, &flex)));
                }
                reads[inner] = Some(read);
            }
        }
        let laid = match (assigned[index], form.flex) {
            // An item of a row or column: its own place is the flow's, and its items are laid out in it.
            (Some((rect, measured)), Some(flex)) => layout::inside(rect, measured, &flex, &items).map(|flowed| (rect, flowed.items)),
            (Some((rect, _)), None) => Ok((rect, Vec::new())),
            // A row or column with no place of its own: its size is worked out with its items.
            (None, Some(flex)) => {
                layout::flow(parent_place.rect, form.written, &flex, &items).map(|flowed| (flowed.container, flowed.items))
            }
            (None, None) => layout::place(parent_place.rect, form.written).map(|rect| (rect, Vec::new())),
        };
        let (rect, item_rects) = match laid {
            Ok(laid) => laid,
            Err(Beyond) => {
                let message = format!(
                    "a place or a size of {} works out beyond {LIMIT} pixels, which this version does not place",
                    node.element().tag()
                );
                refusals.push(Refusal::of_place(Code::FormNotSupported, node.line(), message));
                places.push(None);
                continue;
            }
        };
        refusals.append(&mut pending);
        for ((inner, measured), inner_rect) in item_nodes.into_iter().zip(item_rects) {
            assigned[inner] = Some((inner_rect, measured));
        }

        let slot = match form.clip {
            Some((start, duration)) => {
                let begin = parent_place.origin + start;
                Slot {
                    rect,
                    origin: begin,
                    window: Window::of_clip(begin, begin + duration, root.fps).within(parent_place.window),
                }
            }
            None => Slot {
                rect,
                origin: parent_place.origin,
                window: parent_place.window,
            },
        };
        boxes.push(Placed { slot, colour: form.colour });
        places.push(Some(slot));
    }

    // A row or column reads the boxes inside it ahead, so the reports are put in the order of the file
    // by their lines; a stable sort keeps the order of the reports of one line.
    refusals.sort_by_key(|refusal| refusal.line);
    let reports = refusals
        .into_iter()
        .filter(|refusal| refusal.place || !place_only)
        .map(|refusal| Report::new(file, Problem::new(refusal.code, refusal.line, refusal.message)))
        .collect();
    (boxes, reports)
}

/// A refusal of the root or of a box: its code, its line, its message, and whether it is of a form
/// of a place (`place`: a length, a size, a margin, a position or a word of a row or column), which
/// `check` judges as well.
struct Refusal {
    code: Code,
    line: u32,
    message: String,
    place: bool,
}

impl Refusal {
    /// A refusal of a form of a place, a size, a margin, a position or a row or column.
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

/// What a `<box>` writes, read once from its attributes: its place as written, its margin, its own
/// `align-self`, whether it is `position="absolute"`, the row or column it lays out, its colour, and its
/// clip (`start` and `duration` as written; the origin of its parent is added when it is placed).
#[derive(Clone, Debug, PartialEq)]
struct Form {
    written: Written,
    margin: Sides,
    across: Option<Across>,
    absolute: bool,
    flex: Option<Flex>,
    colour: Option<Colour>,
    clip: Option<(Time, Time)>,
}

/// The reading of a `<box>`: its form, or a refusal of one of its attributes or of its clip.
#[derive(Clone, Debug, PartialEq)]
enum Read {
    Valid(Box<Form>),
    Refused,
}

/// Reads a `<box>`'s attributes (`docs/spec/render-command.md` sections 3, 4.2 and 4.3). Gives its form,
/// or every refusal of its attributes and of its clip; a box with a refusal has no form and no place.
fn read_box(node: &Node) -> (Read, Vec<Refusal>) {
    let mut refusals: Vec<Refusal> = Vec::new();
    let mut written = Written::default();
    let mut colour = None;
    let mut start: Option<Time> = None;
    let mut duration: Option<Time> = None;
    let mut margin = Sides::default();
    let mut absolute = false;
    let mut own_across: Option<Across> = None;
    let mut direction: Option<Direction> = None;
    let mut along = Along::default();
    let mut items_across = Across::default();
    let mut gap: f64 = 0.0;
    let mut padding = Sides::default();
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
            "position" => match attribute.value() {
                "static" => {}
                "absolute" => absolute = true,
                _ => refusals.push(Refusal::of_place(
                    Code::WordNotTaken,
                    attribute.line(),
                    format!("`position=\"{value}\"` is not one of: static, absolute"),
                )),
            },
            "margin" => match attribute.sides().and_then(|values| Sides::from_values(&values)) {
                // The sides are held: a margin moves an item of a row or column, and no other box (N10).
                Some(sides) => margin = sides,
                None => refusals.push(Refusal::of_place(
                    Code::NotANumber,
                    attribute.line(),
                    format!("`margin=\"{value}\"` is not one to four numbers, such as 4 or 4 8"),
                )),
            },
            "flex-direction" => match attribute.value() {
                "row" => direction = Some(Direction::Row),
                "column" => direction = Some(Direction::Column),
                _ => refusals.push(Refusal::of_place(
                    Code::WordNotTaken,
                    attribute.line(),
                    format!("`flex-direction=\"{value}\"` is not one of: row, column"),
                )),
            },
            "justify-content" => match attribute.value() {
                "flex-start" => along = Along::Start,
                "center" => along = Along::Center,
                "flex-end" => along = Along::End,
                "space-between" => refusals.push(Refusal::of_place(
                    Code::FormNotSupported,
                    attribute.line(),
                    unsupported_value(node, name, &value, "flex-start, center or flex-end"),
                )),
                _ => refusals.push(Refusal::of_place(
                    Code::WordNotTaken,
                    attribute.line(),
                    format!("`justify-content=\"{value}\"` is not one of: flex-start, center, flex-end, space-between"),
                )),
            },
            "align-items" | "align-self" => {
                let found = match attribute.value() {
                    "flex-start" => Some(Across::Start),
                    "center" => Some(Across::Center),
                    "flex-end" => Some(Across::End),
                    "stretch" => Some(Across::Stretch),
                    "baseline" => {
                        refusals.push(Refusal::of_place(
                            Code::FormNotSupported,
                            attribute.line(),
                            unsupported_value(node, name, &value, "flex-start, center, flex-end or stretch"),
                        ));
                        None
                    }
                    _ => {
                        refusals.push(Refusal::of_place(
                            Code::WordNotTaken,
                            attribute.line(),
                            format!("`{name}=\"{value}\"` is not one of: flex-start, center, flex-end, stretch, baseline"),
                        ));
                        None
                    }
                };
                if let Some(found) = found {
                    if name == "align-items" {
                        items_across = found;
                    } else {
                        own_across = Some(found);
                    }
                }
            }
            "gap" => match attribute.length() {
                Some(Length::Pixels(pixels)) if pixels < 0.0 => refusals.push(Refusal::of_place(
                    Code::LengthNotAccepted,
                    attribute.line(),
                    format!("`gap=\"{value}\"` must be 0 or more"),
                )),
                Some(Length::Pixels(pixels)) => gap = pixels,
                Some(Length::Percent(_)) => refusals.push(Refusal::of_place(
                    Code::FormNotSupported,
                    attribute.line(),
                    unsupported_value(node, name, &value, "a number of pixels, 0 or more"),
                )),
                None => refusals.push(Refusal::of_place(
                    Code::LengthNotAccepted,
                    attribute.line(),
                    format!("`gap=\"{value}\"` is not a length: a number of pixels, such as 8"),
                )),
            },
            "padding" => match attribute.sides().and_then(|values| Sides::from_values(&values)) {
                None => refusals.push(Refusal::of_place(
                    Code::NotANumber,
                    attribute.line(),
                    format!("`padding=\"{value}\"` is not one to four numbers, such as 4 or 4 8"),
                )),
                Some(sides) if [sides.top, sides.right, sides.bottom, sides.left].iter().any(|side| *side < 0.0) => {
                    refusals.push(Refusal::of_place(
                        Code::LengthNotAccepted,
                        attribute.line(),
                        format!("`padding=\"{value}\"` must be 0 or more"),
                    ))
                }
                Some(sides) => padding = sides,
            },
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
        return (Read::Refused, refusals);
    }
    // The notation judges the pair (FS-0040): a box has both or neither. One without the other is
    // refused here as well, so that no window is made of it.
    let clip = match (start, duration) {
        (Some(start), Some(duration)) => Some((start, duration)),
        (None, None) => None,
        _ => {
            let message = format!("{} needs both start and duration, or neither", node.element().tag());
            return (Read::Refused, vec![Refusal::of_drawing(Code::ClipNotPaired, node.line(), message)]);
        }
    };
    let flex = direction.map(|direction| Flex {
        direction,
        along,
        across: items_across,
        gap,
        padding,
    });
    let form = Form {
        written,
        margin,
        across: own_across,
        absolute,
        flex,
        colour,
        clip,
    };
    (Read::Valid(Box::new(form)), Vec::new())
}

/// The size of each row or column that is a box, measured from its items (`docs/spec/render-command.md`
/// section 4.3, rule 2). An item that is itself a row or column is measured first: an element comes after
/// its parent, so the elements are worked through from the last to the first. `None` for an element that
/// is not a box or is refused, and for a row or column whose size does not work out.
fn measures(tree: &[Node], inside: &[Vec<usize>]) -> Vec<Option<layout::Size>> {
    let forms: Vec<Option<Form>> = tree
        .iter()
        .map(|node| match node.element() {
            Element::Box => match read_box(node).0 {
                Read::Valid(form) => Some(*form),
                Read::Refused => None,
            },
            _ => None,
        })
        .collect();
    let mut sizes: Vec<Option<layout::Size>> = vec![None; tree.len()];
    for index in (0..tree.len()).rev() {
        let Some(flex) = forms[index].as_ref().and_then(|form| form.flex) else {
            continue;
        };
        let items: Vec<Item> = inside[index]
            .iter()
            .filter_map(|&inner| forms[inner].as_ref().map(|form| item_in(form, sizes[inner], &flex)))
            .collect();
        sizes[index] = layout::measure(&flex, &items).ok();
    }
    sizes
}

/// What a box writes as an item of its row or column's flow. Where it writes no size on an axis, and the
/// row or column measured it (`measured`), the size its own items give it (section 4.3, rule 2) is written
/// for it. An absolute box takes the measured size only where its edges do not give one. A flowed box that
/// stretches across writes no size across: its row or column gives it one. Its size across, as its own items
/// give it, is kept for its row or column's size across (rule 1), whether it stretches or not.
fn item_in(form: &Form, measured: Option<layout::Size>, parent: &Flex) -> Item {
    let mut written = form.written;
    let mut measured_across = None;
    if let Some(size) = measured {
        if form.absolute {
            fill(&mut written.width, size.width, both(written.left, written.right));
            fill(&mut written.height, size.height, both(written.top, written.bottom));
        } else {
            let stretched = form.across.unwrap_or(parent.across) == Across::Stretch;
            match parent.direction {
                Direction::Row => {
                    fill(&mut written.width, size.width, false);
                    if !stretched {
                        fill(&mut written.height, size.height, false);
                    }
                    measured_across = Some(size.height);
                }
                Direction::Column => {
                    fill(&mut written.height, size.height, false);
                    if !stretched {
                        fill(&mut written.width, size.width, false);
                    }
                    measured_across = Some(size.width);
                }
            }
        }
    }
    Item {
        written,
        margin: form.margin,
        across: form.across,
        absolute: form.absolute,
        measured_across,
    }
}

/// Gives an unwritten size the measured value, unless the edges of the box give it one.
fn fill(size: &mut Option<Amount>, measured: f64, edged: bool) {
    if size.is_none() && !edged {
        *size = Some(Amount::Pixels(measured));
    }
}

/// Whether both of two written edges are there.
fn both(near: Option<Amount>, far: Option<Amount>) -> bool {
    near.is_some() && far.is_some()
}

/// Which axes of a box that its row or column's flow places have no size of their own, so that the flow
/// measures them from the box's items (section 4.3, rule 2). An axis the box stretches across is the row or
/// column's size, not its own, and an absolute box's axis with both edges is given by them.
fn measured_axes(form: &Form, parent: &Flex) -> Measured {
    let stretched = !form.absolute && form.across.unwrap_or(parent.across) == Across::Stretch;
    let across_edged = form.absolute && both(form.written.left, form.written.right);
    let down_edged = form.absolute && both(form.written.top, form.written.bottom);
    let column = parent.direction == Direction::Column;
    Measured {
        width: form.written.width.is_none() && !across_edged && !(stretched && column),
        height: form.written.height.is_none() && !down_edged && !(stretched && !column),
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
