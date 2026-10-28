//! Dumps the display list of frames of a fixture as JSON, with the font files its glyph runs use: the input that the model of
//! the checked-in pictures of text reads (`evidence/textgaps/`). A development tool only: it is not a command of `framesmith`
//! and no part of the product. It panics on a mark it does not dump, so that the model is never given a list it cannot read.
//!
//! `cargo run -p render --example dump_frame_marks -- <fixture> <folder> <frame>...` writes `<folder>/frame-NNNN.json` for each
//! frame, and each font file it names, as `<folder>/fonts/font-N.ttf`.

use draw::{Colour, Frame, GlyphRun, Mark, Shadow, Step, TextMark};
use project::Project;
use render::Scene;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;
use std::sync::Arc;

/// The font files written so far, by the address of each font's data. The map holds a clone of that data, so the address is
/// not reused while the map lives.
type FontFiles = HashMap<*const Vec<u8>, (Arc<Vec<u8>>, String)>;

fn main() {
    let mut words = std::env::args().skip(1);
    let (Some(name), Some(folder)) = (words.next(), words.next()) else {
        eprintln!("usage: dump_frame_marks <fixture> <folder> <frame>...");
        std::process::exit(2);
    };
    let numbers: Vec<u64> = words
        .map(|word| word.parse().unwrap_or_else(|_| panic!("`{word}` is not a frame number")))
        .collect();
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures").join(&name);
    let project = Project::open(&source).unwrap_or_else(|error| panic!("{error}"));
    let scene = Scene::from_project(&project).unwrap_or_else(|reports| panic!("{reports:?}"));
    let folder = Path::new(&folder);
    fs::create_dir_all(folder.join("fonts")).unwrap();
    let mut fonts: FontFiles = HashMap::new();
    for number in numbers {
        let frame = scene.frame(number).unwrap();
        let text = frame_json(&frame, folder, &mut fonts);
        let path = folder.join(format!("frame-{number:04}.json"));
        fs::write(&path, text).unwrap();
        println!("{}", path.display());
    }
}

/// The frame as JSON: its size, its background, and its marks in order.
fn frame_json(frame: &Frame, folder: &Path, fonts: &mut FontFiles) -> String {
    let marks: Vec<String> = frame.marks.iter().map(|mark| mark_json(mark, folder, fonts)).collect();
    format!(
        "{{\"width\": {}, \"height\": {}, \"background\": {}, \"marks\": [{}]}}\n",
        frame.width,
        frame.height,
        colour_json(frame.background),
        marks.join(", ")
    )
}

fn mark_json(mark: &Mark, folder: &Path, fonts: &mut FontFiles) -> String {
    match mark {
        Mark::Step(Step::Fill { rect, colour }) => format!(
            "{{\"kind\": \"fill\", \"rect\": [{}, {}, {}, {}], \"colour\": {}}}",
            rect.left,
            rect.top,
            rect.width,
            rect.height,
            colour_json(*colour)
        ),
        Mark::Text(text) => text_json(text, folder, fonts),
        other => panic!("the dump has no mark of this kind: {}", kind_of(other)),
    }
}

fn kind_of(mark: &Mark) -> &'static str {
    match mark {
        Mark::Step(_) => "step",
        Mark::Text(_) => "text",
        Mark::Group(_) => "group",
        Mark::Background(_) => "background",
        Mark::Ring(_) => "ring",
        Mark::Cut(_) => "cut",
        Mark::BoxShadow(_) => "box-shadow",
    }
}

fn text_json(text: &TextMark, folder: &Path, fonts: &mut FontFiles) -> String {
    assert!(text.pictures.is_empty(), "the dump has no colour pictures of emoji");
    let shadows: Vec<String> = text.shadows.iter().map(shadow_json).collect();
    let outline = match &text.outline {
        None => "null".to_string(),
        Some(outline) => format!("{{\"width\": {}, \"colour\": {}}}", outline.width, colour_json(outline.colour)),
    };
    let clip = match &text.clip {
        None => "null".to_string(),
        Some(clip) => format!("[{}, {}, {}, {}]", clip.left, clip.top, clip.width, clip.height),
    };
    let runs: Vec<String> = text.runs.iter().map(|run| run_json(run, folder, fonts)).collect();
    format!(
        "{{\"kind\": \"text\", \"clip\": {clip}, \"shadows\": [{}], \"outline\": {outline}, \"runs\": [{}]}}",
        shadows.join(", "),
        runs.join(", ")
    )
}

fn shadow_json(shadow: &Shadow) -> String {
    format!(
        "{{\"across\": {}, \"down\": {}, \"blur\": {}, \"colour\": {}}}",
        shadow.across,
        shadow.down,
        shadow.blur,
        colour_json(shadow.colour)
    )
}

fn run_json(run: &GlyphRun, folder: &Path, fonts: &mut FontFiles) -> String {
    let key = Arc::as_ptr(&run.font.data);
    let next = fonts.len();
    let file = fonts
        .entry(key)
        .or_insert_with(|| {
            let file = format!("fonts/font-{next}.ttf");
            fs::write(folder.join(&file), run.font.data.as_slice()).unwrap();
            (Arc::clone(&run.font.data), file)
        })
        .1
        .clone();
    let mut glyphs = String::new();
    for (at, glyph) in run.glyphs.iter().enumerate() {
        if at > 0 {
            glyphs.push_str(", ");
        }
        write!(glyphs, "[{}, {}, {}]", glyph.id, glyph.x, glyph.y).unwrap();
    }
    format!(
        "{{\"font\": \"{file}\", \"index\": {}, \"size\": {}, \"skew\": {}, \"colour\": {}, \"glyphs\": [{glyphs}]}}",
        run.font.index,
        run.size,
        run.skew_degrees,
        colour_json(run.colour)
    )
}

fn colour_json(colour: Colour) -> String {
    format!("[{}, {}, {}]", colour.red, colour.green, colour.blue)
}
