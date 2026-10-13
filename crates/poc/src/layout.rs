//! Layout: the box of every element, and the place of every glyph. Done once when the
//! composition is loaded.
//!
//! An element is placed in one of three ways: by `left`, `top`, `right`, `bottom` against its
//! parent's box; as an item of a parent that is a row or a column; or, for a `<span>`, in the
//! lines of its text.

use crate::anim::Prop;
use crate::model::{Align, Column, Columns, Composition, Direction, Flex, GlyphRun, Inline, Kind, Node, TextAlign, TypedLayout};
use crate::scene;
use crate::text::{Flowed, FlowedLine, Piece, Shaper};
use std::collections::BTreeSet;
use timebase::Time;
use vello_cpu::kurbo::{Rect, Vec2};

struct Context<'a> {
    shaper: &'a mut Shaper,
    /// The size in pixels of each still picture.
    stills: &'a [(u32, u32)],
}

/// Lays out the whole composition. `stills` holds the size of each still picture, in the order
/// of `Composition::stills`.
pub fn lay_out(composition: &mut Composition, shaper: &mut Shaper, stills: &[(u32, u32)]) {
    let mut context = Context { shaper, stills };
    let frame = (f64::from(composition.width), f64::from(composition.height));
    let frames = timebase::frame_count(composition.duration, composition.fps) as i64;
    for child in &mut composition.root.children {
        numbers(child, Time::ZERO, (0, frames), composition.fps);
        size(child, None, None, frame, &mut context);
        place(child, frame);
        isolate(child);
    }
}

/// Finds, for every text that counts, each number it shows in a frame of the composition, as
/// the text writes it. `offset` is when the time of the node's parent begins, and `shown` the
/// frames the parent is there for, from the first to before the last.
fn numbers(node: &mut Node, offset: Time, shown: (i64, i64), fps: u32) {
    let mut shown = shown;
    let mut inner = offset;
    if let Some((start, duration)) = node.window {
        inner = offset + start;
        shown = (shown.0.max(inner.grid_frame(fps)), shown.1.min((inner + duration).grid_frame(fps)));
    }
    if let Some(format) = node.count {
        let mut written: BTreeSet<String> = (shown.0..shown.1)
            .map(|index| format.write(scene::values(node, scene::moment(index, fps, offset))[Prop::Count.index()]))
            .collect();
        if written.is_empty() {
            written.insert(format.write(node.base[Prop::Count.index()]));
        }
        node.counted = written.into_iter().map(|number| (number, Vec::new())).collect();
    }
    for child in &mut node.children {
        numbers(child, inner, shown, fps);
    }
}

/// Marks the elements that are drawn apart because something inside them blends. Returns
/// whether `node` or anything in it blends.
fn isolate(node: &mut Node) -> bool {
    let mut blends_inside = false;
    for child in &mut node.children {
        blends_inside |= isolate(child);
    }
    let moving = [Prop::X, Prop::Y, Prop::Scale, Prop::ScaleX, Prop::Rotation, Prop::SkewY];
    let moved = moving
        .iter()
        .any(|prop| node.base[prop.index()] != prop.own_value() || node.tracks.iter().any(|track| track.prop == *prop));
    node.isolates = blends_inside && moved;
    blends_inside || node.look.blend != crate::model::Blend::Normal
}

/// Puts `node`, already sized, where its `left`, `top`, `right` and `bottom` say.
fn place(node: &mut Node, parent: (f64, f64)) {
    let (width, height) = (node.rect.width(), node.rect.height());
    let geometry = node.geometry;
    let x = match (geometry.left, geometry.right) {
        (Some(left), _) => left.of(parent.0),
        (None, Some(right)) => parent.0 - right.of(parent.0) - width,
        (None, None) => 0.0,
    };
    let y = match (geometry.top, geometry.bottom) {
        (Some(top), _) => top.of(parent.1),
        (None, Some(bottom)) => parent.1 - bottom.of(parent.1) - height,
        (None, None) => 0.0,
    };
    node.rect = Rect::new(x, y, x + width, y + height);
}

fn move_to(node: &mut Node, x: f64, y: f64) {
    node.rect = Rect::new(x, y, x + node.rect.width(), y + node.rect.height());
}

