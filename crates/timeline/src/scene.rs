//! The scene: the root's canvas, the boxes in it and the compositions shown from other files, as far
//! as this version draws them (`docs/spec/render-command.md` section 3), and the frames drawn from it.

use std::path::PathBuf;
use std::sync::Arc;

use draw::{
    Background, BoxShadow, Colour, Cut, DisplayList, Edges, Filter, Font, Frame, Glyph, GlyphPicture, GlyphRun, Group, Layer, Map, Mark,
    Picture, PlacedPicture, Renderer, Rounded, Shadow, Step, TextMark,
};
use layout::{Across, Along, Amount, Beyond, Cell, Columns, Direction, Flex, Item, Measured, Rect, Sides, Size, Track, Written, LIMIT};
use project::{Attribute, Code, Element, Encoder, Length, Node, Number, Problem, Project, Quality, Report, Unsupported, Workers};
use text::Fonts;
use timebase::Time;

use crate::background;
use crate::edges::read_edges;
use crate::moving::{self, Found, Movement, Mover, MOVEMENT};
use crate::paragraph::{self, read_text_attributes, Clock, Fault, Paragraph, TEXT_ATTRIBUTES, TEXT_TAKES};
use crate::sound::{self, Sound};
use crate::support;
use motion::{ElementMotion, Tween};

/// The lowest and the highest bit rate of the sound, in kbit/s, that the system's AAC encoder takes
/// at 48 kHz in stereo (`frx7/docs/spec/os-codecs.md` section 3.1, measured).
const SOUND_KBITS_LOWEST: u32 = 64;
const SOUND_KBITS_HIGHEST: u32 = 320;

/// What the root takes: its seven attributes, and `class`, the names of the styles it takes (`docs/spec/notation.md` section 9).
const ROOT_TAKES: [&str; 8] = ["id", "class", "version", "width", "height", "fps", "duration", "background"];
/// What a `<box>` takes besides the text attributes: its place, its margin and its position, what it draws and its
/// window, the words and numbers of the row or column it lays out (`docs/spec/render-command.md` section 4.3), its
/// `columns` for a set of columns (section 4.4), its `max-width` and `flex-grow` as an item (sections 4.1 and 4.3), and its
/// border, corner radius and cut (`docs/spec/box-edges.md`).
/// The text attributes pass to the elements inside a box (`docs/spec/text.md` section 8).
const BOX_TAKES: [&str; 29] = [
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
    "background-size",
    "background-position",
    "background-repeat",
    "start",
    "duration",
    "flex-direction",
    "justify-content",
    "align-items",
    "align-self",
    "gap",
    "padding",
    "columns",
    "max-width",
    "flex-grow",
    "border",
    "border-radius",
    "overflow",
    "box-shadow",
];

/// The edges of an element that writes none: square corners, no border, no cut.
const NO_EDGES: Edges = Edges {
    radius: 0.0,
    border: None,
    cut: false,
};
/// What a `<video>` takes in this version (`docs/spec/render-command.md`, section 4.8): its id and class, its file, its size,
/// its place, its clip, its corners and its look.
const VIDEO_TAKES: [&str; 17] = [
    "id",
    "class",
    "src",
    "width",
    "height",
    "left",
    "top",
    "right",
    "bottom",
    "position",
    "margin",
    "start",
    "duration",
    "border-radius",
    "opacity",
    "blur",
    "brightness",
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

/// What a box fills its rectangle with: one opaque colour, drawn as a fill, or layers of colours and gradients
/// (`docs/spec/render-command.md`, section 4.6), shared by every frame it is drawn in.
#[derive(Clone, Debug, PartialEq)]
enum Fill {
    Colour(Colour),
    Layers(Arc<Vec<Layer>>),
}

impl Fill {
    /// The mark of the fill of `rect`, cut to `clip` when there is one; `None` when the part held is empty.
    fn mark(&self, rect: Rect, clip: Option<Rect>) -> Option<Mark> {
        let held = match clip {
            None => rect,
            Some(clip) => intersect(rect, clip),
        };
        if clip.is_some() && (held.width == 0 || held.height == 0) {
            return None;
        }
        Some(match self {
            Fill::Colour(colour) => Mark::Step(Step::Fill {
                rect: held,
                colour: *colour,
            }),
            // A gradient is measured on the whole box and cut to the part held (rule 9).
            Fill::Layers(layers) => {
                let background = Mark::Background(Background {
                    rect,
                    layers: layers.as_ref().clone(),
                });
                match clip {
                    None => background,
                    Some(_) => Mark::Group(Group {
                        map: Map::IDENTITY,
                        opacity: 1.0,
                        clip: Some(held),
                        marks: vec![background],
                        turn: None,
                        filter: Filter::NONE,
                        shadow: None,
                    }),
                }
            }
        })
    }
}

/// A box that is placed and drawn, or placed and drawn as nothing: its slot, what it fills with, the rectangle it is
/// cut to when it stands on a canvas shown from another file (`None` on the root's canvas), its border, corners and cut,
/// and, for a box that cuts its children, the end of the run of entries of its children, which follow it.
#[derive(Clone, Debug, PartialEq)]
struct Placed {
    slot: Slot,
    fill: Option<Fill>,
    clip: Option<Rect>,
    edges: Edges,
    /// The box's shadows, the first listed on top (`docs/spec/render-command.md`, section 4.7).
    shadows: Vec<Shadow>,
    cut_end: Option<usize>,
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

/// A `<video>` that is placed (`docs/spec/render-command.md`, section 4.8): its slot, whose rectangle is the size its pictures
/// are brought to, the rectangle it is cut to (as a box is), and the number of its source in the scene's list.
#[derive(Clone, Debug, PartialEq)]
struct PlacedFootage {
    slot: Slot,
    clip: Option<Rect>,
    source: usize,
    /// The `opacity` the `<video>` writes, 1 when it writes none (`docs/spec/render-command.md`, section 4.8).
    opacity: f64,
    /// The `blur` and `brightness` the `<video>` writes, none when it writes neither.
    filter: Filter,
    /// The `border-radius` the `<video>` writes, 0 when it writes none: the footage is cut to the rounded box.
    radius: f64,
}

/// A box, a paragraph or footage that is placed, in document order: the order its marks are drawn in.
#[derive(Clone, Debug, PartialEq)]
enum Entry {
    Box(Placed),
    Text(Box<PlacedText>),
    Footage(PlacedFootage),
}

impl Entry {
    /// The frames the entry is shown in.
    fn window(&self) -> Window {
        match self {
            Entry::Box(placed) => placed.slot.window,
            Entry::Text(text) => text.slot.window,
            Entry::Footage(footage) => footage.slot.window,
        }
    }
}

/// A file of footage that the scene shows (`docs/spec/render-command.md`, section 4.8): its `src` as written, the path of the
/// machine it names, and the composition file and the line of the first `<video>` that shows it. Two `<video>`s that name one
/// path share one source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FootageSource {
    pub written: String,
    pub path: PathBuf,
    pub file: String,
    pub line: u32,
}

/// What gives the pictures of footage (`docs/spec/render-command.md`, section 4.8): the timeline names the source, the
/// picture and the size, and the render decodes, converts and sizes it through frx7. A source is named by its number in
/// [`Scene::sources`].
pub trait Footage {
    /// How many pictures the source has.
    fn count(&mut self, source: usize) -> Result<u64, String>;
    /// The picture of the source with this number, from 0 in the order they are shown, brought to `width` by `height`: that
    /// many pixels of premultiplied red, green, blue and alpha, row by row with no padding.
    fn picture(&mut self, source: usize, number: u64, size: (u32, u32)) -> Result<Arc<Vec<u8>>, String>;
}

/// The footage of a scene that shows none: it is never asked for a picture.
struct NoFootage;

impl Footage for NoFootage {
    fn count(&mut self, _source: usize) -> Result<u64, String> {
        Err("the frame shows footage, and no source of its pictures was given".to_string())
    }

    fn picture(&mut self, _source: usize, _number: u64, _size: (u32, u32)) -> Result<Arc<Vec<u8>>, String> {
        Err("the frame shows footage, and no source of its pictures was given".to_string())
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
    /// The elements that move, in document order: an outer one before the ones inside it.
    movers: Vec<Mover>,
    /// The sounds of the root, in document order.
    sounds: Vec<Sound>,
    /// The files of footage the scene shows, each once.
    sources: Vec<FootageSource>,
}

/// Why a picture was not made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PictureError {
    /// The frame asked for is not one of the scene: `number` counted from 0, `frames` in all.
    NoSuchFrame { number: u64, frames: u64 },
    /// The renderer could not draw the frame.
    Renderer(draw::Error),
    /// A picture of footage could not be given: the source as written, and the reason.
    Footage { written: String, reason: String },
}

impl std::fmt::Display for PictureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PictureError::NoSuchFrame { number, frames } => {
                write!(f, "there is no frame {number}: the frames are numbered from 0 to {}", frames - 1)
            }
            PictureError::Renderer(error) => write!(f, "{error}"),
            PictureError::Footage { written, reason } => write!(f, "`{written}`: {reason}"),
        }
    }
}

impl std::error::Error for PictureError {}

