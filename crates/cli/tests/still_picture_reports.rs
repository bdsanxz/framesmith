//! What stops a run of a composition with an `<image>` (`docs/spec/still-pictures.md`, section 3): a file that is missing
//! and one that is damaged (exit code 1), a file of a kind not read and one that stores a turn (exit code 2). `check` and
//! `render` each report them before the first frame, on the line of the `<image>`, with its `src` and the reason, and
//! `render` writes no film. The kinds read, PNG and JPEG, pass.

mod common;

#[path = "../../render/tests/common/png.rs"]
mod png;

use std::fs;
use std::path::Path;
use std::process::Output;

use common::{err, framesmith, out, project};

const GROUP: &str = "still-picture-reports";

fn composition(images: &str) -> String {
    format!(
        "<composition version=\"1\" width=\"64\" height=\"48\" fps=\"10\" duration=\"0.5\" background=\"#000000\">\n{images}\n</composition>\n"
    )
}

fn has_a_film(project: &Path) -> bool {
    fs::read_dir(project.join("renders"))
        .map(|entries| {
            entries
                .flatten()
                .any(|entry| entry.path().extension().is_some_and(|ext| ext == "mp4"))
        })
        .unwrap_or(false)
}

/// A PNG of 4 by 2 of our own colours.
fn small_png() -> Vec<u8> {
    let pixels: Vec<u8> = (0..8_u8).flat_map(|index| [index * 30, 200, 255 - index * 30, 255]).collect();
    png::encode(4, 2, &pixels)
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffff_u32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 != 0 { 0xedb8_8320 ^ (crc >> 1) } else { crc >> 1 };
        }
    }
    !crc
}

/// The PNG with an `eXIf` chunk after its header that stores the orientation `turn` (PNG third edition; Exif tag 0x0112).
fn turned_png(turn: u16) -> Vec<u8> {
    let png = small_png();
    let mut exif = b"MM\0\x2a\0\0\0\x08\0\x01\x01\x12\0\x03\0\0\0\x01".to_vec();
    exif.extend(turn.to_be_bytes());
    exif.extend([0, 0, 0, 0, 0, 0]);
    let mut body = b"eXIf".to_vec();
    body.extend(&exif);
    let mut chunk = (exif.len() as u32).to_be_bytes().to_vec();
    chunk.extend(&body);
    chunk.extend(crc32(&body).to_be_bytes());
    // The signature is 8 bytes and the header chunk 25.
    let mut turned = png[..33].to_vec();
    turned.extend(chunk);
    turned.extend(&png[33..]);
    turned
}

/// A BMP of 1 by 1, a kind the system reads and this version does not (the BMP file header and a 40-byte info header).
fn bmp() -> Vec<u8> {
    let mut bytes = b"BM".to_vec();
    bytes.extend(58_u32.to_le_bytes());
    bytes.extend([0, 0, 0, 0]);
    bytes.extend(54_u32.to_le_bytes());
    bytes.extend(40_u32.to_le_bytes());
    bytes.extend(1_i32.to_le_bytes());
    bytes.extend(1_i32.to_le_bytes());
    bytes.extend(1_u16.to_le_bytes());
    bytes.extend(24_u16.to_le_bytes());
    bytes.extend([0; 24]);
    bytes.extend([10, 20, 30, 0]);
    bytes
}

/// Runs `check` and `render` on a project whose composition holds `images`, with `files` written beside it.
fn both(name: &str, images: &str, files: &[(&str, Vec<u8>)]) -> (Output, Output, std::path::PathBuf) {
    let (folder, cwd) = project(GROUP, name, &composition(images));
    for (file, bytes) in files {
        let path = folder.join(file);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }
    let checked = framesmith(&cwd, ["check", folder.to_str().unwrap()], &[]);
    let rendered = framesmith(&cwd, ["render", folder.to_str().unwrap(), "--backend", "cpu"], &[]);
    (checked, rendered, folder)
}

/// Holds both runs to the exit code, the place and the words of the report, and no film.
fn stopped(name: &str, images: &str, files: &[(&str, Vec<u8>)], code: i32, place: &str, words: &[&str]) {
    let (checked, rendered, folder) = both(name, images, files);
    for (command, output) in [("check", &checked), ("render", &rendered)] {
        let printed = format!("{}{}", out(output), err(output));
        assert_eq!(output.status.code(), Some(code), "{command} {name}: {printed}");
        assert!(printed.contains(place), "{command} {name}: {place} in {printed}");
        for word in words {
            assert!(printed.contains(word), "{command} {name}: {word:?} in {printed}");
        }
    }
    assert!(!has_a_film(&folder), "{name}: no film is written");
}

#[test]
fn a_picture_file_that_is_not_there_stops_check_and_render_with_its_line() {
    stopped(
        "missing",
        "<box left=\"0\" top=\"0\" width=\"8\" height=\"8\" background=\"#ff0000\"/>\n<image src=\"pictures/none.png\" left=\"2\" top=\"2\"/>",
        &[],
        1,
        "line 3: FS-0068",
        &["`src=\"pictures/none.png\"` names no file"],
    );
}

