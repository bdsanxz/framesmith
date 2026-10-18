//! The frames of the script fixtures, drawn on the processor at frames 0 and 9, are judged by behaviour: the lines the
//! frame draws, the ink of its picture, the order of the letters, the joined forms of Arabic, and the breaks of Japanese
//! and Thai (`docs/spec/text.md`, sections 12.2 to 12.5). A picture is not compared byte for byte (decision A9). The
//! letters are named by the glyph each one has laid out alone, with the same engine that laid out the frame. The
//! fixtures draw with the machine's font, so this test is compiled for macOS only: that font is a file of the machine.

#![cfg(target_os = "macos")]

mod common;

use std::collections::HashMap;

use common::project_copied_from_fixture;
use draw::{Frame, Mark, Picture};
use draw_cpu::CpuRenderer;
use project::Project;
use render::Scene;
use text::{layout, Align, Fonts, LineHeight, Style};

const MAC_FAMILY: &str = "Arial Unicode";
const MAC_FILE: &str = "/System/Library/Fonts/Supplemental/Arial Unicode.ttf";

/// The frames judged: the first and the last, which are the same picture, since nothing moves.
const JUDGED: [u64; 2] = [0, 9];

/// The left edge of each text box and its width, in the fixtures (`fixtures/text-*/composition.xml`).
const BOX_LEFT: f32 = 10.0;
const BOX_WIDTH: f32 = 300.0;

/// A line of a frame: its baseline, and the glyphs it draws from the left.
#[derive(Debug, PartialEq)]
struct Line {
    baseline: f32,
    glyphs: Vec<u32>,
}

fn mac_fonts() -> Fonts {
    let bytes = std::fs::read(MAC_FILE).unwrap_or_else(|error| panic!("{MAC_FILE}: {error}"));
    let mut fonts = Fonts::new();
    fonts.register(bytes, Some(MAC_FAMILY)).expect("the machine's font registers");
    fonts
}

fn mac_style() -> Style {
    Style {
        families: vec![MAC_FAMILY.to_string()],
        size: 20.0,
        weight: 400.0,
        italic: false,
        line_height: LineHeight::Normal,
        letter_spacing: 0.0,
        align: Align::Left,
    }
}

/// The glyph ids a text draws when it is laid out alone, in the order they sit from the left.
fn drawn_alone(fonts: &mut Fonts, text: &str) -> Vec<u32> {
    let block = layout(fonts, text, &mac_style(), None).unwrap_or_else(|error| panic!("{text:?}: {error}"));
    let mut glyphs: Vec<_> = block.runs.iter().flat_map(|run| run.glyphs.iter().copied()).collect();
    glyphs.sort_by(|first, second| first.x.partial_cmp(&second.x).unwrap());
    glyphs.iter().map(|glyph| glyph.id).collect()
}

/// The character each glyph draws, for the characters of `text` and the space.
fn names_of(fonts: &mut Fonts, text: &str) -> HashMap<u32, char> {
    let mut names = HashMap::new();
    let space = drawn_alone(fonts, "a a")[1];
    names.insert(space, ' ');
    for character in text.chars().filter(|character| !character.is_whitespace()) {
        let glyphs = drawn_alone(fonts, &character.to_string());
        assert_eq!(glyphs.len(), 1, "{character:?} alone is one glyph");
        let previous = names.insert(glyphs[0], character);
        assert!(previous.is_none() || previous == Some(character), "{character:?} shares a glyph");
    }
    names
}

/// The lines of a frame's text, from the top line down; each lists its glyphs from the left.
fn frame_lines(frame: &Frame) -> Vec<Line> {
    let mut glyphs: Vec<(f32, f32, u32)> = Vec::new();
    for mark in &frame.marks {
        if let Mark::Text(text) = mark {
            for run in &text.runs {
                glyphs.extend(run.glyphs.iter().map(|glyph| (glyph.y, glyph.x, glyph.id)));
            }
        }
    }
    glyphs.sort_by(|first, second| {
        first
            .0
            .partial_cmp(&second.0)
            .unwrap()
            .then(first.1.partial_cmp(&second.1).unwrap())
    });
    let mut lines: Vec<Line> = Vec::new();
    for (baseline, _, id) in glyphs {
        match lines.last_mut() {
            Some(line) if (line.baseline - baseline).abs() < 0.01 => line.glyphs.push(id),
            _ => lines.push(Line {
                baseline,
                glyphs: vec![id],
            }),
        }
    }
    lines
}

