//! Writes the small videos the evidence script of the comparison tool uses, with frx7's test-only
//! writer (`measure::write_video`): a video of a moving square and a still box, and videos made from
//! the same pictures with one thing changed. Nothing is started and nothing third-party is used.
//!
//! Usage: make_comparison_inputs <new folder>

use measure::Frame;
use std::path::Path;

const FRAMES: usize = 48;
const WIDTH: u32 = 320;
const HEIGHT: u32 = 180;

fn write(folder: &Path, name: &str, rate: (u32, u32), frames: &[Frame]) {
    let path = folder.join(name);
    measure::write_video(&path, rate, frames).unwrap_or_else(|why| panic!("{name}: {why}"));
    println!(
        "wrote {} ({} frames of {}x{} at {}/{})",
        name,
        frames.len(),
        frames[0].width,
        frames[0].height,
        rate.0,
        rate.1
    );
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [folder] = args.as_slice() else {
        eprintln!("usage: make_comparison_inputs <new folder>");
        std::process::exit(2);
    };
    let folder = Path::new(folder);
    if folder.exists() {
        eprintln!("the folder must be new: {}", folder.display());
        std::process::exit(2);
    }
    std::fs::create_dir_all(folder).expect("the folder is made");
    let base = measure::moving_pictures(FRAMES, WIDTH, HEIGHT);
    write(folder, "ours.mp4", (30, 1), &base);
    // The same pictures written a second time, as a separate file.
    write(folder, "rewritten.mp4", (30, 1), &base);
    // One picture differs: 54 rows of 180 (30 % of the pixels), 2 rows (1.1 %), 1 row (0.56 %).
    write(folder, "frame-30-big-change.mp4", (30, 1), &measure::with_green_rows(&base, 30, 54));
    write(
        folder,
        "frame-30-just-over-one-percent.mp4",
        (30, 1),
        &measure::with_green_rows(&base, 30, 2),
    );
    write(
        folder,
        "frame-30-just-under-one-percent.mp4",
        (30, 1),
        &measure::with_green_rows(&base, 30, 1),
    );
    // Eleven pictures differ, each in 30 % of its pixels: the average falls under 99 %.
    let mut many = base.clone();
    for number in 10..=20 {
        many = measure::with_green_rows(&many, number, 54);
    }
    write(folder, "frames-10-to-20-big-change.mp4", (30, 1), &many);
    // One frame missing, another size, another rate with the same number of frames.
    write(folder, "one-frame-missing.mp4", (30, 1), &base[..FRAMES - 1]);
    write(folder, "other-size.mp4", (30, 1), &measure::moving_pictures(FRAMES, 256, 144));
    write(folder, "other-rate.mp4", (24, 1), &base);
}