/// Sizes `node` and lays out everything inside it. `given` is a width or a height its parent
/// imposes when the element names none; `parent` is what percentages are taken of.
fn size(node: &mut Node, given_width: Option<f64>, given_height: Option<f64>, parent: (f64, f64), context: &mut Context) {
    let geometry = node.geometry;
    let between = |near: Option<crate::model::Dim>, far: Option<crate::model::Dim>, whole: f64| match (near, far) {
        (Some(near), Some(far)) => Some(whole - near.of(whole) - far.of(whole)),
        _ => None,
    };
    // A width that is not the element's own is never above its `max-width`.
    let cap = geometry.max_width.filter(|_| geometry.width.is_none());
    let capped = |width: f64| cap.map_or(width, |cap| width.min(cap));
    let width = geometry
        .width
        .map(|width| width.of(parent.0))
        .or(given_width.map(capped))
        .or_else(|| between(geometry.left, geometry.right, parent.0).map(capped));
    let height = geometry
        .height
        .map(|height| height.of(parent.1))
        .or(given_height)
        .or_else(|| between(geometry.top, geometry.bottom, parent.1));
    let (width, height) = match &node.kind {
        Kind::Box => match (node.flow.columns.clone(), node.flow.flex) {
            (Some(columns), _) => columns_layout(node, &columns, width, height, context),
            (None, Some(flex)) => flex_layout(node, flex, width, height, context),
            (None, None) => {
                let size = (width.unwrap_or(parent.0), height.unwrap_or(parent.1));
                for child in &mut node.children {
                    self::size(child, None, None, size, context);
                    place(child, size);
                }
                size
            }
        },
        // A text that counts: one line, in a box as wide as the widest number it shows, so
        // that what stands beside it keeps still.
        Kind::Text if node.count.is_some() => {
            let padding = node.flow.padding;
            let across = padding[1] + padding[3];
            let numbers: Vec<String> = node.counted.iter().map(|(number, _)| number.clone()).collect();
            let mut widest = 0.0_f64;
            let mut tall = (0.0, 0.0);
            for number in &numbers {
                node.inline = vec![Inline::Words(number.clone())];
                let lines = inline(node, None, None, context);
                widest = widest.max(lines.width);
                tall = (lines.height, lines.above);
            }
            let room = width.map_or(widest, |width| (width - across).max(0.0));
            for (place, number) in numbers.iter().enumerate() {
                node.inline = vec![Inline::Words(number.clone())];
                node.counted[place].1 = inline(node, Some(room), None, context).runs;
            }
            node.runs = node.counted.first().map(|(_, runs)| runs.clone()).unwrap_or_default();
            node.baseline = padding[0] + tall.1;
            (width.unwrap_or(widest + across), height.unwrap_or(tall.0 + padding[0] + padding[2]))
        }
        Kind::Text | Kind::Span => {
            let padding = node.flow.padding;
            let across = padding[1] + padding[3];
            let limit = width.or(cap).map(|width| (width - across).max(0.0));
            let lines = inline(node, limit, None, context);
            // A text held only by its `max-width` is as wide as its lines.
            let limit = limit.filter(|_| width.is_some());
            let size = (
                width.unwrap_or(lines.width + across),
                height.unwrap_or(lines.height + padding[0] + padding[2]),
            );
            node.baseline = padding[0] + lines.above;
            // A text that is typed has a layout for every count of characters shown.
            if node.tracks.iter().any(|track| track.prop == Prop::Typed) {
                let all = own_chars(node);
                node.typed = (0..=all)
                    .map(|shown| {
                        let lines = inline(node, limit.or(Some(lines.width)), Some(shown), context);
                        Some(TypedLayout {
                            runs: lines.runs,
                            children: lines.children,
                        })
                    })
                    .collect();
            }
            let whole = inline(node, limit.or(Some(lines.width)), None, context);
            node.runs = whole.runs;
            for (child, rect) in node.children.iter_mut().zip(whole.children) {
                child.rect = rect;
            }
            size
        }
        Kind::Image { still, .. } => {
            let (natural_width, natural_height) = context.stills.get(*still).copied().unwrap_or((1, 1));
            let ratio = f64::from(natural_height) / f64::from(natural_width.max(1));
            match (width, height) {
                (Some(width), Some(height)) => (width, height),
                (Some(width), None) => (width, width * ratio),
                (None, Some(height)) => (height / ratio.max(1e-9), height),
                (None, None) => (f64::from(natural_width), f64::from(natural_height)),
            }
        }
        Kind::Video { .. } | Kind::Shape { .. } => (width.unwrap_or(0.0), height.unwrap_or(0.0)),
    };
    node.rect = Rect::new(0.0, 0.0, width, height);
}