/// The frame of a fixture at each judged number, with its picture drawn on the processor.
fn judged_frames(fixture: &str) -> Vec<(Frame, Picture)> {
    let folder = project_copied_from_fixture("script-fixtures-on-the-processor", fixture, fixture);
    let project = Project::open(&folder).unwrap();
    let scene = Scene::from_project(&project).unwrap_or_else(|reports| panic!("{reports:?}"));
    JUDGED
        .iter()
        .map(|&number| {
            let frame = scene.frame(number).unwrap();
            let picture = scene.picture(number, &mut CpuRenderer).unwrap();
            (frame, picture)
        })
        .collect()
}

/// Whether a pixel is dark: the text is black on white.
fn is_dark(pixel: &[u8]) -> bool {
    pixel[0] < 128 && pixel[1] < 128 && pixel[2] < 128
}

/// The rows of a picture that hold a dark pixel.
fn ink_rows(picture: &Picture) -> Vec<bool> {
    let width = picture.width() as usize;
    let pixels = picture.pixels();
    (0..picture.height() as usize)
        .map(|row| {
            pixels[row * width * 4..(row + 1) * width * 4]
                .as_chunks::<4>()
                .0
                .iter()
                .any(|pixel| is_dark(pixel))
        })
        .collect()
}

/// The leftmost and the rightmost column with ink.
fn ink_columns(picture: &Picture) -> (u32, u32) {
    let width = picture.width() as usize;
    let mut least = u32::MAX;
    let mut most = 0;
    for (index, pixel) in picture.pixels().as_chunks::<4>().0.iter().enumerate() {
        if is_dark(pixel) {
            let column = (index % width) as u32;
            least = least.min(column);
            most = most.max(column);
        }
    }
    (least, most)
}

/// Each line of a frame stands on its baseline, and its ink lies in its own band of rows: the band that reaches halfway
/// to the line above and halfway to the line below. The rows of one line are not always one unbroken run: a tone mark
/// or the dot of an i sits apart from its letter. So the test asks for ink somewhere in each band, and for ink in the
/// row just above each baseline, which is where the letters of the line stand.
fn check_ink_on_baselines(fixture: &str, number: usize, picture: &Picture, baselines: &[f32]) {
    let rows = ink_rows(picture);
    let height = picture.height() as f32;
    for (index, &baseline) in baselines.iter().enumerate() {
        let top = if index == 0 { 0.0 } else { (baselines[index - 1] + baseline) / 2.0 };
        let bottom = match baselines.get(index + 1) {
            Some(next) => (baseline + next) / 2.0,
            None => height,
        };
        let in_band = |row: usize| (row as f32 + 0.5) >= top && (row as f32 + 0.5) < bottom;
        let inked = (0..rows.len()).any(|row| in_band(row) && rows[row]);
        assert!(
            inked,
            "{fixture}: judged frame {number} has ink in the band of the line at {baseline}"
        );
        let above = baseline as usize - 1;
        assert!(
            rows[above],
            "{fixture}: judged frame {number} has ink in the row above the baseline {baseline}"
        );
    }
}

/// What every judged frame of a fixture shows: its lines stand on their baselines and have ink in their bands, the ink
/// stays in its box, and the frames are the same picture. Returns the glyphs of each line of the first frame.
fn judged_lines(fixture: &str) -> Vec<Vec<u32>> {
    let frames = judged_frames(fixture);
    let (first_frame, first_picture) = &frames[0];
    let lines = frame_lines(first_frame);
    assert!(!lines.is_empty(), "{fixture}: the frame draws its text");
    let baselines: Vec<f32> = lines.iter().map(|line| line.baseline).collect();
    for (number, (frame, picture)) in frames.iter().enumerate() {
        assert_eq!(frame_lines(frame), lines, "{fixture}: judged frame {number} draws the same lines");
        assert_eq!(
            picture.pixels(),
            first_picture.pixels(),
            "{fixture}: judged frame {number} is the same picture"
        );
        check_ink_on_baselines(fixture, number, picture, &baselines);
        let (least, most) = ink_columns(picture);
        assert!(
            (least as f32) >= BOX_LEFT - 1.0 && (most as f32) <= BOX_LEFT + BOX_WIDTH + 1.0,
            "{fixture}: the ink of judged frame {number} stays in its box, which is {least} to {most}"
        );
    }
    lines.into_iter().map(|line| line.glyphs).collect()
}

