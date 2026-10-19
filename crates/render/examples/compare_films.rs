//! Compares the pictures decoded from two films, frame by frame. Development only; nothing that ships uses it.
//!
//! ```text
//! cargo run -p render --example compare_films -- <a.mp4> <b.mp4>
//! ```
//!
//! The pictures of each film are decoded by frx7, from the file. For each frame the number of colour values that differ
//! between the two films is printed, with the largest difference of any value. An encoder may write two films of the same
//! pictures with different bytes, so the comparison is of the decoded pictures, not of the files. Exit code 0 when both films
//! hold the same number of pictures and every picture is equal, 1 when they differ, and 2 when the command is called wrongly
//! or a film cannot be read.

use std::path::Path;

use measure::{open_pictures, Frame};

fn pictures_of(path: &str) -> Vec<Frame> {
    let mut reader = open_pictures(Path::new(path)).unwrap_or_else(|error| {
        eprintln!("{path}: {error}");
        std::process::exit(2);
    });
    let mut pictures = Vec::new();
    loop {
        match reader.next_frame() {
            Ok(Some(frame)) => pictures.push(frame),
            Ok(None) => break,
            Err(error) => {
                eprintln!("{path}: {error}");
                std::process::exit(2);
            }
        }
    }
    pictures
}

fn main() {
    let words: Vec<String> = std::env::args().skip(1).collect();
    let [first, second] = words.as_slice() else {
        eprintln!("usage: compare_films <a.mp4> <b.mp4>");
        std::process::exit(2);
    };
    let (first_pictures, second_pictures) = (pictures_of(first), pictures_of(second));
    println!("{first}: {} pictures", first_pictures.len());
    println!("{second}: {} pictures", second_pictures.len());
    if first_pictures.len() != second_pictures.len() {
        println!("the films hold different numbers of pictures");
        std::process::exit(1);
    }
    let mut equal = true;
    for (number, (a, b)) in first_pictures.iter().zip(&second_pictures).enumerate() {
        if a.width != b.width || a.height != b.height {
            println!("frame {number}: the pictures have different sizes");
            equal = false;
            continue;
        }
        let differing = a.rgb.iter().zip(&b.rgb).filter(|(x, y)| x != y).count();
        let largest = a
            .rgb
            .iter()
            .zip(&b.rgb)
            .map(|(x, y)| (i32::from(*x) - i32::from(*y)).abs())
            .max()
            .unwrap_or(0);
        println!("frame {number}: {differing} of {} values differ, largest difference {largest}", a.rgb.len());
        if differing > 0 {
            equal = false;
        }
    }
    if equal {
        println!("the decoded pictures are equal in every frame");
    } else {
        println!("the decoded pictures differ");
        std::process::exit(1);
    }
}