/// A row or a column. Returns the size of the container.
fn flex_layout(node: &mut Node, flex: Flex, width: Option<f64>, height: Option<f64>, context: &mut Context) -> (f64, f64) {
    let padding = node.flow.padding;
    let column = flex.direction == Direction::Column;
    let inner_width = width.map(|width| width - padding[1] - padding[3]);
    let inner_height = height.map(|height| height - padding[0] - padding[2]);
    // "Main" is the direction the items follow each other in; "cross" is the other.
    let (inner_main, inner_cross) = if column {
        (inner_height, inner_width)
    } else {
        (inner_width, inner_height)
    };
    let percent_base = (inner_width.unwrap_or(0.0), inner_height.unwrap_or(0.0));
    let stretches = |child: &Node| child.flow.align_self.unwrap_or(flex.align) == Align::Stretch;
    let measure = |child: &mut Node, main: Option<f64>, cross: Option<f64>, context: &mut Context| {
        if column {
            size(child, cross, main, percent_base, context);
        } else {
            size(child, main, cross, percent_base, context);
        }
    };
    let main_of = |child: &Node| {
        let margin = child.flow.margin;
        if column {
            child.rect.height() + margin[0] + margin[2]
        } else {
            child.rect.width() + margin[1] + margin[3]
        }
    };
    let cross_of = |child: &Node| {
        let margin = child.flow.margin;
        if column {
            child.rect.width() + margin[1] + margin[3]
        } else {
            child.rect.height() + margin[0] + margin[2]
        }
    };
    for child in node.children.iter_mut().filter(|child| !child.flow.absolute) {
        let cross = inner_cross.filter(|_| stretches(child));
        measure(child, None, cross, context);
        // An item that does not stretch is as wide as its content, but no wider than the
        // container: a text then breaks into lines.
        if let (Some(room), None, true) = (inner_cross, cross, column) {
            if child.rect.width() + child.flow.margin[1] + child.flow.margin[3] > room + 0.01 && child.geometry.width.is_none() {
                measure(child, None, Some(room - child.flow.margin[1] - child.flow.margin[3]), context);
            }
        }
    }
    let items = node.children.iter().filter(|child| !child.flow.absolute).count();
    let gaps = flex.gap * items.saturating_sub(1) as f64;
    let in_flow = |node: &Node| -> (f64, f64) {
        let flowing = node.children.iter().filter(|child| !child.flow.absolute);
        flowing.fold((gaps, 0.0), |(main, cross), child| {
            (main + main_of(child), f64::max(cross, cross_of(child)))
        })
    };
    let (mut content_main, content_cross) = in_flow(node);
    // In a row, the items that share a baseline: how far each is moved down for it.
    let shares = |child: &Node| !column && child.flow.align_self.unwrap_or(flex.align) == Align::Baseline;
    let above = |child: &Node| child.flow.margin[0] + first_baseline(child);
    let line = |node: &Node| {
        let sharing = node.children.iter().filter(|child| !child.flow.absolute && shares(child));
        sharing.map(above).fold(0.0, f64::max)
    };
    let lowered = {
        let line = line(node);
        let sharing = node.children.iter().filter(|child| !child.flow.absolute && shares(child));
        sharing.map(|child| line - above(child) + cross_of(child)).fold(0.0, f64::max)
    };
    let cross_size = inner_cross.unwrap_or(content_cross.max(lowered));
    // An item that stretches takes the container's cross size once that is known.
    if inner_cross.is_none() {
        for child in node.children.iter_mut().filter(|child| !child.flow.absolute) {
            if stretches(child) {
                measure(child, None, Some(cross_size), context);
            }
        }
    }
    // An item that grows shares out what is left of the main size.
    let growth: f64 = node
        .children
        .iter()
        .filter(|child| !child.flow.absolute)
        .map(|child| child.flow.grow)
        .sum();
    if let (Some(inner_main), true) = (inner_main, growth > 0.0) {
        let spare = (inner_main - content_main).max(0.0);
        for child in node
            .children
            .iter_mut()
            .filter(|child| !child.flow.absolute && child.flow.grow > 0.0)
        {
            let own = if column { child.rect.height() } else { child.rect.width() };
            let cross = stretches(child).then_some(cross_size);
            measure(child, Some(own + spare * child.flow.grow / growth), cross, context);
        }
        content_main = in_flow(node).0;
    }
    let main_size = inner_main.unwrap_or(content_main);
    let spare = main_size - content_main;
    // The room left over: before the items, and between each two of them.
    let (mut along, between) = match flex.justify {
        Align::Centre => (spare / 2.0, 0.0),
        Align::End => (spare, 0.0),
        Align::Between if items > 1 && spare > 0.0 => (0.0, spare / (items - 1) as f64),
        _ => (0.0, 0.0),
    };
    let line = line(node);
    for child in node.children.iter_mut().filter(|child| !child.flow.absolute) {
        let margin = child.flow.margin;
        let (before, after, cross_before) = if column {
            (margin[0], margin[2], margin[3])
        } else {
            (margin[3], margin[1], margin[0])
        };
        let own_cross = cross_of(child);
        let across = match child.flow.align_self.unwrap_or(flex.align) {
            Align::Centre => (cross_size - own_cross) / 2.0,
            Align::End => cross_size - own_cross,
            Align::Baseline if !column => line - above(child),
            _ => 0.0,
        };
        along += before;
        let (main_at, cross_at) = (along, across + cross_before);
        if column {
            move_to(child, padding[3] + cross_at, padding[0] + main_at);
            along += child.rect.height();
        } else {
            move_to(child, padding[3] + main_at, padding[0] + cross_at);
            along += child.rect.width();
        }
        along += after + flex.gap + between;
    }
    let size = if column {
        (
            width.unwrap_or(cross_size + padding[1] + padding[3]),
            height.unwrap_or(main_size + padding[0] + padding[2]),
        )
    } else {
        (
            width.unwrap_or(main_size + padding[1] + padding[3]),
            height.unwrap_or(cross_size + padding[0] + padding[2]),
        )
    };
    for child in node.children.iter_mut().filter(|child| child.flow.absolute) {
        self::size(child, None, None, size, context);
        place(child, size);
    }
    size
}