#[test]
fn cyrillic_frames_draw_whole_words_on_their_lines_in_reading_order() {
    const TEXT: &str = "Привет, мир! Это текст на русском языке.";
    let lines = judged_lines("text-cyrillic");
    let mut fonts = mac_fonts();
    let names = names_of(&mut fonts, TEXT);
    let words: Vec<&str> = TEXT.split(' ').collect();
    assert!(lines.len() > 1, "the paragraph breaks into lines: {}", lines.len());
    let mut at = 0;
    for line in &lines {
        let drawn: String = line.iter().map(|id| names[id]).collect();
        let line_words: Vec<&str> = drawn.trim().split(' ').collect();
        assert_eq!(
            &words[at..at + line_words.len()],
            line_words.as_slice(),
            "a line holds whole words, in reading order: {drawn:?}"
        );
        at += line_words.len();
    }
    assert_eq!(at, words.len(), "every word is on a line");
}

#[test]
fn arabic_frames_draw_each_word_right_to_left_with_its_joined_letters() {
    const TEXT: &str = "مرحبا بالعالم، هذا نص عربي قصير للتجربة.";
    let lines = judged_lines("text-arabic");
    let mut fonts = mac_fonts();
    let space = drawn_alone(&mut fonts, "a a")[1];
    let comma = drawn_alone(&mut fonts, "،")[0];
    let stop = drawn_alone(&mut fonts, ".")[0];
    let words: Vec<&str> = TEXT.split(' ').collect();
    let mut at = 0;
    for line in &lines {
        // A right-to-left line is drawn in the reverse of its words' order, each word drawn as it is alone. The
        // segments between the spaces, read from the right, are the words of the line in reading order.
        let segments: Vec<Vec<u32>> = line
            .split(|id| *id == space)
            .filter(|segment| !segment.is_empty())
            .map(|segment| segment.to_vec())
            .rev()
            .collect();
        for segment in segments {
            let word = words[at];
            // The letters are judged by their joined forms. A comma or a full stop is a neutral mark: its place comes from
            // the runs around it, so it is judged apart. A comma after a word of the right-to-left run sits at the left of
            // that word, and the full stop of the paragraph is judged at the right edge of the last line below.
            let letters: Vec<u32> = segment.iter().copied().filter(|id| *id != comma && *id != stop).collect();
            let joined = word.trim_end_matches(['،', '.']);
            assert_eq!(
                letters,
                drawn_alone(&mut fonts, joined),
                "the word {word:?} is drawn with its letters joined, as it is alone"
            );
            if word.ends_with('،') {
                assert_eq!(segment.first(), Some(&comma), "the comma of {word:?} sits at the left of that word");
            }
            at += 1;
        }
    }
    assert_eq!(at, words.len(), "every word is on a line, in reading order from the right");
    // The paragraph's base direction is left to right (`docs/spec/text.md`, section 12.2, rule 2), so its full stop, which
    // ends the paragraph, is drawn at the right edge of the last line, to the right of its last word.
    let last = lines.last().and_then(|line| line.last());
    assert_eq!(
        last,
        Some(&stop),
        "the full stop is drawn at the right edge of the last line, right of the last word"
    );
}

#[test]
fn hebrew_and_latin_frames_draw_each_run_in_its_direction_and_in_order() {
    const TEXT: &str = "Hello שלום World, שלום עולם again.";
    let lines = judged_lines("text-hebrew-latin");
    assert_eq!(lines.len(), 2, "the paragraph breaks into two lines");
    let mut fonts = mac_fonts();
    let names = names_of(&mut fonts, TEXT);
    let drawn: String = lines
        .iter()
        .flatten()
        .map(|id| names[id])
        .filter(|character| *character != ' ')
        .collect();
    // Each run in its own direction: the Latin runs in reading order, each Hebrew run reversed, in the order the runs
    // of the text take on a left-to-right line.
    let expected = "Hello\u{5dd}\u{5d5}\u{5dc}\u{5e9}World,\u{5dd}\u{5dc}\u{5d5}\u{5e2}\u{5dd}\u{5d5}\u{5dc}\u{5e9}again.";
    assert_eq!(drawn, expected);
}

