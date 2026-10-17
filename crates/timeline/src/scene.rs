//! The scene: the root's canvas, the boxes in it and the compositions shown from other files, as far
//! as this version draws them (`docs/spec/render-command.md` section 3), and the frames drawn from it.

use draw::{Colour, DisplayList, Font, Frame, Glyph, GlyphRun, Mark, Picture, Renderer, Step, TextMark};
use layout::{Across, Along, Amount, Beyond, Direction, Flex, Item, Measured, Rect, Sides, Written, LIMIT};
use project::{Attribute, Code, Codec, Element, Encoder, Length, Node, Problem, Project, Quality, Report, Workers};
use timebase::Time;

use crate::paragraph::{self, read_text_attributes, Fault, Paragraph, TEXT_ATTRIBUTES, TEXT_TAKES};

/// The lowest and the highest bit rate of the sound, in kbit/s, that the system's AAC encoder takes
/// at 48 kHz in stereo (`frx7/docs/spec/os-codecs.md` section 3.1, measured).
const SOUND_KBITS_LOWEST: u32 = 64;
const SOUND_KBITS_HIGHEST: u32 = 320;

/// What the root takes: its seven attributes, and `class`, the names of the styles it takes (`docs/spec/notation.md` section 9).
const ROOT_TAKES: [&str; 8] = ["id", "class", "version", "width", "height", "fps", "duration", "background"];
/// What a `<box>` takes besides the text attributes: its place, its margin and its position, what it draws and its
/// window, and the words and numbers of the row or column it lays out (`docs/spec/render-command.md` section 4.3).
/// The text attributes pass to the elements inside a box (`docs/spec/text.md` section 8).
const BOX_TAKES: [&str; 19] = [
    "id",
    "class",
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

/// What a `<composition src>` takes: its id and class, its file, its clip and its place (`docs/spec/render-command.md`
/// section 3). Its size is its file's, so `width` and `height` are not taken.
const NESTED_TAKES: [&str; 11] = [
    "id", "class", "src", "start", "duration", "left", "top", "right", "bottom", "position", "margin",
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

/// A box that is placed and drawn, or placed and drawn as nothing: its slot, the colour it fills with, and the
/// rectangle it is cut to when it stands on a canvas shown from another file (`None` on the root's canvas).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Placed {
    slot: Slot,
    colour: Option<Colour>,
    clip: Option<Rect>,
}

/// A `<text>` that is placed: its slot, the rectangle it is cut to (as a box is), its lines, the composition file
/// it is written in and its line there.
#[derive(Clone, Debug, PartialEq)]
struct PlacedText {
    slot: Slot,
    clip: Option<Rect>,
    paragraph: Paragraph,
    file: String,
    line: u32,
}

/// A box or a paragraph that is placed, in document order: the order its marks are drawn in.
#[derive(Clone, Debug, PartialEq)]
enum Entry {
    Box(Placed),
    Text(PlacedText),
}

impl Entry {
    /// The frames the entry is shown in.
    fn window(&self) -> Window {
        match self {
            Entry::Box(placed) => placed.slot.window,
            Entry::Text(text) => text.slot.window,
        }
    }
}

/// A composition ready to draw.
#[derive(Clone, Debug, PartialEq)]
pub struct Scene {
    width: u32,
    height: u32,
    fps: u32,
    frames: u64,
    background: Colour,
    entries: Vec<Entry>,
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
        let (entries, found) = walk(project, false);
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
        let boxes = entries.iter().filter(|entry| matches!(entry, Entry::Box(_))).count();
        tracing::debug!(target: "render::scene", boxes, frames = root.frames, "scene made");
        Ok(Scene {
            width: root.width,
            height: root.height,
            fps: root.fps,
            frames: root.frames,
            background,
            entries,
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
        self.entries.iter().filter(|entry| matches!(entry, Entry::Box(_))).count()
    }

    /// The composition file and the line of the first `<text>` of the scene, in document order; `None` for a scene with
    /// no text. A renderer that draws no text refuses the scene at that line (`docs/spec/text.md`, section 9).
    pub fn first_text(&self) -> Option<(&str, u32)> {
        self.entries.iter().find_map(|entry| match entry {
            Entry::Text(text) => Some((text.file.as_str(), text.line)),
            Entry::Box(_) => None,
        })
    }

    /// The steps that draw frame `number`, when the frame holds no text: the same list for the same number, whatever
    /// was asked for before. A frame with text is drawn with [`Scene::frame`].
    pub fn list(&self, number: u64) -> Result<DisplayList, PictureError> {
        match self.frame(number)?.display_list() {
            Some(list) => Ok(list),
            None => Err(PictureError::Renderer(draw::Error::new(
                "the frame holds text, which a list of steps does not draw",
            ))),
        }
    }

    /// The marks that draw frame `number`, in document order: the same marks for the same number, whatever was asked
    /// for before. A box is drawn in the frames of its window and in no other, so it appears and disappears only with
    /// its window (`docs/spec/render-command.md`, section 4.2).
    pub fn frame(&self, number: u64) -> Result<Frame, PictureError> {
        if number >= self.frames {
            return Err(PictureError::NoSuchFrame {
                number,
                frames: self.frames,
            });
        }
        let marks = self
            .entries
            .iter()
            .filter(|entry| entry.window().contains(number))
            .filter_map(|entry| match entry {
                Entry::Box(placed) => fill_mark(placed),
                Entry::Text(text) => text_mark(text),
            })
            .collect();
        Ok(Frame {
            width: self.width,
            height: self.height,
            background: self.background,
            marks,
        })
    }

    /// Draws frame `number` with `renderer`.
    pub fn picture(&self, number: u64, renderer: &mut dyn Renderer) -> Result<Picture, PictureError> {
        renderer.draw_frame(&self.frame(number)?).map_err(PictureError::Renderer)
    }
}

/// The fill of a box in its frame: the part of its rectangle that its canvas holds, when it has a colour and
/// the part is not empty (`docs/spec/timeline.md` section 4).
fn fill_mark(placed: &Placed) -> Option<Mark> {
    // A box on a canvas shown from another file is drawn in the part of its rectangle that the canvas holds, and not
    // at all where that part is empty (`docs/spec/timeline.md` section 4).
    let rect = match placed.clip {
        None => placed.slot.rect,
        Some(clip) => {
            let held = intersect(placed.slot.rect, clip);
            if held.width == 0 || held.height == 0 {
                return None;
            }
            held
        }
    };
    placed.colour.map(|colour| Mark::Step(Step::Fill { rect, colour }))
}

/// The glyph runs of a paragraph, moved to its place in the frame, and the mark that draws them.
fn text_mark(text: &PlacedText) -> Option<Mark> {
    let paragraph = &text.paragraph;
    if paragraph.block.runs.is_empty() {
        return None;
    }
    let (left, top) = (paragraph.rect.left as f32, paragraph.rect.top as f32);
    let runs = paragraph
        .block
        .runs
        .iter()
        .map(|run| GlyphRun {
            font: Font {
                data: run.font.clone(),
                index: run.index,
            },
            size: run.size,
            skew_degrees: run.skew_degrees,
            colour: paragraph.colour,
            glyphs: run
                .glyphs
                .iter()
                .map(|glyph| Glyph {
                    id: glyph.id,
                    x: glyph.x + left,
                    y: glyph.y + top,
                })
                .collect(),
        })
        .collect();
    Some(Mark::Text(TextMark { runs, clip: text.clip }))
}

/// Whether an element takes an attribute of this name: a `<text>` takes what `TEXT_TAKES` lists, and a `<box>` what
/// `BOX_TAKES` lists and the text attributes, which pass to the elements inside it (`docs/spec/text.md`, section 8).
fn takes(node: &Node, name: &str) -> bool {
    match node.element() {
        Element::Text => TEXT_TAKES.contains(&name),
        _ => BOX_TAKES.contains(&name) || TEXT_ATTRIBUTES.contains(&name),
    }
}

/// The reports of the forms of a place, a size, a margin, a position and the words and numbers of a
/// row or column that this version does not list, in the order of the file, and of an attribute that a
/// `<style>` gives and this version does not draw. They are the reports `Scene::from_project` makes of
/// those forms, by the same code, without the settings of the project file and without the other
/// refusals of what is drawn: `check` judges the forms of a place and the attributes that a style gives,
/// and does not judge the rest (`docs/spec/check-command.md`, sections 3b and 3c).
pub fn place_reports(project: &Project) -> Vec<Report> {
    walk(project, true).1
}

/// Walks the project's composition: the root's canvas, which is the frame, and then each element in document
/// order. Gives the boxes that are placed, in document order, and the reports of the refusals. With `place_only`,
/// the reports are those of a place form alone.
fn walk(project: &Project, place_only: bool) -> (Vec<Entry>, Vec<Report>) {
    let root = project.root();
    walk_canvas(
        project,
        Canvas {
            file: project.composition(),
            tree: project.tree().nodes(),
            slot: Slot {
                rect: layout::frame(root.width, root.height),
                origin: Time::ZERO,
                window: Window::all(root.frames),
            },
            clip: None,
            translucent: root.background.alpha != 255,
        },
        root.fps,
        place_only,
    )
}

/// A canvas to draw: the file its elements are read from, the elements of that file, the slot of its root (the
/// frame, or the place and window of the element that shows it), the rectangle its boxes are cut to (none on the
/// root's canvas), and whether the root's background is not opaque, which is refused. The background of a file
/// shown from another file is not drawn, so it is not judged there (`docs/spec/notation.md` section 10).
#[derive(Clone, Copy)]
struct Canvas<'a> {
    file: &'a str,
    tree: &'a [Node],
    slot: Slot,
    clip: Option<Rect>,
    translucent: bool,
}

/// Walks one canvas: its root, then each element after its parent (`docs/spec/timeline.md` section 4). A box or a
/// composition shown from another file is placed unless it, or an element it stands in, is refused. A row or
/// column reads the boxes and the compositions inside it ahead, since its flow places them
/// (`docs/spec/render-command.md` section 4.3). A composition shown from another file is walked on a canvas of
/// its own, at its place, in its window and cut to its rectangle. Gives the boxes that are placed, in document
/// order, and the reports: those of this file in the order of their lines, then those of the files it shows, each
/// report once, in the order the files are reached.
fn walk_canvas(project: &Project, canvas: Canvas<'_>, fps: u32, place_only: bool) -> (Vec<Entry>, Vec<Report>) {
    let tree = canvas.tree;
    // The elements inside each element, in document order.
    let mut inside: Vec<Vec<usize>> = vec![Vec::new(); tree.len()];
    for (index, node) in tree.iter().enumerate() {
        if let Some(parent) = node.parent() {
            inside[parent].push(index);
        }
    }

    let mut refusals: Vec<Refusal> = Vec::new();
    // The fonts this file declares, and the faults of the font files that are not read.
    let (mut fonts, font_faults) = paragraph::load_fonts(project, tree);
    refusals.extend(font_faults.into_iter().map(Refusal::of_text));
    // The reports of the files shown from this one, each once.
    let mut shown: Vec<Report> = Vec::new();
    let mut entries: Vec<Entry> = Vec::new();
    // Which elements are rows or columns: a text inside one is not placed in this version.
    let mut flows: Vec<bool> = vec![false; tree.len()];
    // For each element, in document order: the slot its box is placed in, and `None` for an element that is
    // reported and for one inside it, which is not looked into. An element comes after its parent in the tree,
    // so the parent's entry is already known.
    let mut places: Vec<Option<Slot>> = Vec::with_capacity(tree.len());
    // A box or a composition that its row or column has read ahead, in the form it was read in.
    let mut reads: Vec<Option<Read>> = vec![None; tree.len()];
    // The rectangle and the measured axes that the flow of a row or column gives each of its items.
    let mut assigned: Vec<Option<(Rect, Measured)>> = vec![None; tree.len()];
    // The size of each row or column measured from its items, from the inside out.
    let sizes = measures(project, tree, &inside);

    for (index, node) in tree.iter().enumerate() {
        let Some(parent) = node.parent() else {
            // The root of the canvas.
            for attribute in node.attributes() {
                if !ROOT_TAKES.contains(&attribute.name()) {
                    refusals.push(Refusal::of_attribute(
                        Code::FormNotSupported,
                        attribute,
                        unsupported_attribute(node, attribute),
                    ));
                }
            }
            if let (Some(attribute), true) = (node.attribute("background"), canvas.translucent) {
                refusals.push(Refusal::of_attribute(
                    Code::FormNotSupported,
                    attribute,
                    unsupported_colour(node, attribute),
                ));
            }
            places.push(Some(canvas.slot));
            continue;
        };
        let Some(parent_place) = places[parent] else {
            // Inside an element that is reported: nothing more is said of it.
            places.push(None);
            continue;
        };
        // A style draws nothing: it is read, and it stands in the file without a picture.
        if node.element() == Element::Style {
            places.push(None);
            continue;
        }
        if tree[parent].element() == Element::Nested {
            // A composition shown from another file takes no element of its own: its file is drawn in its place.
            refusals.push(Refusal::of_drawing(
                Code::FormNotSupported,
                node.line(),
                unsupported_inside_nested(node.element()),
            ));
            places.push(None);
            continue;
        }
        if node.element() == Element::Font {
            // A font stands in the root, where its file is read (`load_fonts`); inside an element it is refused.
            if parent != 0 {
                refusals.push(Refusal::of_drawing(Code::FormNotSupported, node.line(), unsupported_font_place()));
            }
            places.push(None);
            continue;
        }
        if node.element() == Element::Text {
            if tree[parent].element() == Element::Text {
                refusals.push(Refusal::of_drawing(
                    Code::FormNotSupported,
                    node.line(),
                    unsupported_inside_text(node.element()),
                ));
                places.push(None);
                continue;
            }
            if flows[parent] {
                refusals.push(Refusal::of_place(Code::FormNotSupported, node.line(), unsupported_inside_flow()));
                places.push(None);
                continue;
            }
        }
        if !matches!(node.element(), Element::Box | Element::Nested | Element::Text) {
            let message = if parent == 0 {
                unsupported_element(node.element())
            } else if tree[parent].element() == Element::Text {
                unsupported_inside_text(node.element())
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
                let (read, found) = read_element(project, node);
                refusals.extend(found);
                read
            }
        };
        let Read::Valid(form) = read else {
            places.push(None);
            continue;
        };
        if node.element() == Element::Text {
            match paragraph::place_text(&mut fonts, tree, index, form.written, parent_place.rect) {
                Ok(paragraph) => {
                    let slot = slot_of(form.clip, parent_place, paragraph.rect, fps);
                    entries.push(Entry::Text(PlacedText {
                        slot,
                        clip: canvas.clip,
                        paragraph,
                        file: canvas.file.to_string(),
                        line: node.line(),
                    }));
                    places.push(Some(slot));
                }
                Err(fault) => {
                    refusals.push(Refusal::of_text(fault));
                    places.push(None);
                }
            }
            continue;
        }
        flows[index] = form.flex.is_some();

        // The boxes and compositions inside a row or column are read now: they are its items, and their places
        // come from its flow. Their refusals are kept until this element is placed.
        let mut pending: Vec<Refusal> = Vec::new();
        let mut items: Vec<Item> = Vec::new();
        let mut item_nodes: Vec<(usize, Measured)> = Vec::new();
        if let Some(flex) = form.flex {
            for &inner in &inside[index] {
                let inner_node = &tree[inner];
                if !matches!(inner_node.element(), Element::Box | Element::Nested) {
                    continue;
                }
                let (read, found) = read_element(project, inner_node);
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

        let slot = slot_of(form.clip, parent_place, rect, fps);

        let Some(name) = &form.shown else {
            entries.push(Entry::Box(Placed {
                slot,
                colour: form.colour,
                clip: canvas.clip,
            }));
            places.push(Some(slot));
            continue;
        };
        let Some(file) = project.shown_file(name) else {
            // The project's open reports a file that is not there or cannot be read, so the run does not get here.
            places.push(None);
            continue;
        };
        // The file is cut to the clip of the element and to its own length (`docs/spec/notation.md` section 10).
        let file_window = Window::of_clip(slot.origin, slot.origin + file.root().duration, fps);
        let clip = match canvas.clip {
            Some(outer) => intersect(rect, outer),
            None => rect,
        };
        let (placed, found) = walk_canvas(
            project,
            Canvas {
                file: file.name(),
                tree: file.tree().nodes(),
                slot: Slot {
                    rect,
                    origin: slot.origin,
                    window: slot.window.within(file_window),
                },
                clip: Some(clip),
                translucent: false,
            },
            fps,
            place_only,
        );
        entries.extend(placed);
        for report in found {
            if !shown.contains(&report) {
                shown.push(report);
            }
        }
        places.push(Some(slot));
    }

    // A row or column reads its items ahead, so the reports are put in the order of the file by their lines; a
    // stable sort keeps the order of the reports of one line.
    refusals.sort_by_key(|refusal| refusal.line);
    let mut reports: Vec<Report> = refusals
        .into_iter()
        .filter(|refusal| refusal.place || refusal.style || !place_only)
        .map(|refusal| Report::new(canvas.file, Problem::new(refusal.code, refusal.line, refusal.message)))
        .collect();
    reports.extend(shown);
    (entries, reports)
}

/// The slot of an element placed in `rect` inside `parent`: the origin and the window that its clip gives, or the
/// parent's when it has no clip (`docs/spec/render-command.md`, section 4.2).
fn slot_of(clip: Option<(Time, Time)>, parent: Slot, rect: Rect, fps: u32) -> Slot {
    match clip {
        Some((start, duration)) => {
            let begin = parent.origin + start;
            Slot {
                rect,
                origin: begin,
                window: Window::of_clip(begin, begin + duration, fps).within(parent.window),
            }
        }
        None => Slot {
            rect,
            origin: parent.origin,
            window: parent.window,
        },
    }
}

/// The rectangle that both `a` and `b` cover. Where they do not overlap, it has no area.
fn intersect(a: Rect, b: Rect) -> Rect {
    let left = a.left.max(b.left);
    let top = a.top.max(b.top);
    let right = (i64::from(a.left) + i64::from(a.width)).min(i64::from(b.left) + i64::from(b.width));
    let bottom = (i64::from(a.top) + i64::from(a.height)).min(i64::from(b.top) + i64::from(b.height));
    Rect {
        left,
        top,
        width: (right - i64::from(left)).max(0) as u32,
        height: (bottom - i64::from(top)).max(0) as u32,
    }
}

/// Reads a box or a composition shown from another file: its form, or its refusals.
fn read_element(project: &Project, node: &Node) -> (Read, Vec<Refusal>) {
    match node.element() {
        Element::Nested => read_nested(project, node),
        _ => read_box(node),
    }
}

/// Reads a `<composition src>` (`docs/spec/render-command.md` section 3): the places a box has, and its clip, which
/// is written both or neither. Its size is the size of its file, and an unwritten clip is the whole file: a `start`
/// of 0 and a `duration` of the file's length (`docs/spec/notation.md` section 10).
fn read_nested(project: &Project, node: &Node) -> (Read, Vec<Refusal>) {
    let mut refusals: Vec<Refusal> = Vec::new();
    let mut written = Written::default();
    let mut absolute = false;
    let mut margin = Sides::default();
    let mut start: Option<Time> = None;
    let mut duration: Option<Time> = None;
    let mut src: Option<&str> = None;
    for attribute in node.attributes() {
        let name = attribute.name();
        if !NESTED_TAKES.contains(&name) {
            refusals.push(Refusal::of_attribute(
                Code::FormNotSupported,
                attribute,
                unsupported_attribute(node, attribute),
            ));
            continue;
        }
        match name {
            "id" | "class" => {}
            "src" => src = Some(attribute.value()),
            "start" | "duration" => match attribute.time() {
                Some(time) if name == "start" => start = Some(time),
                Some(time) => duration = Some(time),
                None => refusals.push(Refusal::of_attribute(
                    Code::NotATime,
                    attribute,
                    format!(
                        "`{name}=\"{}\"` is not a time in seconds: digits with an optional point, such as 5 or 1.25",
                        attribute.shown()
                    ),
                )),
            },
            _ => read_place(node, attribute, &mut written, &mut absolute, &mut margin, &mut refusals),
        }
    }
    if !refusals.is_empty() {
        return (Read::Refused, refusals);
    }
    let Some(name) = src else {
        return (Read::Refused, Vec::new());
    };
    // The project's open reads the file and reports one that is not there or cannot be read (`FS-0047`, `FS-0048`),
    // so a file is found here.
    let Some(shown) = project.shown_file(name) else {
        return (Read::Refused, Vec::new());
    };
    let file = shown.root();
    written.width = Some(Amount::Pixels(f64::from(file.width)));
    written.height = Some(Amount::Pixels(f64::from(file.height)));
    let form = Form {
        written,
        margin,
        across: None,
        absolute,
        flex: None,
        colour: None,
        clip: Some((start.unwrap_or(Time::ZERO), duration.unwrap_or(file.duration))),
        shown: Some(name.to_string()),
    };
    (Read::Valid(Box::new(form)), Vec::new())
}

/// Reads one attribute that a `<box>` and a `<composition src>` both take as a place: `left`, `top`, `right`,
/// `bottom`, `position` and `margin`. A refusal is kept on the attribute's line; any other attribute is left alone.
fn read_place(
    node: &Node,
    attribute: &Attribute,
    written: &mut Written,
    absolute: &mut bool,
    margin: &mut Sides,
    refusals: &mut Vec<Refusal>,
) {
    let name = attribute.name();
    let value = attribute.shown();
    match name {
        "left" | "top" | "right" | "bottom" => match amount(attribute.length(), &value, name, node.element().tag()) {
            Ok(found) => match name {
                "left" => written.left = Some(found),
                "top" => written.top = Some(found),
                "right" => written.right = Some(found),
                _ => written.bottom = Some(found),
            },
            Err((code, message)) => refusals.push(Refusal::of_place(code, attribute.line(), message)),
        },
        "position" => match attribute.value() {
            "static" => {}
            "absolute" => *absolute = true,
            _ => refusals.push(Refusal::of_place(
                Code::WordNotTaken,
                attribute.line(),
                format!("`position=\"{value}\"` is not one of: static, absolute"),
            )),
        },
        "margin" => match attribute.sides().and_then(|values| Sides::from_values(&values)) {
            // The sides are held: a margin moves an item of a row or column, and no other box (N10).
            Some(sides) => *margin = sides,
            None => refusals.push(Refusal::of_place(
                Code::NotANumber,
                attribute.line(),
                format!("`margin=\"{value}\"` is not one to four numbers, such as 4 or 4 8"),
            )),
        },
        _ => {}
    }
}

/// A refusal of the root, of a box or of a text: its code, its line, its message, whether it is of a form that
/// `check` judges (`place`: a length, a size, a margin, a position, a word of a row or column, or a text or a font
/// form, `docs/spec/text.md` section 9), and whether a `<style>` gives the attribute it is about (`style`), which
/// `check` judges too (`docs/spec/check-command.md`, section 3c).
struct Refusal {
    code: Code,
    line: u32,
    message: String,
    place: bool,
    style: bool,
}

impl Refusal {
    /// A refusal of a form of a place, a size, a margin, a position or a row or column.
    fn of_place(code: Code, line: u32, message: String) -> Refusal {
        Refusal {
            code,
            line,
            message,
            place: true,
            style: false,
        }
    }

    /// A refusal of a text or a font. It is a form `check` judges as well as a place, so that `check` names what `render`
    /// refuses (`docs/spec/text.md`, section 9).
    fn of_text(fault: Fault) -> Refusal {
        Refusal {
            code: fault.code,
            line: fault.line,
            message: fault.message,
            place: true,
            style: fault.style,
        }
    }

    /// A refusal of anything else that this version does not draw or apply.
    fn of_drawing(code: Code, line: u32, message: String) -> Refusal {
        Refusal {
            code,
            line,
            message,
            place: false,
            style: false,
        }
    }

    /// A refusal of an attribute of the root or of a box, which is on the attribute's line. It is one
    /// `check` judges as well when a `<style>` gives the attribute.
    fn of_attribute(code: Code, attribute: &Attribute, message: String) -> Refusal {
        Refusal {
            code,
            line: attribute.line(),
            message,
            place: false,
            style: attribute.given_by().is_some(),
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
    /// The file a `<composition src>` shows, as written; `None` for a box.
    shown: Option<String>,
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
    let mut text_faults: Vec<Fault> = Vec::new();
    read_text_attributes(node, &mut text_faults);
    refusals.extend(text_faults.into_iter().map(Refusal::of_text));
    for attribute in node.attributes() {
        let name = attribute.name();
        if !takes(node, name) {
            refusals.push(Refusal::of_attribute(
                Code::FormNotSupported,
                attribute,
                unsupported_attribute(node, attribute),
            ));
            continue;
        }
        let value = attribute.shown();
        match name {
            "width" | "height" => match amount(attribute.length(), &value, name, node.element().tag()) {
                Ok(found) => match name {
                    "width" => written.width = Some(found),
                    _ => written.height = Some(found),
                },
                Err((code, message)) => refusals.push(Refusal::of_place(code, attribute.line(), message)),
            },
            "left" | "top" | "right" | "bottom" | "position" | "margin" => {
                read_place(node, attribute, &mut written, &mut absolute, &mut margin, &mut refusals)
            }
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
                _ => refusals.push(Refusal::of_attribute(
                    Code::FormNotSupported,
                    attribute,
                    unsupported_colour(node, attribute),
                )),
            },
            "start" | "duration" => match attribute.time() {
                Some(time) if name == "start" => start = Some(time),
                Some(time) => duration = Some(time),
                None => refusals.push(Refusal::of_attribute(
                    Code::NotATime,
                    attribute,
                    format!("`{name}=\"{value}\"` is not a time in seconds: digits with an optional point, such as 5 or 1.25"),
                )),
            },
            "white-space" if attribute.value() != "normal" => refusals.push(Refusal::of_place(
                Code::FormNotSupported,
                attribute.line(),
                unsupported_value(node, name, &value, "normal"),
            )),
            "text-transform" if attribute.value() != "none" => refusals.push(Refusal::of_place(
                Code::FormNotSupported,
                attribute.line(),
                unsupported_value(node, name, &value, "none"),
            )),
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
        shown: None,
    };
    (Read::Valid(Box::new(form)), Vec::new())
}

/// The size of each row or column that is a box, measured from its items (`docs/spec/render-command.md`
/// section 4.3, rule 2). An item that is itself a row or column is measured first: an element comes after
/// its parent, so the elements are worked through from the last to the first. `None` for an element that
/// is not a box or is refused, and for a row or column whose size does not work out.
fn measures(project: &Project, tree: &[Node], inside: &[Vec<usize>]) -> Vec<Option<layout::Size>> {
    let forms: Vec<Option<Form>> = tree
        .iter()
        .map(|node| match node.element() {
            Element::Box | Element::Nested => match read_element(project, node).0 {
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
    format!(
        "{} is not supported by this version, which draws <box>, <text> and <composition src> only",
        element.tag()
    )
}

/// The message for an element written inside a composition shown from another file, which takes none.
fn unsupported_inside_nested(element: Element) -> String {
    format!(
        "{} inside <composition src> is not supported by this version, which draws a <composition src> with no element inside it",
        element.tag()
    )
}

/// The message for an element written inside a `<text>`: a `<span>` is not drawn in this version (`text.md`, section 1).
fn unsupported_inside_text(element: Element) -> String {
    format!(
        "{} inside <text> is not supported by this version, which draws the words of a <text> and no element inside it",
        element.tag()
    )
}

/// The message for a `<text>` in a row or a column: the flow of such a row or column places its boxes only.
fn unsupported_inside_flow() -> String {
    "<text> inside a row or column is not supported by this version, which places a <box> or a <composition src> there only".to_string()
}

/// The message for a `<font>` written inside an element: a font stands directly in the composition.
fn unsupported_font_place() -> String {
    "<font> stands directly in <composition>, not inside an element, and this version reads it there only".to_string()
}

fn unsupported_inside_box(element: Element) -> String {
    format!(
        "{} inside a <box> is not supported by this version, which draws a <box> or a <composition src> inside a <box> and no other element",
        element.tag()
    )
}

/// The message for an attribute this version does not draw; one a style gives names the style, since
/// the line given is the element's (`docs/spec/check-command.md` section 3c).
fn unsupported_attribute(node: &Node, attribute: &Attribute) -> String {
    let message = format!(
        "{} has an attribute this version does not support: `{}`",
        node.element().tag(),
        project::shown(attribute.name())
    );
    match attribute.given_by() {
        None => message,
        Some(style) => format!("{message}, which the style `{}` gives it", project::shown(style)),
    }
}

fn unsupported_value(node: &Node, name: &str, value: &str, takes: &str) -> String {
    format!(
        "`{name}=\"{value}\"` on {} is not a form this version supports: {takes}",
        node.element().tag()
    )
}

/// The message for a `background` that is not one opaque colour; one a style gives names the style,
/// as the message of an attribute this version does not draw does (`docs/spec/check-command.md`
/// section 3c).
fn unsupported_colour(node: &Node, attribute: &Attribute) -> String {
    let message = unsupported_value(node, attribute.name(), &attribute.shown(), "one opaque colour, #rrggbb");
    match attribute.given_by() {
        None => message,
        Some(style) => format!("{message}, which the style `{}` gives it", project::shown(style)),
    }
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
