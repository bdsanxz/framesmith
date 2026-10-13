//! A short clip written with frx7's own writer and read back with frx7: no `ffmpeg`, no `ffprobe`.

use measure::{clip_facts, write_clip, ClipSpec};
use std::path::PathBuf;

/// A folder of its own for one test, made new.
fn folder(name: &str) -> PathBuf {
    let folder = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("clips").join(name);
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).expect("the folder is made");
    folder
}

#[test]
fn a_clip_comes_back_with_the_facts_it_was_written_with() {
    let path = folder("facts").join("clip.mp4");
    let spec = ClipSpec {
        width: 64,
        height: 32,
        frames: 24,
        rate: (30, 1),
    };
    write_clip(&path, &spec).expect("the clip is written");
    let facts = clip_facts(&path).expect("the clip is read");
    assert_eq!(facts.container, "mp4");
    assert_eq!(facts.codec, "h264");
    assert_eq!((facts.width, facts.height), (64, 32));
    assert_eq!(facts.frames, 24);
    assert_eq!(facts.rate, (30, 1));
    assert!(
        (facts.seconds - 0.8).abs() < 1e-9,
        "24 pictures at 30 a second last 0.8 s, not {}",
        facts.seconds
    );
    assert_eq!(facts.bytes, std::fs::metadata(&path).expect("the file is there").len());
}

#[test]
fn a_fractional_rate_is_kept_as_the_fraction() {
    let path = folder("fraction").join("clip.mp4");
    let spec = ClipSpec {
        width: 16,
        height: 16,
        frames: 10,
        rate: (30_000, 1001),
    };
    write_clip(&path, &spec).expect("the clip is written");
    let facts = clip_facts(&path).expect("the clip is read");
    assert_eq!(facts.rate, (30_000, 1001));
    assert_eq!(facts.frames, 10);
    let expected = 10.0 * 1001.0 / 30_000.0;
    assert!((facts.seconds - expected).abs() < 1e-9, "{} against {expected}", facts.seconds);
}

#[test]
fn the_tables_stand_before_the_pictures_so_a_player_can_start_at_once() {
    let path = folder("tables-first").join("clip.mp4");
    let spec = ClipSpec {
        width: 32,
        height: 32,
        frames: 12,
        rate: (24, 1),
    };
    write_clip(&path, &spec).expect("the clip is written");
    let bytes = std::fs::read(&path).expect("the file is read");
    let place = |code: &[u8; 4]| {
        bytes
            .windows(4)
            .position(|window| window == code)
            .unwrap_or_else(|| panic!("no `{}` box", String::from_utf8_lossy(code)))
    };
    assert!(place(b"ftyp") < place(b"moov"), "the brand comes first");
    assert!(place(b"moov") < place(b"mdat"), "the tables come before the pictures");
}

#[test]
fn a_file_that_is_not_a_clip_is_refused_with_its_name() {
    let path = folder("words").join("words.mp4");
    std::fs::write(&path, "These are words, not boxes.").expect("a text file is written");
    let refused = clip_facts(&path).expect_err("words are not a clip");
    assert!(refused.contains("words.mp4"), "{refused}");
}

#[test]
fn a_clip_without_pictures_or_size_is_refused_before_a_file_is_made() {
    let path = folder("empty").join("clip.mp4");
    for spec in [
        ClipSpec {
            width: 0,
            height: 32,
            frames: 4,
            rate: (30, 1),
        },
        ClipSpec {
            width: 32,
            height: 32,
            frames: 0,
            rate: (30, 1),
        },
        ClipSpec {
            width: 32,
            height: 32,
            frames: 4,
            rate: (30, 0),
        },
    ] {
        let refused = write_clip(&path, &spec).expect_err("the spec is refused");
        assert!(refused.contains("a clip needs a size"), "{refused}");
    }
    assert!(!path.exists(), "no file is made for a refused clip");
}