#[test]
fn japanese_frames_break_between_characters_and_never_before_a_closing_mark() {
    const TEXT: &str = "日本語の文章を、改行の位置を確かめるために「区切り」の記号を含めて、正しく折り返すかを調べます。";
    let lines = judged_lines("text-japanese");
    let mut fonts = mac_fonts();
    let names = names_of(&mut fonts, TEXT);
    assert!(lines.len() > 1, "the paragraph breaks into lines: {}", lines.len());
    let texts: Vec<String> = lines.iter().map(|line| line.iter().map(|id| names[id]).collect()).collect();
    assert_eq!(texts.concat(), TEXT, "no character is lost or repeated");
    for (number, line) in texts.iter().enumerate() {
        if number > 0 {
            let first = line.chars().next().unwrap();
            assert!(!"。、」』）".contains(first), "line {number} starts with {first:?}");
        }
        if number + 1 < texts.len() {
            let last = line.chars().last().unwrap();
            assert!(!"「『（".contains(last), "line {number} ends with {last:?}");
        }
    }
}

#[test]
fn thai_frames_break_only_at_word_boundaries() {
    const WORDS: [&str; 11] = ["สวัสดี", "ชาว", "โลก", "ที่", "สวยงาม", "มาก", "และ", "ยินดี", "ต้อนรับ", "ทุก", "คน"];
    let lines = judged_lines("text-thai");
    let mut boundaries = Vec::new();
    let mut at = 0;
    for word in &WORDS[..WORDS.len() - 1] {
        at += word.chars().count();
        boundaries.push(at);
    }
    // Each character of this text is one glyph, so the glyphs of a line count its characters.
    let counts: Vec<usize> = lines.iter().map(Vec::len).collect();
    assert_eq!(
        counts.iter().sum::<usize>(),
        WORDS.concat().chars().count(),
        "one glyph per character: {counts:?}"
    );
    assert!(counts.len() > 1, "the paragraph breaks into lines: {counts:?}");
    let mut end = 0;
    for count in &counts[..counts.len() - 1] {
        end += count;
        assert!(
            boundaries.contains(&end),
            "a line ends after {end} characters, inside a word: {counts:?}"
        );
    }
}

/// The text of the emoji fixture: a Latin line with one emoji, which wraps at the spaces inside its width.
const EMOJI_FIXTURE: &str = "text-emoji";
const EMOJI_TEXT: &str = "Smile \u{1F600} now, a line of Latin text holds an emoji in the middle of its words, and the words wrap at the spaces inside the width.";
/// The lines of the emoji fixture as the rules give them, with the emoji left out and each run of spaces made one.
const EMOJI_LINES: [&str; 4] = [
    "Smile now, a line of Latin text",
    "holds an emoji in the middle of its",
    "words, and the words wrap at the",
    "spaces inside the width.",
];

/// The pictures of the emoji a frame draws, each with the place it is drawn at.
fn pictures_of(frame: &Frame) -> Vec<&draw::GlyphPicture> {
    let mut found = Vec::new();
    for mark in &frame.marks {
        if let Mark::Text(text) = mark {
            found.extend(text.pictures.iter());
        }
    }
    found
}

/// Whether a pixel is a yellow of the emoji face: red high, green high, blue low.
fn is_emoji_yellow(pixel: &[u8]) -> bool {
    pixel[0] >= 200 && pixel[1] >= 140 && pixel[2] <= 120
}

/// Whether a pixel is the blue of the text in the fixture, to within a wide margin.
fn is_text_blue(pixel: &[u8]) -> bool {
    let [red, green, blue] = [0x1e_i32, 0x6f, 0xd9];
    (i32::from(pixel[0]) - red).abs() < 40 && (i32::from(pixel[1]) - green).abs() < 40 && (i32::from(pixel[2]) - blue).abs() < 40
}