impl Scene {
    /// Makes the scene of an opened project, or gives every report of what this version does not
    /// draw or apply (`FS-0035`), of the lengths and the words that are wrong (`FS-0032`, `FS-0034`,
    /// `FS-0039`): first those of the project file, then those of the composition, each in the order
    /// of the file.
    pub fn from_project(project: &Project, codecs: &codec::Registry) -> Result<Scene, Vec<Report>> {
        let mut reports = settings(project);
        let (entries, movers, sources, found) = walk(project, false, codecs);
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
            movers,
            sounds: sound::gather(project, project.tree().nodes()).0,
            sources,
        })
    }

    /// The sounds of the film, each placed to the sample, in document order (`docs/spec/sound.md`).
    pub fn sounds(&self) -> &[Sound] {
        &self.sounds
    }

    /// The files of footage the scene shows, each once, in the order they are first shown; a [`Footage`] is asked for a
    /// source by its number in this list.
    pub fn sources(&self) -> &[FootageSource] {
        &self.sources
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
            Entry::Box(_) | Entry::Footage(_) => None,
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
        self.frame_with(number, &mut NoFootage)
    }

    /// The marks that draw frame `number`, as [`Scene::frame`] gives them, with the pictures of the footage the frame shows
    /// taken from `footage` (`docs/spec/render-command.md`, section 4.8).
    pub fn frame_with(&self, number: u64, footage: &mut dyn Footage) -> Result<Frame, PictureError> {
        if number >= self.frames {
            return Err(PictureError::NoSuchFrame {
                number,
                frames: self.frames,
            });
        }
        let time = Time::of_frame(number as i64, self.fps);
        let marks = self.marks(0..self.entries.len(), &self.movers, number, time, true, footage)?;
        Ok(Frame {
            width: self.width,
            height: self.height,
            background: self.background,
            marks,
        })
    }

    /// The marks of the entries in `range` at frame `number`, at `time`, with the elements that move among them (`movers`, in
    /// document order) wrapped in groups (`docs/spec/timeline.md` section 5). With `clipped`, a mark is cut to the canvas it
    /// stands on; inside a group that moves it is not, and the group is cut instead.
    fn marks(
        &self,
        range: std::ops::Range<usize>,
        movers: &[Mover],
        number: u64,
        time: Time,
        clipped: bool,
        footage: &mut dyn Footage,
    ) -> Result<Vec<Mark>, PictureError> {
        let mut marks = Vec::new();
        let mut index = range.start;
        let mut next = 0;
        while index < range.end {
            if let Some(mover) = movers.get(next).filter(|mover| mover.first == index) {
                let inner_end = next + 1 + movers[next + 1..].iter().take_while(|inner| inner.first < mover.end).count();
                let inner = &movers[next + 1..inner_end];
                let (map, opacity, turn) = mover.at(time);
                if map.is_identity() && opacity == 1.0 && turn.is_none() && mover.filter.is_none() && mover.shadow.is_none() {
                    marks.extend(self.marks(mover.first..mover.end, inner, number, time, clipped, footage)?);
                } else if opacity > 0.0 {
                    let mut held = self.marks(mover.first..mover.end, inner, number, time, false, footage)?;
                    // A shown composition is cut to its rectangle before it moves: the cut of a group is in the space of its
                    // parent, here the moving element's (`docs/spec/render-command.md`, section 4.5, rule 12).
                    if let (Some(cut), false) = (mover.cut, held.is_empty()) {
                        held = vec![Mark::Group(Group {
                            map: Map::IDENTITY,
                            opacity: 1.0,
                            clip: Some(cut),
                            marks: held,
                            turn: None,
                            filter: Filter::NONE,
                            shadow: None,
                        })];
                    }
                    if !held.is_empty() {
                        marks.push(Mark::Group(Group {
                            map,
                            opacity,
                            clip: mover.clip,
                            marks: held,
                            turn,
                            filter: mover.filter,
                            shadow: mover.shadow,
                        }));
                    }
                }
                index = mover.end;
                next = inner_end;
                continue;
            }
            let entry = &self.entries[index];
            index += 1;
            if let Entry::Box(placed) = entry {
                if let Some(end) = placed.cut_end {
                    // The children of a box that cuts are drawn inside its cut, with the elements that move among them.
                    let inner_end = next + movers[next..].iter().take_while(|inner| inner.first < end).count();
                    if entry.window().contains(number) {
                        let children = self.marks(index..end, &movers[next..inner_end], number, time, clipped, footage)?;
                        marks.extend(box_marks(placed, clipped, children));
                    }
                    index = end;
                    next = inner_end;
                    continue;
                }
            }
            if !entry.window().contains(number) {
                continue;
            }
            let mark = match entry {
                Entry::Box(placed) => {
                    marks.extend(box_marks(placed, clipped, Vec::new()));
                    None
                }
                Entry::Text(text) if clipped => text_mark(text, number, time),
                Entry::Text(text) => text_mark(text, number, time).map(|mark| match mark {
                    Mark::Text(mark) => Mark::Text(TextMark { clip: None, ..mark }),
                    other => other,
                }),
                Entry::Footage(placed) => self.footage_mark(placed, number, clipped, footage)?,
            };
            marks.extend(mark);
        }
        Ok(marks)
    }

    /// The picture of footage on frame `number` (`docs/spec/render-command.md`, section 4.7, rules 2 to 4): the source picture
    /// numbered from the first frame of its window, the last one holding past the source's end, at the size of its box and
    /// with its corner at the box's, cut to its canvas when `clipped`. A box of no size shows nothing.
    fn footage_mark(
        &self,
        placed: &PlacedFootage,
        number: u64,
        clipped: bool,
        footage: &mut dyn Footage,
    ) -> Result<Option<Mark>, PictureError> {
        let rect = placed.slot.rect;
        if rect.width == 0 || rect.height == 0 {
            return Ok(None);
        }
        let source = &self.sources[placed.source];
        let failed = |reason: String| PictureError::Footage {
            written: source.written.clone(),
            reason,
        };
        let count = footage.count(placed.source).map_err(failed)?;
        if count == 0 {
            return Err(failed("the file holds no pictures".to_string()));
        }
        let shown = (number - placed.slot.window.first).min(count - 1);
        let pixels = footage.picture(placed.source, shown, (rect.width, rect.height)).map_err(failed)?;
        let picture = Mark::Picture(PlacedPicture {
            width: rect.width,
            height: rect.height,
            pixels,
            x: rect.left as f32,
            y: rect.top as f32,
        });
        // The look the `<video>` writes is drawn as on any element: faded and filtered as one picture
        // (`docs/spec/render-command.md`, section 4.8). At an opacity of 0 nothing is drawn.
        if placed.opacity <= 0.0 {
            return Ok(None);
        }
        let marked = if placed.opacity == 1.0 && placed.filter.is_none() {
            picture
        } else {
            Mark::Group(Group {
                map: Map::IDENTITY,
                opacity: placed.opacity,
                clip: None,
                marks: vec![picture],
                turn: None,
                filter: placed.filter,
                shadow: None,
            })
        };
        // Round corners cut the footage to the rounded box, as of a box (`docs/spec/box-edges.md`, section 3).
        let marked = match Rounded::of_box(rect, placed.radius) {
            Some(shape) if shape.radius > 0.0 => Mark::Cut(Cut {
                shape,
                marks: vec![marked],
                own: None,
            }),
            _ => marked,
        };
        Ok(match placed.clip.filter(|_| clipped) {
            None => Some(marked),
            Some(clip) => {
                let held = intersect(rect, clip);
                (held.width > 0 && held.height > 0).then_some(Mark::Group(Group {
                    map: Map::IDENTITY,
                    opacity: 1.0,
                    clip: Some(held),
                    marks: vec![marked],
                    turn: None,
                    filter: Filter::NONE,
                    shadow: None,
                }))
            }
        })
    }

    /// Draws frame `number` with `renderer`.
    pub fn picture(&self, number: u64, renderer: &mut dyn Renderer) -> Result<Picture, PictureError> {
        renderer.draw_frame(&self.frame(number)?).map_err(PictureError::Renderer)
    }

    /// Draws frame `number` with `renderer`, with the pictures of its footage taken from `footage`.
    pub fn picture_with(&self, number: u64, renderer: &mut dyn Renderer, footage: &mut dyn Footage) -> Result<Picture, PictureError> {
        renderer
            .draw_frame(&self.frame_with(number, footage)?)
            .map_err(PictureError::Renderer)
    }
}

/// The marks of a box and of the children it cuts, in the order of `docs/spec/box-edges.md` section 3: its fill, its border and
/// its children, with its corners and its cut. With `clipped`, a box on a canvas shown from another file is cut to that canvas.
/// A box with square corners, no border and no cut is its fill alone, as before.
fn box_marks(placed: &Placed, clipped: bool, children: Vec<Mark>) -> Vec<Mark> {
    let edges = placed.edges;
    if edges.radius == 0.0 && edges.border.is_none() && !edges.cut && placed.shadows.is_empty() {
        let fill = if clipped {
            fill_mark(placed)
        } else {
            placed.fill.as_ref().and_then(|fill| fill.mark(placed.slot.rect, None))
        };
        return fill.into_iter().chain(children).collect();
    }
    let rect = placed.slot.rect;
    let background = placed.fill.as_ref().and_then(|fill| fill.mark(rect, None));
    // The shadows lie under the box, the last listed first (`docs/spec/render-command.md`, section 4.7, rule 4).
    let shadows = Rounded::of_box(rect, edges.radius).into_iter().flat_map(|shape| {
        placed
            .shadows
            .iter()
            .rev()
            .map(move |&shadow| Mark::BoxShadow(BoxShadow { shape, shadow }))
    });
    let marks: Vec<Mark> = shadows
        .chain(edges.marks(rect, background.into_iter().collect(), children))
        .collect();
    match placed.clip {
        Some(clip) if clipped && !marks.is_empty() => vec![Mark::Group(Group {
            map: Map::IDENTITY,
            opacity: 1.0,
            clip: Some(clip),
            marks,
            turn: None,
            filter: Filter::NONE,
            shadow: None,
        })],
        _ => marks,
    }
}