#[test]
fn a_damaged_picture_file_stops_check_and_render_with_the_reason() {
    // Its pixel data written over, so that its checksum no longer matches; and the file cut short after its header.
    let mut written_over = small_png();
    let at = written_over.len() - 20;
    written_over[at] ^= 0xff;
    let cut = small_png()[..40].to_vec();
    for (name, bytes) in [("written-over", written_over), ("cut-short", cut)] {
        stopped(
            name,
            "<image src=\"pictures/hurt.png\" left=\"2\" top=\"2\"/>",
            &[("pictures/hurt.png", bytes)],
            1,
            "line 2: FS-0069",
            &["`src=\"pictures/hurt.png\"` is damaged or cannot be read", "is damaged"],
        );
    }
}

#[test]
fn a_file_of_a_kind_not_read_is_reported_with_the_kinds_read() {
    for (name, bytes) in [("bmp", bmp()), ("words", b"not a picture at all\n".to_vec())] {
        stopped(
            name,
            "<image src=\"pictures/other.png\" left=\"2\" top=\"2\"/>",
            &[("pictures/other.png", bytes)],
            2,
            "line 2: FS-0035",
            &[
                "`src=\"pictures/other.png\"` is not a picture of a kind this version reads",
                "the kinds read are PNG and JPEG",
            ],
        );
    }
}

#[test]
fn a_picture_that_stores_a_turn_is_reported_unsupported() {
    stopped(
        "turned",
        "<image src=\"pictures/turned.png\" left=\"2\" top=\"2\"/>",
        &[("pictures/turned.png", turned_png(6))],
        2,
        "line 2: FS-0035",
        &["`src=\"pictures/turned.png\"` stores a turn (orientation 6), which this version does not apply (S26)"],
    );
}

#[test]
fn a_picture_of_each_kind_read_passes_check_and_render() {
    let (folder, cwd) = project(
        GROUP,
        "kinds-read",
        &composition("<image src=\"pictures/a.png\" left=\"2\" top=\"2\"/>"),
    );
    fs::create_dir_all(folder.join("pictures")).unwrap();
    fs::write(folder.join("pictures/a.png"), small_png()).unwrap();
    // A JPEG made from the PNG by the system's own tool.
    let made = std::process::Command::new("/usr/bin/sips")
        .args(["-s", "format", "jpeg"])
        .arg(folder.join("pictures/a.png"))
        .arg("--out")
        .arg(folder.join("pictures/b.jpg"))
        .output()
        .unwrap();
    assert!(made.status.success(), "{}", String::from_utf8_lossy(&made.stderr));
    fs::write(
        folder.join("composition.xml"),
        composition("<image src=\"pictures/a.png\" left=\"2\" top=\"2\"/>\n<image src=\"pictures/b.jpg\" left=\"10\" top=\"2\"/>"),
    )
    .unwrap();
    let checked = framesmith(&cwd, ["check", folder.to_str().unwrap()], &[]);
    assert_eq!(checked.status.code(), Some(0), "{}{}", out(&checked), err(&checked));
    let rendered = framesmith(&cwd, ["render", folder.to_str().unwrap(), "--backend", "cpu"], &[]);
    assert_eq!(rendered.status.code(), Some(0), "{}{}", out(&rendered), err(&rendered));
    assert!(has_a_film(&folder));
}

#[test]
fn what_an_image_does_not_take_in_this_version_is_reported_on_its_line() {
    let files = [("pictures/a.png", small_png())];
    stopped(
        "in-a-row",
        "<box left=\"0\" top=\"0\" width=\"60\" height=\"40\" flex-direction=\"row\">\n<image src=\"pictures/a.png\"/>\n</box>",
        &files,
        2,
        "FS-0035",
        &["<image> in a row, a column or a set of columns is not supported by this version"],
    );
    stopped(
        "bad-fit",
        "<image src=\"pictures/a.png\" object-fit=\"stretch\"/>",
        &files,
        1,
        "line 2: FS-0039",
        &["`object-fit=\"stretch\"` is not one of: fill, cover, contain"],
    );
    stopped(
        "bad-position",
        "<image src=\"pictures/a.png\" object-fit=\"cover\" object-position=\"left top\"/>",
        &files,
        1,
        "line 2: FS-0039",
        &["`object-position=\"left top\"` is not two percentages"],
    );
}

#[test]
fn a_picture_file_that_cannot_be_read_stops_check_and_render_as_damaged() {
    // Mode 000: the file is there and is a picture, but no one may read it. It is damaged or cannot be read (FS-0069, exit
    // code 1), not a kind this version does not read.
    use std::os::unix::fs::PermissionsExt;
    let (folder, cwd) = project(
        GROUP,
        "unreadable",
        &composition("<image src=\"pictures/locked.png\" left=\"2\" top=\"2\"/>"),
    );
    let path = folder.join("pictures/locked.png");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, small_png()).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o000)).unwrap();
    let checked = framesmith(&cwd, ["check", folder.to_str().unwrap()], &[]);
    let rendered = framesmith(&cwd, ["render", folder.to_str().unwrap(), "--backend", "cpu"], &[]);
    for (command, output) in [("check", &checked), ("render", &rendered)] {
        let printed = format!("{}{}", out(output), err(output));
        assert_eq!(output.status.code(), Some(1), "{command}: {printed}");
        assert!(printed.contains("line 2: FS-0069"), "{command}: {printed}");
        assert!(
            printed.contains("`src=\"pictures/locked.png\"` is damaged or cannot be read"),
            "{command}: {printed}"
        );
    }
    assert!(!has_a_film(&folder), "no film is written");
}
