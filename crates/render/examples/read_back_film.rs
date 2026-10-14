//! Reads a film back and shows what is in it: the pictures decoded from the file by frx7, measured
//! and put on one labelled sheet, with the checked-in pictures of the same frames under them when a
//! folder of them is given. Development only; nothing that ships uses it.
//!
//! ```text
//! read_back_film <film.mp4> <sheet.png> [--checked-in DIR] [--palette RRGGBB]... [--at X,Y]... <frame>...
//! ```
//!
//! For each frame asked for it prints the time it is shown at, the colour at every `--at` point, and,
//! with the colours of `--palette`, how many pixels are nearest to each of them (an encoder blurs an
//! edge by a pixel, so a pixel belongs to the colour it is nearest to) and the box round those pixels.
//! The sheet has a row `A`
//! for the pictures decoded from the file and, when `--checked-in` is given, a row `B` for the
//! pictures of `DIR/frame-NNNN.png`; each picture has its number and time written above it.

#[path = "../tests/common/png.rs"]
mod png;

use measure::{open_pictures, video_facts, Frame};
use std::path::{Path, PathBuf};

/// How many times each picture is magnified on the sheet.
const SCALE: u32 = 4;
const GAP: u32 = 8;
const LABEL_HEIGHT: u32 = 5 * 3 + 6;

fn usage() -> ! {
    eprintln!("usage: read_back_film <film.mp4> <sheet.png> [--checked-in DIR] [--palette RRGGBB]... [--at X,Y]... <frame>...");
    std::process::exit(2);
}

fn colour(word: &str) -> [u8; 3] {
    let digits = word.trim_start_matches('#');
    if digits.len() != 6 || !digits.is_ascii() {
        usage();
    }
    let byte = |at: usize| u8::from_str_radix(&digits[at..at + 2], 16).unwrap_or_else(|_| usage());
    [byte(0), byte(2), byte(4)]
}

/// The place in `palette` of the colour nearest to `pixel`.
fn nearest(palette: &[[u8; 3]], pixel: [u8; 3]) -> usize {
    let distance = |colour: [u8; 3]| -> i32 { colour.iter().zip(pixel).map(|(a, b)| (i32::from(*a) - i32::from(b)).pow(2)).sum() };
    (0..palette.len()).min_by_key(|&at| distance(palette[at])).unwrap_or(0)
}

fn hex(pixel: [u8; 3]) -> String {
    format!("{:02x}{:02x}{:02x}", pixel[0], pixel[1], pixel[2])
}

/// The glyphs the labels use, each 3 wide and 5 high, one row of three bits per line.
fn glyph(character: char) -> [u8; 5] {
    match character {
        '0' => [7, 5, 5, 5, 7],
        '1' => [2, 6, 2, 2, 7],
        '2' => [7, 1, 7, 4, 7],
        '3' => [7, 1, 7, 1, 7],
        '4' => [5, 5, 7, 1, 1],
        '5' => [7, 4, 7, 1, 7],
        '6' => [7, 4, 7, 5, 7],
        '7' => [7, 1, 1, 1, 1],
        '8' => [7, 5, 7, 5, 7],
        '9' => [7, 5, 7, 1, 7],
        'A' => [2, 5, 7, 5, 5],
        'B' => [6, 5, 6, 5, 6],
        's' => [3, 4, 2, 1, 6],
        '.' => [0, 0, 0, 0, 2],
        _ => [0; 5],
    }
}

struct Sheet {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

impl Sheet {
    fn new(width: u32, height: u32) -> Sheet {
        let mut rgba = vec![0u8; (width * height * 4) as usize];
        for pixel in rgba.as_chunks_mut::<4>().0 {
            *pixel = [0xf0, 0xf0, 0xf0, 255];
        }
        Sheet { width, height, rgba }
    }

    fn set(&mut self, x: u32, y: u32, rgb: [u8; 3]) {
        let at = ((y * self.width + x) * 4) as usize;
        self.rgba[at..at + 4].copy_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
    }

    fn text(&mut self, left: u32, top: u32, text: &str) {
        for (index, character) in text.chars().enumerate() {
            for (row, bits) in glyph(character).into_iter().enumerate() {
                for column in 0..3 {
                    if bits >> (2 - column) & 1 == 1 {
                        for dy in 0..3 {
                            for dx in 0..3 {
                                self.set(left + (index as u32 * 4 + column) * 3 + dx, top + row as u32 * 3 + dy, [0, 0, 0]);
                            }
                        }
                    }
                }
            }
        }
    }