/// From the top of an element's box to the baseline of its first line. A row, a column or a set
/// of columns has that of its first item; what holds no line has it at its bottom edge.
fn first_baseline(node: &Node) -> f64 {
    match node.kind {
        Kind::Text | Kind::Span => node.baseline,
        Kind::Box if node.flow.flex.is_some() || node.flow.columns.is_some() => {
            let first = node.children.iter().find(|child| !child.flow.absolute);
            first.map_or(node.rect.height(), |child| child.rect.y0 + first_baseline(child))
        }
        _ => node.rect.height(),
    }
}

/// Columns, filled row by row. Returns the size of the container.
fn columns_layout(node: &mut Node, columns: &Columns, width: Option<f64>, height: Option<f64>, context: &mut Context) -> (f64, f64) {
    let padding = node.flow.padding;
    let inner_width = width.map(|width| width - padding[1] - padding[3]);
    let inner_height = height.map(|height| height - padding[0] - padding[2]);
    let count = columns.widths.len();
    let items: Vec<usize> = (0..node.children.len())
        .filter(|index| !node.children[*index].flow.absolute)
        .collect();
    let across = |child: &Node| child.flow.margin[1] + child.flow.margin[3];
    let upright = |child: &Node| child.flow.margin[0] + child.flow.margin[2];
    // The widths: a column of pixels as written; a share of what those and the gaps leave, or,
    // in a box with no width, as wide as its widest item.
    let gaps = columns.gap * count.saturating_sub(1) as f64;
    let fixed: f64 = columns
        .widths
        .iter()
        .map(|column| if let Column::Px(px) = column { *px } else { 0.0 })
        .sum();
    let shares: f64 = columns
        .widths
        .iter()
        .map(|column| if let Column::Share(share) = column { *share } else { 0.0 })
        .sum();
    let mut widths: Vec<f64> = Vec::with_capacity(count);
    for (place, column) in columns.widths.iter().enumerate() {
        widths.push(match (column, inner_width) {
            (Column::Px(px), _) => *px,
            (Column::Share(share), Some(inner)) => (inner - fixed - gaps).max(0.0) * share / shares,
            (Column::Share(_), None) => {
                let mut widest = 0.0_f64;
                for index in items.iter().skip(place).step_by(count) {
                    let child = &mut node.children[*index];
                    size(child, None, None, (0.0, 0.0), context);
                    widest = widest.max(child.rect.width() + across(child));
                }
                widest
            }
        });
    }
    for (place, index) in items.iter().enumerate() {
        let child = &mut node.children[*index];
        let room = widths[place % count];
        size(child, Some((room - across(child)).max(0.0)), None, (room, 0.0), context);
    }
    // A row is as high as its tallest item; what a box with a height has left is shared out.
    let mut heights: Vec<f64> = items
        .chunks(count)
        .map(|row| {
            row.iter()
                .map(|index| node.children[*index].rect.height() + upright(&node.children[*index]))
                .fold(0.0, f64::max)
        })
        .collect();
    let row_gaps = columns.gap * heights.len().saturating_sub(1) as f64;
    if let Some(inner) = inner_height {
        let spare = inner - heights.iter().sum::<f64>() - row_gaps;
        if spare > 0.0 {
            let rows = heights.len() as f64;
            heights.iter_mut().for_each(|height| *height += spare / rows);
        }
    }
    let mut top = padding[0];
    for (row, members) in items.chunks(count).enumerate() {
        let mut left = padding[3];
        for (place, index) in members.iter().enumerate() {
            let child = &mut node.children[*index];
            let align = child.flow.align_self.unwrap_or(columns.align);
            if align == Align::Stretch && child.geometry.height.is_none() {
                let room = widths[place];
                size(
                    child,
                    Some((room - across(child)).max(0.0)),
                    Some((heights[row] - upright(child)).max(0.0)),
                    (room, heights[row]),
                    context,
                );
            }
            let own = child.rect.height() + upright(child);
            let down = match align {
                Align::Centre => (heights[row] - own) / 2.0,
                Align::End => heights[row] - own,
                _ => 0.0,
            };
            move_to(child, left + child.flow.margin[3], top + down + child.flow.margin[0]);
            left += widths[place] + columns.gap;
        }
        top += heights[row] + columns.gap;
    }
    let size = (
        width.unwrap_or(widths.iter().sum::<f64>() + gaps + padding[1] + padding[3]),
        height.unwrap_or(heights.iter().sum::<f64>() + row_gaps + padding[0] + padding[2]),
    );
    for child in node.children.iter_mut().filter(|child| child.flow.absolute) {
        self::size(child, None, None, size, context);
        place(child, size);
    }
    size
}