#[test]
fn emoji_frames_draw_the_emoji_in_its_own_colours_whatever_the_text_colour() {
    for (number, (frame, rendered)) in JUDGED.iter().zip(judged_frames(EMOJI_FIXTURE)) {
        let pictures = pictures_of(&frame);
        assert_eq!(pictures.len(), 1, "frame {number}: the one emoji is drawn as a picture");
        let emoji = pictures[0];
        let (left, top) = (emoji.x.round() as u32, emoji.y.round() as u32);
        let (mut yellow, mut blue) = (0_usize, 0_usize);
        for row in 0..emoji.height {
            for column in 0..emoji.width {
                let pixel = rendered.pixel(left + column, top + row).expect("the emoji lies on the frame");
                yellow += usize::from(is_emoji_yellow(&pixel));
                blue += usize::from(is_text_blue(&pixel));
            }
        }
        assert!(yellow > 50, "frame {number}: the emoji has yellow of its own, {yellow} pixels");
        assert_eq!(blue, 0, "frame {number}: no pixel of the emoji takes the colour of the text");
    }
}

/// The glyphs of a frame's text, each as its baseline, its pen position and its glyph id.
fn glyphs_of(frame: &Frame) -> Vec<(f32, f32, u32)> {
    let mut glyphs: Vec<(f32, f32, u32)> = Vec::new();
    for mark in &frame.marks {
        if let Mark::Text(text) = mark {
            for run in &text.runs {
                glyphs.extend(run.glyphs.iter().map(|glyph| (glyph.y, glyph.x, glyph.id)));
            }
        }
    }
    glyphs
}

/// The letters of the first line that lie where `keep` says, from the left, each named by its glyph.
fn letters_of_first_line(glyphs: &[(f32, f32, u32)], names: &HashMap<u32, char>, keep: impl Fn(f32) -> bool) -> String {
    let first = glyphs.iter().map(|glyph| glyph.0).fold(f32::MAX, f32::min);
    let mut on_line: Vec<(f32, u32)> = glyphs
        .iter()
        .filter(|glyph| (glyph.0 - first).abs() < 0.01 && names.contains_key(&glyph.2) && keep(glyph.1))
        .map(|glyph| (glyph.1, glyph.2))
        .collect();
    on_line.sort_by(|first, second| first.0.partial_cmp(&second.0).unwrap());
    on_line.iter().map(|(_, id)| names[id]).collect()
}

#[test]
fn emoji_frames_keep_the_emoji_between_the_words_it_sits_between() {
    let mut fonts = mac_fonts();
    let names = names_of(&mut fonts, EMOJI_TEXT.replace('\u{1F600}', "").as_str());
    for (number, (frame, _)) in JUDGED.iter().zip(judged_frames(EMOJI_FIXTURE)) {
        let emoji = pictures_of(&frame)[0];
        let glyphs = glyphs_of(&frame);
        let (left, right) = (emoji.x, emoji.x + emoji.width as f32);
        let before = letters_of_first_line(&glyphs, &names, |x| x < left);
        let after = letters_of_first_line(&glyphs, &names, |x| x >= right);
        assert_eq!(before.trim_end(), "Smile", "frame {number}: the letters before the emoji");
        assert!(
            after.trim_start().starts_with("now,"),
            "frame {number}: the letters after the emoji: {after:?}"
        );
    }
}

#[test]
fn emoji_frames_break_the_lines_at_the_spaces_the_width_allows() {
    let mut fonts = mac_fonts();
    let names = names_of(&mut fonts, EMOJI_TEXT.replace('\u{1F600}', "").as_str());
    for (number, (frame, _)) in JUDGED.iter().zip(judged_frames(EMOJI_FIXTURE)) {
        let mut glyphs = glyphs_of(&frame);
        glyphs.sort_by(|first, second| {
            first
                .0
                .partial_cmp(&second.0)
                .unwrap()
                .then(first.1.partial_cmp(&second.1).unwrap())
        });
        let mut lines: Vec<(f32, String)> = Vec::new();
        for (baseline, _, id) in glyphs {
            let Some(&character) = names.get(&id) else {
                continue;
            };
            match lines.last_mut() {
                Some((line, text)) if (*line - baseline).abs() < 0.01 => text.push(character),
                _ => lines.push((baseline, character.to_string())),
            }
        }
        let texts: Vec<String> = lines
            .iter()
            .map(|(_, text)| text.split_whitespace().collect::<Vec<_>>().join(" "))
            .collect();
        assert_eq!(
            texts, EMOJI_LINES,
            "frame {number}: the lines the rules give, from the emoji fixture"
        );
    }
}