    fn picture(&mut self, left: u32, top: u32, width: u32, height: u32, rgb: impl Fn(u32, u32) -> [u8; 3]) {
        for y in 0..height * SCALE {
            for x in 0..width * SCALE {
                self.set(left + x, top + y, rgb(x / SCALE, y / SCALE));
            }
        }
    }
}

fn main() {
    let mut words = std::env::args().skip(1);
    let (Some(film), Some(sheet_path)) = (words.next(), words.next()) else {
        usage()
    };
    let mut checked_in: Option<PathBuf> = None;
    let mut palette: Vec<[u8; 3]> = Vec::new();
    let mut at: Vec<(u32, u32)> = Vec::new();
    let mut numbers: Vec<usize> = Vec::new();
    while let Some(word) = words.next() {
        match word.as_str() {
            "--checked-in" => checked_in = Some(PathBuf::from(words.next().unwrap_or_else(|| usage()))),
            "--palette" => palette.push(colour(&words.next().unwrap_or_else(|| usage()))),
            "--at" => {
                let point = words.next().unwrap_or_else(|| usage());
                let (x, y) = point.split_once(',').unwrap_or_else(|| usage());
                at.push((x.parse().unwrap_or_else(|_| usage()), y.parse().unwrap_or_else(|_| usage())));
            }
            number => numbers.push(number.parse().unwrap_or_else(|_| usage())),
        }
    }
    if numbers.is_empty() {
        usage();
    }

    let facts = video_facts(Path::new(&film)).unwrap_or_else(|error| panic!("{error}"));
    let mut reader = open_pictures(Path::new(&film)).unwrap_or_else(|error| panic!("{error}"));
    let mut frames: Vec<Frame> = Vec::new();
    while let Some(frame) = reader.next_frame().unwrap_or_else(|error| panic!("{error}")) {
        frames.push(frame);
    }
    println!(
        "decoded {} of {} pictures from the file, {}x{}",
        frames.len(),
        facts.frames,
        facts.width,
        facts.height
    );

    let (width, height) = (facts.width, facts.height);
    let rate = f64::from(facts.rate.0) / f64::from(facts.rate.1);
    let columns = numbers.len() as u32;
    let rows = if checked_in.is_some() { 2 } else { 1 };
    let cell = (width * SCALE + GAP, LABEL_HEIGHT + height * SCALE + GAP);
    let mut sheet = Sheet::new(GAP + columns * cell.0, GAP + rows * cell.1);

    for (column, &number) in numbers.iter().enumerate() {
        let frame = frames.get(number).unwrap_or_else(|| panic!("the file has no frame {number}"));
        let seconds = number as f64 / rate;
        println!("frame {number}, shown at {seconds:.3} s:");
        for &(x, y) in &at {
            println!("  colour at ({x}, {y}): {}", hex(frame.pixel(x, y)));
        }
        for (index, &wanted) in palette.iter().enumerate() {
            let (mut count, mut left, mut top, mut right, mut bottom) = (0u32, u32::MAX, u32::MAX, 0, 0);
            for y in 0..frame.height {
                for x in 0..frame.width {
                    if nearest(&palette, frame.pixel(x, y)) == index {
                        count += 1;
                        (left, top, right, bottom) = (left.min(x), top.min(y), right.max(x), bottom.max(y));
                    }
                }
            }
            if count == 0 {
                println!("  pixels nearest to {}: none", hex(wanted));
            } else {
                println!(
                    "  pixels nearest to {}: {count}, in left {left} top {top} right {right} bottom {bottom} ({}x{})",
                    hex(wanted),
                    right - left + 1,
                    bottom - top + 1
                );
            }
        }
        let left = GAP + column as u32 * cell.0;
        sheet.text(left, GAP, &format!("A {number} {seconds:.3}s"));
        sheet.picture(left, GAP + LABEL_HEIGHT, width, height, |x, y| frame.pixel(x, y));

        if let Some(folder) = &checked_in {
            let path = folder.join(format!("frame-{number:04}.png"));
            let bytes = std::fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            let (w, h, rgba) = png::decode(&bytes).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            assert_eq!((w, h), (width, height), "the checked-in picture of frame {number} has another size");
            let top = GAP + cell.1;
            sheet.text(left, top, &format!("B {number}"));
            sheet.picture(left, top + LABEL_HEIGHT, width, height, |x, y| {
                let at = ((y * w + x) * 4) as usize;
                [rgba[at], rgba[at + 1], rgba[at + 2]]
            });
        }
    }
    std::fs::write(&sheet_path, png::encode(sheet.width, sheet.height, &sheet.rgba)).unwrap();
    println!("sheet {}x{} written", sheet.width, sheet.height);
}