/// How many characters of its own a text holds, as typing counts them.
fn own_chars(node: &Node) -> usize {
    let words = node.inline.iter().filter_map(|part| match part {
        Inline::Words(words) => Some(words.trim_matches(['\n', '\r', '\t'])),
        Inline::Child(_) => None,
    });
    words.map(|words| words.chars().count()).sum()
}

struct Lines {
    /// The glyphs of the element's own words, placed relative to its box.
    runs: Vec<GlyphRun>,
    /// The boxes of the children, in their order.
    children: Vec<Rect>,
    width: f64,
    height: f64,
    /// From the top of the first line to its baseline.
    above: f64,
}

/// Flows the words and the spans of a text into lines no wider than `limit`. With `shown`, only
/// that many of the text's own characters are there.
fn inline(node: &mut Node, limit: Option<f64>, shown: Option<usize>, context: &mut Context) -> Lines {
    let strut = context.shaper.line_box(&node.text);
    // The words and the spans as one paragraph: white space between words is one space, and a
    // span is a box at its place.
    let collapses = |c: char| matches!(c, ' ' | '\t' | '\n' | '\r' | '\x0c');
    let mut text = String::new();
    // Each span: where it stands in the text and its outer width; then its place among the
    // children and how far its box reaches above and below the baseline.
    let mut boxes: Vec<(usize, f64)> = Vec::new();
    let mut spans: Vec<(usize, f64, f64)> = Vec::new();
    let mut left = shown.unwrap_or(usize::MAX);
    // Whether a space is owed before the next word or span.
    let mut gap = false;
    for part in &node.inline {
        match part {
            Inline::Words(words) => {
                let words = words.trim_matches(['\n', '\r', '\t']);
                let words: String = words.chars().take(left).collect();
                left = left.saturating_sub(words.chars().count());
                let words = if node.text.upper { words.to_uppercase() } else { words };
                let mut rest = words.as_str();
                while !rest.is_empty() {
                    let at = rest.find(collapses).unwrap_or(rest.len());
                    if at == 0 {
                        gap = true;
                        rest = rest.trim_start_matches(collapses);
                        continue;
                    }
                    if gap && !(text.is_empty() && boxes.is_empty()) {
                        text.push(' ');
                    }
                    gap = false;
                    text.push_str(&rest[..at]);
                    rest = &rest[at..];
                }
            }
            Inline::Child(index) => {
                let child = &mut node.children[*index];
                size(child, None, None, (0.0, 0.0), context);
                let margin = child.flow.margin;
                if gap && !(text.is_empty() && boxes.is_empty()) {
                    text.push(' ');
                }
                gap = false;
                boxes.push((text.len(), child.rect.width() + margin[1] + margin[3]));
                spans.push((*index, child.baseline, child.rect.height() - child.baseline));
            }
        }
    }
    let empty = text.is_empty() && boxes.is_empty();
    let wraps = limit.is_some() && !node.text.nowrap;
    let flowed = if text.is_empty() {
        // Spans alone, with nothing between them: one line, each span after the one before.
        let mut x = 0.0;
        let mut pieces = Vec::new();
        for (place, (_, width)) in boxes.iter().enumerate() {
            pieces.push(Piece::Box(place, x));
            x += width;
        }
        Flowed {
            lines: vec![FlowedLine { width: x, pieces }],
        }
    } else {
        context.shaper.flow(&text, &boxes, &node.text, limit.filter(|_| wraps))
    };
    let widest = flowed.lines.iter().map(|line| line.width).fold(0.0, f64::max);
    let room = limit.unwrap_or(widest);
    let padding = node.flow.padding;
    let mut runs = Vec::new();
    let mut children = vec![Rect::ZERO; node.children.len()];
    let mut top = 0.0;
    let mut first_above = strut.above;
    for (number, line) in flowed.lines.iter().enumerate() {
        let mut above = strut.above;
        let mut below = strut.below;
        for piece in &line.pieces {
            if let Piece::Box(place, _) = piece {
                above = above.max(spans[*place].1);
                below = below.max(spans[*place].2);
            }
        }
        if number == 0 {
            first_above = above;
        }
        let start = match node.text.align {
            TextAlign::Left => 0.0,
            TextAlign::Centre => (room - line.width) / 2.0,
            TextAlign::Right => room - line.width,
        };
        let baseline = top + above;
        for piece in &line.pieces {
            match piece {
                Piece::Run(run) => {
                    let offset = Vec2::new(padding[3] + start, padding[0] + baseline);
                    let mut run = run.clone();
                    for glyph in &mut run.glyphs {
                        glyph.x += offset.x as f32;
                        glyph.y += offset.y as f32;
                    }
                    runs.push(run);
                }
                Piece::Box(place, x) => {
                    let (index, reach, _) = spans[*place];
                    let child = &node.children[index];
                    let left = padding[3] + start + x + child.flow.margin[3];
                    let top = padding[0] + baseline - reach;
                    children[index] = Rect::new(left, top, left + child.rect.width(), top + child.rect.height());
                }
            }
        }
        top += above + below;
    }
    Lines {
        runs,
        children,
        width: if empty { 0.0 } else { widest },
        height: if empty { 0.0 } else { top },
        above: first_above,
    }
}