/// The fill of a box in its frame: the part of its rectangle that its canvas holds, when it has a fill and
/// the part is not empty (`docs/spec/timeline.md` section 4).
fn fill_mark(placed: &Placed) -> Option<Mark> {
    placed.fill.as_ref().and_then(|fill| fill.mark(placed.slot.rect, placed.clip))
}

/// The glyph runs of a paragraph at frame `number`, at `time`, moved to its place in the frame, and the mark that draws them.
fn text_mark(text: &PlacedText, number: u64, time: Time) -> Option<Mark> {
    let paragraph = &text.paragraph;
    let (block, offset) = paragraph.block_at(number, time);
    if block.runs.is_empty() && block.pictures.is_empty() {
        return None;
    }
    let (left, top) = (paragraph.rect.left as f32 + offset, paragraph.rect.top as f32);
    let runs = block
        .runs
        .iter()
        .map(|run| GlyphRun {
            font: Font {
                data: run.font.clone(),
                index: run.index,
            },
            size: run.size,
            skew_degrees: run.skew_degrees,
            colour: Colour {
                red: run.colour[0],
                green: run.colour[1],
                blue: run.colour[2],
            },
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
    let pictures = block
        .pictures
        .iter()
        .map(|picture| GlyphPicture {
            width: picture.width,
            height: picture.height,
            pixels: picture.pixels.clone(),
            x: picture.x + left,
            y: picture.y + top,
        })
        .collect();
    Some(Mark::Text(TextMark {
        runs,
        pictures,
        clip: text.clip,
        shadows: paragraph.dressing.shadows.clone(),
        outline: paragraph.dressing.outline,
    }))
}

/// Whether an element takes an attribute of this name: a `<text>` takes what `TEXT_TAKES` lists, and a `<box>` what
/// `BOX_TAKES` lists and the text attributes, which pass to the elements inside it (`docs/spec/text.md`, section 8).
fn takes(node: &Node, name: &str) -> bool {
    if (MOVEMENT.contains(&name) || name == "drop-shadow") && moving::moves(node.element()) {
        return true;
    }
    match node.element() {
        Element::Text => TEXT_TAKES.contains(&name),
        Element::Video => VIDEO_TAKES.contains(&name),
        _ => BOX_TAKES.contains(&name) || TEXT_ATTRIBUTES.contains(&name),
    }
}

/// The reports of the forms of a place, a size, a margin, a position and the words and numbers of a
/// row or column that this version does not list, in the order of the file, of an attribute that a
/// `<style>` gives and this version does not draw, and of a text form: an element inside a `<text>` and an
/// attribute of a `<text>` that this version does not draw. They are the reports `Scene::from_project` makes of
/// those forms, by the same code, without the settings of the project file and without the other refusals of
/// what is drawn: `check` judges the forms of a place, the attributes that a style gives and the text forms, and
/// does not judge the rest (`docs/spec/check-command.md`, sections 3b and 3c; `docs/spec/text.md`, section 9).
pub fn place_reports(project: &Project, codecs: &codec::Registry) -> Vec<Report> {
    walk(project, true, codecs).3
}

/// The reports of [`place_reports`], and the notes that `check` prints (`docs/spec/check-command.md`, section 1): one for
/// each `<text>` with a character that no family of its list and no family of the machine maps, on the line of the
/// `<text>`, which names the first such character (`docs/spec/text.md`, section 2.4, point 3).
pub fn place_reports_and_notes(project: &Project, codecs: &codec::Registry) -> (Vec<Report>, Vec<String>) {
    let mut reports = settings(project);
    let (entries, _, _, found) = walk(project, false, codecs);
    reports.extend(found);
    let mut notes: Vec<String> = Vec::new();
    for entry in &entries {
        let Entry::Text(text) = entry else {
            continue;
        };
        let Some(character) = text.paragraph.block.missing else {
            continue;
        };
        let note = format!(
            "note: {}, line {}: <text> holds the character U+{:04X}, which no font of its list and no font of the machine maps: it is drawn as the missing-character mark. No rule names a font for it in this version, since the stand-in fonts are chosen in Sprint 5",
            text.file, text.line, character as u32
        );
        if !notes.contains(&note) {
            notes.push(note);
        }
    }
    (reports, notes)
}

/// Walks the project's composition: the root's canvas, which is the frame, and then each element in document
/// order. Gives the boxes that are placed, in document order, the elements that move, the files of footage shown, and the
/// reports of the refusals. With `place_only`, the reports are those of a place form alone.
fn walk(project: &Project, place_only: bool, codecs: &codec::Registry) -> (Vec<Entry>, Vec<Mover>, Vec<FootageSource>, Vec<Report>) {
    let root = project.root();
    let mut sources = Vec::new();
    let (entries, movers, reports) = walk_canvas(
        project,
        codecs,
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
            frames: root.frames,
        },
        root.fps,
        place_only,
        &mut sources,
    );
    (entries, movers, sources, reports)
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
    /// The number of frames of the composition.
    frames: u64,
}

/// Walks one canvas: its root, then each element after its parent (`docs/spec/timeline.md` section 4). A box or a
/// composition shown from another file is placed unless it, or an element it stands in, is refused. A row or
/// column reads the boxes and the compositions inside it ahead, since its flow places them
/// (`docs/spec/render-command.md` section 4.3). A composition shown from another file is walked on a canvas of
/// its own, at its place, in its window and cut to its rectangle. Gives the boxes that are placed, in document
/// order, and the reports: those of this file in the order of their lines, then those of the files it shows, each
/// report once, in the order the files are reached.
fn walk_canvas(
    project: &Project,
    codecs: &codec::Registry,
    canvas: Canvas<'_>,
    fps: u32,
    place_only: bool,
    sources: &mut Vec<FootageSource>,
) -> (Vec<Entry>, Vec<Mover>, Vec<Report>) {
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
    let (mut fonts, font_faults) = paragraph::load_fonts(project, tree, codecs);
    refusals.extend(font_faults.into_iter().map(Refusal::of_text));
    // The sounds are those of the root's canvas; their faults are found from the file alone and reported by `check`
    // as a font's are (`docs/spec/sound.md`, section 5).
    if canvas.clip.is_none() {
        let (_, sound_faults) = sound::gather(project, tree);
        refusals.extend(sound_faults.into_iter().map(Refusal::of_text));
    }
    // The reports of the files shown from this one, each once.
    let mut shown: Vec<Report> = Vec::new();
    let mut entries: Vec<Entry> = Vec::new();
    // For each element, in document order: the slot its box is placed in, and `None` for an element that is
    // reported and for one inside it, which is not looked into. An element comes after its parent in the tree,
    // so the parent's entry is already known.
    let mut places: Vec<Option<Slot>> = Vec::with_capacity(tree.len());
    // A box or a composition that its row or column has read ahead, in the form it was read in.
    let mut reads: Vec<Option<Read>> = vec![None; tree.len()];
    // The rectangle and the measured axes that the flow of a row or column gives each of its items.
    let mut assigned: Vec<Option<(Rect, Measured)>> = vec![None; tree.len()];
    // The size and the first baseline of each row, column, set of columns and text that is an item, from the inside out.
    // The time the children of each element count their `start` from: the start of its clip, or its parent's time.
    let mut origin_in: Vec<Time> = vec![canvas.slot.origin; tree.len()];
    for (index, node) in tree.iter().enumerate() {
        let Some(parent) = node.parent() else {
            continue;
        };
        origin_in[index] = origin_in[parent];
        if matches!(node.element(), Element::Box | Element::Nested | Element::Text) {
            if let (Read::Valid(form), _) = read_element(project, node) {
                if let Some((start, _)) = form.clip {
                    origin_in[index] = origin_in[parent] + start;
                }
            }
        }
    }
    let clock_of = |index: usize| Clock {
        fps,
        frames: canvas.frames,
        origin: tree[index].parent().map_or(canvas.slot.origin, |parent| origin_in[parent]),
        inner: origin_in[index],
    };
    let sizes = measures(project, tree, &inside, &mut fonts, &clock_of);
    // The motion of each element (`docs/spec/render-command.md`, section 4.5): the number of entries before it, the entry
    // of its own box or text, its rectangle, the start of its parent's time, what it writes of its motion and its tweens,
    // with the line of each tween. The elements that move are gathered after the walk, when the entries inside each are known.
    let mut entries_before: Vec<usize> = Vec::with_capacity(tree.len() + 1);
    let mut entry_at: Vec<Option<usize>> = vec![None; tree.len()];
    let mut rect_of: Vec<Option<Rect>> = vec![None; tree.len()];
    let mut parent_origin: Vec<Time> = vec![Time::ZERO; tree.len()];
    let mut movement_of: Vec<Option<Movement>> = vec![None; tree.len()];
    let mut tweens_of: Vec<Vec<(Tween, u32)>> = vec![Vec::new(); tree.len()];
    let mut movers: Vec<Mover> = Vec::new();

    for (index, node) in tree.iter().enumerate() {
        entries_before.push(entries.len());
        let Some(parent) = node.parent() else {
            // The root of the canvas.
            for attribute in node.attributes() {
                if !ROOT_TAKES.contains(&attribute.name()) {
                    let (code, message) = unsupported_attribute(node, attribute);
                    refusals.push(Refusal::of_attribute(code, attribute, message));
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
        if tree[parent].element() == Element::Nested && !matches!(node.element(), Element::Tween | Element::Set) {
            // A composition shown from another file takes no element of its own but its tweens and sets: its file is drawn
            // in its place.
            refusals.push(Refusal::of_drawing(
                Code::FormNotSupported,
                node.line(),
                unsupported_inside(node.element(), Element::Nested),
            ));
            places.push(None);
            continue;
        }
        if node.element() == Element::Audio {
            // A sound is heard, not drawn: it is gathered from the root's canvas (`sound::gather`). Its level is not
            // applied in this version, and a sound of a file shown from another file is not placed.
            if canvas.clip.is_some() {
                refusals.push(Refusal::of_drawing(
                    Code::FormNotSupported,
                    node.line(),
                    "<audio> in a composition shown from another file is not supported by this version, which places the sounds of the project's own composition only".to_string(),
                ));
            } else if let Some(attribute) = node.attribute("volume") {
                refusals.push(Refusal::of_attribute(
                    Code::FormNotSupported,
                    attribute,
                    unapplied_volume(attribute),
                ));
            }
            places.push(Some(parent_place));
            continue;
        }
        if node.element() == Element::Volume {
            refusals.push(Refusal::of_drawing(
                Code::FormNotSupported,
                node.line(),
                "<volume> is not applied by this version, which plays a sound at the level of its file".to_string(),
            ));
            places.push(None);
            continue;
        }
        if node.element() == Element::Font {
            // A font stands in the root, where its file is read (`load_fonts`). The schema allows it nowhere else.
            places.push(None);
            continue;
        }
        if node.element() == Element::Span {
            // A span is drawn in the paragraph of the text it stands in, and its faults are reported with the text
            // (`docs/spec/text.md`, section 6.5). The elements inside it are judged against its text's place.
            places.push(Some(parent_place));
            continue;
        }
        if node.element() == Element::Text && tree[parent].element() == Element::Text {
            refusals.push(Refusal::of_text_form(
                Code::FormNotSupported,
                node.line(),
                unsupported_inside(node.element(), Element::Text),
            ));
            places.push(None);
            continue;
        }
        if matches!(node.element(), Element::Tween | Element::Set) && tree[parent].element() == Element::Span {
            // A tween or a set in a span changes `typed` alone; the paragraph reads it (`docs/spec/text.md`, section 7).
            refusals.extend(span_motion_faults(node));
            places.push(None);
            continue;
        }
        if node.element() == Element::Set && moving::moves(tree[parent].element()) {
            // A set is a tween of no length, kept in the order written with the element's tweens (`docs/spec/notation.md`
            // sections 8.3 and 8.5).
            let (set, found) = moving::read_set(node, parent_origin[parent]);
            refusals.extend(found.into_iter().map(Refusal::of_motion));
            if let Some(set) = set {
                tweens_of[parent].push((set, node.line()));
            }
            places.push(None);
            continue;
        }
        if node.element() == Element::Tween && moving::moves(tree[parent].element()) {
            let (tween, found) = moving::read_tween(node, parent_origin[parent]);
            refusals.extend(found.into_iter().map(Refusal::of_motion));
            if let Some(tween) = tween {
                tweens_of[parent].push((tween, node.line()));
            }
            places.push(None);
            continue;
        }
        // Footage in a row, a column or a set of columns is refused (S52): a flow places its items by a rule that
        // does not take footage. Any other `<video>` is drawn in this version (`docs/spec/render-command.md`, section 4.8)
        // and judged as drawn, like a box or a text.
        if node.element() == Element::Video && in_flow(&tree[parent]) {
            refusals.push(Refusal::of_drawing(
                Code::FormNotSupported,
                node.line(),
                Unsupported::FootageInFlow.message(&format!("{} holds <video> in a flow", tree[parent].element().tag())),
            ));
            refusals.extend(
                forms_of_unsupported(node)
                    .into_iter()
                    .map(|(line, message)| Refusal::of_drawing(Code::FormNotSupported, line, message)),
            );
            places.push(None);
            continue;
        }
        if !matches!(node.element(), Element::Box | Element::Nested | Element::Text | Element::Video) {
            refusals.push(Refusal::of_drawing(
                Code::FormNotSupported,
                node.line(),
                unsupported_inside(node.element(), tree[parent].element()),
            ));
            // Each form of the support table is listed on its own, also on an element that is itself not drawn, so that
            // its line stays when the element's entry goes (`docs/spec/check-command.md` section 3d).
            refusals.extend(
                forms_of_unsupported(node)
                    .into_iter()
                    .map(|(line, message)| Refusal::of_drawing(Code::FormNotSupported, line, message)),
            );
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
        parent_origin[index] = parent_place.origin;
        if moving::moves(node.element()) {
            let (movement, found) = moving::read_movement(node);
            refusals.extend(found.into_iter().map(Refusal::of_motion));
            movement_of[index] = Some(movement);
        }
        if node.element() == Element::Text {
            refusals.extend(paragraph::span_faults(tree, index).into_iter().map(Refusal::of_text));
        }
        let Read::Valid(form) = read else {
            places.push(None);
            continue;
        };
        if node.element() == Element::Text {
            // A text that is an item of a row, a column or a set takes the rectangle its flow gives it; any other is
            // placed in its parent.
            let placed = match assigned[index] {
                Some((rect, _)) => paragraph::lay_out_item(&mut fonts, tree, index, form.written.clone(), form.absolute, clock_of(index))
                    .map(|item| Paragraph {
                        rect,
                        block: item.block,
                        dressing: item.dressing,
                        content: item.content,
                    }),
                None => {
                    let margin = absolute_margin(&form);
                    paragraph::place_text(&mut fonts, tree, index, form.written, margin, parent_place.rect, clock_of(index))
                }
            };
            match placed {
                Ok(paragraph) => {
                    let slot = slot_of(form.clip, parent_place, paragraph.rect, fps);
                    entry_at[index] = Some(entries.len());
                    rect_of[index] = Some(paragraph.rect);
                    entries.push(Entry::Text(Box::new(PlacedText {
                        slot,
                        clip: canvas.clip,
                        paragraph,
                        file: canvas.file.to_string(),
                        line: node.line(),
                    })));
                    places.push(Some(slot));
                }
                Err(fault) => {
                    refusals.push(Refusal::of_text(fault));
                    places.push(None);
                }
            }
            continue;
        }
        // The boxes, compositions and texts inside a row, a column or a set of columns are read now: they are its items
        // or its cells, and their places come from its layout. Their refusals are kept until this element is placed.
        let mut pending: Vec<Refusal> = Vec::new();
        let mut items: Vec<Item> = Vec::new();
        let mut cells: Vec<Cell> = Vec::new();
        let mut item_nodes: Vec<(usize, Measured)> = Vec::new();
        if form.flex.is_some() || form.columns.is_some() {
            for &inner in &inside[index] {
                let inner_node = &tree[inner];
                if !matches!(inner_node.element(), Element::Box | Element::Nested | Element::Text) {
                    continue;
                }
                let (read, found) = read_element(project, inner_node);
                pending.extend(found);
                if let Read::Valid(inner_form) = &read {
                    let measure = sizes[inner];
                    let measured = if let Some(flex) = form.flex {
                        items.push(item_in(inner_form, measure, &flex));
                        measured_axes(inner_form, &flex)
                    } else {
                        cells.push(cell_in(inner_form, measure));
                        form.columns
                            .as_ref()
                            .map_or_else(Measured::default, |set| set_measured_axes(inner_form, set))
                    };
                    item_nodes.push((inner, measured));
                }
                reads[inner] = Some(read);
            }
        }
        let laid = match (assigned[index], form.flex, form.columns.as_ref()) {
            // An item of a row or column, or a cell of a set: its own place is the flow's, and its children are laid out
            // in it.
            (Some((rect, measured)), Some(flex), _) => layout::inside(rect, measured, &flex, &items).map(|flowed| (rect, flowed.items)),
            (Some((rect, measured)), None, Some(set)) => {
                layout::inside_columns(rect, measured, set, &cells).map(|flowed| (rect, flowed.items))
            }
            (Some((rect, _)), None, None) => Ok((rect, Vec::new())),
            // A row or column, or a set, with no place of its own: its size is worked out with its children.
            (None, Some(flex), _) => layout::flow_absolute(parent_place.rect, &form.written, absolute_margin(&form), &flex, &items)
                .map(|flowed| (flowed.container, flowed.items)),
            (None, None, Some(set)) => layout::columns_absolute(parent_place.rect, &form.written, absolute_margin(&form), set, &cells)
                .map(|flowed| (flowed.container, flowed.items)),
            (None, None, None) => {
                layout::place_absolute(parent_place.rect, &form.written, absolute_margin(&form)).map(|rect| (rect, Vec::new()))
            }
        };
        let (rect, item_rects) = match laid {
            Ok(laid) => laid,
            Err(Beyond) => {
                let message = Unsupported::BeyondTheLimit.message(&format!(
                    "a place or a size of {} works out beyond {LIMIT} pixels, or needs more digits than this version works with",
                    node.element().tag()
                ));
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

        if node.element() == Element::Video {
            // The notation has judged that a `<video>` has its `src` (FS-0068), so a placed one names its file.
            let Some(written) = node.attribute("src").map(|attribute| attribute.value().to_string()) else {
                places.push(None);
                continue;
            };
            let path = project.resolve(&written);
            let source = match sources.iter().position(|source| source.path == path) {
                Some(found) => found,
                None => {
                    sources.push(FootageSource {
                        written,
                        path,
                        file: canvas.file.to_string(),
                        line: node.line(),
                    });
                    sources.len() - 1
                }
            };
            let (opacity, filter, found) = moving::read_look(node);
            refusals.extend(found.into_iter().map(Refusal::of_motion));
            // The corner radius, as of a box (`docs/spec/box-edges.md`, section 6): a negative radius is reported.
            let mut radius = 0.0;
            if let Some(attribute) = node.attribute("border-radius") {
                match attribute.number() {
                    Some(written) if written >= 0.0 => radius = written,
                    Some(_) => refusals.push(Refusal::of_place(
                        Code::LengthNotAccepted,
                        attribute.line(),
                        format!("`border-radius=\"{}\"` must be 0 or more", attribute.shown()),
                    )),
                    None => {}
                }
            }
            entries.push(Entry::Footage(PlacedFootage {
                slot,
                clip: canvas.clip,
                source,
                opacity,
                filter,
                radius,
            }));
            places.push(Some(slot));
            continue;
        }

        let Some(name) = &form.shown else {
            entry_at[index] = Some(entries.len());
            rect_of[index] = Some(rect);
            entries.push(Entry::Box(Placed {
                slot,
                fill: form.fill.clone(),
                clip: canvas.clip,
                edges: form.edges,
                shadows: form.shadows.clone(),
                cut_end: None,
            }));
            places.push(Some(slot));
            continue;
        };
        let Some(file) = project.shown_file(name) else {
            // The project's open reports a file that is not there or cannot be read, so the run does not get here.
            places.push(None);
            continue;
        };
        entry_at[index] = Some(entries.len());
        rect_of[index] = Some(rect);
        // The file is cut to the clip of the element and to its own length (`docs/spec/notation.md` section 10).
        let file_window = Window::of_clip(slot.origin, slot.origin + file.root().duration, fps);
        let clip = match canvas.clip {
            Some(outer) => intersect(rect, outer),
            None => rect,
        };
        let (placed, inner_movers, found) = walk_canvas(
            project,
            codecs,
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
                frames: canvas.frames,
            },
            fps,
            place_only,
            sources,
        );
        let offset = entries.len();
        let placed = placed.into_iter().map(|entry| match entry {
            Entry::Box(inner) => Entry::Box(Placed {
                cut_end: inner.cut_end.map(|end| end + offset),
                ..inner
            }),
            other => other,
        });
        movers.extend(inner_movers.into_iter().map(|mover| Mover {
            first: mover.first + offset,
            end: mover.end + offset,
            ..mover
        }));
        entries.extend(placed);
        for report in found {
            if !shown.contains(&report) {
                shown.push(report);
            }
        }
        places.push(Some(slot));
    }

    entries_before.push(entries.len());
    // A box that cuts its children holds the run of entries of its subtree: the elements after it whose parents lie in it.
    for (index, first) in entry_at.iter().enumerate() {
        let Some(first) = *first else {
            continue;
        };
        // A shown composition takes the place of its entries, which may be none, so only a box can cut its children.
        if tree[index].element() != Element::Box {
            continue;
        }
        if let Some(Entry::Box(placed)) = entries.get_mut(first) {
            if placed.edges.cut {
                let mut after = index + 1;
                while after < tree.len() && tree[after].parent().is_some_and(|parent| parent >= index) {
                    after += 1;
                }
                placed.cut_end = Some(entries_before[after]);
            }
        }
    }
    let own = gather_movers(
        tree,
        &entries_before,
        &entry_at,
        &rect_of,
        &movement_of,
        &tweens_of,
        canvas.clip,
        fps,
    );
    refusals.extend(own.1);
    // An element comes before the elements inside it; a moving `<composition src>` and a mover of its file that starts at the
    // same entry are ordered by their ends, the one holding the other first.
    let mut all = own.0;
    all.append(&mut movers);
    movers = all;
    movers.sort_by_key(|mover| (mover.first, std::cmp::Reverse(mover.end)));

    // A row or column reads its items ahead, so the reports are put in the order of the file by their lines; a
    // stable sort keeps the order of the reports of one line.
    refusals.sort_by_key(|refusal| refusal.line);
    let mut reports: Vec<Report> = refusals
        .into_iter()
        .filter(|refusal| refusal.place || refusal.style || !place_only)
        .map(|refusal| Report::new(canvas.file, Problem::new(refusal.code, refusal.line, refusal.message)))
        .collect();
    reports.extend(shown);
    (entries, movers, reports)
}

/// The elements of one canvas that move: each placed `<box>` or `<text>` that writes a movement attribute or holds a tween,
/// with the run of entries of its subtree, which follow one another in document order. One that holds a
/// `<composition src>` is refused on the line of each movement attribute and tween (`docs/spec/render-command.md` section 3),
/// and does not move.
#[allow(clippy::too_many_arguments)]
fn gather_movers(
    tree: &[Node],
    entries_before: &[usize],
    entry_at: &[Option<usize>],
    rect_of: &[Option<Rect>],
    movement_of: &[Option<Movement>],
    tweens_of: &[Vec<(Tween, u32)>],
    clip: Option<Rect>,
    fps: u32,
) -> (Vec<Mover>, Vec<Refusal>) {
    let mut movers = Vec::new();
    let mut refusals = Vec::new();
    for index in 0..tree.len() {
        let (Some(first), Some(rect), Some(movement)) = (entry_at[index], rect_of[index], &movement_of[index]) else {
            continue;
        };
        let tweens = &tweens_of[index];
        if !movement.written() && tweens.is_empty() {
            continue;
        }
        // The subtree of an element is the run of elements after it whose parents lie in it: in document order, those whose
        // parent is the element or comes after it.
        let mut after = index + 1;
        let mut holds_shown = false;
        while after < tree.len() && tree[after].parent().is_some_and(|parent| parent >= index) {
            holds_shown |= tree[after].element() == Element::Nested;
            after += 1;
        }
        if holds_shown {
            let tag = tree[index].element().tag();
            for line in movement.lines.iter().copied().chain(tweens.iter().map(|(_, line)| *line)) {
                refusals.push(Refusal::of_motion((
                    Code::FormNotSupported,
                    line,
                    Unsupported::MotionAroundShown.message(&format!("{tag} that holds a <composition src> moves")),
                )));
            }
            if let Some((_, line)) = movement.shadow {
                refusals.push(Refusal::of_motion((
                    Code::FormNotSupported,
                    line,
                    format!(
                        "drop-shadow of {tag} that holds a <composition src> is not supported by this version, which draws it on a <box> or a <text> that holds no <composition src>"
                    ),
                )));
            }
            continue;
        }
        let written: Vec<Tween> = tweens.iter().map(|(tween, _)| tween.clone()).collect();
        movers.push(Mover {
            first,
            end: entries_before[after],
            own: movement.own,
            rect,
            origin: Mover::origin_in(rect, movement.origin.clone()),
            perspective: movement.perspective,
            filter: movement.filter,
            motion: (!written.is_empty()).then(|| ElementMotion::build(index, &written, movement.own, fps)),
            clip,
            shadow: movement.shadow.and_then(|(shadow, _)| shadow),
            cut: (tree[index].element() == Element::Nested).then_some(rect),
        });
    }
    (movers, refusals)
}

/// The refusals of a `<tween>` or a `<set>` in a `<span>`: each property it changes other than `typed`, on the line of its
/// first attribute (`docs/spec/text.md`, section 7): a tween's `from-` and `to-` of one property are one report. The
/// message is the entry's, as for a `<tween>` or a `<set>` the walk finds in a span (`unsupported_inside`).
fn span_motion_faults(node: &Node) -> Vec<Refusal> {
    let mut named: Vec<&str> = Vec::new();
    node.attributes()
        .iter()
        .filter_map(|attribute| {
            let name = attribute.name();
            let property = match node.element() {
                Element::Tween => name.strip_prefix("from-").or_else(|| name.strip_prefix("to-"))?,
                _ if name == "at" => return None,
                _ => name,
            };
            if property == "typed" || named.contains(&property) {
                return None;
            }
            named.push(property);
            Some(Refusal::of_motion((
                Code::FormNotSupported,
                attribute.line(),
                unsupported_inside(node.element(), Element::Span),
            )))
        })
        .collect()
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
        // The movement attributes are read with the element's motion (`moving::read_movement`).
        if MOVEMENT.contains(&name) {
            continue;
        }
        if !NESTED_TAKES.contains(&name) {
            let (code, message) = unsupported_attribute(node, attribute);
            refusals.push(Refusal::of_attribute(code, attribute, message));
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
    written.width = Some(Amount::Pixels(f64::from(file.width).into()));
    written.height = Some(Amount::Pixels(f64::from(file.height).into()));
    let form = Form {
        written,
        margin,
        across: None,
        absolute,
        flex: None,
        columns: None,
        grow: 0.0,
        text: false,
        fill: None,
        edges: NO_EDGES,
        shadows: Vec::new(),
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

    /// A refusal of an element inside a `<text>`, or of an attribute of a `<text>` that this version does not draw. It is
    /// a text form, which `check` judges as well as `render` (`docs/spec/text.md`, section 9).
    fn of_text_form(code: Code, line: u32, message: String) -> Refusal {
        Refusal {
            code,
            line,
            message,
            place: true,
            style: false,
        }
    }

    /// A refusal of a form of motion: a tween, a `<set>` or a movement attribute that this version does not apply. `check`
    /// judges it as well as `render`.
    fn of_motion((code, line, message): Found) -> Refusal {
        Refusal {
            code,
            line,
            message,
            place: true,
            style: false,
        }
    }

    /// A refusal of a background, of one of its layers or of one of its settings (`docs/spec/render-command.md`, section 4.6,
    /// rule 10). `check` judges it as well as `render`.
    fn of_background(code: Code, line: u32, message: String) -> Refusal {
        Refusal {
            code,
            line,
            message,
            place: true,
            style: false,
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

/// What a `<box>` or a `<text>` writes, read once from its attributes: its place as written (with its `max-width`), its
/// margin, its own `align-self`, whether it is `position="absolute"`, the row or column it lays out, or the set of columns,
/// its `flex-grow`, whether it is a `<text>`, its colour, and its clip (`start` and `duration` as written; the origin of
/// its parent is added when it is placed).
#[derive(Clone, Debug, PartialEq)]
struct Form {
    written: Written,
    margin: Sides,
    across: Option<Across>,
    absolute: bool,
    flex: Option<Flex>,
    columns: Option<Columns>,
    /// The share of the spare room of a row or a column (`flex-grow`), 0 for none.
    grow: f64,
    /// Whether the element is a `<text>`: a text is not stretched across, and is sized by its words.
    text: bool,
    fill: Option<Fill>,
    /// The border, corners and cut of a box (`docs/spec/box-edges.md`); none for a text and a `<composition src>`.
    edges: Edges,
    /// The shadows of a box (`docs/spec/render-command.md`, section 4.7); none for a text and a `<composition src>`.
    shadows: Vec<Shadow>,
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
    let mut layers: Option<Vec<draw::Paint>> = None;
    let mut background_line = None;
    let mut settings: Vec<(&'static str, String, u32)> = Vec::new();
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
    let mut grow: f64 = 0.0;
    let mut tracks: Option<Vec<Track>> = None;
    let mut columns_line: u32 = 0;
    let mut text_faults: Vec<Fault> = Vec::new();
    read_text_attributes(node, &mut text_faults);
    refusals.extend(text_faults.into_iter().map(Refusal::of_text));
    for attribute in node.attributes() {
        let name = attribute.name();
        if !takes(node, name) {
            // An attribute of a `<text>` is a text form that `check` judges; a `<box>`'s is judged only when a style
            // gives it (`docs/spec/check-command.md`, section 3c).
            let (code, message) = unsupported_attribute(node, attribute);
            refusals.push(Refusal::of_attribute(code, attribute, message));
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
                "space-between" => along = Along::SpaceBetween,
                "space-around" | "space-evenly" => refusals.push(Refusal::of_place(
                    Code::FormNotSupported,
                    attribute.line(),
                    Unsupported::RestOfFlexAndGrid.message(&support::written(node.element(), attribute)),
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
                    "baseline" => Some(Across::Baseline),
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
            "max-width" => match attribute.length() {
                Some(Length::Pixels(pixels)) if pixels.to_f64() > 0.0 => written.max_width = Some(pixels.to_f64()),
                Some(Length::Pixels(_)) => refusals.push(Refusal::of_place(
                    Code::LengthNotAccepted,
                    attribute.line(),
                    format!("`max-width=\"{value}\"` must be above 0"),
                )),
                _ => refusals.push(Refusal::of_place(
                    Code::NotANumber,
                    attribute.line(),
                    format!("`max-width=\"{value}\"` is not a number of pixels above 0, such as 240"),
                )),
            },
            "flex-grow" => match attribute.length() {
                Some(Length::Pixels(pixels)) if pixels.to_f64() >= 0.0 => grow = pixels.to_f64(),
                Some(Length::Pixels(_)) => refusals.push(Refusal::of_place(
                    Code::LengthNotAccepted,
                    attribute.line(),
                    format!("`flex-grow=\"{value}\"` must be 0 or more"),
                )),
                _ => refusals.push(Refusal::of_place(
                    Code::NotANumber,
                    attribute.line(),
                    format!("`flex-grow=\"{value}\"` is not a number, such as 1 or 2.5"),
                )),
            },
            "columns" => match columns_tracks(&value) {
                Ok(found) => {
                    tracks = Some(found);
                    columns_line = attribute.line();
                }
                Err((code, message)) => refusals.push(Refusal::of_place(code, attribute.line(), message)),
            },
            "gap" => match attribute.length() {
                Some(Length::Pixels(pixels)) if pixels.to_f64() < 0.0 => refusals.push(Refusal::of_place(
                    Code::LengthNotAccepted,
                    attribute.line(),
                    format!("`gap=\"{value}\"` must be 0 or more"),
                )),
                Some(Length::Pixels(pixels)) => gap = pixels.to_f64(),
                Some(Length::Percent(_)) => refusals.push(Refusal::of_place(
                    Code::FormNotSupported,
                    attribute.line(),
                    Unsupported::GapPercentage.message(&support::written(node.element(), attribute)),
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
            "background" => {
                background_line = Some(attribute.line());
                match attribute.colour() {
                    Some(found) if found.alpha == 255 => {
                        colour = Some(Colour {
                            red: found.red,
                            green: found.green,
                            blue: found.blue,
                        })
                    }
                    _ => match background::layers(attribute.value()) {
                        Ok(found) => layers = Some(found),
                        Err((code, message)) => {
                            // One a style gives names the style, as the message of an attribute this version does not
                            // draw does (`docs/spec/check-command.md` section 3c).
                            let message = match attribute.given_by() {
                                None => message,
                                Some(style) => format!("{message}, which the style `{}` gives it", project::shown(style)),
                            };
                            refusals.push(Refusal::of_background(code, attribute.line(), message))
                        }
                    },
                }
            }
            "background-size" | "background-position" | "background-repeat" => {
                let name = match name {
                    "background-size" => "background-size",
                    "background-position" => "background-position",
                    _ => "background-repeat",
                };
                settings.push((name, attribute.value().to_string(), attribute.line()));
            }
            "start" | "duration" => match attribute.time() {
                Some(time) if name == "start" => start = Some(time),
                Some(time) => duration = Some(time),
                None => refusals.push(Refusal::of_attribute(
                    Code::NotATime,
                    attribute,
                    format!("`{name}=\"{value}\"` is not a time in seconds: digits with an optional point, such as 5 or 1.25"),
                )),
            },
            _ => {}
        }
    }
    // The border, the corner radius and the cut of a box (`docs/spec/box-edges.md`, section 6); a text takes none of them.
    let edges = if node.element() == Element::Text {
        NO_EDGES
    } else {
        match read_edges(node) {
            Ok(edges) => edges,
            Err(faults) => {
                refusals.extend(
                    faults
                        .into_iter()
                        .map(|(code, line, message)| Refusal::of_place(code, line, message)),
                );
                NO_EDGES
            }
        }
    };
    // The shadows of a box (`docs/spec/render-command.md`, section 4.7, rule 8 for the reports).
    let shadows = match node.attribute("box-shadow").filter(|_| node.element() != Element::Text) {
        Some(attribute) => match crate::shadows::box_shadows(&support::written(node.element(), attribute), attribute.value()) {
            Ok(shadows) => shadows,
            Err((code, message)) => {
                refusals.push(Refusal::of_place(code, attribute.line(), message));
                Vec::new()
            }
        },
        None => Vec::new(),
    };
    // A `<box>` lays its children out in a row or a column, or in a set of columns, and not in both (`FS-0055`).
    let columns = match tracks {
        Some(_) if direction.is_some() => {
            refusals.push(Refusal::of_place(
                Code::ColumnsWithDirection,
                columns_line,
                "a <box> has `columns` or `flex-direction`, not both".to_string(),
            ));
            None
        }
        Some(found) => Some(Columns {
            tracks: found,
            gap,
            padding,
            across: items_across,
        }),
        None => None,
    };
    let fill = match background_fill(colour, layers, background_line, &settings) {
        Ok(fill) => fill,
        Err(mut found) => {
            refusals.append(&mut found);
            None
        }
    };
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
        columns,
        grow,
        text: node.element() == Element::Text,
        fill,
        edges,
        shadows,
        clip,
        shown: None,
    };
    (Read::Valid(Box::new(form)), Vec::new())
}

/// What a box fills with: its colour, or its layers with their settings (`docs/spec/render-command.md`, section 4.6). A
/// setting with no `background` is reported (`FS-0066`), as is a list of settings with neither one entry nor one for each
/// layer (`FS-0065`) and a setting not in its form (`FS-0064`).
fn background_fill(
    colour: Option<Colour>,
    layers: Option<Vec<draw::Paint>>,
    line: Option<u32>,
    settings: &[(&'static str, String, u32)],
) -> Result<Option<Fill>, Vec<Refusal>> {
    if line.is_none() {
        let alone = settings.iter().map(|(name, value, at)| {
            Refusal::of_background(
                Code::BackgroundSettingAlone,
                *at,
                format!(
                    "`{name}=\"{}\"` is written on <box>, which has no `background`",
                    project::shown(value)
                ),
            )
        });
        return match alone.collect::<Vec<_>>() {
            found if found.is_empty() => Ok(None),
            found => Err(found),
        };
    }
    let paints = match (colour, layers) {
        (Some(colour), _) if settings.is_empty() => return Ok(Some(Fill::Colour(colour))),
        (Some(colour), _) => vec![draw::Paint::Colour(draw::Rgba {
            red: colour.red,
            green: colour.green,
            blue: colour.blue,
            alpha: 255,
        })],
        (None, Some(paints)) => paints,
        // The `background` was refused already.
        (None, None) => return Ok(None),
    };
    let count = paints.len();
    let mut sizes = vec![draw::Size::Auto; count];
    let mut positions = vec![(draw::Amount::Pixels(0.0), draw::Amount::Pixels(0.0)); count];
    let mut repeats = vec![true; count];
    let mut refusals = Vec::new();
    for (name, value, at) in settings {
        let read = match *name {
            "background-size" => background::sizes(value)
                .and_then(|found| background::for_each(name, value, found, count))
                .map(|found| sizes = found),
            "background-position" => background::positions(value)
                .and_then(|found| background::for_each(name, value, found, count))
                .map(|found| positions = found),
            _ => background::repeats(value)
                .and_then(|found| background::for_each(name, value, found, count))
                .map(|found| repeats = found),
        };
        if let Err((code, message)) = read {
            refusals.push(Refusal::of_background(code, *at, message));
        }
    }
    if !refusals.is_empty() {
        return Err(refusals);
    }
    Ok(Some(Fill::Layers(Arc::new(background::settle(paints, sizes, positions, repeats)))))
}

/// The column widths of a `columns` attribute (`docs/spec/render-command.md` section 4.4, rule 1): numbers of pixels above
/// 0, or shares such as `1fr` above 0, separated by spaces. A word that is not one of these is not a list (`FS-0054`); a
/// width of 0 or below is reported as `FS-0053`.
fn columns_tracks(value: &str) -> Result<Vec<Track>, (Code, String)> {
    let not_a_list = || {
        (
            Code::ColumnsNotAList,
            format!("`columns=\"{value}\"` is not a list of column widths: pixels above 0, or shares such as 1fr"),
        )
    };
    let mut tracks = Vec::new();
    for word in value.split_whitespace() {
        let (text, share) = match word.strip_suffix("fr") {
            Some(text) => (text, true),
            None => (word, false),
        };
        let amount = paragraph::number(text).ok_or_else(not_a_list)?;
        if amount <= 0.0 {
            return Err((
                Code::SizeNotAboveZero,
                format!("`columns=\"{value}\"` has a column width that is not above 0"),
            ));
        }
        tracks.push(if share { Track::Share(amount) } else { Track::Pixels(amount) });
    }
    if tracks.is_empty() {
        return Err(not_a_list());
    }
    Ok(tracks)
}

/// The size and the first baseline of an item that is measured from its own children or its lines: a row, a column,
/// a set of columns (`docs/spec/render-command.md` sections 4.3 and 4.4, rule 2) or a `<text>` (`text.md` section 8).
/// The baseline is that of the first child, or of the first line, from the top; `None` for a bottom edge.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Measure {
    size: Size,
    baseline: Option<f64>,
}

/// The measures of each element that is an item of a flow: a row, a column, a set of columns, or a `<text>`. A row, a
/// column or a set is measured from its children, which are measured first: an element comes after its parent, so the
/// elements are worked through from the last to the first. `None` for an element that is not one of these or is refused,
/// and for a row, a column or a set whose size does not work out.
fn measures(
    project: &Project,
    tree: &[Node],
    inside: &[Vec<usize>],
    fonts: &mut Fonts,
    clock_of: &dyn Fn(usize) -> Clock,
) -> Vec<Option<Measure>> {
    let forms: Vec<Option<Form>> = tree
        .iter()
        .map(|node| match node.element() {
            Element::Box | Element::Nested | Element::Text => match read_element(project, node).0 {
                Read::Valid(form) => Some(*form),
                Read::Refused => None,
            },
            _ => None,
        })
        .collect();
    let mut measures: Vec<Option<Measure>> = vec![None; tree.len()];
    for index in (0..tree.len()).rev() {
        let Some(form) = forms[index].as_ref() else {
            continue;
        };
        if tree[index].element() == Element::Text {
            measures[index] = paragraph::lay_out_item(fonts, tree, index, form.written.clone(), form.absolute, clock_of(index))
                .ok()
                .map(|item| Measure {
                    size: Size {
                        width: item.width,
                        height: item.height,
                    },
                    baseline: item.baseline,
                });
            continue;
        }
        if form.flex.is_none() && form.columns.is_none() {
            continue;
        }
        if let Some(flex) = form.flex {
            let items: Vec<Item> = inside[index]
                .iter()
                .filter_map(|&inner| forms[inner].as_ref().map(|found| item_in(found, measures[inner], &flex)))
                .collect();
            measures[index] = layout::measure(&flex, &items).ok().map(|size| Measure {
                size,
                baseline: layout::inside(whole_rect(size), Measured { width: true, height: true }, &flex, &items)
                    .ok()
                    .and_then(|flowed| flowed.baseline),
            });
        } else if let Some(set) = form.columns.as_ref() {
            let cells: Vec<Cell> = inside[index]
                .iter()
                .filter_map(|&inner| forms[inner].as_ref().map(|found| cell_in(found, measures[inner])))
                .collect();
            measures[index] = layout::measure_columns(set, &cells).ok().map(|size| Measure {
                size,
                baseline: layout::inside_columns(whole_rect(size), Measured { width: true, height: true }, set, &cells)
                    .ok()
                    .and_then(|flowed| flowed.baseline),
            });
        }
    }
    measures
}

/// The rectangle at the origin of a size measured from children, in whole pixels, for placing those children to find
/// their baseline.
fn whole_rect(size: Size) -> Rect {
    Rect {
        left: 0,
        top: 0,
        width: size.width.round().max(0.0) as u32,
        height: size.height.round().max(0.0) as u32,
    }
}

/// The margin that moves a box or a text placed in a plain parent: its own, since every child of a plain box is
/// absolutely placed, whether or not it writes `position` (`render-command.md` section 4.1, rule 8; `notation.md` N10, B41).
fn absolute_margin(form: &Form) -> Sides {
    form.margin
}

/// What a box writes as an item of its row or column's flow. Where it writes no size on an axis, and the
/// row or column measured it (`measured`), the size its own items give it (section 4.3, rule 2) is written
/// for it. An absolute box takes the measured size only where its edges do not give one. A flowed box that
/// stretches across writes no size across: its row or column gives it one. Its size across, as its own items
/// give it, is kept for its row or column's size across (rule 1), whether it stretches or not.
fn item_in(form: &Form, measure: Option<Measure>, parent: &Flex) -> Item {
    let mut written = form.written.clone();
    let mut measured_across = None;
    if let Some(Measure { size, .. }) = measure {
        if form.absolute {
            fill(&mut written.width, size.width, both(written.left.clone(), written.right.clone()));
            fill(&mut written.height, size.height, both(written.top.clone(), written.bottom.clone()));
        } else if form.text {
            // A text is not stretched across: it takes its own size on both axes (`render-command.md` section 4.3, rule 14).
            fill(&mut written.width, size.width, false);
            fill(&mut written.height, size.height, false);
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
        baseline: measure.and_then(|found| found.baseline),
        grow: form.grow,
        // The widest width caps an item that writes no width of its own, a box's from its items included (section 4.3,
        // rule 13). A text is fitted to its `max-width` by its words, so it is not capped again.
        max_width: if form.text || form.written.width.is_some() {
            None
        } else {
            form.written.max_width
        },
    }
}

/// What a box or a `<text>` writes as a cell of a set of columns (`docs/spec/render-command.md` section 4.4). A text takes
/// its own size on both axes, as an item does; a box keeps what it writes, with the size its own children give it.
fn cell_in(form: &Form, measure: Option<Measure>) -> Cell {
    let mut written = form.written.clone();
    let measured = measure.map(|found| found.size);
    if let (true, Some(size)) = (form.text, measured) {
        if form.absolute {
            fill(&mut written.width, size.width, both(written.left.clone(), written.right.clone()));
            fill(&mut written.height, size.height, both(written.top.clone(), written.bottom.clone()));
        } else {
            fill(&mut written.width, size.width, false);
            fill(&mut written.height, size.height, false);
        }
    }
    Cell {
        written,
        margin: form.margin,
        across: form.across,
        absolute: form.absolute,
        measured,
        baseline: measure.and_then(|found| found.baseline),
    }
}

/// Gives an unwritten size the measured value, unless the edges of the box give it one.
fn fill(size: &mut Option<Amount>, measured: f64, edged: bool) {
    if size.is_none() && !edged {
        *size = Some(Amount::Pixels(measured.into()));
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
    let across_edged = form.absolute && both(form.written.left.clone(), form.written.right.clone());
    let down_edged = form.absolute && both(form.written.top.clone(), form.written.bottom.clone());
    let column = parent.direction == Direction::Column;
    Measured {
        width: form.written.width.is_none() && !across_edged && !(stretched && column),
        height: form.written.height.is_none() && !down_edged && !(stretched && !column),
    }
}

/// Which axes of a box or a text that is a cell of a set of columns have no size of their own (`render-command.md`
/// section 4.4, rule 2). A cell that is not absolute is as wide as its column, which the set gives it, and is as high as
/// its row unless it does not stretch across; an absolute cell takes the size its own children give it on an axis that
/// its edges do not give.
fn set_measured_axes(form: &Form, set: &Columns) -> Measured {
    let stretched = !form.absolute && form.across.unwrap_or(set.across) == Across::Stretch;
    let across_edged = form.absolute && both(form.written.left.clone(), form.written.right.clone());
    let down_edged = form.absolute && both(form.written.top.clone(), form.written.bottom.clone());
    Measured {
        width: form.absolute && form.written.width.is_none() && !across_edged,
        height: form.written.height.is_none() && !down_edged && !stretched,
    }
}

/// A place or a size as written (`notation.md` section 4): a length. A length in pixels is a whole
/// number within the limit (from -1000000 to 1000000 for an edge, from 0 to 1000000 for a size); a
/// percentage is any value of its kind; a size below 0 is wrong (N6), whatever its form.
fn amount(length: Option<Length>, value: &str, name: &str, tag: &str) -> Result<Amount, (Code, String)> {
    let sized = matches!(name, "width" | "height");
    match length {
        None => Err((Code::LengthNotAccepted, not_a_length(name, value))),
        Some(Length::Pixels(number) | Length::Percent(number)) if sized && number.to_f64() < 0.0 => Err((
            Code::LengthNotAccepted,
            format!("`{name}=\"{value}\"` is below 0; a size is 0 or more"),
        )),
        Some(Length::Pixels(number)) if number.is_whole() && number.to_f64().abs() <= LIMIT as f64 => {
            Ok(Amount::Pixels(decimal(&number, name, value)?))
        }
        Some(Length::Percent(number)) => Ok(Amount::Percent(decimal(&number, name, value)?)),
        Some(Length::Pixels(number)) if !number.is_whole() => Err((
            Code::FormNotSupported,
            Unsupported::FractionOfAPixel.message(&format!("{tag} has `{name}=\"{value}\"`")),
        )),
        Some(_) => Err((
            Code::FormNotSupported,
            Unsupported::BeyondTheLimit.message(&format!("{tag} has `{name}=\"{value}\"`, beyond {LIMIT} pixels")),
        )),
    }
}

/// The exact decimal of a written length: every digit it is written with, and its exponent (`render-command.md`, section 4.1).
fn decimal(number: &Number, name: &str, value: &str) -> Result<layout::Decimal, (Code, String)> {
    layout::Decimal::from_parts(number.negative(), number.digits(), number.exponent())
        .ok_or_else(|| (Code::LengthNotAccepted, not_a_length(name, value)))
}

fn not_a_length(name: &str, value: &str) -> String {
    format!("`{name}=\"{value}\"` is not a length: a number of pixels or a percentage, such as 40 or 50%")
}

/// The message of an element written in a place where this version does not draw it, with the entry of the support table
/// that names it: the elements this version does not read at all, a `<tween>` or a `<set>` inside a span, a shown composition
/// or footage, and any other element inside a shown composition or a text.
fn unsupported_inside(element: Element, parent: Element) -> String {
    let entry = match (element, parent) {
        (Element::Rect, _) => Unsupported::Rect,
        (Element::Image, _) => Unsupported::StillPictures,
        (Element::Shape, _) => Unsupported::Shapes,
        (Element::Tween | Element::Set, Element::Span) => Unsupported::CountingTypingSpanMotion,
        (Element::Tween | Element::Set, Element::Nested | Element::Video) => Unsupported::PictureMotion,
        _ => Unsupported::ElementInsideLeaf,
    };
    let what = match parent {
        Element::Composition => format!("the composition holds {}", element.tag()),
        _ => format!("{} holds {}", parent.tag(), element.tag()),
    };
    entry.message(&what)
}

/// The forms of the support table written on an element that is itself not drawn: a mask made from a picture (S18), and
/// each attribute that an entry names on that element. Each is given with its line and its message.
fn forms_of_unsupported(node: &Node) -> Vec<(u32, String)> {
    let mut found = Vec::new();
    for attribute in node.attributes() {
        let entry = if attribute.name() == "mask" {
            picture_mask(attribute.value()).then_some(Unsupported::PictureMask)
        } else {
            support::attribute_entry(node.element(), attribute.name())
        };
        if let Some(entry) = entry {
            found.push((attribute.line(), entry.message(&support::written(node.element(), attribute))));
        }
    }
    found
}

/// Whether an element places what it holds in a row, a column or a set of columns.
fn in_flow(node: &Node) -> bool {
    node.attribute("flex-direction").is_some() || node.attribute("columns").is_some()
}

/// Whether a `mask` names a picture and not a gradient (support table S18): it ends in the name of a picture file.
fn picture_mask(value: &str) -> bool {
    let value = value.trim().to_ascii_lowercase();
    !value.contains("gradient(")
        && [".png", ".jpg", ".jpeg", ".webp", ".gif", ".avif", ".heic"]
            .iter()
            .any(|kind| value.ends_with(kind))
}

/// The message for a `volume` on an `<audio>`, which this version does not apply (`docs/spec/sound.md` section 1).
fn unapplied_volume(attribute: &Attribute) -> String {
    let message = format!(
        "`volume=\"{}\"` on <audio> is not applied by this version, which plays a sound at the level of its file",
        attribute.shown()
    );
    match attribute.given_by() {
        None => message,
        Some(style) => format!("{message}, which the style `{}` gives it", project::shown(style)),
    }
}

/// The code and the message of an attribute this version does not take on its element, with the entry of the support table
/// that names it (`support::refused_attribute`).
fn unsupported_attribute(node: &Node, attribute: &Attribute) -> (Code, String) {
    support::refused_attribute(node.element(), attribute.name(), support::written(node.element(), attribute))
}

/// The message of a background of the root that is not opaque (support table S61).
fn unsupported_colour(node: &Node, attribute: &Attribute) -> String {
    Unsupported::AlphaOutsideBoxBackground.message(&support::written(node.element(), attribute))
}

/// Settings in use that this version does not apply, each with the entry of the support table that names it: a film whose
/// path does not end in `.mp4` or `.m4v` (S34), an encoder or a quality other than the hardware encoder at `standard`
/// (S49), more than one worker (S50), and a sound bit rate outside 64 to 320 kbit/s (S62). A value an option gave is
/// reported on the command line; a value of the project file on the line of the value.
fn settings(project: &Project) -> Vec<Report> {
    let settings = project.settings();
    let mut from_options: Vec<String> = Vec::new();
    let mut found: Vec<(u32, String)> = Vec::new();
    let mut check = |key: &str, option: Option<&str>, accepted: bool, value: String, entry: Unsupported| {
        if accepted {
            return;
        }
        if let (true, Some(option)) = (settings.from_option(key), option) {
            from_options.push(entry.message(&format!("`{option} {value}` is given")));
        } else if let Some(line) = settings.line_of(key) {
            found.push((line, entry.message(&format!("`{key}: {value}` is written"))));
        }
    };
    // A film is written as an MP4 file: its path ends in `.mp4` or `.m4v`, in any case (support table S34).
    let output = project.output();
    let kind = std::path::Path::new(&output)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase);
    check(
        "render.output",
        Some("--output"),
        matches!(kind.as_deref(), Some("mp4" | "m4v")),
        output.clone(),
        Unsupported::FilmNotMp4,
    );
    check(
        "render.quality",
        Some("--quality"),
        settings.quality() == Quality::Standard,
        settings.quality().to_string(),
        Unsupported::EncoderOrQuality,
    );
    check(
        "render.encoder",
        Some("--encoder"),
        settings.encoder() == Encoder::Hardware,
        settings.encoder().to_string(),
        Unsupported::EncoderOrQuality,
    );
    check(
        "render.workers",
        Some("--workers"),
        matches!(settings.workers(), Workers::Auto | Workers::Count(1)),
        settings.workers().to_string(),
        Unsupported::Workers,
    );
    check(
        "render.audio-bitrate",
        // No option sets the sound's bit rate: only the project file does.
        None,
        (SOUND_KBITS_LOWEST..=SOUND_KBITS_HIGHEST).contains(&settings.audio_bitrate()),
        settings.audio_bitrate().to_string(),
        Unsupported::SoundBitRate,
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
